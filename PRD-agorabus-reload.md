# PRD: agorabus-reload — one non-destructive command to roll the bus

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/agorabus
Vision: visions/vigil.md
deferred_acs: [AC2]
mock_unjustified_for: [AC2]
mock_justifications:
  AC2: Full end-to-end requires a live installed binary and a real OS-level daemon
    process found via /proc scan; send_sigterm + nohup launch cannot be replicated
    against the in-process DaemonHandle. The reconnect mechanism is proven by
    acceptance_reconnect_survives_restart.rs. The reload verdict logic (peers_recovered,
    peers_missing, status=reloaded) is validated at the component level in
    tests/acceptance_reload_recovers_peers.rs (3 tests covering the compute_verdict
    path that run_reload calls).

## TL;DR

Rolling fresh code into the running bus is a five-step hand dance today:
rebuild, reinstall, find the pid, kill it, relaunch, then re-run the
SessionStart hook in every live terminal. vigil's end-state promises
"one command instead of a five-step hand-rolled rebuild/reinstall/kill/
relaunch/push." This PRD ships that command — `agorabus reload` — as the
*non-destructive* bounce: it drains the old daemon, execs the fresh
binary, waits for the socket to rebind, and polls `peers` until the
pre-bounce session set has reconnected, emitting a structured verdict.
It relies on client-reconnect, drain-notice, and state-persist so that
"non-destructive" is true rather than aspirational.

## Why this exists

- **vigil's one-command promise.** visions/vigil.md end-state: the
  escalate-don't-auto-restart call "stays a deliberate human choice —
  vigil makes that choice *one command* instead of a five-step
  hand-rolled rebuild/reinstall/kill/relaunch/push." The hand-rolled
  steps are spelled out verbatim in `self-review/SKILL.md:261-266`
  (cargo build → kill → nohup relaunch → verify socket → re-run hook).
  This PRD is that one command.
- **The bounce is only safe once Fleet 3's mechanism exists.** Before
  PRD-agorabus-client-reconnect, a reload strands live sessions
  (root cause, see vigil OQ #3). `agorabus reload` is the orchestrator
  that *uses* reconnect + drain + persist; it must ship after them.
- **Recurring, verified pain.** The stale-binary item was carried
  forward at runs 16–19 and again 2026-05-29 (journals + recall
  `01KSRV7R4FERPP…`): pid 2138939 → 1750, binary `(deleted)`, src newer
  than bin. Each run re-types the five steps or escalates. A single
  audited command removes the re-typing and de-risks the escalation.
- **Composes under `rollout`.** vigil Fleet 1's `rollout` orchestrates
  the whole daemon fleet; for the bus specifically it can shell out to
  `agorabus reload` (graceful) instead of a bare SIGTERM+relaunch, and
  fall back to SIGTERM for daemons that lack a reload verb.

## What this builds

Extends `~/wintermute/agorabus/` (rust-extend; new subcommand, mirrors
the `Command::Doctor` precedent from PRD-agorabus-doctor-selfstale).
Current version 0.4.0. Depends on PRDs
agorabus-client-reconnect + agorabus-drain-notice + agorabus-state-persist.

- New `Command::Reload` variant in `src/main.rs` and a `src/reload.rs`
  module. Flow:
  1. **Pre-flight.** Resolve the installed binary path (the one the
     running daemon would exec on relaunch) and the running daemon pid
     (`pgrep -f 'agorabus daemon'` equivalent, or PID file if present).
     Refuse with a clear error if no daemon is running (nothing to
     reload) unless `--start-if-absent`.
  2. **Freshness check.** Optionally `--build` (run `cargo build
     --release` in a configured repo dir) then confirm via the same
     `(deleted)`/inode/provfs signals `agorabus doctor` uses that the
     on-disk binary differs from the running one. With `--require-fresh`
     (default on), abort if the running binary is already current (no-op
     reload is refused — nothing to roll).
  3. **Snapshot.** Record pre-bounce `peers` (session_id set + count).
  4. **Drain + bounce.** Send SIGTERM (triggers PRD-agorabus-drain-notice
     broadcast + PRD-agorabus-state-persist flush), wait for the old
     process to exit (bounded `--drain-timeout-ms`, default 2000), then
     launch the fresh daemon (`nohup … daemon` detached, same socket).
  5. **Confirm.** Poll `test -S <sock>` then `peers` until the pre-bounce
     session_id set has re-registered (the PRD-1 reconnect path brings
     them back) or `--reconnect-timeout-ms` (default 8000) elapses.
  6. **Verdict.** Emit JSON (and a human table): `old_pid`, `new_pid`,
     `binary_before`/`binary_after` provenance ts, `peers_before`,
     `peers_after`, `peers_recovered` (set diff), `elapsed_ms`, and
     `status: reloaded | reloaded-degraded | failed`. Exit 0 only on full
     recovery; nonzero (with the verdict on stderr) otherwise.
- `--dry-run` (default posture per vigil's "`--dry-run` is the default")
  prints the plan + freshness verdict + the session set that *would* be
  bounced, and makes no mutation.
- Reuses `agorabus doctor`'s staleness logic (do not reimplement the
  `(deleted)`/inode/provfs checks — call the shared function).

No protocol change beyond consuming the drain event. New CLI surface only.

## Acceptance criteria

1. **AC1 — dry-run mutates nothing.** `agorabus reload --dry-run` against
   a running daemon prints the plan (old pid, binary verdict, peer set)
   and the daemon pid is unchanged afterward. Integration test:
   `tests/acceptance_reload_dryrun.rs`.
2. **AC2 — non-destructive bounce recovers peers.** With a reconnect-
   capable subscriber attached, `agorabus reload` (apply) results in a
   new daemon pid AND the subscriber's session_id present in `peers`
   afterward; verdict `status` is `reloaded` and `peers_recovered`
   includes that session_id. Test:
   `tests/acceptance_reload_recovers_peers.rs`.
3. **AC3 — refuses a no-op.** With `--require-fresh` (default) and the
   running binary already current, `reload` exits nonzero with a
   "already current" message and does not bounce the daemon.
4. **AC4 — verdict shape.** `--format json` emits an object with all of:
   `old_pid, new_pid, binary_before, binary_after, peers_before,
   peers_after, peers_recovered, elapsed_ms, status`. Schema asserted in
   test.
5. **AC5 — degraded is reported, not hidden.** If, after
   `--reconnect-timeout-ms`, some pre-bounce sessions have not
   re-registered, `status` is `reloaded-degraded`, the missing
   session_ids are listed, and the exit code is nonzero (no silent "all
   good"). Test forces a non-reconnecting subscriber and asserts the
   degraded verdict.
6. **AC6 — no daemon → clear refusal.** With no daemon running and
   without `--start-if-absent`, `reload` exits nonzero with a "no daemon
   to reload" message (fail-clear, not fail-open — a reload of nothing is
   an operator error worth surfacing).
