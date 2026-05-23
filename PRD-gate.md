# PRD: autobuilder-gate — the 8-receipt risk-gate keystone under adversarial discipline

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-23
**Sibling to:** `PRD-receipt.md` (extracts the digest primitives the gate consumes)

---

## TL;DR

`autobuilder-gate` is a Rust lib crate (~250 LoC) that owns the pure-function
core of the 7+1-receipt risk gate: schema matching, head_sha binding, verdict
checking, and aggregation into `release-receipt.json`. Today this logic lives
inside `autobuilder/src/gate.rs` (366 LoC, mixed with clap Args, file IO, git
invocation, and println). The autobuilder repo's own AC5 tests it via *one*
happy-path scenario (gate walks 7 receipts on a clean tree). The single test
covers the success path of the keystone of "is it shipped?" — that asymmetry
is what justifies the dogfood.

Extracted under its own intent-card, the load-bearing properties become
proptest-asserted:
- Mutation-kill rate: corrupting any single field of any receipt (schema,
  head_sha, verdict, blocking_count) flips the corresponding check.pass from
  true to false. Aggregate verdict flips from "pass" to "block".
- Permutation invariance: shuffling the input check vector produces identical
  pass/block counts and identical aggregate verdict.
- Receipt-spec coverage: every receipt name in the gate's RECEIPT_SPECS table
  has at least one test that proves both the happy path (correct receipt
  passes) and three failure modes (wrong schema, wrong head_sha, wrong
  verdict).

The CLI subcommand (`autobuilder gate`) stays in the bin as a thin
orchestrator that wraps `gate_lib::check_all` + `gate_lib::aggregate` + file
IO + git rev-parse. The bin's diff becomes ≤20 lines of glue.

---

## 1. Why this exists (what one happy-path test misses)

The autobuilder repo's AC5 (`scripts/run-metrics.sh::ac5_gate_catches_head_sha_mismatch`):

> `autobuilder gate --project <dir>` walks the 7 receipts, verifies schema +
> head_sha + verdict on each, and emits release-receipt.json with the
> aggregated verdict.

The test synthesizes a fake project tree, plants 7 receipts including one
with a deliberately-wrong head_sha, and asserts the gate blocks. That's **one
fixed scenario** with **one mutation type** (head_sha mismatch on
ci-checks). It does not prove:
- Other receipts catch the same mutation
- Other mutation types (schema swap, verdict swap, blocking_count drift,
  decision-field corruption for reviewer-agent) are caught
- The aggregate verdict is deterministic over receipt-set ordering
- An empty receipt file → fail (not silent pass)
- A non-JSON receipt file → fail with a clear note (not panic)
- The `pass_verdicts` allowlist semantics hold for every receipt independently

Every "verdict=pass" the autobuilder ever emits flows through `check_verdict`
and the `pass_count == checks.len()` aggregation. A subtle bug here would not
move the unfakeable metric (`stage4_receipt_producers_callable=5`) because
that metric only counts subcommands responding to `--help`. The gate could
silently rubber-stamp blocked receipts and the meta-bootstrap would notice
nothing.

---

## 2. Public surface

```rust
pub struct ReceiptSpec { /* same as today */ }
pub enum ReceiptPath { Static(&'static str), HeadShaJson }
pub struct ReceiptCheck { /* same shape */ }
pub struct ReleaseReceipt { /* same shape */ }
pub const RECEIPT_SPECS: &[ReceiptSpec] = &[ /* 8 entries — preserved verbatim */ ];

/// Pure function: parse + validate one receipt JSON against a spec.
pub fn check_receipt_value(
    spec: &ReceiptSpec,
    value: &serde_json::Value,
    head_sha: &str,
) -> ReceiptCheck;

/// I/O wrapper: read the file at `path`, hand bytes to check_receipt_value.
/// Returns a ReceiptCheck with `present=false` if the file is missing.
pub fn check_receipt_at(spec: &ReceiptSpec, path: &Path, head_sha: &str) -> ReceiptCheck;

/// Pure aggregate: collapse a Vec<ReceiptCheck> into pass_count/block_count/verdict.
pub fn aggregate(checks: &[ReceiptCheck]) -> (usize, usize, &'static str);

/// Pure verdict-evaluator (today's check_verdict, exposed for testing).
pub fn check_verdict(
    spec: &ReceiptSpec,
    verdict: Option<&str>,
    decision: Option<&str>,
    blocking_count: Option<i64>,
    notes: &mut Vec<String>,
) -> bool;
```

The bin's `gate.rs` shrinks to:
1. Parse clap Args → project dir
2. `git rev-parse HEAD` → head_sha
3. Loop `RECEIPT_SPECS` → `check_receipt_at` each
4. `aggregate` → ReleaseReceipt struct
5. `receipt::write` (now via autobuilder-receipt crate)
6. println output + Ok/Err

---

## 3. Acceptance criteria

All MUST. Unfakeable scalar `gate_invariants_passing` (target=8) counts AC1..AC8.

### AC1 (MUST) — happy path: 8 valid receipts → verdict=pass

Synthesize a full set of 8 receipts (one per spec in RECEIPT_SPECS) where
every receipt has the correct schema, matching head_sha, and a verdict in
pass_verdicts. `check_receipt_value` returns `pass=true` for each; `aggregate`
returns `(8, 0, "pass")`.

**Test:** `tests/acceptance_ac1_happy_path.rs`

### AC2 (MUST) — schema mismatch is caught

