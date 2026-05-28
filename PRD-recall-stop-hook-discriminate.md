# PRD: recall-stop-hook-discriminate — accept used memories, abstain on surfaced-only

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** [visions/fidelity.md](visions/fidelity.md)
**Depends on:** [PRD-recall-surfaced-tracking.md](PRD-recall-surfaced-tracking.md) AND [PRD-recall-use-evidence.md](PRD-recall-use-evidence.md) shipped
build_target: rust-extend
build_into: /home/jsy/wintermute/recall
**Version target:** `recall v0.7.3` (patch — adds `used_count` column +
`--accept-used` flag, replaces Stop hook blanket-accept with split).

---

## TL;DR

This is the load-bearing PRD of the fidelity vision. With `surfaced.json`
and `used.json` both written per session, the Stop hook switches from
"apply `--accept` on every recalled id" to "apply `--accept-used` on
used ids, `--abstain` on surfaced-but-unused ids." A new `used_count`
column tracks how many sessions confirmed a memory's utility — separate
from `feedback_count` (any feedback) and `surfaced_count` (any hook
surfacing). For the first time, recall's ranking signal is
discriminating between "surface was useful" and "surface was noise."

---

## 1. Why this exists

1. **All prior fidelity work is plumbing for this PRD.** Surfaced
   tracking and use-evidence collected the data; this PRD acts on it.
2. **The current Stop hook code is one block to replace.** Existing
   `recall-stop.sh` (lines 39–50) applies `--accept` on every id from
   `recalled.json`. The replacement reads `used.json` and
   `surfaced.json`, computes set difference, and applies two distinct
   feedback batches.
3. **`used_count` belongs at the data layer, not as a derived stat.**
   A column lets `recall query` ranking and `recall doctor` both read
   utility cheaply. Computing it from session logs on every query is
   wrong-shaped — sessions get garbage-collected eventually.
4. **Today's behavior is biased and we can prove it.** 158 weather
   session dirs all applied uniform `+0.02` accept. Even modest
   per-session bias compounds over months. The longer we wait, the
   harder ranking calibration becomes.

---

## 2. What this builds

### 2.1 Schema migration

`src/index.rs`:

