# PRD: Cross-window delegation (codename: *baton*)

**Author:** Claude (Opus 4.7), drafted with jsy
**Status:** Draft v0.1
**Date:** 2026-05-24
**Supersedes:** the `delegate.run` method shipped in `~/.claude/scripts/agorabus-worker.sh` (kept as `baton.spawn` fallback; the load-bearing primitive moves)
**Sibling to:** [[cross-session-bus]] (agorabus — the transport), [[agentsh]] (the agent-mode shell — natural home for the per-window receiver)
**Worked example:** From this window (`claude-890-jsy`), I publish a baton; the *interactive* claude that jsy is currently looking at in window 2 (`claude-2049-jsy`, attached to `/dev/pts/1` inside an xterm) types the prompt itself and submits it — as if jsy had typed it. jsy watches it work in real time.

---

## TL;DR

`delegate.run` v0.1 spawns a *fresh headless* `claude --print` whenever a peer asks for work. The session that "delegates" is invisible — a third claude on the laptop that no one is watching, running until exit, replying once on completion. That isn't what "delegate to the other window" means. **`baton` is the missing primitive: a way to drive a target claude session that is already open, interactive, and visible to the user**, so the user's existing window picks up the work in front of them. Mechanism: each interactive claude registers its addressable surface (X11 window-id for an xterm/foot/kitty; tmux pane address for a tmux-hosted session) at SessionStart and announces it on agorabus. A `baton` sender resolves the target's surface, types the prompt into it via the appropriate injector (`xdotool type` for X11, `tmux send-keys` for tmux), and presses Enter. The target claude treats the input as a normal user prompt. No new headless process, no invisible work. Progress is visible because it happens in the user's actual terminal. TIOCSTI is *not* an option on this laptop — the kernel has `CONFIG_LEGACY_TIOCSTI` unset and `dev.tty.legacy_tiocsti=0`, confirmed 2026-05-24.

`delegate.run` is kept as a fallback under a renamed `baton.spawn` method for the genuine fire-and-forget case (long-running background scaffold, no human watching). The default delegation primitive becomes `baton.send` to a registered live window.

---

## 1. Why this exists

1. **`delegate.run` is the wrong shape for "control the other window."** Today the call does `claude --print --dangerously-skip-permissions` in a fresh subprocess. Output streams only to a worker log; the user can't watch it; the reply is single-shot on exit. When jsy asked the two windows on his desk to split a workload, he meant *the two windows he has open*, not "spawn an invisible third claude." Discovered live 2026-05-24 when I fired `delegate.run id=rpc-e81b39` and jsy noticed his interactive `claude-2049` terminal stayed silent — because the worker had correctly forked a headless instance (PID 5480), which is exactly the bug.

2. **The native primitives almost work.** Both X11 and tmux already expose keystroke-injection APIs to anyone with the right authority on the local socket: `xdotool type --window <wid>` and `tmux send-keys -t <pane>`. The blocker is *resolution*: how does the sender know which window-id or pane corresponds to which agorabus session id? Today the sender doesn't, because nothing announces it. Adding that announcement closes the gap.

3. **TIOCSTI is gone.** The historical "write to the slave pty as if it were typed input" path is locked on modern kernels. Confirmed on this laptop: `7.0.9-arch1-1`, `CONFIG_LEGACY_TIOCSTI` not set, `dev.tty.legacy_tiocsti=0`. We are not going to argue with that — the design must work without it.

4. **The user is the watcher.** The reason the user wants control of the live window is that they want to *see and steer* the delegated work — same way they steer this window. Pushing work into an invisible headless instance produces "trust me, it's running" updates, which is precisely the failure mode of background agents. Baton restores the property that work is visible by default.

5. **It's a small protocol on top of agorabus.** No daemon changes, no new transports. One new SessionStart announcement, one new method, one resolver. The convention scales to any future window type (kitty, foot, GNOME Terminal, Windows Terminal under WSL) by registering a new injector backend per terminal-class.

---

## 2. Who this is for

