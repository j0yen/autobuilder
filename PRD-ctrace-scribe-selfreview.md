# PRD: ctrace-scribe-selfreview — close the missing-summary loop in self-review

Status: Draft v0.1
build_target: shell
Vision: visions/scribe.md

## TL;DR

Self-review notices missing ctrace summaries every run and never fixes
them — it hand-counts the gap (1→4→5 across runs 16–18) and rebuilds the
cross-session aggregate by hand each tick. This PRD wires `scribe
backfill` and `scribe rollup` (from PRD-ctrace-scribe and
PRD-ctrace-scribe-rollup) into self-review Phase B.5 so the daily review
*repairs* the record and *reads* a deterministic digest instead of
manually reconstructing both.

## Why this exists

From this vision's Phase 1 research (2026-05-28):

- `~/brain/journal/2026-05-28.md` runs 16, 17, 18 each contain a "ctrace
  missing summaries (N)" line under **Pending your call** — the count
  grows (1→4→5) and is never acted on. The review is a detector with no
  effector for this anomaly.
- The same journal entries contain a hand-built "Cross-session aggregate"
  that run 17 had to **sample** (40 of 268 files) because the full set was
  too large to stream in shell.
- PRD-binstale-self-review (visions/vigil.md) establishes the exact
  pattern this PRD follows: a small shell PRD that wires a new read-only
  CLI into self-review Phase B.5 and degrades safely when the CLI isn't
  installed yet.

## What this builds

A focused edit to the self-review skill's Phase B.5 (and its supporting
scripts under `~/.claude/skills/self-review/`), plus a journal-section
convention. No new binary — this is the wiring PRD.

### Behavior added to Phase B.5

1. **Backfill step.** If `scribe` is on `PATH`, run
   `scribe backfill ~/.cache/ctrace/sessions` and record the
   `rendered N, skipped M` count in the journal's ctrace section. Missing
   summaries become a *repaired* count, not a *pending* count. If `scribe`
   is absent, fall back to the existing per-file shell summarizer over the
   missing set (bounded, e.g. via `find … -newer`), or — if neither is
   available — preserve today's hand-count behavior. Never fail the review.
2. **Rollup step.** If `scribe` is on `PATH`, replace the hand-built
   "Cross-session aggregate" with `scribe rollup --since today --format md`
   piped into the journal. If absent, keep the existing sampled hand-built
   aggregate.
3. **Residual-gap report.** After backfill, re-count `*.ndjson` without a
   `*.summary.md`. A nonzero residual (e.g. the live session's own log, or
   a genuinely corrupt log) is reported with the reason, distinguishing
   "expected: active session" from "anomaly: corrupt log".

### Guardrails

- The backfill writes only `*.summary.md` files under
  `~/.cache/ctrace/sessions/`; wrap it in the existing wchg scope-guard so
  any write outside that dir is caught.
- Idempotent: re-running Phase B.5 in the same review renders 0 on the
  second pass.

## Acceptance criteria

1. With `scribe` installed, a self-review pass runs `scribe backfill` over
   `~/.cache/ctrace/sessions` and the journal's ctrace section reports the
   rendered/skipped counts instead of a bare "missing (N)".
2. After the backfill step, the count of `*.ndjson` lacking a matching
   `*.summary.md` is 0 except for the active session log (and any log the
   step explicitly flags as corrupt with a reason).
3. With `scribe` installed, the journal's cross-session aggregate is the
   output of `scribe rollup --since today`, not a hand-built sample.
4. With `scribe` **absent** from `PATH`, Phase B.5 completes without error
   and preserves the prior hand-count + sampled-aggregate behavior
   (graceful degradation).
5. The backfill step writes only under `~/.cache/ctrace/sessions/`,
   verified by a wchg scope-guard around it; a write elsewhere fails the
   step loudly.
6. Running Phase B.5 twice in one review renders 0 summaries on the second
   pass (idempotent).
7. The change is confined to the self-review skill; `bash -n` clean on
   every edited script.
