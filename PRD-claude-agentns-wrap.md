# PRD: claude-agentns-wrap — route every Claude launch through `agentns-claude`

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-27
**Vision:** [visions/onramp.md](visions/onramp.md)
**Depends on:** [PRD-agentns-claude.md](PRD-agentns-claude.md) shipped + installed at `~/.local/bin/agentns-claude`
build_auto: false
build_target: mixed
build_into: /home/jsy

---

## TL;DR

`PRD-agentns-claude.md` (continuity Fleet 1) builds the launcher that
calls `unshare(CLONE_NEWAGENT)` and sets `intent_tag`. But no piece of
the user's environment routes the actual `claude` launch through it,
so every session today reads zeros from `/proc/self/agent_session`.
The SessionStart hook (`agorabus-session-start.sh`) can't fix this
post-hoc — `unshare` is per-process and self-only; you can't enter a
namespace from the outside.

This PRD wires three Claude entry points to go through
`agentns-claude`:

1. **Interactive shell** — a `claude()` function in `~/.zshrc` (or
   `~/.zshenv` if interactive vs non-interactive matters). Calls
   `agentns-claude --intent interactive -- /usr/local/bin/claude
   "$@"` when the launcher is present, otherwise execs the real
   binary unchanged.
2. **Headless systemd-user units** — `/build`, `/dream`,
   `/self-review`, and any other timer-driven services edit their unit
   `ExecStart=` to prefix `agentns-claude --intent <slash-name> --`.
3. **agorabus SessionStart hook** — `agorabus-session-start.sh` reads
   `/proc/self/agent_session` and uses the kernel id as the canonical
   `sid` when non-zero, falling back to the existing pgrep+awk synthesis
   when zero. Doesn't try to unshare; just trusts what's there.

After this PRD ships, a fresh `claude` from an interactive shell yields
non-zero `/proc/self/agent_session`, propagated to every grandchild.
The same is true for headless services. The SessionStart hook gets
a stable kernel sid for free.

---

## 1. Why this exists

### 1.1 The kernel surface is unused

Observed 2026-05-27 inside an interactive Claude session:

```
$ cat /proc/self/agent_session
00000000000000000000000000000000
$ ls /proc/self/ns/agent
agent
$ stat -c '%i' /proc/self/ns/agent
4026531837   # init agentns
```

The agent namespace exists, but every Claude session shares the init
namespace because nothing called `unshare`. PRD-agentns-claude builds
the wrapper; this PRD installs it on the path the user actually uses.

### 1.2 Hook-time fix is structurally impossible

`~/.claude/scripts/agorabus-session-start.sh` runs after `claude`
has already started and forked the hook process. The hook can call
`unshare(CLONE_NEWAGENT)` on itself, but that only enters the hook
process into a new namespace — the Claude process and all its other
children remain in init. To make `claude` *itself* live in agentns,
something has to wrap the exec of `claude`. That's the gap this PRD
closes.

### 1.3 The repeat self-review signal

`~/brain/journal/2026-05-26.md`, runs 13/14/15 (~6h between runs):

- run 13: "agentns userspace wrapping STILL missing — single highest-
  value follow-up"
- run 14: "STILL missing — 3 consecutive runs flagged — the fix is one
  edit in agorabus-session-start.sh"
- run 15: "STILL missing — this needs a deliberate session, not
  another self-review tick"

The proposed fix in those reviews ("edit agorabus-session-start.sh")
is structurally wrong per §1.2; it would unshare the hook process
only. This PRD names the actual fix.

### 1.4 Continuity Fleet 1 is gated on this

- **PRD-recall-session-stamp.md** — wants to stamp memories with
  `agent_session`. Useless when every session reads zeros.
- **PRD-memlog-witness.md** — wants to demux records by `agent_session`
  of the writer. Today every writer reports zero, so all records
  collapse into one "session = init" bucket.
- **PRD-session-postmortem.md** — wants to join memlog + provfs +
  recall by session id. Same problem.
- **PRD-provq.md** — wants `provq /path` to map a file to its writing
  session. With agent_session = zero everywhere, provq falls back to
  comm-tags and gives the same useless attribution PRD-provfs-comm-richer
  is trying to enrich.

Closing this PRD closes a transitive blocker on 4 of the 5 continuity
Fleet 1 PRDs.

---

## 2. What this builds

### 2.1 Path 1: Interactive shell function

Add to `~/.zshrc` (kept user-editable; no auto-sourced dotfile-package
involved):

```zsh
# wintermute: route interactive `claude` through agentns wrapper when present
claude() {
  if [[ -x "$HOME/.local/bin/agentns-claude" ]] && [[ -z "$AGENTNS_WRAPPED" ]]; then
    AGENTNS_WRAPPED=1 "$HOME/.local/bin/agentns-claude" --intent interactive -- claude "$@"
  else
    command claude "$@"
  fi
}
```

- The `AGENTNS_WRAPPED` guard prevents infinite recursion if
  `agentns-claude` happens to exec via the function path.
- Falls through silently when the launcher isn't installed (no warning
  spam; the next /self-review will notice).

### 2.2 Path 2: systemd-user units