- **Me, talking to my other self in real time.** When jsy splits work between two windows, each instance is itself an agent that needs to address the other. Baton is the addressing primitive.
- **jsy as the human steering both.** Visible work in a real terminal. He can ctrl-c, redirect, ask a clarifying question — the same way he steers either window today.
- **Not** for: external IPC into a remote machine (use agorabus over a tunnel + spawn for that), driving non-claude TUIs (out of scope; the receiver-side semantics assume claude's prompt loop), or replacing slash commands within a single window.

---

## 3. What I'd use it for (concretely)

| Use case | Today (broken) | With baton |
| --- | --- | --- |
| "Split N PRDs between window A and B, both visible" | A invisibly forks a third claude; B is idle | A types into B; jsy watches B work |
| "Ask the other window to run a specific check" | rare; not attempted today | `baton.send to=B prompt="show me current journal entry"` — B answers in its own UI |
| "Hand off a long task when the current window is about to compact" | not possible | `baton.send` to a fresh window opened explicitly to receive |
| "Background scaffold work, no human watching" | `delegate.run` — fine for this | `baton.spawn` — preserves the headless mode under its honest name |
| "Coordinate a pair-of-claudes review pass" | spawn two headless, hope output merges | type into two open windows, watch them speak in turn |

---

## 4. Functional requirements

### 4.1 Surface registration

At SessionStart, each interactive claude session announces its **injectable surface** on agorabus. New method-namespace: `baton.*`. Announcement event published once on `baton.surface.<self-session-id>`, also re-published on a 60s heartbeat for late subscribers.

```jsonc
// payload for baton.surface.<sid>
{
  "session_id": "claude-2049-jsy",
  "surface": {
    "kind": "x11",                       // or "tmux", "kitty", "wsl-conpty"
    "window_id": "0x4a00007",            // hex window-id from `xdotool search`
    "wm_class": "XTerm",                 // for fallback re-resolution
    "pid_chain": [879, 2049],            // xterm → claude
    "display": ":0"
  },
  "capabilities": {
    "type": true,                        // can accept `baton.type` (text + ENTER)
    "key": true,                         // can accept `baton.key` (single keychord)
    "paste": false                       // X11 primary-selection paste; v0.2
  },
  "claude_version": "2.1.150 (Claude Code)",
  "registered_unix": 1779654473
}
```

For tmux-hosted sessions:

```jsonc
{
  "session_id": "claude-2049-jsy",
  "surface": {
    "kind": "tmux",
    "socket": "/tmp/tmux-1000/default",
    "pane_target": "main:0.0",           // session:window.pane
    "pid_chain": [tmux_server_pid, 2049]
  },
  "capabilities": { "type": true, "key": true, "paste": true }
}
```

The registration is *advisory* — a sender that can't reach the surface (wrong DISPLAY, tmux socket gone) replies `baton.unreachable` with the resolved address, and the requester decides whether to fall back to `baton.spawn`.

### 4.2 RPC methods

Added to the per-session worker's whitelist:

- **`baton.send`** — primary entry point.
  - params: `{prompt: string, target_session_id?: string, target_surface?: SurfaceRef, dry_run?: bool, submit?: bool}`
  - default: types `prompt`, then a literal Enter, then waits up to `settle_ms` (default 750) before reply.
  - `dry_run:true` — resolves the surface and returns it, types nothing.
  - `submit:false` — types prompt, omits the Enter. For staging multi-line input.
  - result: `{ok:true, result:{surface_used: SurfaceRef, bytes_typed, submitted: bool}}`
- **`baton.key`** — single chord (`C-c`, `Esc`, `Up`). For interruption / navigation.
  - params: `{chord: string, target_session_id: string, repeat?: u32}`
- **`baton.spawn`** — explicit fire-and-forget headless. Equivalent to today's `delegate.run`. Kept under its honest name so callers who *want* invisibility opt into it.
- **`baton.surface`** — discovery / health-check. `{ok:true, result: SurfaceRef}` or `unreachable`.
- **`baton.peers`** — list registered surfaces (just the agorabus-peers shape with surface info merged).

### 4.3 Sender protocol

```text
1. baton peers                          # discovery
2. baton send --to <sid> "<prompt>"     # publishes baton.send on rpc.req.<sid>
3. local-side prep: subscribe to rpc.reply.<self> with --max-events 1
4. receiver's worker: resolve surface, dispatch to injector backend, type, ENTER, reply
5. sender: read reply within deadline; on `unreachable`, prompt user (or fall back per --on-unreachable)
```

A tiny CLI wrapper `~/.local/bin/baton` provides the human-typeable shape:

```sh
baton peers
baton send claude-2049-jsy "implement PRD-foo.md"
baton key  claude-2049-jsy --chord C-c    # interrupt the other window
baton spawn --any "do this headless"      # falls back to the v0.1 shape
baton dry  claude-2049-jsy "preview, don't type"
```

### 4.4 Receiver / injector

Per surface kind, a small adapter under `~/.local/lib/baton/injectors/`:

- `x11.sh`   — `xdotool type --window <wid> --delay <ms> -- "<text>" && xdotool key --window <wid> Return`
- `tmux.sh`  — `tmux -S <socket> send-keys -t <pane> -l -- "<text>" && tmux send-keys -t <pane> Enter`
- `kitty.sh` — `kitty @ send-text --match id:<id> -- "<text>"; kitty @ send-key Return`
- `wsl-conpty.sh` — out of scope v0.1; documented stub.

Each injector reads JSON params on stdin, types, exits 0 on success or 2–9 on a specific failure (window gone, surface mismatch, X11 unreachable, etc.). The worker calls the right injector based on `surface.kind` and translates the exit code into the agorabus reply envelope.

The injector is the ONLY component that needs root or `--dangerously-skip-permissions` style escalation — and even that only insofar as `xdotool` needs access to `$DISPLAY` (no privileges).

### 4.5 Auth / safety

Two new constraints beyond the existing AGORABUS_RPC ones:

1. **Surface ownership.** A baton sender can address only surfaces registered by sessions running as the same uid. The receiver's worker verifies `proc/<owner_pid>/status:Uid` matches `getuid()` before injecting; rejects with `not_owner` otherwise.
2. **Cooldown between injects.** Per-target rate limit (default: max 1 inject per 250 ms; configurable). Stops a runaway sender from spamming. The receiver tracks a small per-`from` count and rejects with `cooldown` if exceeded.

Neither is a security boundary against a malicious local user — they own the bus already — but both stop accidental floods that would lose the user's keystrokes.

### 4.6 Idempotency / late delivery

`baton.send` carries an `id`. The receiver remembers the last 64 `(from, id)` pairs it processed; a duplicate gets a `replay` reply instead of re-typing. Solves the late-delivery race where a sender retransmits because the deadline lapsed before the reply landed.

### 4.7 Observability

Every successful inject appends an NDJSON line to `~/.cache/baton/inject-log.ndjson`:

```jsonc
{"unix": 1779654533, "from": "claude-890-jsy", "to": "claude-2049-jsy",
 "surface": "x11/0x4a00007", "bytes": 142, "submitted": true, "result": "ok"}
```

Failure path logs the same shape with `result` set to the error code and `detail` field. This is the audit log when "the other window suddenly typed something weird" happens.

### 4.8 Storage layout

```
~/.cache/baton/
  inject-log.ndjson                     (audit log, rotates daily)
  surfaces/<session_id>.json            (registered surface, last-known)

~/.local/bin/baton                      (CLI symlink → ~/wintermute/baton/target/release/baton)
~/.local/lib/baton/injectors/           (x11.sh, tmux.sh, kitty.sh, …)
~/.claude/scripts/baton-register.sh     (SessionStart helper — detects surface, publishes)
~/.claude/scripts/agorabus-worker.sh    (existing — gains baton.* method dispatch)
```

---

## 5. Architecture

Three components:

- **`baton-register.sh`** — runs as a SessionStart hook. Detects the current terminal kind (env: `TMUX`, `KITTY_LISTEN_ON`, then `xdotool search --pid <claude_pid>` for X11), constructs the surface descriptor, writes `~/.cache/baton/surfaces/<sid>.json`, and publishes `baton.surface.<sid>` on agorabus. Re-published every 60s by a backgrounded heartbeater.

- **`agorabus-worker.sh`** — gains `baton.*` method handlers. On `baton.send`: load own surface, validate sender uid, dispatch to injector, capture exit, reply.

- **`baton` CLI** — sender ergonomics. One binary (Rust), reads `~/.cache/baton/surfaces/` for discovery, falls back to `baton.peers` RPC if the cache is stale or missing. Sub-200 LoC because all the load-bearing work is in the worker.

Why a Rust binary for the CLI when injectors are shell: the CLI handles JSON envelope shape, subscribe-before-publish race, deadline math, and exit-code mapping. Those are exactly the things that bite when written in bash (already evidenced by the 13:37 incident where my hand-rolled bash subscribe-then-publish worked but was 30+ lines of jq).

---

## 6. Non-goals

1. **Cross-machine delegation.** baton is single-host. If you need to drive a remote claude, that's agorabus-over-ssh + a tunnel — different design, different threat model.
2. **Driving non-claude TUIs.** The receiver semantics assume the target is at a claude prompt. Driving `vim` or `htop` is undefined behavior (injector will type; nothing parses).
3. **A general "remote-control-any-app" framework.** baton is purpose-built for the claude/claude interlocutor pattern. The injectors are stable, but the registration story (claude SessionStart hook publishing surface) is specific.
4. **Auth across trust boundaries.** Same as agorabus v0.1: anyone on the local socket can publish. baton adds uid-match on the receiver side, no more. Multi-user laptops need signing.
5. **Bidirectional streaming.** baton sends keystrokes one way. The target's reply is whatever the target chooses to type back — not piped over the bus. (If a reply is structured, the target can `agorabus publish` to a topic the sender subscribes to. That's already supported; it's not new.)
6. **Injecting prompts during the target's in-flight turn.** v0.1 assumes the target is at its input prompt. If it's mid-response, the injected keystrokes go to whatever input field is focused. v0.2 may add a `wait_for_prompt` capability if claude's TUI exposes a detectable idle state.

---

## 7. Phasing

| Phase | Scope |
| --- | --- |
| **0** | Surface detection: write `baton-register.sh`; verify it correctly identifies the current x11 xterm window-id for both live claudes; publish to agorabus and confirm subscribers see it. |
| **1** | `x11` injector + worker dispatch + minimum-viable `baton send` CLI. Manual end-to-end: from window A, baton-send a "hello" to window B, watch window B's prompt receive it. |
| **2** | `tmux` injector + automatic surface-kind dispatch. Convert one persistent claude session to tmux-hosted; verify tmux send-keys path is reachable. |
| **3** | `baton.key` + `baton.spawn` (move v0.1 `delegate.run` under the renamed method, deprecate the old name with a 90-day shim). Inject-log + dedupe table. |
| **4** | Uid check, cooldown, `dry_run`. Hardening. |
| **5** | Per-session `claude-self` integration: each session's CLAUDE_SELF.md gains a "I can be reached via baton at surface X" line; the autobuilder receipts include a `baton.unreachable_count` metric so degraded delegation surfaces fast. |

Phase 0–1 is the minimum that fixes today's bug. Phase 5 is the "treat baton as a load-bearing primitive" milestone.

---

## 8. Risks

- **Typed text races with the user's own typing.** If jsy is typing into window B while window A baton-sends, characters interleave at the X11 keyboard event layer. *Mitigation:* injector takes a per-surface inject-lock (file lock under `~/.cache/baton/locks/<sid>.lock`); a single keystroke from the human is enough to break a still-typing baton (`xdotool` is paced — the user's typing physically interleaves at the event queue). Document the property: "if you start typing while a baton is in progress, expect garbled input on this side; baton retries are NOT automatic." Same property tmux exhibits with `send-keys` during human typing.
- **`xdotool` needs the X11 window to exist; fails silently otherwise.** *Mitigation:* injector verifies `xdotool getwindowname` succeeds before typing; failure → `unreachable` reply, not partial type.
- **Surface staleness.** A claude session crashes; its surface is still in `~/.cache/baton/surfaces/`. The next sender targets a dead window. *Mitigation:* SessionEnd hook unlinks the surface file; 60s heartbeat marks stale ones with `last_seen_unix`; sender treats >120s stale as `unreachable` unless `--force`.
- **WM steals focus.** `xdotool type --window <wid>` injects directly into that window without raising it — but some WMs reject input to unfocused windows. *Mitigation:* register the surface kind, not just `x11`; for WMs known to reject (e.g. Wayland under non-XWayland), surface kind becomes `wayland-unsupported` and baton refuses to inject.
- **Wayland.** Long-term, X11 keystroke injection is a dying art. *Mitigation:* under Wayland the surface kind is `wayland-unsupported` and baton falls back to `baton.spawn`. Native Wayland injection needs compositor-specific protocols (Hyprland has one; sway via wlroots has one; GNOME does not) — out of scope v0.1 but architecturally provided via the kind dispatcher.
- **`--dangerously-skip-permissions` not on the target.** baton types a prompt; if the target then runs tool calls that prompt the user for permission, jsy sees those prompts and approves/denies in his real window — which is the design (we *want* him in the loop). Document: baton injects prompts, not approvals.
- **Long prompts hit `xdotool type` length quirks.** `xdotool` chunks long strings; rate is `--delay` per char (default 12ms). A 5KB prompt is 60 seconds of typing. *Mitigation:* for prompts > N bytes (default 1024), use clipboard-paste: `xclip -selection clipboard` → `xdotool key --window <wid> shift+Insert` → wait → restore prior clipboard. Documented capability `paste:true` opt-in; off by default because it clobbers the user's clipboard.
- **Submit-press lands on the wrong widget.** If the target claude TUI is in a slash-command picker, the injected Enter selects a command rather than submitting a prompt. *Mitigation:* `submit:false` is the safe stage-only mode; default `submit:true` documented as "type AND press Enter; target should be at its main prompt."
- **Headless agents using `baton.spawn` lose the visibility property.** That's by design — explicit opt-in to invisibility. The PRD makes the trade explicit in the method name.

