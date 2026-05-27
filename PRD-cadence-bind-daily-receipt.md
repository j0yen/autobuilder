# PRD: cadence-bind-daily-receipt — daily-receipt emits a cadence record on every print

**Status:** Draft v0.1
**build_auto:** false
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/daily-receipt
**build_version_bump:** minor
**Vision:** visions/cadence.md
**Depends on:** PRD-cadence-substrate.md (must ship first)
**Created:** 2026-05-24

---

## TL;DR

`daily-receipt` is the bottom tier of the cadence pyramid. It emits an
ESC/POS byte stream for a day's printable thermal strip, but no record
of that emission survives in a form `confidant` (the weekly composer)
can consume. This PRD wires `daily-receipt` to call `cadence record
daily …` on every emit, so each day's receipt is registered as a
canonical `daily` artifact in the substrate.

## Why this exists

Phase 1 research, 2026-05-24:

- `~/wintermute/daily-receipt/` exists; crate name `daily-receipt`;
  binary `daily-receipt`.
- README: "given a day's structured summary plus a chosen day-type plus
  the content payload supplied by upstream … emit a byte-stable ESC/POS
  command stream for one ~3-8cm thermal strip." Confirms it produces
  bytes, not records.
- `cadence` (PRD-cadence-substrate.md) introduces the substrate. Until
  daily-receipt registers its emits there, the weekly tier has no
  primary input to pull from.

## What this builds

### Extension shape

`rust-extend` into `~/wintermute/daily-receipt/`. Add one module
(`src/cadence.rs`, ~50 LOC) and wire it into the existing emit path.
No new binaries. Version bump: minor (introduces a side effect on
emit; opt-out flag preserves the byte-stable output guarantee).

### CLI surface

- New flag `--no-cadence-record` (default: record). Preserves the
  byte-stable test runs that don't want side effects.
- New flag `--cadence-summary <s>` (default: derived from day-type +
  basic counts of input payload). Used as the record's `summary`.

### Behavior

On every `daily-receipt` emit:

1. Compute the ESC/POS bytes as today (no change).
2. If `--no-cadence-record` not set:
   - Shell out to `cadence record daily --produced-by daily-receipt
     --path <out-path-or-/dev/stdout> --summary <derived>` (or call
     into a thin Rust wrapper if cadence is also available as a lib;
     v0.1 can shell out).
   - If `cadence` is not on `$PATH`, log a one-line warning to stderr
     and proceed.
3. Emit bytes as usual.

### Dependencies

If shelling out: no new deps. If linking `cadence` as a lib: depend
on `cadence` crate from path or git.

Default v0.1: shell out. Keeps `daily-receipt` decoupled.

## Acceptance criteria

1. `daily-receipt --out /tmp/r.escpos --day-type workday <payload>`
   produces `/tmp/r.escpos` (existing byte-stable behavior — verify
   byte-equal to pre-extension output).
2. After AC1, `cadence list daily --produced-by daily-receipt
   --since 1h --json | jq -r '.[].path'` includes `/tmp/r.escpos`.
3. The cadence record's `summary` is non-empty and includes the
   `day-type`.
4. `daily-receipt --no-cadence-record --out /tmp/r2.escpos
   --day-type workday <payload>` produces the receipt and does NOT
   add a cadence record.
5. With `cadence` removed from PATH, `daily-receipt --out
   /tmp/r3.escpos …` still emits bytes successfully and logs one
   warning to stderr; no crash.
6. `cargo test --release` green; new tests verify the cadence-record
   path is exercised and the no-cadence path is preserved.
7. Version bumped to v0.2.0 (minor); `CHANGELOG.md` records the
   change.
8. Installed binary `~/.local/bin/daily-receipt --version` reports
   `0.2.0`.

## Out of scope

- Backfilling cadence records for past `daily-receipt` runs (the user
  can run `cadence record daily ...` manually if they want history).
- Reading from cadence as input (daily-receipt is bottom of the
  pyramid; nothing feeds it from below).

## Notes for /build

- Trivial PRD by line count, but it's the first composable bind.
  Once this ships, the weekly tier has primary input.
- The shell-out approach is intentional: keep daily-receipt's byte-
  stable core untouched, and let the substrate be a side effect.
