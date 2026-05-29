Status: Draft v0.1
build_target: rust-cli
build_priority: normal
build_into: (new repo) `/home/jsy/wintermute/wm-hardware-drift` → `j0yen/wm-hardware-drift`

# PRD — `wm-hardware-drift`: surface mock-vs-real-hardware drift as a self-review receipt

## TL;DR

The hardware-mock convention (PRD-autobuilder-hardware-mock-convention,
shipped) pairs every hardware-bound AC with EITHER an in-process mock test
(`tests/mocks/ac<N>.rs`) OR a `mock_unjustified_for:` + `mock_justifications:`
frontmatter entry, and gates a `real-hardware` cargo feature on each crate
(`cargo test --features=real-hardware`). The mock proves the call sequence +
signature + invariant at the type level; the hardware run proves they hold in
the world. What is still missing — explicitly deferred as Artifact 4 / AC10 of
that PRD — is the sweep that *compares the two* and flags drift when a mock
passes but the real device fails.

`wm-hardware-drift` is that sweep: a small CLI that runs both the default
(mock) and `--features=real-hardware` test sets for a crate, diffs the
per-test outcomes, and emits a `hardware-drift.json` receipt. /self-review
surfaces any drift (mock-green / real-red) as a finding.

## Why this exists

A green mock with a red real device is the single most dangerous state the
convention can produce: it reads as "verified" on the manifest while the
hardware path is actually broken. Today nothing detects that gap — the
`real-hardware` feature exists and is documented in five crate READMEs
(platform, dialog, audio, stt, tts) but is never run automatically, and its
outcome is never compared against the mock baseline. The convention's own PRD
calls this out: "the sweep itself is a future PRD."

This closes the loop without forcing hardware into the default `cargo test`
path: the sweep is opt-in (run on a host that actually has the mic / PipeWire
graph / whisper.cpp toolchain / live systemd-user target), and its only side
effect is a receipt the existing /self-review pass already knows how to read.

## What this builds

### Artifact 1: the `wm-hardware-drift` CLI

```
wm-hardware-drift run --crate-dir <path> [--out <receipt.json>]
wm-hardware-drift report [--receipt <path>]   # human-readable summary
```

`run`:
1. Runs `cargo test` (default features) capturing per-test pass/fail via
   `cargo test -- --format=json` (libtest JSON) or a parsed text fallback.
2. Runs `cargo test --features=real-hardware` the same way.
3. For each test name present in both runs, classifies:
   - `agree-pass`, `agree-fail`, `drift` (mock pass / real fail), or
     `inverse-drift` (mock fail / real pass — a stale/over-strict mock).
4. Writes `hardware-drift.json` under `target/autobuilder/receipts/`
   (per the autobuilder receipt-order convention) with a schema:
   `{ crate, ran_at, real_hardware_available: bool, tests: [{name, mock, real, class}], drift_count }`.
   If the `real-hardware` run cannot start (no feature, no device), it records
   `real_hardware_available: false` and exits 0 — absence is not drift.

### Artifact 2: /self-review integration (docs + finding rule)

A documented finding rule for /self-review Phase B: if any
`hardware-drift.json` under a wintermute crate has `drift_count > 0`, surface
it as a finding naming the crate + drifting test names. No auto-fix — drift is
a human signal. This artifact is the README/SKILL-doc note plus the receipt
schema contract; wiring the literal Phase B step is left to a self-mod tick.

### Out of scope (v0.1)

- Auto-running the sweep on a timer. It runs when a human (or a hardware CI
  host) invokes it; the receipt is durable.
- Auto-repairing a drifting mock. The mock is hand-written documentation of the
  boundary; a drift means a human revisits it.
- Cross-crate aggregation dashboards.

## Acceptance criteria

- **AC1**: `wm-hardware-drift run --crate-dir <c>` on a crate with a passing
  mock test and no `real-hardware` device records that test as `agree-pass`
  with `real_hardware_available: false`, and exits 0.
- **AC2**: Given a synthetic fixture crate where the mock test passes and the
  `--features=real-hardware` test fails, the receipt classes that test as
  `drift` and `drift_count == 1`.
- **AC3**: The inverse fixture (mock fails, real passes) classes as
  `inverse-drift` and is reported distinctly from `drift`.
- **AC4**: `hardware-drift.json` validates against the documented schema and is
  written under `target/autobuilder/receipts/`.
- **AC5**: `wm-hardware-drift report` prints a one-line-per-test human summary
  and a final `drift=<n>` tally; exit code is non-zero iff `drift_count > 0`
  (so it can gate a hardware CI step).
- **AC6**: Running against a crate with no tests at all is a clean no-op
  (empty `tests`, `drift_count 0`, exit 0), not a crash.
- **AC7**: The CLI parses libtest JSON when available and falls back to text
  parsing without losing the pass/fail classification (mock-tested against
  captured fixture output, no real `cargo` invocation needed in unit tests).
- **AC8**: /self-review finding rule documented in this repo + the crate
  READMEs' "Hardware reality verification" sections updated to point at
  `wm-hardware-drift` as the sweep that was previously "scaffolded as a
  follow-on PRD."

## Files

```
~/wintermute/wm-hardware-drift/          # new crate
├── src/main.rs                          # clap CLI: run / report
├── src/sweep.rs                         # run both test sets, classify
├── src/receipt.rs                       # hardware-drift.json schema + io
├── tests/                               # fixture-output-driven unit tests
└── README.md

~/.claude/skills/self-review/SKILL.md    # +Phase B drift finding rule (self-mod tick)
~/wintermute/wintermute-{platform,dialog,audio,stt,tts}/README.md  # point at the sweep
```

## Non-functional

- The sweep never runs hardware tests implicitly; it only shells `cargo test`
  with the explicit feature and reports. No device is touched by this crate's
  own logic.
- Receipt writes obey the autobuilder receipt-order convention
  (`target/autobuilder/receipts/`), so a `cargo clean` + receipt-dir wipe
  removes them like any other producer's output.
- Unit tests must not depend on a real audio device, model, or systemd target;
  they exercise the classifier against captured libtest output fixtures. Same
  lint discipline as the convention it serves (unwrap/expect/panic = deny).
