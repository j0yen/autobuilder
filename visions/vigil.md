# Vision: vigil

> Code is committed. The binary is reinstalled. The daemon keeps
> running yesterday's bytes. Vigil is the discipline of watching the
> live fleet for stale code — and rolling fresh code in without
> dropping the conversation.

Created: 2026-05-28
Seed: reflection — run-18 self-review (2026-05-28 ~20:02 PDT) re-opened
  the "agorabus daemon stale binary" item the **same day** it had been
  resolved at runs 16–17. Caught live during this dream's Phase 1: pid
  2138939 still exec'ing `/home/jsy/.local/bin/agorabus (deleted)`.
Pace: opt-in (default — drafts ship to /build per 2026-05-27 instruction).

## TL;DR

`freshness` catches stale *memory bodies*. `drift` catches stale
*skill text*. Neither catches the third axis: a **running process
executing a binary that no longer matches its source**. That axis bit
the laptop repeatedly. Run-18 journal (verbatim):

> agorabus daemon stale binary — RE-OPENED. commit `02350fb` /
> `cf98f2d` (v0.4.0 multi-prefix-subscribe) landed 19:56, running
> daemon pid 2138939 built 14:55 = pre-fix. Escalated, not
> auto-restarted.

It is *still* stale during this Phase 1: `/proc/2138939/exe` resolves to
`/home/jsy/.local/bin/agorabus (deleted)` — the 20:52 reinstall unlinked
the inode the daemon is running from. The kernel hands us a crisp
staleness flag (`(deleted)` suffix) and provfs hands us a second one
(`user.prov.ts` on the on-disk binary, observed `1780026726` =
2026-05-28 20:52). Yet nothing on the laptop reads either. Every
self-review hand-writes the same finding and parks it.

Vigil builds the missing layer:

1. A **read-only detector** (`binstale`) that classifies, per running
   PID or per fleet glob, whether the executing binary is stale —
   using the kernel's `(deleted)` signal, inode drift, and provfs
   timestamps — and compares against the source repo's HEAD.
2. A **safe rolling-restart orchestrator** (`rollout`) that consumes
   the detector's verdict and brings stale daemons current one at a
   time, with a window guard so it never kills a voice daemon
   mid-conversation.
3. **Integration** so self-review surfaces stale daemons structurally
   (not a hand-written journal note every tick) and so agorabus can
   answer "am I current?" about its own running process.

This is the sibling of `freshness` (memory drift) and `drift` (skill
drift): same evidence-rich, proposal-first, never-silently-mutate
ethos — but the action half (`rollout`) genuinely mutates the live
fleet, so it is a separate, opt-in, one-at-a-time, window-guarded tool
with `--dry-run` as the default posture.

## End-state

When vigil is fully built:

- `binstale scan` reports every long-lived wintermute daemon
  (`agorabus`, `recalld`, `wm-audio|dialog|stt|tts`, …) with a verdict:
  `fresh | deleted-exe | inode-drift | prov-stale | behind-head`, plus
  the evidence (exe path, inode pair, provfs ts, HEAD commit ts).
- The "stale binary" anomaly that self-review re-discovers by hand
  every tick is surfaced by a structured probe, with the exact
  `rollout` command pre-filled in Pending.
- `rollout apply` rebuilds → reinstalls → gracefully restarts → polls
  agorabus `peers` to confirm re-registration, **one daemon at a time**,
  refusing to touch a voice daemon while a dialog turn is in flight.
- `agorabus doctor` answers whether the running bus daemon is current
  with its installed binary — the daemon introspecting its own
  staleness rather than waiting for an external scan.
- The escalate-don't-auto-restart call (which paid off at run 16) stays
  a deliberate human choice — vigil makes that choice *one command*
  instead of a five-step hand-rolled rebuild/reinstall/kill/relaunch/push.

## Components

**Fleet 1 — five PRDs:**

1. **binstale** (`rust-cli`, new repo `~/wintermute/binstale/`) —
   foundational, read-only detector. `binstale check <pid>` and
   `binstale scan --match <regex>` classify a running process's exe as
   `fresh | deleted-exe | inode-drift | prov-stale`. Signals: the
   `(deleted)` suffix on `/proc/PID/exe`; inode mismatch between
   `/proc/PID/exe` and the resolved installed path; provfs
   `user.prov.ts` of the on-disk binary older than a reference. JSON +
   table output. No mutation.

2. **binstale-source-cmp** (`rust-extend` → `~/wintermute/binstale/`) —
   the `behind-head` verdict. Maps a daemon to its source repo, reads
   `git log -1 --format=%ct -- src/` for the newest commit touching
   source, and flags a running binary whose provenance/install ts
   predates that commit — even when the binary still exists on disk
   (the run-18 case exactly: binary built 14:55, fix committed 19:56).

