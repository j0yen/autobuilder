# PRD: recall-doctor-utility — expose surfaced / used / ratio via `recall doctor`

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** [visions/fidelity.md](visions/fidelity.md)
**Depends on:** [PRD-recall-stop-hook-discriminate.md](PRD-recall-stop-hook-discriminate.md) shipped (used_count populated)
build_target: rust-extend
build_into: /home/jsy/wintermute/recall
**Version target:** `recall v0.7.4` (patch — extends `recall doctor`
output; no schema change, no new subcommand).
**Coordinates with:** `recall-doctor-claims` (v0.7.0, freshness vision)
— both extend `doctor`. Different sections; no code collision.

---

## TL;DR

`recall doctor --format json` today reports structural health (file
counts, supersedes graph, confidence_drift). It says nothing about
statistical health — how often recall's surfaced choices actually
help. This PRD adds a `utility` section to doctor's JSON and a
human-readable text summary block. Per memory: `{id, surfaced, used,
ratio, confidence, calibration_drift}` where `calibration_drift =
confidence - (0.5 + ratio * 0.5)`. Text output prints the top 10
high-surface-low-use ("ranks well, rarely used") and the top 10
high-surface-high-use ("validated workhorses"). No mutation, no
sweep — purely diagnostic. Sets the table for `recall vacuum`.

---

## 1. Why this exists

1. **The data is there now; the surface isn't.** PRDs #1-3 populated
   `surfaced_count` and `used_count`. Without a surface, the user
   has no way to inspect drift between observed utility and ranking
   confidence.
2. **Self-review wants this for its playbook.** Self-review's Phase
   B.5 playbooks check structural health (file divergence,
   confidence_drift). Adding utility as an inspectable surface lets
   self-review add a `recall_utility_drift` playbook entry that
   flags when the high-surface-low-use list grows.
3. **recall-doctor-claims (queued, freshness vision)** extends doctor
   with factual claim checks. This PRD extends doctor with
   statistical health. Both fit naturally; both target different JSON
   keys; both keep the existing doctor surfaces stable.
4. **The metric is straightforward to compute.** One SQL aggregation
   per memory; doctor already reads `memories_meta` once and walks
   all rows. Adding two columns to the projection is trivial.

---

## 2. What this builds

### 2.1 New doctor output: `utility` JSON section

```json
{
  "files": { ... },
  "supersedes": { ... },
  "confidence_drift": [ ... ],
  "utility": {
    "total_memories": 56,
    "with_surface_data": 42,
    "low_utility_high_surface": [
      {
        "id": "01KS...",
        "kind": "reflective",
        "subject": "self",
        "surfaced": 27,
        "used": 1,
        "ratio": 0.037,
        "confidence": 0.74,
        "calibration_drift": 0.222
      }
    ],
    "high_utility_validated": [
      { "id": "01KS...", "surfaced": 18, "used": 17, "ratio": 0.944, "confidence": 0.78, "calibration_drift": -0.194 }
    ]
  }
}
```

Selection rules:

- `with_surface_data`: memories with `surfaced_count >= 5` (cold-start
  cutoff from fidelity vision §Open questions).
- `low_utility_high_surface`: top 10 by `surfaced_count` where
  `ratio < 0.2` (heavily surfaced, rarely used). Sorted descending
  by `surfaced_count`.
- `high_utility_validated`: top 10 by `surfaced_count` where
  `ratio >= 0.7`. Sorted descending by `used_count`.

`calibration_drift` formula:
`drift = confidence - (0.5 + ratio * 0.5)`. Positive means
"ranked higher than utility justifies"; negative means
"underranked workhorse." Same shape as existing `confidence_drift`
metric (PRD-recall-outcome-feedback AC7) so doctor's JSON stays
internally consistent.

### 2.2 Text output additions

`recall doctor` (text format) gains:

```
utility (n=56, with_surface=42):
  low-utility, high-surface (top 10, ratio < 0.2):
    01KS... reflective/self    surf=27 used=1  ratio=0.04 conf=0.74 drift=+0.22
    ...
  high-utility, validated (top 10, ratio >= 0.7):
    01KS... procedural/self    surf=18 used=17 ratio=0.94 conf=0.78 drift=-0.19
    ...
```

Same style as `confidence_drift` text block from v0.6.0.

### 2.3 Self-review playbook hook (light)

Self-review's `playbooks/` dir gets a new entry
`recall_utility_drift.md` describing the check:

```markdown
# playbook: recall-utility-drift

Trigger: `recall doctor --format json` `utility.low_utility_high_surface[]`
length > threshold (default 5).

Action: surface the top 3 ids in self-review's "Pending your call"
section with a one-line suggested action: "recall update <id>
--confidence 0.4" or "recall vacuum --dry-run" (latter once PRD #5
ships).

Not auto-applied — these are calibration warnings, not bugs.
```

