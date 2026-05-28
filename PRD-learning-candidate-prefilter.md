# PRD: learning-candidate-prefilter — fewer, higher-signal drafts

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** [visions/harvest.md](visions/harvest.md)
build_target: shell
build_into: /home/jsy/.claude/scripts/recall-learning-candidate.sh

---

## TL;DR

`recall-learning-candidate.sh` (Stop hook) currently emits a draft any
time **any** pattern match fires anywhere in the last 200 turns. Today
that produced 3 drafts from a single session, two of them on a single
"save as feedback" + "always use" match each (hit counts of 2 and 2),
and one on 84 hits of `turns out` alone — a phrase that's high-volume
but low-signal. This PRD replaces the single-match-creates-draft
threshold with a scored threshold that weights imperative patterns
above observational ones and requires a minimum signal score before
emission.

## Why this exists

Direct evidence from this dream's Phase 1 (2026-05-28T08:00Z):

- `~/.claude/scratch/learning-candidates/20260528T055216Z.md` — 84 hits
  of `turns out` alone. Body contains a status report on
  `PRD-wintermute-fleet-agorabus-announce-fix`, not a durable learning.
  Pure noise that crowded out signal.
- `~/.claude/scratch/learning-candidates/20260528T052911Z.md` and
  `20260528T053034Z.md` — both 1 hit on `save as feedback` + 1 hit on
  `always use` in the SAME session (one ran ~1min after the other; the
  Stop hook seems to fire on both partial and final stops). Same user
  utterances, two drafts. Half of today's queue is duplicates of itself.
- `recall-learning-candidate.sh:38-58` — pattern list has 15 entries
  with no per-pattern weighting; the emit-decision (further down in the
  same script) is "any match → write draft." No threshold, no de-dup.

Today's 3 drafts is one session's noise. Scale linearly: 5 sessions a
day × 1–3 drafts each = 5–15 drafts/day, of which most are pattern
sediment. The triage skill (`PRD-learning-candidate-triage.md`) can
handle volume but its sustainability depends on the queue's
signal-to-noise ratio.

## What this builds

A scored-threshold pass over the existing pattern hits, plus
intra-session de-duplication.

### Edits to `~/.claude/scripts/recall-learning-candidate.sh`

1. **Per-pattern weights.** Replace the flat pattern list with an
   associative array of `pattern → weight`. Imperative patterns get
   weight 2; observational patterns get weight 1. Cap-noise patterns
   (`turns out`) get weight 0.5 (still surface eventually if repeated,
   but each match contributes less).

   Initial weighting (revisable):
   - **Imperative (weight 2):** `save as feedback`, `save this`,
     `remember that`, `remember this`, `save to memory`,
     `always use`, `never use`, `from now on`.
   - **Observational (weight 1):** `actually no`, `wait no`,
     `correction`, `i meant`.
   - **Cap-noise (weight 0.5):** `turns out`.

2. **Emit threshold.** Total score `sum(weight × match_count_capped)`
   must reach ≥3 to emit. `match_count_capped` caps a single pattern's
   contribution at 3 occurrences to prevent one runaway phrase from
   dominating (closes the `turns out × 84` issue without losing the
   case where many distinct phrases co-occur). Threshold and caps are
   shell variables at the top of the script for easy tuning.

3. **Intra-session de-dup.** Before writing, check if a draft already
   exists for the same `session_id` (filename pattern matches: any
   existing `<ts>.md` whose body's `**Detected:** session <sid>` line
   matches). If yes, skip emission (the earlier draft will be picked up
   by triage; the second Stop hook fire for the same session adds
   nothing new). This closes the duplicate-pair issue.

4. **Audit log.** When the hook decides NOT to emit (below threshold or
   duplicate), append a single line to
   `~/.claude/scratch/learning-candidates/.audit.log`:
   `<ts> session=<sid> score=<n> drafts_in_session=<n> decision=<below_threshold|duplicate>`.
   This makes the prefilter's behavior observable; if the queue feels
   *too* quiet we can tune the threshold down with data.

### No changes elsewhere

- `learning-candidates-start.sh` (SessionStart) is untouched.
- The triage skill (`PRD-learning-candidate-triage.md`) is untouched —
  it just consumes a smaller queue.
- The draft *format* is unchanged (matched-patterns + recent prompts +
  suggested action). Only the *emit decision* changes.

## Acceptance criteria

1. **AC1: Imperative-only match emits a draft.** Given a session JSONL
   with 1 occurrence of `save as feedback` and zero other matches,
   total score = 2 × 1 = 2 < 3 → no draft. Given 1 occurrence of
   `save as feedback` + 1 of `always use`, score = 2 × 1 + 2 × 1 = 4
   ≥ 3 → draft is emitted. (This is the *more conservative* default;
   the current behavior would emit on the single-pattern case.)

2. **AC2: Cap-noise pattern alone doesn't emit.** Given a session
   with `turns out` appearing 84 times and no other patterns, total
   score = 0.5 × min(84, 3) = 1.5 < 3 → no draft. The audit log records
   `decision=below_threshold score=1.5`.

3. **AC3: Mixed imperative + cap-noise emits.** Given 1 `save as
   feedback` + 5 `turns out`, score = 2 × 1 + 0.5 × min(5, 3) = 3.5
   ≥ 3 → draft emitted. The valid imperative still wins through.

4. **AC4: Duplicate within a session is suppressed.** Given a session
   that already has a draft on disk (matching by `session_id`), a
   subsequent Stop hook fire for the same session above-threshold
   writes nothing. The audit log records `decision=duplicate`.

5. **AC5: Audit log captures every decision.** After three Stop hook
   fires (one emit, one below-threshold, one duplicate), the audit log
   has exactly three lines matching the documented format with correct
   decision values.

6. **AC6: Existing drafts are not modified.** The three pre-existing
   drafts (2026-05-28T052911Z, T053034Z, T055216Z) are still on disk
   after the prefilter ships; the prefilter only changes *future*
   emission decisions, not past artifacts. Triage handles backlog.

7. **AC7: Tunable thresholds are top-of-file constants.** The script
   exposes `THRESHOLD=3`, `PER_PATTERN_CAP=3`, `WEIGHT_IMPERATIVE=2`,
   `WEIGHT_OBSERVATIONAL=1`, `WEIGHT_CAPNOISE=0.5` as named variables
   at the top, so future tuning is a single-line edit.

## Out of scope

- LLM-as-judge scoring (using a model to decide if the matched turn is
  durable) — could replace heuristics later but is a separate, larger
  scope.
- Per-user customization of pattern weights — single-user system, not
  needed.
- Real-time draft consumption (emit→triage as the session ends) —
  separate from threshold-based emission.

## Dependencies

None. Edits a single existing script in-place; no new files, no new
binaries.