3. **rollout** (`rust-cli`, new repo `~/wintermute/rollout/`) — the
   orchestrator. Consumes `binstale scan --format json`; for each stale
   daemon runs rebuild → install → graceful SIGTERM+relaunch → poll
   agorabus `peers` for re-registration, strictly serialized. `--dry-run`
   is the default; `apply` mutates. Per-daemon launch recipe lives in a
   config map.

4. **binstale-self-review** (`shell`, edits
   `~/.claude/skills/self-review/SKILL.md`) — wires `binstale scan` into
   self-review Phase B.5 so the recurring stale-binary item becomes a
   deterministic probe with the `rollout` command pre-filled in Pending,
   retiring the hand-written journal note.

5. **agorabus-doctor-selfstale** (`rust-extend` →
   `~/wintermute/agorabus/`) — `agorabus doctor` reports whether the
   running bus daemon is current with its installed binary, via the same
   `(deleted)`/inode/provfs signals applied to its own pid. Lets the bus
   answer "am I current?" without an external scan.

## Order

```
binstale  (read-only detector; ship first)
   ├──► binstale-source-cmp     (extends binstale: behind-head verdict)
   ├──► binstale-self-review    (wires binstale scan into self-review)
   └──► rollout                 (consumes binstale scan; mutates fleet)
agorabus-doctor-selfstale       (independent; agorabus self-introspection)
```

- `binstale-source-cmp` and `binstale-self-review` both extend/consume
  `binstale` — ship `binstale` first.
- `rollout` consumes `binstale scan` output; it can ship before
  `binstale-source-cmp` (it acts on `deleted-exe`/`inode-drift` alone)
  but is strictly better with the `behind-head` verdict in hand.
- `agorabus-doctor-selfstale` extends a *different* repo (agorabus) and
  has no dependency — it can ship any time, but coordinate with the live
  agorabus rollout (see gossip / open questions).

## Fleet 3 — the handover mechanism (drafted 2026-05-29)

