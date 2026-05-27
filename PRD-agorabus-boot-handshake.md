# PRD: agorabus boot handshake — verified subscribe with retry

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-25
**Vision:** visions/handshake.md
**Builds on:** agorabus v0.1.0 (current; no version bump from this PRD)
build_auto: false
build_target: shell
build_into: /home/jsy/.claude/scripts
build_version_bump: none

## TL;DR

The `agorabus-session-start.sh` SessionStart hook can produce orphan
subscribers under heavy boot load: the script's 0.1s × 5 socket-wait
runs out before the daemon is accepting connections, then the
subscriber spawns anyway, the daemon eventually starts, and the
subscriber's announce never reaches it — so the daemon's `peers`
list lacks the subscriber even though the subscriber is alive and
appending to its inbox. Today's self-review observed this with
PID 917 (subscribers 1888, 2091 orphaned). This PRD upgrades the
hook to verified handshake: extended socket-wait, post-subscribe
peer-record polling, re-spawn on missing peer, structured log of
each handshake attempt for future self-reviews.

## Why this exists

- **2026-05-25 self-review (carried in `~/brain/journal/2026-05-25.md`)**:
  PID 917's two subscribers (1888 subscriber, 2091 worker) are alive
  and the daemon binary IS post-fix, so the cause is a daemon-not-
  ready race at boot, not the pre-fix collision bug. The
  `agorabus_orphan_subscriber` playbook detected the state but had
  to escalate because no programmatic re-attach exists.
- **Current hook script** (`~/.claude/scripts/agorabus-session-start.sh`
  lines 23-29) polls `[ -S "$sock" ]` for at most 0.5s. With a
  kernel build at load 10.42 in the background, the daemon process
  can take longer than 0.5s to bind its UDS, and the script proceeds
  to spawn a subscriber against a socket that's about to exist but
  doesn't yet — the subscriber retries internally but its first
  announce often hits no listener.
- **The fix is already named** in the journal's §Notable section:
  "extending the hook's socket-wait to ~0.5s × N or adding a peer-
  record-explicit re-announce after the subscribe handshake." Both
  go in this PRD.
- **No new Rust.** This is a shell-script-only PRD. The agorabus
  binary's behavior is unchanged; only the bring-up choreography is
  hardened.

## What this builds

Edits to `~/.claude/scripts/agorabus-session-start.sh`:

1. **Extended socket-wait.** Replace `for _ in 1 2 3 4 5` /
   `sleep 0.1` with `for _ in $(seq 1 10)` / `sleep 0.3`. New max
   wait: ~3s (up from 0.5s). Loud-failure path: if the socket still
   isn't there, log a line to `~/.cache/agorabus/handshake/<sid>.log`
   with the elapsed time and exit 0 (preserve fail-open semantics).
2. **Verified subscriber attach.** After spawning the subscriber +
   `sleep 0.2`, poll `agorabus peers` (filtered by `$sid`) up to
   10 times at 0.3s intervals. If still missing after that window:
   re-spawn the subscriber once, poll another 5 × 0.3s, then give up
   and log the failure. (No infinite retry — failing closed would
   block Claude startup, failing loud preserves recovery via the
   self-review playbook.)
3. **Verified worker attach.** Same pattern as #2, but for the
   `${sid}-worker` peer record produced by `agorabus-worker.sh`.
4. **Structured handshake log.** New directory
   `~/.cache/agorabus/handshake/`; one append per attempt:
   `{ts, sid, phase, attempt_n, ok|fail, elapsed_ms}`. Phases:
   `daemon_up`, `sub_attach`, `worker_attach`. Bounded log rotation:
   keep last 14 days (mtime-based prune at start of each run).
5. **Idempotence preserved.** All existing `pgrep -f` guards stay;
   re-running the hook against a healthy session is a no-op except
   for one append-only handshake log line per phase indicating
   already-attached.

Helper script added: none — all logic stays inline in
`agorabus-session-start.sh` to keep the hook self-contained (no new
PATH dependency at boot).

Optional follow-on (NOT in this PRD; captured in vision Fleet 2):
the daemon-side `ready` marker file would let us replace the socket-
poll with a marker-poll, but that's a Rust change and out of scope.

## Acceptance criteria

