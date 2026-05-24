# PRD: autobuilder-receipt — digest-bound JSON receipts under adversarial discipline

**Author:** Claude (Opus 4.7), drafted with jsy
**Status:** Draft v0.1 — wider-PRD restart of receipt.rs extraction (commit 9771694, reverted in f83149e)
**Date:** 2026-05-23
**Replaces:** the manual hand-extract of `autobuilder/src/receipt.rs`

---

## TL;DR

`autobuilder-receipt` is a tiny Rust library (≤200 LoC of `src/`) that owns the
self-binding `receipt_digest` algorithm and RFC3339 timestamp formatting for
every receipt the autobuilder risk gate walks. It currently lives as a single
file at `autobuilder/src/receipt.rs` with one happy-path test (the autobuilder
repo's own AC7). This PRD extracts it into a workspace crate at
`autobuilder/crates/receipt/` under its own autobuilder-discipline intent-card,
**built greenfield via the `/autobuilder` skill rather than copied verbatim**,
with an adversarial proptest suite that hammers the forgery-resistance,
permutation-stability, and timestamp-format invariants the autobuilder gate
unknowingly relies on.

The crate's existence is not the point. The proof — via the iterate-and-prove
loop + the 7-receipt gate on the crate itself — that the digest algorithm
holds across an adversarial input distribution **is** the point.

---

## 1. Why this exists (what one happy-path test misses)

The autobuilder repo's intent-card declares AC7:

> Every receipt produced by autobuilder includes a sha256 `receipt_digest`
> over its own canonicalized JSON (keys sorted), plus a UTC `captured_at`
> (RFC3339). The digest field is overwritten with an empty string before
> computing the digest so the field is self-binding.

The test for AC7 (`scripts/run-metrics.sh::ac_digest_roundtrip`) invokes
`autobuilder rollback-plan` once, recomputes the digest over the resulting
file with `jq --sort-keys '.receipt_digest = ""'`, and compares to the stored
value. That's **one** example. It proves the algorithm works on the one shape
of receipt the rollback-plan producer emits.

What it does not prove:
- Canonicalization is permutation-stable on **arbitrary** object key orderings
  (matters if a future `serde_json` feature flag or dependency switches the
  internal map type)
- The digest **changes** when a post-hoc edit mutates any field (the
  forgery-detection direction is asserted in the docstring but never tested)
- `secs_to_rfc3339` produces a parseable RFC3339 string for **any** plausible
  u64 input (the hand-rolled Hinnant-from-days calculation could regress at
  leap-year boundaries)
- `write()` rejects non-object top-level values rather than silently producing
  a malformed receipt
- The internal state (`receipt_digest = ""` placeholder during hashing) is
  the actual algorithm and not an implementation accident

Every receipt the gate walks is hashed by this code. Every "verdict=pass" on
every project past, present, and future depends on it being correct. The
asymmetry between "one happy-path roundtrip" and "the keystone of the audit
trail" is exactly what autobuilder's own discipline is supposed to fix —
applied to autobuilder itself.

---

## 2. Who this is for

The 9 callers inside `autobuilder/src/` (gate, loop_runner, postmortem,
evolve, rollback, vti_plan, ci_checks, reviewer, adversarial). The public
API stays minimal: `write(path, value)` and `now_rfc3339()` are the two
symbols every caller imports today. The internals (`canonical_json_bytes`,
`sort_keys`, `secs_to_rfc3339`) become `pub` so the test suite can exercise
them directly without going through the file-IO path.

Future callers in the workspace (when gate.rs and evolve.rs extract into
their own crates) will depend on this same crate via path.

---

## 3. Public surface

```rust
// Two existing entry points — preserved verbatim
pub fn write(path: &Path, value: serde_json::Value) -> Result<()>;
pub fn now_rfc3339() -> Result<String>;

// Newly-public helpers (today private) so the adversarial suite can probe
// without round-tripping through the filesystem
pub fn canonical_json_bytes(value: &serde_json::Value) -> Vec<u8>;
pub fn sort_keys(value: &serde_json::Value) -> serde_json::Value;
pub fn secs_to_rfc3339(secs: u64) -> String;
```

No new behavior; the API is observability-into-the-existing-algorithm, not
a feature expansion.

---

## 4. Acceptance criteria

All MUST-level ACs must pass for verdict=advance. The unfakeable scalar
`receipt_invariants_passing` counts AC1..AC7 (target=7).

### AC1 (MUST) — self-binding round-trip

Calling `write(path, value)` then parsing the on-disk JSON back, blanking
the `receipt_digest` field to `""`, and recomputing
`sha256(canonical_json_bytes(blanked))` produces a string that **equals** the
stored `receipt_digest` byte-for-byte. Asserted on at least 3 hand-crafted
receipt shapes (small, nested, with arrays) and via proptest on arbitrary
objects.

**Test:** `tests/acceptance_ac1_self_binding.rs`

### AC2 (MUST) — permutation stability under proptest

For any two `serde_json::Value::Object`s `a` and `b` whose key-value pairs are
identical multisets, `canonical_json_bytes(&a) == canonical_json_bytes(&b)`.
Proptest generates objects with up to 32 keys, randomly permutes the
insertion order, and asserts byte equality. Recursive — must hold on nested
objects too.

**Test:** `tests/acceptance_ac2_permutation_stable.rs`

### AC3 (MUST) — forgery detection under mutation

For any written receipt and any single-field mutation (string, number, bool
swap, key removal, key addition), the **recomputed** digest over the mutated
payload **must not equal** the stored digest. Proptest generates a receipt,
writes it, parses the file, applies a single random mutation, asserts the
recomputed digest differs. Mutation kill rate target: 100% of generated
mutations on objects with ≥ 1 mutable field.

**Test:** `tests/acceptance_ac3_forgery_detection.rs`

### AC4 (MUST) — write rejects non-object top-level

`write(path, value)` returns `Err` whose message contains "must be a JSON
object" for each of: `Null`, `Bool(true)`, `Bool(false)`, `Number(0)`,
`Number(i64::MIN)`, `String("")`, `String("x")`, `Array(vec![])`,
`Array(vec![Null])`. No file is written when the validation fails.

**Test:** `tests/acceptance_ac4_object_required.rs`

### AC5 (MUST) — RFC3339 format invariants

`secs_to_rfc3339(s)` for any `s: u64` produces a string of length exactly
20, ending in `Z`, with `-` at positions 4 and 7, `T` at position 10, and
`:` at positions 13 and 16. Proptest covers `0..=u32::MAX as u64` plus
fixed leap-year boundary samples (2000-02-29, 2024-02-29, 2100-03-01).

**Test:** `tests/acceptance_ac5_rfc3339_format.rs`

### AC6 (MUST) — workspace integration

After the crate is built, `autobuilder/src/receipt.rs` is reduced to a
re-export shim and `cargo build --workspace && cargo test --workspace &&
cargo clippy --bin autobuilder -- -D warnings` all succeed. The autobuilder
repo's own `scripts/run-metrics.sh` reports `ac_passing_count: 7/7`
(no regression in the meta-bootstrap).

**Test:** scripts/run-metrics.sh — invoked from the autobuilder repo root

### AC7 (SHOULD) — strict-clippy under -D warnings on the crate itself

`cargo clippy -p autobuilder-receipt --all-targets -- -D warnings` exits 0.
All `unwrap_used`, `expect_used`, `panic`, and `unsafe_code` are denied in
`src/`; permitted in `tests/` via the file-level allow convention the
workspace uses elsewhere.

**Test:** scripts/run-metrics.sh — clippy step

---

## 5. Architecture

One library crate. No CLI. No binary.

```
autobuilder/crates/receipt/
  Cargo.toml              ← name = "autobuilder-receipt", lib only
  src/lib.rs              ← ≤200 LoC; the existing receipt.rs code with pub surface
  tests/
    acceptance_ac1_self_binding.rs
    acceptance_ac2_permutation_stable.rs
    acceptance_ac3_forgery_detection.rs
    acceptance_ac4_object_required.rs
    acceptance_ac5_rfc3339_format.rs
    proptest_invariants.rs   ← shared proptest strategies (arb_value, etc.)
  agent/
    intent-card.json
    proof-lanes.toml
  scripts/
    run-metrics.sh        ← invokes cargo test + reports per-AC pass/fail
    audit.sh
    risk-gate.sh
```

The crate becomes a workspace member of `autobuilder/`. The existing
`autobuilder/src/receipt.rs` becomes a re-export shim so the 9 callers
inside the binary continue to compile unchanged.

---

## 6. Non-goals

1. **New API surface.** No new public functions beyond making existing
   private helpers `pub` for test access.
2. **Replacing the algorithm.** The digest scheme, canonicalization rule, and
   timestamp format are byte-for-byte preserved. Any receipt this crate emits
   must be a bit-for-bit match for what the in-tree implementation emits
   today, given identical inputs.
3. **Multi-version receipt schemas.** v1 only; no schema evolution.
4. **Cross-binary fuzzing.** A separate `receipt-fuzzer` crate that runs the
   mutator from a CLI is deferred (see [[PRD-receipt-fuzzer]] if/when written).
5. **Replacing serde_json with a pure-Rust JSON canonicalizer.** Out of scope.
6. **Persisting proptest seeds for regression replay.** proptest's default
   shrinking + seed-on-failure is enough for v0.1.

---

## 7. Hard constraints

- `rust_edition = "2024"`
- `target_kind = "lib"`
- `deny_unsafe = true` (and `unsafe_code = "deny"` in `[lints.rust]`)
- `msrv = "1.85"`
- `max_deps = 4` — anyhow, serde_json, sha2, and `proptest` as dev-dep
- `additional`:
  - `max_lib_lines: 200` — the entire `src/lib.rs` must fit
  - `no_unwrap_in_src: true`
  - `no_expect_in_src: true`
  - `every_pub_fn_has_doc: true`

---

## 8. Five whys

1. **Why extract a 100-LoC file into its own crate?** Because the algorithm it
   owns underlies every "verdict=pass" the gate ever emits, and the only test
   today is one happy-path roundtrip. The size is the asymmetry that justifies
   the extraction: small enough to fully cover, load-bearing enough to deserve it.

2. **Why /autobuilder rather than the manual hand-extraction I did in 9771694?**
   The hand-extraction reproduced the existing code with three example tests.
   It did not prove the invariants — it asserted three more specific examples.
   /autobuilder forces falsifiable ACs before code, proptest-based coverage,
   and a 7-receipt gate that has to pass before the crate is considered
   built. The discipline is the deliverable, not the file.

3. **Why proptest + mutation tests rather than more example tests?** Example
   tests pass when the inputs happen to be the ones the implementer thought
   of. Proptest passes when the invariant holds across an adversarial input
   distribution. Mutation tests pass when removing or corrupting code makes
   the proptests fail — they are the test-of-the-tests.

4. **Why preserve the public API byte-for-byte?** The 9 callers inside the
   binary already work. Changing the API would force a parallel
   call-site rewrite that contributes nothing to the load-bearing invariant
   (digest forgery resistance). The re-export shim keeps the diff small and
   the risk surface bounded.

5. **Why `target_kind = "lib"` not `"cli"`?** This crate has no end-user
   surface. It is library code consumed by the autobuilder binary. Forcing a
   `main.rs` for the sake of /autobuilder's scaffold would emit
   ceremony-only code that adds clippy warnings the gate then has to filter.
   Better to honor the actual shape: lib, no binary.

---

## 9. Phasing

| Phase | Scope |
|-------|-------|
| 0 | This PRD reviewed; /autobuilder skill invoked; intent-card scaffolded; baseline iteration runs (all ACs failing on stubs). |
| 1 | Edit-agent fills in `src/lib.rs` (migrate from existing receipt.rs). All 5 example ACs pass. AC6 (workspace integration) verified via the autobuilder repo's own harness. |
| 2 | Proptest suite implemented for AC2, AC3, AC5. Mutation kill rate measured. Loop reaches verdict=advance with `receipt_invariants_passing = 7`. |
| 3 | 7-receipt gate run on the crate itself (intake, vti-plan, proof-receipt, risk-gate, reviewer-agent, rollback-plan, ci-checks). release-receipt.json emitted. |
| 4 | Subtree-merge or copy the standalone build into `autobuilder/crates/receipt/`. Update `autobuilder/src/receipt.rs` to the re-export shim. Verify the parent repo's 7/7 AC harness still passes. Commit. |

---

## 10. Risks

- **Algorithmic drift during the edit-agent fill-in.** The migrated code must
  produce byte-identical output to the in-tree version. *Mitigation:* AC1's
  roundtrip property includes a fixed-input fixture from a known-good
  receipt in `target/autobuilder/receipts/`; comparison is byte-for-byte.

- **Proptest finding a real bug.** The Hinnant date algorithm or the
  `try_from(u64) -> i64 unwrap_or(0)` fallback may have edge cases. If
  proptest finds one, the AC genuinely fails until the algorithm is fixed.
  This is the desired outcome, not a risk.

- **Scope creep from "while I'm in here."** The extraction is the
  scope. Refactoring to use `serde_json::ser::PrettyFormatter`, switching to
  `chrono`, or replacing anyhow with thiserror are all out of scope. The
  PRD is small on purpose.

- **The receipt-fuzzer CLI deferred to a future PRD.** If never written, the
  attestation lives entirely inside this crate's `tests/` and is not
  independently runnable. *Mitigation:* document as a follow-up.

---

## 11. Unfakeable scalar metric

```json
{
  "name": "receipt_invariants_passing",
  "lower_is_better": false,
  "harness_command": "scripts/run-metrics.sh",
  "target": 7
}
```

Measured by `scripts/run-metrics.sh` invoking `cargo test --tests` and
parsing the per-test pass/fail output. Each of AC1..AC7 maps 1:1 to a test
file (or a logical group of property-tests within a file). The metric
trajectory is what tells the loop whether iter-N improved over iter-(N-1).

---

## 12. Open questions

1. Should `now_rfc3339` be split into `now_secs()` + `secs_to_rfc3339` for
   easier testing? Currently the `now_rfc3339()` test cannot mock the clock
   without injecting through an env var. Defer to a v0.2 if it becomes
   inconvenient.
2. Should `canonical_json_bytes` panic on serialization failure (currently
   `.unwrap_or_default()` silently swallows)? Probably yes — silent
   degradation in canonicalization corrupts the digest. Filed as a follow-up
   if AC3's proptest doesn't catch it organically.
3. Is `sha2 0.10` pinned tight enough that a transitive upgrade can't change
   the digest output? sha2 outputs are determined by the algorithm spec, not
   implementation choice, so probably yes. AC1's roundtrip would catch any
   regression immediately.