Fleet 1 detects staleness and orchestrates a fleet-wide rolling restart
(`rollout`). It assumes a bus bounce is a *brief* drop. Phase 1 of the
2026-05-29 dream proved that assumption false (see Open Questions →
Restart vs reload). Fleet 3 makes a live agorabus bounce **non-
destructive**, so `rollout` (and self-review's auto-fix) can act on the
bus without stranding live sessions. These extend `~/wintermute/agorabus/`
except where noted; they compose under `rollout` (which can call
`agorabus reload` for the bus and fall back to SIGTERM+relaunch for
daemons that lack it).

1. **agorabus-client-reconnect** (`rust-extend` → `~/wintermute/agorabus/`,
   `src/client.rs`) — **the keystone.** The long-lived `subscribe` client
   detects EOF / `ECONNRESET`, then loops: reopen the socket with
   bounded exponential backoff + jitter, re-`announce`, re-`subscribe`
   to the same prefixes, and resume appending to the same inbox ndjson —
   without re-running the SessionStart hook. Makes any daemon bounce
   survivable for already-running sessions. Everything else in Fleet 3
   depends on this existing.

2. **agorabus-drain-notice** (`rust-extend` → `~/wintermute/agorabus/`,
   `src/daemon.rs` + `src/main.rs`) — graceful shutdown. On SIGTERM the
   daemon broadcasts `{"op":"bus.draining","resume_after_ms":N}` to all
   subscribers before closing the listener, so reconnect clients stagger
   their retry by a server-suggested delay (no thundering herd at
   rebind). Consumed by the reconnect loop from PRD 1.

3. **agorabus-state-persist** (`rust-extend` → `~/wintermute/agorabus/`,
   `src/daemon.rs`) — finishes the persistence `daemon.rs:72` defers
   ("claims … dropped on daemon restart per PRD-chord-claim §State
   persistence"). Journals the claims table (and sticky intents) to
   `~/.cache/agorabus/state.json` on mutation + on drain, rehydrates on
   start. Survives a bounce so chord-claim locks and intents are not
   silently dropped during a reload.

4. **agorabus-reload** (`rust-extend` → `~/wintermute/agorabus/`, new
   `Command::Reload` + `src/reload.rs`) — the *non-destructive* version
   of vigil's one-command bounce. Verifies a fresh binary exists (built
   ahead), records the pre-bounce peer count, sends the drain signal to
   the running daemon, waits for clean exit, execs the new daemon, waits
   for socket bind, then polls `peers` until the count recovers (via the
   PRD-1 reconnect path) within a timeout — emits a verdict. Depends on
   PRDs 1–3.

5. **agorabus-reload-self-review** (`shell` → edits
   `~/.claude/skills/self-review/SKILL.md` playbook
   `agorabus_daemon_stale_binary`) — once the bounce is non-destructive,
   the auto-fix can use `agorabus reload` and the ≤5-subscriber ceiling
   (`SKILL.md:259`) can be lifted to a higher bound (reconnect handles
   the disruption). Closes the 4+-run carried-forward escalation loop.
   Depends on PRD 4 shipping + verified.

**Order (Fleet 3):**

```
agorabus-client-reconnect   (keystone; ship first)
   ├──► agorabus-drain-notice    (reconnect consumes the drain delay)
   └──► agorabus-state-persist   (independent of drain; both extend agorabus — serialize commits)
agorabus-reload                  (depends on reconnect + drain + persist)
   └──► agorabus-reload-self-review  (depends on reload shipped + verified)
```

All four agorabus extends touch the same crate — **serialize** their
/build cycles (reconnect → drain → persist → reload) to avoid lib.rs /
Cargo churn rebases, same caution Fleet 1's gossip raised for the
companion fleet.

## Fleet 2 (not drafted — honest deferrals per dream rule 6)

- **rollout-window-guard** — refuse/defer restart of a voice daemon
  (`wm-dialog|stt|tts`) while a dialog turn is in flight. Depends on a
  reliable "turn in progress" signal, which the
  *continuity-of-conversation* vision (`wmd-session-boundary`,
  `wm.brain.session.{start,end}`) is about to mint. Draft once that
  session-boundary event exists; until then `rollout` uses a coarse
  `--window` time guard only. Cross-vision dependency, not yet real.
- **binstale-watch** — a `pevent`-supervised daemon that scans on
  source-commit / install events (inotify on `~/.local/bin/` +
  repo `.git/`) and emits `wm.fleet.stale` on agorabus, so rollout can
  be event-driven rather than polled. Draft after `binstale` proves the
  verdict taxonomy is stable.
- **rollout-receipt** — autobuilder-style receipts per rolled daemon
  (pre/post pid, binary provenance ts, peer-count delta, elapsed) so a
  rollout is auditable. Draft after the first real `rollout apply`.

## Open questions

- **Per-daemon launch recipe**: `rollout` must know how each daemon is
  (re)launched. `pevent list` is empty (the bus daemon is *not*
  pevent-supervised), and `install.sh` uses `cargo install --path .`
  (→ `~/.cargo/bin`) while the running binary is at `~/.local/bin/agorabus`
  (provfs `comm:install` stamp) — two install paths. Fleet 1 `rollout`
  reads launch recipes from a config file the user authors; deriving
  them automatically (from systemd units / hook scripts / the
  SessionStart handshake) is deferred. **Discuss the canonical launch
  path per daemon before `rollout apply` runs against the live fleet.**
- **Who owns the live agorabus rollout right now?** Run-18 escalated
  pid 2138939 deliberately ("a live /build session likely owns the
  rollout"). `agorabus-doctor-selfstale` and any `rollout` test must not
  collide with that in-flight human/skill-owned rollout. Coordinate via
  gossip.
- **Restart vs reload** — **RESOLVED 2026-05-29 → see Fleet 3.** The
  Fleet 1 assumption ("brief peer-drop acceptable; the SessionStart
  handshake re-attaches peers") is **false for live sessions.** Phase 1
  of the 2026-05-29 dream pass read `agorabus/src/client.rs` and found
  the long-lived `subscribe` client has **no reconnect logic** — when
  the daemon dies, the subscriber process dies with it. The SessionStart
  hook (`agorabus-session-start.sh`) is the *only* re-registration path
  and it fires only at session *start*, never on daemon death. So a live
  bounce permanently strands every current session's subscriber until
  that session restarts. This is the root cause of the carried-forward
  stale-binary debt: self-review's `agorabus_daemon_stale_binary`
  playbook escalates rather than auto-fixes whenever subscribers > 5
  (`SKILL.md:259`), because the bounce is genuinely destructive
  (`SKILL.md:270`: "other live Claude sessions will need to re-run their
  SessionStart hook … a user-visible disruption"). Fleet 3 builds the
  handover *mechanism* so the bounce is non-destructive — which is what
  finally lets the auto-fix run within guardrails.
- **provfs reliability for binaries**: the `(deleted)` signal is
  kernel-truth and needs no provfs. The `prov-stale` verdict relies on
  `user.prov.ts` being stamped on install — verified present on
  `~/.local/bin/agorabus` (`1780026726`), but a `cargo install` to
  `~/.cargo/bin` may not pass through the same `install(1)` codepath that
  triggers provfs's close-after-write stamp. `binstale` must degrade
  gracefully (fall back to mtime) when the xattr is absent.
