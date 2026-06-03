# PRD: wintermute-homestead — one install-path convention, enforced

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/homestead.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-platform
**build_version_bump:** minor
**Depends on:** PRD-wintermute-fleet-install-doctor
**Codename:** *one-true-path* — every fleet binary lives where its unit looks.

## TL;DR

The fleet's six systemd units declare three different `ExecStart`
conventions (`~/.cargo/bin`, `~/.local/bin`, `/usr/local/bin`). Five
resolve by luck; the sixth (`wmd-init` → `/usr/local/bin/wmd-init`)
points at a directory nothing installs to, so the supervisor is dead.
This PRD makes `wintermute-platform`'s `install.sh` enforce **one**
convention idempotently and end by running `wm doctor` (from the
fleet-install-doctor PRD), so an install cannot declare success while a
unit can't exec. It unbricks `wmd-init` as its first consequence.

## 1. Why this exists

- **Live failure.** `wmd-init.service` is `failed (start-limit-hit)`,
  `status=203/EXEC`, because `/usr/local/bin/wmd-init` does not exist
  while the binary sits at `~/.local/bin/wmd-init`
  (`visions/homestead.md` Phase-1 table).
- **The vision named it, nobody owns it.** `visions/companion.md`
  Notes-for-/build: "install-path drift (cargo install → ~/.cargo/bin;
  systemd → ~/.local/bin) bit four PRDs in a row today. Companion-boot
  should fix it at the systemd unit level." companion-boot's ACs are
  kiosk/greeter/autologin — they never enforce a path convention.
- **`install.sh` is real but unenforced.** `wintermute-platform/install.sh`
  exists (9815 bytes, mtime 2026-05-28 22:39) and installs the platform
  binaries, but produced a unit (`wmd-init`) whose `ExecStart` it does
  not populate. There is no post-install verification.

## 2. What this builds

### 2.1 Pick one convention

Default to **`~/.local/bin`** (least churn — 5 of 6 units already point
there; matches `/build`'s publish/install target; user-scope matches the
`--user` systemd units). The build may choose `/usr/local/bin` instead
if the kiosk decision in the vision's OQ#1 has been made — but it must be
**one** convention for the whole user-scope fleet, recorded in a single
constant/config the units and the installer both read.

### 2.2 Make install.sh enforce it idempotently

- For every fleet binary the platform owns or depends on, install (or
  symlink) it to the chosen convention's directory. Idempotent: re-running
  install.sh converges, never errors on already-correct state.
- Reconcile the units: either (a) rewrite each unit's `ExecStart` to the
  chosen path during install, or (b) place a binary/symlink at every path
  a unit declares. (a) is cleaner; pick one and apply it to all six units
  including the wm-audio `~/.cargo/bin` outlier.
- Specifically: `wmd-init`'s unit must end up with an `ExecStart` that
  resolves to a present executable, and `systemctl --user reset-failed
  wmd-init.service && systemctl --user start wmd-init.service` must
  succeed (the installer may do this, or document it as the operator's
  one post-install command).

### 2.3 Gate the install on the doctor

`install.sh` ends by invoking `wm doctor` (the fleet-install-doctor
PRD's subcommand). If doctor exits nonzero, the install reports failure
loudly and nonzero — it does not silently leave a half-wired fleet.

## 3. Acceptance tests

1. **AC1 — `cargo test --release --lib` ≥ current+3** covering the
   convention constant, idempotent-install logic, and unit-reconcile
   path-rewrite (pure functions; the systemd side is integration-tested).
2. **AC2 — install.sh is idempotent.** Running install.sh twice in a
   sandbox (or against a fakeroot) leaves identical state the second
   time (no errors, no duplicate symlinks). Verified by an acceptance
   test or a `sbx`-scoped receipt.
3. **AC3 — post-install doctor passes.** After install.sh runs in the
   test environment, `wm doctor` exits 0 for every fleet unit whose
   binary the test built. (Where a binary genuinely isn't built in the
   test, the test asserts doctor's verdict matches reality, not a
   hard-coded pass.)
4. **AC4 — `wmd-init` unbricks.** A documented, reproducible step (in the
   PRD receipt) takes `wmd-init.service` from `failed (start-limit-hit)`
   to `active` on this laptop: reconcile path → `reset-failed` → `start`
   → `is-active` returns `active`. This is the load-bearing real-world
   outcome.
5. **AC5 — single convention.** An acceptance test or doctor invariant
   asserts all user-scope fleet units share one `ExecStart` directory
   after install (no three-convention state).
6. **AC6 — `install.sh --help` (or header comment) documents the
   convention and the doctor gate.**

## 4. Non-goals

- Boot-on-power / kiosk / greeter (companion-boot owns those).
- Recovering an already-failed unit at runtime (unit-recovery-watchdog).
- Choosing the convention for a *system*-scope kiosk deployment beyond
  the default — that's vision OQ#1, decided with jsy.

## 5. Files this PRD likely touches

- Modified: `wintermute-platform/install.sh`, the six systemd unit files
  under `pkg/systemd/` (or wherever platform ships them), `src/bin/wm.rs`
  or a small lib module if the convention constant is shared with doctor.
- New: `tests/acceptance_install_convention.rs`.

## 6. Open questions

- If the build chooses path-rewrite (2.2a), the unit files in the repo
  should carry the canonical path so a fresh `pkg` install is correct
  from the start — confirm the units in `pkg/systemd/` are the source of
  truth and not generated.