1. **Cold boot, daemon present**: with a fresh `~/.cache/agorabus/`
   removed and the daemon already running on a stable socket, the
   hook completes < 100ms and the handshake log shows one
   `daemon_up:ok` (no wait), one `sub_attach:ok attempt_1`, one
   `worker_attach:ok attempt_1`.
2. **Cold boot, daemon absent**: with `~/.cache/agorabus/sock`
   removed and daemon killed, the hook brings up daemon + waits
   for socket. Handshake log shows `daemon_up:ok` with elapsed_ms
   in [0, 3000]. `agorabus peers | jq` includes both `$sid` and
   `${sid}-worker`.
3. **Slow-daemon race (synthetic)**: daemon binary wrapped to sleep
   2s before `listen()`. Hook must still succeed (max 3s wait +
   peer poll); log shows `daemon_up:ok elapsed_ms ≥ 2000`.
4. **Subscriber loses announce (synthetic)**: daemon
   restart-on-bind injected between subscribe spawn and peer-record
   visibility. Hook must observe `sub_attach:fail attempt_1`,
   re-spawn, then `sub_attach:ok attempt_2`. `agorabus peers` then
   includes `$sid`.
5. **Persistent failure (synthetic)**: daemon kept down. After full
   retry exhaustion, hook logs `sub_attach:fail attempt_15` (or
   whatever the exhaustion line is) and exits 0. Claude startup
   is not blocked.
6. **Idempotent re-run**: running the hook twice in succession
   against a healthy bus produces two log entries per phase with
   `already_attached:ok` markers, no duplicate subscribers (verify
   via `pgrep -fc "agorabus subscribe --session-id $sid"` == 1).
7. **Log rotation**: handshake log files older than 14 days are
   deleted at start of each run. Verify by `touch -d "20 days ago"`
   on a synthetic old log file and confirming it's gone after the
   next hook invocation.
8. **No regression against PID-917-class bug**: replay the
   2026-05-25 morning conditions by killing the daemon + spawning
   the hook with kernel-build-equivalent CPU load (`stress-ng
   --cpu N` is acceptable). Verify peer records exist after the
   hook returns.
9. **Replayable from journal**: include the 2026-05-25 journal
   §Notable excerpt verbatim in a comment block at the top of the
   updated hook, so future Claudes reading the script understand
   why the wait windows are what they are.
10. **End-to-end live verification**: jsy reboots, observes a clean
    `agorabus peers` listing of the interactive session within 5s
    of SessionStart firing, and the handshake log shows no
    `*:fail` entries on a healthy boot.

## Notes for /build

- **build_target is `shell`.** /build's shell-target path may not
  yet be hardened (chord-async-delegate is the only other shell
  PRD and it hasn't shipped). If /build can't yet run this PRD,
  it's safe to defer; this PRD does not block any other in-flight
  work.
- **No autobuilder cycle.** Edits one existing file
  (`~/.claude/scripts/agorabus-session-start.sh`) + creates the
  log directory at runtime. No Cargo, no tests/ tree, no version
  bump.
- **Verification needs root-ish conditions** — AC8's synthetic
  load + AC10's reboot can't be fully automated by /build. /build
  should ship to AC7 + AC9 mechanically and mark AC8/AC10 as
  user-verify checkpoints, same pattern as
  `recall-observer-correlation` used for its live-only ACs.
- **Don't auto-merge to main.** This script is on the Claude
  startup path; a bug here blocks all sessions. /build should
  open a PR (or commit + announce) and wait for jsy to test on
  one session before installing globally.
- **Identity:** `Joe Yen <jyen.tech@gmail.com>` for any commit
  (matches the `~/.claude/scripts/` path convention).

## Open questions

- Should the handshake log carry the orphan subscriber count
  observed at run start (subscribers alive but not in peers)?
  Useful for the self-review playbook but out of scope unless
  trivial. **Default:** skip for v0.1; capture in Fleet 2 as
  `handshake-orphan-snapshot`.
- The retry parameters (10 × 0.3s) are tuned for today's boot load
  (10.42). If load patterns change post-kernel-7.0.10 upgrade or
  post-`linux-wintermute` boot, re-tune. **Default:** ship the
  values, let self-review observations drive any future
  adjustment.