For every receipt in RECEIPT_SPECS, swap its schema string to a deliberately
wrong value. `check_receipt_value` returns `pass=false` with a note containing
"schema mismatch". `aggregate` returns block.

**Test:** `tests/acceptance_ac2_schema_mismatch.rs` (table-driven over all 8 specs)

### AC3 (MUST) — head_sha mismatch is caught (when required)

For every receipt with `requires_head_match=true`, mutate its `head_sha` to a
sibling commit's sha. `check_receipt_value` returns `pass=false` with a note
containing "head_sha mismatch".

**Test:** `tests/acceptance_ac3_head_sha_mismatch.rs`

### AC4 (MUST) — verdict not in pass_verdicts is caught (proptest)

For any receipt with non-empty `pass_verdicts`, setting verdict to a random
string not in the allowlist causes `check_verdict` to return false. Setting
to a string IN the allowlist returns true. Proptest covers up to 64 random
strings per spec.

**Test:** `tests/acceptance_ac4_verdict_allowlist.rs`

### AC5 (MUST) — risk-gate blocking_count semantics

risk-gate is special-cased: `blocking_count=0` → pass; `blocking_count>0` →
fail with note; `blocking_count` missing → fail with note. Proptest covers
`0..=10_000` for the value.

**Test:** `tests/acceptance_ac5_risk_gate_special_case.rs`

### AC6 (MUST) — aggregate is permutation-invariant

For any `Vec<ReceiptCheck>`, `aggregate(shuffled)` returns the same
`(pass_count, block_count, verdict)` triple as `aggregate(original)`.
Proptest generates random check vectors of length 1..16, shuffles them, and
asserts equality.

**Test:** `tests/acceptance_ac6_aggregate_permutation.rs`

### AC7 (MUST) — empty file and invalid JSON fail cleanly (no panic)

`check_receipt_at` on a 0-byte file returns `pass=false` with a note
containing "file is empty". On a non-JSON-parseable file returns
`pass=false` with a note containing "invalid JSON". Neither path panics.

**Test:** `tests/acceptance_ac7_malformed_receipts.rs`

### AC8 (MUST) — parent-repo integration (post-merge, env-gated)

After subtree-merging into `autobuilder/crates/gate/` and shimming
`autobuilder/src/gate.rs`, the parent's `scripts/run-metrics.sh` still reports
`ac_passing_count: 7`. Same two-key env-var gate pattern as
`autobuilder-receipt`'s AC6.

**Test:** `tests/acceptance_ac8_parent_integration.rs`

---

## 4. Hard constraints

- `rust_edition = "2024"`
- `target_kind = "lib"`
- `deny_unsafe = true`
- `max_deps = 3` — anyhow, serde, serde_json (sha2 not needed; this crate
  doesn't compute digests, only reads `receipt_digest` strings for observation)
- `msrv = "1.85"`
- `max_lib_lines = 350` — gate.rs is 366; the extracted lib is slightly
  smaller after stripping clap/IO/println

---

## 5. Five whys

1. **Why extract the gate logic?** Every "is it shipped?" depends on it.
   The current single happy-path test cannot catch a regression in the
   per-receipt-type mutation handling.
2. **Why /autobuilder vs hand-extract?** Same reason as PRD-receipt: example
   tests pass when the inputs are the ones the implementer thought of;
   proptest passes when the invariant holds across an adversarial distribution.
3. **Why preserve the CLI Args + run() in the bin?** clap derives are bin-coupled.
   Extracting them would force a parallel rewrite that contributes nothing to
   the load-bearing invariant. The pure-function core is what matters.
4. **Why expose `RECEIPT_SPECS` as `pub const`?** External (test) code needs
   to enumerate every spec for the table-driven mutation tests. Making it
   private would force test-side duplication that drifts.
5. **Why a separate crate vs adding to autobuilder-receipt?** Different
   concerns: receipt owns digest primitives (forgery resistance); gate owns
   aggregation rules (verdict computation). Coupling them would force every
   change to one to re-validate the other.

---

## 6. Phasing

| Phase | Scope |
|-------|-------|
| 0 | PRD + intent-card + scaffold. Baseline iter records all panicking stubs. |
| 1 | Edit-agent migrates check_receipt_value, check_verdict, aggregate, RECEIPT_SPECS, all struct/enum types from gate.rs into src/lib.rs with pub surface. Fills in AC1, AC2, AC3, AC7. |
| 2 | Proptest for AC4 (verdict allowlist), AC5 (risk-gate blocking_count), AC6 (permutation). |
| 3 | Subtree-merge into autobuilder/crates/gate/. Shim autobuilder/src/gate.rs to: parse Args, git rev-parse, loop RECEIPT_SPECS via gate_lib, aggregate via gate_lib, write release-receipt via autobuilder_receipt::write, print, return Ok/Err. |
| 4 | Verify parent harness still 7/7. Run AC8 with both env keys set. |

---

## 7. Unfakeable scalar

```json
{
  "name": "gate_invariants_passing",
  "lower_is_better": false,
  "harness_command": "scripts/run-metrics.sh",
  "target": 8
}
```

---

## 8. Non-goals

1. Extracting clap Args, file IO, git invocation, or println into the lib —
   those stay in the bin.
2. New receipt types beyond the 8 in RECEIPT_SPECS today.
3. Changing the schema strings, pass_verdicts allowlists, or per-receipt
   special-cases (risk-gate, reviewer-agent). Byte-for-byte preservation.
4. A "gate-mutator" CLI that runs the adversarial mutations from the command
   line — deferred to PRD-gate-mutator if/when written.