Self-review skill repo gets a one-line entry in its playbooks index
pointing here. Skill-side change is small; the work happens in
recall.

### 2.4 Out of scope

- **No mutation.** Doctor reports; it does not adjust confidence or
  schedule decay. (PRD #5 owns that.)
- **No query-time impact.** Doctor's utility section computes only
  when `doctor` is invoked.
- **No per-kind / per-subject aggregation.** v1 keeps the projection
  flat. Aggregations are easy to add in v2 if useful.

---

## 3. Acceptance criteria

1. **AC1 — `recall doctor --format json` includes `utility` section.**
   Smoke against a recall store with at least one memory; assert
   `utility.total_memories` and `utility.with_surface_data` fields
   present. Test:
   `tests/doctor_utility_section.rs`.
2. **AC2 — `low_utility_high_surface` only includes memories with
   `surfaced_count >= 5 AND ratio < 0.2`.** Synthetic test: 4
   memories with (surf=10, used=0), (surf=10, used=1), (surf=4,
   used=0), (surf=10, used=8); only the first two qualify.
3. **AC3 — `high_utility_validated` only includes memories with
   `surfaced_count >= 5 AND ratio >= 0.7`.** Same fixture; only the
   last memory (10/8) qualifies.
4. **AC4 — `calibration_drift` matches `confidence -
   (0.5 + ratio * 0.5)` per row.** Property test:
   `proptest! { fn drift_formula_holds(...) }`.
5. **AC5 — Sorting is descending by `surfaced_count`.** Synthetic
   test: 12 memories all matching the low-utility cutoff with
   distinct surfaced_count; assert returned order.
6. **AC6 — Text format prints both blocks with consistent column
   alignment.** Snapshot test: `tests/doctor_utility_text.rs` runs
   doctor against a fixture and diffs against a stored expected
   snapshot.
7. **AC7 — Empty / cold-start store returns
   `low_utility_high_surface: []` and `high_utility_validated: []`
   without error.** Smoke against fresh DB.
8. **AC8 — Self-review playbook reads the JSON section.** Sim test
   that runs `recall doctor --format json | jq
   '.utility.low_utility_high_surface | length'` returns an integer
   on a non-empty store.

---

## 4. Implementation notes

### 4.1 SQL projection

```sql
SELECT id, kind, subject, confidence, surfaced_count, used_count,
       CAST(used_count AS REAL) / NULLIF(surfaced_count, 0) AS ratio
FROM memories_meta
WHERE surfaced_count >= 5
ORDER BY surfaced_count DESC;
```

Filter post-query in Rust by ratio thresholds (keeps SQL simple).

### 4.2 Output structure

`src/doctor.rs`:

```rust
#[derive(Serialize)]
pub struct UtilityReport {
    pub total_memories: u64,
    pub with_surface_data: u64,
    pub low_utility_high_surface: Vec<MemoryUtility>,
    pub high_utility_validated: Vec<MemoryUtility>,
}

#[derive(Serialize)]
pub struct MemoryUtility {
    pub id: String,
    pub kind: String,
    pub subject: String,
    pub surfaced: u32,
    pub used: u32,
    pub ratio: f32,
    pub confidence: f32,
    pub calibration_drift: f32,
}
```

Drift sign convention: positive = confidence higher than utility
suggests; negative = underranked.

### 4.3 Threshold rationale

- `surfaced_count >= 5`: any lower and the ratio swings wildly on
  small samples (one surfacing-without-use looks worse than it is).
  5 is enough for stable ratios.
- `ratio < 0.2`: 20% utility floor. Below that the memory contributes
  more noise than signal.
- `ratio >= 0.7`: 70% utility ceiling for "validated."

All three are configurable in `recall.toml` `[doctor]` section if
field reports demand tuning.

---

## 5. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Doctor latency increases with utility section | SQL is one extra scan; well-indexed. Should add <50ms even on 10K memories. |
| Calibration drift formula is rough | Acknowledged; v1 is intentionally simple. Tweakable via config. |
| Self-review surfaces too many warnings | Playbook trigger threshold is configurable; default 5 keeps signal:noise reasonable. |
| Snapshot tests break on natural sort changes | Snapshot includes only id/ratio precision rounded; tests deterministic. |

---

## 6. Phasing

- **v0.7.4** (this PRD): doctor `utility` section (JSON + text),
  self-review playbook entry.
- v0.7.5 (next: recall-corpus-vacuum): act on the ratio — sweep
  candidates, propose supersede or aggressive decay.