---

## 9. Open questions

1. **Should baton type the literal prompt text, or wrap it in a marker the receiver auto-extracts?** Wrapping (`>>>BATON_FROM:sid\n<prompt>\n<<<BATON`) gives the receiver provenance for free, but only if the receiver claude is taught to strip it. Probably v0.1: type the literal text; v0.2: optional marker recognized by a target-side hook.
2. **What about the human-pasted clipboard collision?** Paste-mode (long prompts) clobbers the user's clipboard. Save/restore is racy. Maybe better to use X11 PRIMARY selection (middle-click paste lane) instead of CLIPBOARD; less commonly used by humans. Or refuse paste-mode unless an X11 selection-owner check shows the clipboard is empty.
3. **Should `baton send` block until the target finishes responding?** Hard, because the target's prompt loop has no externally observable "done" signal. v0.1 returns as soon as the inject completes; `wait` is a v0.2 question that probably needs a target-side hook publishing a `baton.done` event.
4. **Is `baton.key` enough for interruption?** ctrl-c via `xdotool key C-c` should propagate to the foreground process in xterm — but it depends on the WM and on the xterm focus model. Test before promising.
5. **Tmux as the default for future claude sessions?** If we converted every interactive claude on this laptop to launch inside tmux instead of bare xterm, baton's tmux path becomes the default and the x11/xdotool path becomes the fallback. Tmux is more reliable (no WM in the loop) and gives us free pane history. Cost: one extra wrapper script in the launcher. Probably worth a separate decision soon.
6. **Should the registration heartbeat live in agorabus-worker.sh or its own process?** Probably the worker — already long-running, already publishes — but the worker is currently `set -u` and the heartbeat adds branching. Cleaner separation may be worth a process.
7. **Should jsy be able to baton-send to himself in this window?** Useful for "queue up the next prompt" or "stage long input." But violates the invariant that baton's user is the agent, not the human. Probably no in v0.1.

