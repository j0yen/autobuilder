# PRD: learning-candidate-triage — process the Stop-hook queue

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** [visions/harvest.md](visions/harvest.md)
build_target: shell
build_into: /home/jsy/.claude/skills/triage

---

## TL;DR

The Stop hook `recall-learning-candidate.sh` writes one markdown draft
per session that matched any learning-pattern phrase. The SessionStart
hook surfaces them at startup with "Review and either `recall write` a
memory or delete the draft." Nobody does. Three drafts have sat in
`~/.claude/scratch/learning-candidates/` since 2026-05-28T05:29Z; the
oldest is from session 6554d28b at 05:29:11Z. This PRD builds the
consumer skill: a `/triage` slash command that walks the queue, makes
a save/discard/defer call on each draft with reasoning, and acts.

## Why this exists

Direct evidence from this dream's Phase 1 (2026-05-28T08:00Z):

- `ls ~/.claude/scratch/learning-candidates/` → 3 files
  (`20260528T052911Z.md`, `20260528T053034Z.md`, `20260528T055216Z.md`).
- All three drafts surface verbatim in the SessionStart banner of the
  current session — proving the producer-side wiring works.
- `grep -l "learning-candidate" ~/wintermute/autobuilder/*.md
  visions/*.md` → zero hits. No PRD, no vision references the consumer.
- `~/.claude/scripts/recall-learning-candidate.sh:9-11` says the
  drafts "are picked up by the SessionStart hook at the next session's
  startup" — but pickup means *surfacing*, not *processing*. Same
  three drafts will surface tomorrow if nothing changes.
- All three drafts are from the same session, all surface user
  utterances Claude probably should have saved ("save as feedback",
  "create skills so these are always used", "why dont you use recall
  more often?"). The signal is real; the consumption gap is the
  problem.

## What this builds

A new skill at `~/.claude/skills/triage/` exposing a `/triage` slash
command. Anatomy:

### Files

- `~/.claude/skills/triage/SKILL.md` — the skill description that
  Claude reads when `/triage` is invoked. Spells out the queue
  semantics, decision tree, and acted-on outcomes.
- `~/.claude/skills/triage/state/` — directory for any state files
  (none in v0.1; reserved for future auto-promote thresholds).

### Behavior (single invocation = one full pass over the queue)

1. List `~/.claude/scratch/learning-candidates/*.md`, oldest first.
2. For each draft in order, read its contents (matched patterns +
   recent user prompts) and decide:
   - **save** — `recall write` with inferred kind/subject/confidence;
     then `rm` the draft.
   - **discard** — `rm` the draft; append a one-line note to today's
     `~/brain/journal/YYYY-MM-DD.md` recording slug + matched patterns
     + one-sentence reason ("matched 'turns out' only; observational
     not durable").
   - **defer** — leave the draft in place; move to the next.
3. After the pass, print a one-line summary: `triage: saved N,
   discarded M, deferred K (Q remaining)`.

### Classification defaults

Conservative inference rules for `recall write` arguments when saving:

- `--kind feedback` if any imperative-pattern matched (today's list:
  `save as feedback`, `save this`, `remember that`, `remember this`,
  `save to memory`, `always use`, `never use`, `from now on`).
- `--kind reflective` otherwise (observational patterns: `turns out`,
  `actually no`, `wait no`).
- `--subject self` by default; `--subject user` if the matched user
  prompt is a direct preference statement about how to behave.
- `--confidence 0.6` for save-with-clear-reasoning; `0.4` for save-on-
  thin-evidence (when the matched pattern is ambiguous but the body
  contains a quoted concrete preference).

### Invocation

- `/triage` — interactive pass; Claude steps through and explains
  decisions in chat.
- Future: scheduled invocation via /self-review Phase D or systemd-user
  timer is **out of scope for v0.1** — manual only until the
  classification defaults are validated against real triage data.

## Acceptance criteria

1. **AC1: Skill exists and is discoverable.** `~/.claude/skills/triage/SKILL.md`
   is present and parseable; `/triage` is listed in the available skills
   block at session start.

2. **AC2: Empty-queue case is silent.** When
   `~/.claude/scratch/learning-candidates/` has zero `.md` files,
   `/triage` reports `triage: queue empty (0 remaining)` and exits
   without other output.

3. **AC3: Save path works end-to-end.** Given a draft with an
   imperative-pattern match (e.g., "save as feedback"), `/triage`
   issues `recall write --kind feedback ...`, the new memory appears in
   `recall list --kind feedback --limit 5`, AND the draft file is
   removed from `~/.claude/scratch/learning-candidates/`.

4. **AC4: Discard path works end-to-end.** Given a draft with only
   observational-pattern matches (e.g., "turns out" alone), `/triage`
   removes the draft and appends a one-line note to today's journal
   matching the format `triage discard: <slug> — <patterns> — <reason>`.

5. **AC5: Defer is non-destructive.** Given a draft Claude classifies
   as `defer`, the draft is unchanged on disk after the pass and the
   summary reports it under `deferred`.

6. **AC6: Summary line is accurate.** The final summary's
   saved+discarded+deferred sums to the count of drafts processed, and
   `remaining` equals the count of `.md` files still in the directory
   after the pass.

7. **AC7: SessionStart hook coordination.** After a complete triage
   pass that empties the queue, the next session's SessionStart banner
   does not mention learning candidates (the `learning-candidates-start.sh`
   hook output is empty when the directory is empty — confirm by
   inspecting the hook's existing behavior; no edit to that hook is
   required by this PRD).

## Out of scope

- Auto-promote (skip-review for highest-confidence drafts) — captured
  as an open question in the vision; defer to a successor PRD.
- Scheduled/unattended triage (timer-driven, /self-review-driven) —
  v0.1 is manual-invocation only.
- Prefilter changes to the producer hook — that's
  `PRD-learning-candidate-prefilter.md`.
- Auto-deletion of stale drafts — that's `PRD-learning-candidate-prune.md`.

## Dependencies

None. `recall write` exists. The Stop and SessionStart hooks already
produce drafts. The skill is purely additive.
