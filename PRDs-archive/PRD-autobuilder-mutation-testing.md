# PRD: autobuilder-mutation-testing — cargo-mutants integration, phased gate

**Status:** Verified-completed 2026-05-29
**build_target:** self-mod
**build_priority:** high
**build_into:** /home/jsy/.claude/skills/autobuilder
**Research:** research/quality-verification-2026-05-28.md §2a, §4 Test 2
**Created:** 2026-05-28
**Author:** Claude (Opus 4.7), for jsy

---

## TL;DR

Run `cargo-mutants` after every successful `cargo test` and populate
the receipt's `mutants_alive_count` / `mutants_killed_count` /
`kill_rate` fields (currently nullable, never written).

Phase 1: telemetry only — no gate, just calibration. After 20 crates
ship with mutation data, set a calibrated `kill_rate` floor in
Phase 2 and promote to a hard gate.

Catches the "tests pass but cover narrow input classes" failure mode
that the current `cargo test` + clippy + adversarial-agent stack does
not detect.

## Why this exists

Receipt schema at `~/.claude/skills/autobuilder/schemas/proof-receipt.schema.json`
already names `mutants_alive_count` / `mutants_killed_count`. Spot-check
of `~/wintermute/agorabus/target/autobuilder/metrics.json` shows
`mutants_alive_count: null` — the field is nullable and autobuilder
never writes it. The quality-score formula already weights
`proptest_density` but mutation testing is the stronger signal: it
*proves* that a test would fail if the implementation broke, instead
of asserting it via density heuristic.

LLM-generated code is exactly the population where false-green is
plausible: the same agent that wrote the test wrote the implementation,
so the test's input class tends to match the implementation's happy
path.

Research report §2a + §4 Test 2 traces evidence.

## What this builds

### Artifact

A shell script `~/.claude/skills/autobuilder/scripts/run-mutants.sh`
(<150 LOC, POSIX sh + jq) that:

1. Detects whether `cargo-mutants` is installed; if not, install via
   `cargo install cargo-mutants --locked` once per autobuilder install.
2. Runs `cargo mutants --in-place --no-shuffle --jobs $(nproc) --json`
   in the crate root.
3. Parses output (`target/mutants.json`); extracts `caught`, `missed`,
   `unviable`, `timeout` counts.
4. Populates `target/autobuilder/metrics.json` with:
   ```json
   {
     "mutants_total": <caught + missed>,
     "mutants_killed_count": <caught>,
     "mutants_alive_count": <missed>,
     "mutation_kill_rate": <caught / (caught + missed)>,
     "mutation_wall_seconds": <int>
   }
   ```
5. Writes `target/autobuilder/mutants-receipt.json` with the per-mutation
   detail for forensics.

### Phased gate (recorded in SKILL.md)

- **Phase 1 (telemetry; ships with this PRD):** Stage 3 step 4
  (`run-metrics.sh`) gains a final step that invokes `run-mutants.sh`
  if the crate has tests. Failure to populate the field is logged but
  doesn't block. The quality score formula gains `+5*mutation_kill_rate`
  (weighted high but not gated).
- **Phase 2 (gate; future PRD after calibration):** add
  `mutation_kill_rate < THRESHOLD` to Stage 3 hard gates. Initial
  threshold guess: 0.60. Calibrate by running the script against the
  20 most-recently-shipped crates and inspecting distribution.

This PRD ships Phase 1 only. Phase 2 is a follow-on PRD drafted after
20 crates have mutation data.

### Cache

Mutation testing is slow (1–10× test wall time). Cache by
`sha256(src/ + tests/ + Cargo.toml)`. On cache hit, reuse the prior
`mutants.json`. Stored under `target/autobuilder/mutants-cache/`.
Skipped if `--no-cache` env or file `.no-mutation-cache` present.

### Out of scope

- Per-mutation triage UI. v1 emits JSON; humans grep.
- Targeting mutations at specific src files (the user might want to
  exclude generated code). v1 mutates everything in `src/`.
- Killing a specific surviving mutant. The receipt names them;
  improving tests is the human's call.

## Acceptance criteria

- **AC1**: Against a fixture crate with `assert!(x == 2)` for a fn
  returning `2`, `run-mutants.sh` produces `mutation_kill_rate: 1.0`
  (the test catches the mutation that returns `3`).
- **AC2**: Against a fixture crate with `assert!(x > 0)` for a fn
  returning `5`, `run-mutants.sh` produces `kill_rate < 1.0` (the
  mutation that returns `1` is *not* caught; the assertion is too
  weak). Exact value depends on cargo-mutants' mutation set; assert
  `mutants_alive_count >= 1`.
- **AC3**: First run installs `cargo-mutants` if missing; second run
  finds it cached and skips install. Verified via `which cargo-mutants`
  before and after.
- **AC4**: `metrics.json` gains all four new fields; existing
  fields untouched. JSON validates after the merge.
- **AC5**: Cache hit on identical src+tests+Cargo.toml: re-run is
  <10% of cold-run wall time.
- **AC6**: Quality score formula in
  `~/.claude/skills/autobuilder/SKILL.md` updated to include
  `+5*mutation_kill_rate`. Documentation block names this PRD as the
  source.
- **AC7**: Backfill: running against two wintermute crates produces
  non-null mutation counts and a kill_rate in (0, 1]. The two values are
  recorded in this PRD's archive commit body for future calibration.
  - **Backfill data (2026-05-29):**
    - `episodic-observer`: kill_rate **0.5753** (caught=107, missed=79, total=186)
    - `skill-manifest`: kill_rate **0.9615** (caught=25, missed=1, total=26,
      unviable=2, wall=149s; surviving mutant
      `src/lib.rs:192:75 replace == with != in is_semver`)
  - Note: the originally-named second target `agorabus` was substituted
    because its lib-test build is red at source (doctor.rs:307 E0255/E0364)
    — pre-existing agorabus debt, out of this PRD's scope. cargo-mutants
    requires a green `cargo test --no-run` baseline, so `skill-manifest`
    (a green-baseline crate) was used as the second data point instead.
- **AC8**: `run-mutants.sh` exits 0 on success even when
  `kill_rate < 0.60` (Phase 1 is telemetry only). Non-zero exit only
  on infrastructure failure (cargo-mutants crashed, can't write
  receipt).

## Files

```
~/.claude/skills/autobuilder/
├── scripts/run-mutants.sh                 # new
├── schemas/mutants-receipt.schema.json    # new
└── SKILL.md                                # +Stage 3 step + quality score
```

## Non-functional

- Wall time cap: 30 min per crate. Past that, emit
  `"verdict": "timeout"` and don't block.
- Disk: caches under `target/`; cleaned by `cargo clean`. Per
  user memory `self_autobuilder_receipt_order.md`, receipt-producing
  steps run before clean.