---

## 10. Acceptance criteria

| ID | Level | Test |
| --- | --- | --- |
| AC1 | MUST | `baton-register.sh` correctly publishes a `baton.surface.<sid>` event with `kind=x11`, a window_id resolvable by `xdotool getwindowname`, and the live pid in `pid_chain`. Test: in a fresh xterm-hosted claude, run the script and verify both the agorabus event and the cache file. |
| AC2 | MUST | `baton send <other-sid> "echo HELLO_BATON"` typed from sender's worker arrives in the target window as a normal prompt, target replies, and target's reply contains "HELLO_BATON" (or claude's expanded form of it). Verified by inspecting the target's transcript JSONL after the test. |
| AC3 | MUST | `baton send` to a stale surface (target killed mid-test) returns `unreachable` with the resolved surface in the reply detail; types no characters into any window. |
| AC4 | MUST | `dev.tty.legacy_tiocsti=0` does NOT regress baton: baton's path doesn't depend on TIOCSTI, and `setterm/echo > /dev/pts/N` is never invoked. Verified by static grep over the codebase. |
| AC5 | SHOULD | `baton key <sid> --chord C-c` interrupts a running tool call in the target window (target sees the interrupt; claude's TUI shows the standard "Request interrupted" line). Hand-verified across at least one xterm + one tmux test. |
| AC6 | SHOULD | The receiver's audit log (`~/.cache/baton/inject-log.ndjson`) records every successful inject with `from`, `to`, `surface`, `bytes`, `submitted`, `result`. |
| AC7 | SHOULD | `baton.spawn` (renamed delegate.run) behaves identically to v0.1: spawns `claude --print` headless, replies with stdout. Existing callers continue to work via a deprecation shim (old method name → new method, with stderr warning). |
| AC8 | MAY | Long-prompt paste mode (>1024 bytes) uses CLIPBOARD with save/restore; falls back to `type` if `xclip` is missing; documented. |
| AC9 | MAY | Tmux-hosted target works end-to-end via `tmux send-keys`. Surface registration auto-detects `$TMUX`. |

The MUSTs are the bar for declaring v0.1 done.

---

## 11. Relationship to other PRDs / tools

- **[[cross-session-bus]]** (agorabus) — baton is built entirely on top; no changes needed to the bus itself.
- **`AGORABUS_RPC.md`** (the convention doc) — gains a new section listing the `baton.*` method namespace as a standard extension.
- **[[agentsh]]** — the natural home for `baton-register.sh` once agentsh ships (the agent-mode shell wraps the launch and knows its terminal). v0.1 lives as a SessionStart hook; v0.2+ moves into agentsh's launch path.
- **[[claude-self]]** — each session's CLAUDE_SELF.md gains a one-line "I can be reached via baton at <surface>" entry, auto-maintained.
- **[[mirror]]** (self-evaluator) — baton-driven delegations between windows are a new evaluable interaction class; mirror's metric set should grow a "baton turnaround latency" measure.
- **[[skill-telemetry]]** (spool) — every `baton send` invocation is a skill-shaped event; log to spool for usage shape over time.
- **[[ambient-compositions]]** — baton inject events are a clean event stream for the audio piece (the "ping" sound when one window types into the other). Aesthetic, not load-bearing.

---

## 12. Today's open situation (2026-05-24)

The window that prompted this PRD: claude-890-jsy (this one) fired `delegate.run id=rpc-e81b39` at 13:37 PT targeting claude-2049-jsy. The receiver's worker correctly spawned a *headless third claude* (PID 5480, RSS ~268MB, running on the kernel-triad prompt). jsy noticed his interactive `claude-2049` window was idle and called the bug. The headless instance is still alive as of PRD-write time. Decisions to make once this PRD is accepted:

1. Whether to let PID 5480 keep grinding (it's doing real work on agent-namespace/memlog/provenance-fs scaffolds, just invisibly) or kill it cleanly and re-fire as a `baton.send` to the interactive window once baton ships.
2. Whether to launch the next interactive claude inside tmux to make AC9 viable now rather than later.
3. Whether to convert the existing `delegate.run` shim before or after `baton.send` is proven end-to-end.

None of these block this PRD landing; they are the first three rows of its implementation backlog.

---

## Changelog

- 2026-05-24 (v0.1): initial draft, written under instruction "GET THIS RIGHT" after the user observed that `delegate.run` v0.1 doesn't control the live window — it spawns a third invisible instance. Tractability probed: TIOCSTI is locked on the wintermute kernel; xdotool + xterm path is the v0.1 baseline; tmux path is the v0.1 cleanup. The "control the live window" semantics are the load-bearing change, not the transport.
