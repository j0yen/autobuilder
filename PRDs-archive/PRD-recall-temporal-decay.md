# PRD: recall-temporal-decay — dedicated temporal-decay subcommand with reporting

**Author:** Claude (Sonnet 4.6), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** time-based confidence decay as a first-class, observable operation
**Depends on:** recall v0.6.0 baseline (decay_toward_neutral + apply_decay_sweep already exist)
build_target: rust-extend
build_into: /home/jsy/wintermute/recall
version_bump: minor

---

## TL;DR

The recall library already has `feedback::decay_toward_neutral` and
`index::apply_decay_sweep`, but decay is only accessible as a hidden flag
(`recall feedback --decay-sweep`). This PRD promotes temporal decay to a
first-class subcommand (`recall temporal-decay`) with dry-run support,
per-memory reporting, configurable thresholds, and a structured `temporal_decay`
module that separates the business logic from the command handler.

---

## 1. Why this exists

1. **Decay is invisible.** The current `--decay-sweep` flag fires silently;
   there is no way to see which memories decayed, by how much, or what would
   decay before applying.
2. **Decay is buried.** `recall feedback --decay-sweep` mixes outcome feedback
   with time-based decay; they are different signals.
3. **No dry-run.** Operators cannot preview what would change before committing.
4. **No configuration surface.** Half-life and min-interval live only in
   `Config::Feedback`; no per-run override.

---

## 2. What this builds

### 2.1 New module: `src/temporal_decay.rs`

Pure functions and types. No SQLite or file I/O — fully testable without a store.

```rust
pub struct DecayCandidate {
    pub id: String,
    pub confidence_before: f64,
    pub confidence_after: f64,        // projected / applied
    pub days_since_baseline: f64,
    pub half_life_d: u32,
}

pub fn project_decay(
    confidence: f64,
    days_since_baseline: f64,
    half_life_d: u32,
) -> f64;

pub fn is_worth_updating(before: f64, after: f64, min_delta: f64) -> bool;
```

### 2.2 New subcommand: `recall temporal-decay`

```
recall temporal-decay
    [--dry-run]                       # default: true (no writes)
    [--apply]                         # perform writes
    [--half-life-d <days>]            # override config (default: cfg.feedback.half_life_d)
    [--min-interval-d <days>]         # skip rows swept within N days (default: 1)
    [--min-delta <f64>]               # skip if |before-after| < threshold (default: 0.001)
    [--subject <prefix>]              # filter by subject prefix
    [--format text|json]
```

Output (text):

```
Temporal decay sweep (half-life=90d, dry-run=true):
  01KS...  semantic/user  conf 0.820 → 0.799  (-0.021, 3.2 days)
  01KR...  episodic/self  conf 0.600 → 0.597  (-0.003, 0.5 days)
2 memories would decay (0 applied).
```

Output (json):

```json
{
  "half_life_d": 90,
  "min_interval_d": 1,
  "dry_run": true,
  "candidates": 2,
  "applied": 0,
  "memories": [
    {
      "id": "01KS...",
      "kind": "semantic",
      "subject": "user",
      "confidence_before": 0.820,
      "confidence_after": 0.799,
      "delta": -0.021,
      "days_since_baseline": 3.2,
      "applied": false
    }
  ]
}
```

### 2.3 Index extension: `temporal_decay_report`

New method on `Index`:

```rust
pub fn temporal_decay_report(
    &self,
    half_life_d: u32,
    min_interval_d: u32,
    min_delta: f64,
    subject_prefix: Option<&str>,
    apply: bool,
) -> Result<Vec<DecayCandidate>>
```

Reuses the existing `apply_decay_sweep` logic but:
- Returns structured `DecayCandidate` rows instead of `usize`
- Respects `min_delta` (skip negligible changes)
- Respects `subject_prefix` filter
- Only writes when `apply == true`

---

## 3. Acceptance criteria

1. **AC1 — dry-run reports candidates without mutation.** Synthetic store with
   one memory at confidence=0.9, created 30 days ago. Run
   `temporal_decay_report(half_life_d=90, min_interval_d=0, min_delta=0.0,
   None, apply=false)`. Returns 1 candidate; memory confidence in store
   unchanged. Test: `tests/temporal_decay.rs::dry_run_no_mutation`.

2. **AC2 — apply decreases confidence.** Same fixture, `apply=true`. After call,
   `index.get_meta(id).confidence < 0.9`. Projected value matches formula
   `0.5 + (0.9-0.5) * 2^(-30/90) ≈ 0.817`. Test:
   `tests/temporal_decay.rs::apply_decreases_confidence`.

3. **AC3 — min_delta skips negligible changes.** Memory at confidence=0.5
   (neutral). `project_decay(0.5, 100.0, 90)` returns exactly 0.5. With
   `min_delta=0.001`, the memory is excluded from candidates. Test:
   `tests/temporal_decay.rs::neutral_memory_excluded`.

4. **AC4 — min_interval_d idempotency.** After `apply=true`, re-running within
   `min_interval_d=1` returns 0 candidates (last_decay_sweep_at is recent).
   Test: `tests/temporal_decay.rs::idempotency_gate`.

5. **AC5 — subject_prefix filter.** Two memories: one `user`, one `self`. With
   `subject_prefix=Some("user")`, only the user memory is a candidate (assuming
   both would otherwise qualify). Test:
   `tests/temporal_decay.rs::subject_prefix_filter`.

6. **AC6 — project_decay formula matches decay_toward_neutral.** Unit test:
   `temporal_decay::project_decay(0.9, 90.0, 90)` equals
   `feedback::decay_toward_neutral(0.9, 90.0, 90)`. No divergence between
   module and existing function.

---

## 4. Implementation notes

### 4.1 Baseline for "days since"

Same as existing `apply_decay_sweep`: prefer `last_decay_sweep_at`, fall back
to `created_at`. No new fields needed.

### 4.2 `temporal_decay_report` vs `apply_decay_sweep`

The new method calls `feedback::decay_toward_neutral` (same formula) and
writes via the same SQL as `apply_decay_sweep`. It is a superset: when
`apply=true` and no filters are set, the behavior is identical. The existing
`apply_decay_sweep` method is kept for backward compat; the new one is the
preferred path.

### 4.3 MSRV

Rust 1.85, edition 2024. No let-chains. No nightly features.

---

## 5. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Formula divergence between module and index | AC6 unit test |
| Dry-run silently mutates | AC1 asserts post-call confidence unchanged |
| Filter skips all memories (wrong prefix) | Output reports `candidates: 0` clearly |

---

## 6. Phasing

- **v0.7.0** (this PRD): `temporal_decay` module, `temporal_decay_report`
  on `Index`, `recall temporal-decay` subcommand. Existing `--decay-sweep`
  flag kept for backward compat.