- Add `used_count: u32` to `MemoryFront` and `MemoryMeta` structs.
- Idempotent `ALTER TABLE memories_meta ADD COLUMN used_count
  INTEGER NOT NULL DEFAULT 0` migration (same pattern as
  surfaced_count from PRD #1).
- Roundtrip + upsert include the new column.

### 2.2 New `recall feedback --accept-used` mode

```
recall feedback --accept-used <id> [<id>...]
```

- Behaves like `--accept` (confidence += accept_delta, clamped to
  ceiling, increments `feedback_count`).
- ALSO increments `used_count` by 1.
- Existing `--accept` flag is preserved (callers wanting the old
  semantics — e.g., user-driven manual accept — keep working).

### 2.3 Stop hook rewrite

`~/.claude/scripts/recall-stop.sh`:

```bash
# After PRD #2's use-detect call writes used.json:
if [ -f "$weather_dir/surfaced.json" ] && [ -f "$weather_dir/used.json" ]; then
    used_ids="$(jq -r '.[]?' "$weather_dir/used.json" | tr '\n' ' ')"
    surfaced_ids="$(jq -r '.[]?' "$weather_dir/surfaced.json" | tr '\n' ' ')"

    # Set difference: surfaced minus used = abstain set
    abstain_ids="$(jq -rn --slurpfile s "$weather_dir/surfaced.json" \
                              --slurpfile u "$weather_dir/used.json" \
        '($s[0] - $u[0])[]?' | tr '\n' ' ')"

    [ -n "${used_ids// /}" ] && \
        "$RECALL_BIN" feedback --accept-used $used_ids --format text || true
    [ -n "${abstain_ids// /}" ] && \
        "$RECALL_BIN" feedback --abstain $abstain_ids --format text || true
fi
```

Removes the existing blanket-accept block on `recalled.json`.

### 2.4 Backward compat for sessions pre-PRD-#1

A weather dir from before surfaced.json existed has `recalled.json`
only. The Stop hook keeps a fallback path: if `surfaced.json` is
missing but `recalled.json` is present, apply old blanket-accept
behavior. This degrades to legacy semantics rather than dropping
the feedback entirely. Smoke at AC5.

### 2.5 Ranking input (deferred to PRD #4 / #5)

This PRD does NOT change `recall query` ranking weights. The
`used_count` column is populated but not yet used by retrieval. PRD
#4 (`recall-doctor-utility`) is the first surface that reads it; PRD
#5 (`recall-corpus-vacuum`) is the first action on it.

### 2.6 Out of scope

- No retroactive recomputation of confidence. Memories that drifted
  up from blanket-accept stay where they are; the abstain semantics
  apply going forward only.
- No transcript replay tool. Past sessions' use-evidence is lost.
  (Could be added later as `recall replay --since 2026-05-28`.)
- No change to `--reject` path (braid correlator still owns it).

---

## 3. Acceptance criteria

1. **AC1 — schema migration adds `used_count`.** Open a v0.7.2 DB,
   verify column appears after migration. Idempotent on re-open.
   Test: `tests/migration_used_count.rs`.
2. **AC2 — `recall feedback --accept-used <id>` increments both
   `used_count` and `feedback_count`, bumps confidence by
   `accept_delta`.** Test:
   `feedback::tests::accept_used_increments_use_and_feedback`.
3. **AC3 — `recall feedback --abstain <id>` leaves all three
   counters unchanged.** Test exists from v0.6.0 surfacing PRD; verify
   still passes with new column. (Regression test.)
4. **AC4 — Stop hook applies `--accept-used` on used ids and
   `--abstain` on surfaced-but-unused.** Synthetic session: 5
   surfaced ids; use-detect marks 2 as used; after Stop, 2 ids have
   `used_count=1` AND `confidence` bumped; 3 ids have `used_count=0`
   AND `confidence` unchanged.
5. **AC5 — Stop hook falls back to blanket-accept on legacy weather
   dir.** Pre-seed a weather dir with `recalled.json` only (no
   surfaced.json, no used.json). After Stop, all ids have
   `feedback_count` incremented (legacy path fired).
6. **AC6 — Stop hook handles missing used.json (use-detect failed).**
   Pre-seed surfaced.json but not used.json. After Stop, every
   surfaced id is treated as abstain (conservative default —
   "we don't know if it was used, so don't reward it").
7. **AC7 — markdown frontmatter round-trips `used_count: N`.**
   Test: `feedback::tests::used_roundtrip_markdown`.
8. **AC8 — over 10 synthetic sessions, used_ratio of a "never-used"
   id stays at 0 while a "used-every-time" id reaches 1.0.** Test:
   `tests/utility_compound.rs` simulates 10 stop-hook fires and
   asserts the per-id distribution.

---

## 4. Implementation notes

### 4.1 Atomicity per session

Stop hook fires once per session. The two feedback calls are
sequential (used first, abstain second). If the abstain call fails
mid-batch, the used calls already landed — that's acceptable; both
are idempotent at the row level.

### 4.2 Test simulation harness

`tests/utility_compound.rs` uses an in-memory recall store, fakes a
sequence of Stop-hook firings via direct `feedback --accept-used` /
`--abstain` calls, and asserts:

- After session 1: a used id at confidence 0.52, used=1, surfaced=1.
- After session 10: same id at confidence ≈0.5+(10*0.02) clamped to
  ceiling 0.95 (so caps at 0.95 by session ~22, but at 10 sessions
  it's at 0.7), used=10, surfaced=10.
- Never-used id: confidence 0.5 unchanged across 10 sessions,
  used=0, surfaced=10.

### 4.3 Why two flags instead of one

`--accept-used` is distinct from `--accept` so external callers (a
hypothetical IDE button that lets the user manually accept a memory)
get clear semantics: manual accept doesn't necessarily mean
"system observed use." The Stop hook is one specific caller; manual
accept is another. Both paths bump confidence and feedback_count,
but only Stop-hook-with-evidence sets used_count.

---

## 5. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Use-detect false negatives over-penalize | Abstain is no-op (not negative); same as today for accept-on-no-contradiction. Net effect: less inflation, not artificial deflation. |
| Migration leaves used_count=0 on every existing memory | Acceptable: PRD #4 (doctor utility) ignores memories with `surfaced_count < 5` so cold-start memories aren't flagged. |
| Legacy path keeps fueling drift while it's there | Acceptable bridge during rollout; v0.7.4+ can remove the legacy fallback once a few weeks of new surfaced.json data accumulate. |
| Concurrent Stop hook fires (two sessions ending same time) | Already safe — weather dirs are per-sid; the SQLite upsert is row-level atomic. |

---

## 6. Phasing

- **v0.7.3** (this PRD): schema, `--accept-used`, Stop hook rewrite,
  legacy fallback.
- v0.7.4 (next: recall-doctor-utility): expose `used_count` + ratio
  via `recall doctor --format json`.
- v0.7.5 (recall-corpus-vacuum): act on the ratio.
