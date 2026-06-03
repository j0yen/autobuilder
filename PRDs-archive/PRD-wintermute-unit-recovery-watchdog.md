# PRD: wintermute-homestead — unit recovery watchdog

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/homestead.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-platform
**build_version_bump:** minor
**Depends on:** none
**Codename:** *revenant* — a failed daemon comes back without a human.

## TL;DR

`wmd-init.service` declares `Restart=always RestartSec=2` and uses
systemd's default start limit. When it failed to exec, it retried, blew
through the start limit in ~10 seconds, and entered `failed (Result:
start-limit-hit)` — where it has sat for 9 hours and will sit forever,
because clearing it requires a human running `systemctl reset-failed`.
On a device at jsy's mother's home there is no human. This PRD ships
`wintermute-watchdog` (a new platform binary + a system/user unit) that
detects any wintermute unit in `failed` state, clears it, and restarts
it with **capped exponential backoff** so a genuinely-broken unit doesn't
hot-loop but a transient flap recovers — and it tunes the fleet units'
`StartLimit*` so a flap never again becomes permanent death.

## 1. Why this exists

- **Live permanent failure.** `systemctl --user status wmd-init.service`:
  `Active: failed (Result: start-limit-hit) since Thu 2026-05-28
  14:29:52 PDT; 9h ago`. `Restart=always RestartSec=2` in the unit
  (verified via `systemctl --user cat`). Nothing recovers it.
- **start-limit-hit is recurrent.** Recorded across self-review runs and
  carried as a homeless "companion-reliability surface" in the
  `vision-kin` gossip aside ("flag for the companion-reliability surface
  / next self-review — not in kin's scope").
- **companion-boot's recovery is reboot-scoped.** Its
  `wintermute-boot-recovery.service` handles *power loss → reboot → fleet
  comes back*. It does not handle *a unit that fails while the device
  stays powered on* — a different trigger, no overlap.

## 2. What this builds

### 2.1 `wintermute-watchdog` binary (new `[[bin]]` in platform)

A small long-running (or timer-driven — see OQ) process that, on each
tick:

- Lists wintermute units (reuse the discovery logic from
  fleet-install-doctor if it can be shared as a lib function; otherwise
  the same glob). For any unit in `failed` state:
  - Records the failure in an in-memory per-unit backoff table.
  - Runs `systemctl [--user] reset-failed <unit>` then
    `systemctl [--user] start <unit>`.
  - Backs off exponentially per unit (e.g. 2s, 4s, 8s … capped at a
    ceiling like 5 min) so a unit that is genuinely broken (e.g. missing
    binary) is retried slowly, not hot-looped.
  - After K consecutive failed recoveries, stops retrying that unit and
    emits a `wm.health.*` event (so the readiness-beacon / kin can
    surface "wintermute-watchdog gave up on <unit>") rather than silently
    spinning.
- Never touches non-wintermute units.

### 2.2 Fleet StartLimit tuning

Adjust each fleet unit (via the unit files in `pkg/systemd/`) so a
transient flap doesn't permanently brick it before the watchdog can act:
e.g. raise `StartLimitIntervalSec`/`StartLimitBurst`, or set
`StartLimitIntervalSec=0` on units the watchdog owns recovery for, with
a comment explaining the watchdog is the backstop. Document the chosen
policy.

### 2.3 The watchdog's own unit

Ship `wintermute-watchdog.service` (scope per OQ). It must itself be
resilient — `Restart=always` with a sane `StartLimitIntervalSec` — and
must not be in scope for its own recovery loop (no self-restart storms).

## 3. Acceptance tests

1. **AC1 — `cargo test --release --lib` ≥ current+5** covering: backoff
   schedule (monotonic, capped), per-unit independent backoff state,
   give-up after K failures, wintermute-unit filter (a non-wintermute
   failed unit is ignored), `wm.health.*` event shape on give-up.
2. **AC2 — recovers a failed unit (integration).** In a sandbox/fixture
   with a deliberately-failing then fixable user unit, the watchdog
   detects `failed`, reset-fails, restarts, and the unit reaches
   `active`. The test must use a *throwaway* unit, never a live wm-*
   daemon.
3. **AC3 — backoff caps.** A unit that cannot be fixed (binary stays
   absent) is retried with increasing intervals up to the ceiling and
   then given up on after K attempts — verified against the backoff table,
   not wall-clock.
4. **AC4 — scoped to wintermute.** A failed non-wintermute unit present
   in the test environment is never touched.
5. **AC5 — no self-loop.** The watchdog does not attempt to recover its
   own unit (asserted by the unit-filter excluding `wintermute-watchdog`).
6. **AC6 — `--help` / unit documents** the backoff policy, give-up
   threshold, and the StartLimit tuning rationale.

## 4. Non-goals

- Reboot/power-loss recovery (companion-boot's boot-recovery service).
- Fixing *why* a unit fails (that's install-path-convention for the
  ExecStart case; the watchdog only recovers and, failing that, reports).
- Stale-binary detection (vigil/binstale).

## 5. Files this PRD likely touches

- Modified: `Cargo.toml` (new `[[bin]] wintermute-watchdog`), fleet unit
  files under `pkg/systemd/` (StartLimit tuning).
- New: `src/bin/wintermute_watchdog.rs`, `pkg/systemd/wintermute-watchdog.service`,
  `tests/acceptance_watchdog.rs`.

## 6. Open questions

- **Timer vs daemon.** A `wintermute-watchdog.timer` firing every N
  seconds is simpler and crash-resilient; a long-running daemon gives
  tighter latency and easier backoff state. Default: daemon with
  in-memory backoff (latency matters for a device that should recover in
  seconds), with the unit's own `Restart=always` as the crash backstop.
- **Scope (user vs system).** The fleet is user-scope; a system-scope
  watchdog is more reliable but needs `systemctl --user` reach into the
  user manager (via `machinectl`/`XDG_RUNTIME_DIR`). Default user-scope
  for v1 to match the fleet; revisit for kiosk. Consistent with the
  vision's OQ#2.
- **Could systemd do this natively?** `Restart=always` +
  `StartLimitIntervalSec=0` nearly suffices for the simple case. The
  watchdog earns its keep on *give-up reporting* and *capped backoff
  across the whole fleet* — note in the build whether a pure-unit-file
  solution covers enough to defer the binary.