Edit each user-unit's `ExecStart=` to prefix `agentns-claude --intent
<slash-name> --`. The units affected (audit at draft time, before
implementing):

```
~/.config/systemd/user/build.service          → --intent /build
~/.config/systemd/user/dream.service          → --intent /dream
~/.config/systemd/user/self-review.service    → --intent /self-review
```

(Other timer-fired services in the same directory get the same
treatment if they exec `claude` directly. Enumerate at implementation
time via `grep -l 'ExecStart=.*claude' ~/.config/systemd/user/`.)

Each edit is conditional on `agentns-claude` being installed —
otherwise the unit keeps its current ExecStart. Encode as an `ExecStartPre=`
existence check? Or a wrapper script? Leaning: do the existence check
once at PRD-implementation time, and skip the edit if the launcher
isn't there yet. The /self-review playbook can re-check.

### 2.3 Path 3: SessionStart hook reads kernel sid

Modify `~/.claude/scripts/agorabus-session-start.sh` to derive `sid`
from the kernel when available:

```bash
# In agorabus-session-start.sh, replacing the pgrep+awk block:
sid_kernel=$(cat /proc/self/agent_session 2>/dev/null || echo "")
if [[ -n "$sid_kernel" ]] && [[ "$sid_kernel" != "00000000000000000000000000000000" ]]; then
  sid="claude-${sid_kernel:0:16}-${project}"  # first 64 bits, dash-project
else
  # existing pgrep+awk fallback for non-wintermute kernels
  …
fi
```

Stable across `claude` restarts in the same kernel session; survives
the SessionEnd-hook-missing case (the id is a property of the
namespace, not the process). Subscriber filenames at
`~/.cache/agorabus/sessions/<sid>.ndjson` become deterministic and
greppable.

### 2.4 What this does NOT do

- Does not auto-install `agentns-claude`. That's
  PRD-agentns-claude.md's responsibility. This PRD assumes the
  binary is present at `~/.local/bin/`.
- Does not change the `claude` binary itself. The shell function and
  unit edits all wrap the existing binary.
- Does not enforce that all Claude launches go through the wrapper.
  Direct `/usr/local/bin/claude` invocation bypasses agentns; that's
  intentional (escape hatch for debugging).
- Does not change `~/.config/systemd/user/timer` files. Only `.service`
  unit `ExecStart=` lines.
- Does not pass `--budget`. Budget enforcement is a separate
  policy decision; this PRD is identity-only.

---

## 3. Acceptance criteria

1. **Interactive `claude` enters agentns.** From a fresh shell after
   re-sourcing `~/.zshrc`, running `claude` and then inside it
   `cat /proc/self/agent_session` returns a non-zero 32-char hex
   string. Same value across every subprocess in the same session
   (`bash -c 'cat /proc/self/agent_session'` matches).
2. **Subprocesses inherit.** `claude` → spawn a `pevent`-tracked
   subprocess → `cat /proc/self/agent_session` inside that subprocess
   returns the same value as the parent.
3. **Headless service enters agentns.** After `systemctl --user
   restart build.service`, `cat /proc/<pid-of-claude-in-build>/agent_session`
   returns a non-zero id. (Verify via `pgrep -f 'claude' | xargs -I{}
   cat /proc/{}/agent_session`.) Different from interactive session id.
4. **`intent_tag` is set.** `cat /proc/<pid>/agent_intent_tag` returns
   the value passed (`interactive`, `/build`, etc.).
5. **Fallback works pre-agentns-claude.** With `~/.local/bin/agentns-claude`
   absent (rename it aside temporarily), `claude` still launches via
   the shell function (`command claude` branch). No error spam.
6. **SessionStart hook uses kernel sid.** After AC1, the
   subscriber's log file appears at `~/.cache/agorabus/sessions/claude-<16hex>-<project>.ndjson`
   (kernel-derived 16-hex prefix), not the old `claude-<pid>-<project>`
   form.
7. **Non-wintermute kernel graceful degradation.** Simulated by
   pointing the hook at a `/tmp/agent_session_mock` returning all
   zeros: `sid` falls back to the pgrep+awk synthesis; no error;
   subscriber filename matches the legacy form.
8. **No infinite recursion.** Calling `claude` from a sub-shell inside
   a Claude session does not double-wrap (the `AGENTNS_WRAPPED` env
   guard catches it). Single-wrap is correct; multiple-wrap would
   create nested agentns and break the kernel-id propagation
   guarantee.
9. **systemd unit edits are idempotent.** Running the edit script
   twice doesn't double-prefix `ExecStart=`.
10. **AC1–9 verified live by jsy** with `agentns-claude` installed.
    Mechanical-only verification is insufficient — the kernel
    namespace propagation has to be observed in a real session, not
    just smoke-tested.

---

## 4. Out of scope

- Building `agentns-claude` itself. Strictly dependent on
  PRD-agentns-claude shipping first.
- Budget enforcement (`--budget` flag wiring).
  PRD-agentns-budget-policy in continuity Fleet 2 covers this.
- A `claude-doctor` CLI to check namespace status from outside.
  Could fold into onramp Fleet 2's `onramp-doctor`.
- Per-PRD/per-session intent tags inside `/build` (e.g. `--intent
  /build/recall-daemon-iter-15`). Coarse tags only here; refinement
  is a follow-up.
- Distributing the shell function via a chezmoi/dotfiles repo. The
  user's `~/.zshrc` is hand-edited.

---

## 5. Bootstrap notes

- This is a `mixed` target: edits live across `~/.zshrc`,
  `~/.config/systemd/user/*.service`, and
  `~/.claude/scripts/agorabus-session-start.sh`. No new repo.
- Run order matters: PRD-agentns-claude must be built + installed
  first; only then can this PRD's ACs verify.
- Verifying AC3 (headless service) requires `systemctl --user
  daemon-reload` after the unit edits. The implementer should
  document this in the changelog.
- Verifying AC2 (subprocess inheritance) is a good place to add a
  `recall observe` note for future-Claude: "kernel session id
  inheritance verified at <iso8601> for session <id>."

[continuity]: visions/continuity.md
