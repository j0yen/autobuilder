# PRD: wintermute-homestead — fleet install doctor (`wm doctor`)

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/homestead.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-platform
**build_version_bump:** minor
**Depends on:** none
**Codename:** *doctor* — before a device serves, prove every daemon can start.

## TL;DR

A wintermute systemd unit can declare an `ExecStart` that points at a
path nothing ever installed to. When that happens the daemon dies with
`status=203/EXEC`, and on a device with no operator it stays dead.
Nothing on the laptop currently verifies that the fleet's units and the
fleet's binaries agree. This PRD extends `wintermute-platform`'s `wm`
binary with a `wm doctor` subcommand that enumerates every wintermute
unit, resolves its `ExecStart` (specifiers expanded), and reports — per
unit — whether the binary exists and is executable, whether the unit is
enabled in `wintermute.target`, and whether it is active. It exits
nonzero if any unit's `ExecStart` is unresolvable. This is the
foundation the rest of the homestead vision reads.

## 1. Why this exists

Phase-1 live evidence (2026-05-29 ~06:30 UTC), this exact laptop:

```
unit          ExecStart                   resolves to                      state    bin?
wm-audio      %h/.cargo/bin/wm-audio      /home/jsy/.cargo/bin/wm-audio    active   OK
wm-dialog     %h/.local/bin/wm-dialog     /home/jsy/.local/bin/wm-dialog   active   OK
wm-stt        %h/.local/bin/wm-stt        /home/jsy/.local/bin/wm-stt      active   OK
wm-tts        %h/.local/bin/wm-tts        /home/jsy/.local/bin/wm-tts      active   OK
wmd           %h/.local/bin/wmd           /home/jsy/.local/bin/wmd         active   OK
wmd-init      /usr/local/bin/wmd-init     /usr/local/bin/wmd-init          FAILED   MISSING
```

- `systemctl --user status wmd-init.service` reports `Active: failed
  (Result: start-limit-hit)`, `Main PID: ... (code=exited,
  status=203/EXEC)`. The binary actually exists at `~/.local/bin/wmd-init`
  (`command -v wmd-init`), but the unit points at `/usr/local/bin/wmd-init`.
- Three install conventions are in play across six units. Nothing
  detects the mismatch; the only "detector" is a human reading
  `systemctl status` per unit.
- `visions/companion.md` Notes-for-/build flagged this drift but assigned
  the fix to companion-boot, whose ACs never enforce it. The drift is
  still live and now failing.

A `wm doctor` that fails CI/install when a unit can't exec turns this
from "a daemon silently dies on a remote device" into "the install
refuses to declare success."

## 2. What this builds

### 2.1 `wm doctor` subcommand (extend `src/bin/wm.rs`)

Enumerate wintermute units by globbing the systemd unit search paths for
`wm-*.service`, `wmd.service`, `wmd-init.service`, and any unit with
`Documentation=…/wintermute-*` (so the set is discovered, not
hard-coded). For each unit:

- Parse `ExecStart=` from `systemctl --user cat <unit>` (and the
  system manager for system-scope units).
- Expand systemd specifiers that affect the path — at minimum `%h`
  (home), `%u`/`%U` if present. Take the first token as the binary.
- Verify the resolved path exists and is executable (`access(X_OK)`).
- Read `is-enabled` and `is-active`.
- Check whether the unit is pulled in by `wintermute.target` (appears in
  the target's `Wants=`/`Requires=` or its `.wants/` dir).

Emit a per-unit row and a summary. Support `--format json` (object per
unit: `{unit, exec_start, resolved, exists, executable, enabled, active,
in_target}`) and a human table by default. `--scope user|system|both`
(default both). `--quiet` prints only failures.

### 2.2 Exit semantics

Exit `0` only if every discovered unit's `ExecStart` resolves to an
executable file. Exit nonzero (e.g. `2`) if any unit is `MISSING`. A
unit that is merely `inactive`/`disabled` but whose binary exists is
**not** a failure (that's a deploy choice); a unit whose binary is
absent **is** a failure regardless of active state.

### 2.3 No system mutation

`wm doctor` is strictly read-only. It never installs, restarts, or
resets anything. (Recovery is the watchdog PRD; fixing paths is the
convention PRD.) This keeps it safe to run in CI, in install scripts,
and on a live device.

## 3. Acceptance tests

1. **AC1 — `cargo test --release --lib` ≥ current+5** covering: ExecStart
   parsing from a unit-file fixture, `%h` specifier expansion, executable
   detection (exists+X_OK vs exists-but-not-exec vs absent), in-target
   membership parse, exit-code mapping (all-OK→0, one-missing→nonzero).
2. **AC2 — discovers the live fleet.** `wm doctor --format json` on this
   laptop emits one object per running wintermute unit (≥6: wm-audio,
   wm-dialog, wm-stt, wm-tts, wmd, wmd-init) with no hard-coded unit
   list. Verified by an acceptance test that asserts ≥6 units and that
   each has the documented fields.
3. **AC3 — flags the live regression.** On this laptop (or a fixture
   reproducing it), `wm doctor` reports `wmd-init` as `exists=false`,
   exits nonzero, and names `/usr/local/bin/wmd-init` as the
   unresolvable path.
4. **AC4 — passing case exits 0.** A fixture where every unit's
   `ExecStart` resolves to a present executable yields exit 0 and a
   "all N units OK" summary.
5. **AC5 — read-only.** An acceptance test (or a `wchg`-scoped manual
   step recorded in the receipt) shows `wm doctor` mutates no files and
   issues no `systemctl start/stop/restart/reset-failed`.
6. **AC6 — `--help` documents `doctor`** with the exit-code contract and
   `--format`/`--scope`/`--quiet` flags.

## 4. Non-goals

- Fixing the path (that's install-path-convention).
- Restarting/reset-failing units (that's unit-recovery-watchdog).
- Stale-binary detection vs source HEAD (that's vigil/binstale —
  `ExecStart` resolution is the *absent* axis, not the *stale* axis).

## 5. Files this PRD likely touches

- Modified: `src/bin/wm.rs` (new `doctor` subcommand), `Cargo.toml`
  (no new deps expected beyond clap/serde already present).
- New: `tests/acceptance_doctor.rs`, unit-file fixtures under `tests/`.

## 6. Open questions

- System-scope unit enumeration needs the system manager
  (`systemctl --system cat`), which may require no privilege for `cat`
  but the test environment should not assume root. Default `--scope`
  may need to be `user` if system queries are flaky headless; decide at
  build time and document.
