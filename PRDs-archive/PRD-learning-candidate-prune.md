# PRD: learning-candidate-prune — bound the inbox

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** [visions/harvest.md](visions/harvest.md)
build_target: shell
build_into: /home/jsy/.claude/scripts/learning-candidates-prune.sh

---

## TL;DR

Even with triage (consumption) and prefilter (better producer), some
drafts will sit untouched — sessions where the user moved on, learnings
that nobody got back to. Without a decay rule, the queue grows
unboundedly. This PRD adds a small script that deletes drafts older
than 7 days (configurable) and journals a one-line note recording the
slug + matched patterns so the lost signal is at least observable in
the historical record.

## Why this exists

Direct evidence from this dream's Phase 1 (2026-05-28T08:00Z):

- 3 drafts on disk, oldest already 3.5 hours old at session start
  without being touched. Realistic projection: most drafts will never
  be processed by hand; triage will catch some; the rest will
  accumulate.
- The SessionStart hook (`learning-candidates-start.sh`) surfaces the
  full queue every fresh session. Once the queue grows past a handful,
  the surface becomes noise — exactly the failure mode that motivated
  the fidelity vision elsewhere (high-surface-low-use bias). Pruning
  preserves the surface's signal value.
- Self-review run 8 (2026-05-28T00:01Z) already tracks "8 stale
  reflective/self memories with `recalls=0` older than 30d" as a
  pending item. That's the same antipattern in a different store; we
  don't want to grow a second backlog.

## What this builds

A standalone shell script invoked by either the user
(`bash ~/.claude/scripts/learning-candidates-prune.sh`), the
/self-review skill's Phase D action set, or a systemd-user timer that
fires daily.

### File: `~/.claude/scripts/learning-candidates-prune.sh`

Pseudocode:

```sh
#!/usr/bin/env bash
set -uo pipefail

DIR="${LEARNING_CANDIDATES_DIR:-$HOME/.claude/scratch/learning-candidates}"
MAX_AGE_DAYS="${LEARNING_CANDIDATES_MAX_AGE_DAYS:-7}"
JOURNAL_DIR="${BRAIN_JOURNAL_DIR:-$HOME/brain/journal}"
DRY_RUN="${DRY_RUN:-0}"

# Iterate drafts older than MAX_AGE_DAYS.
# For each:
#   - parse slug from filename
#   - parse matched patterns from body (line after "**Matched patterns:**")
#   - either print (dry-run) or rm + append note to today's journal
# Print one-line summary at end: "prune: dropped N, kept M (older-than: 7d)"
```

### Journal note format

One line per pruned draft, appended to `~/brain/journal/YYYY-MM-DD.md`
under a `## learning-candidate prune` heading created lazily on first
prune of the day:

```
## learning-candidate prune

- prune: <slug> — patterns: "save as feedback" (1), "always use" (1) — age: 8d
```

### No timer in this PRD

This PRD ships the *script*. Wiring it to a systemd-user timer OR to
/self-review's Phase D action list is **out of scope for v0.1** — the
script is callable manually; automation is a follow-up once we've seen
it run a few times by hand.

## Acceptance criteria

1. **AC1: Default age threshold.** Without `LEARNING_CANDIDATES_MAX_AGE_DAYS`
   set, the script uses 7 days. A draft with mtime 8 days ago is
   pruned; a draft with mtime 6 days ago is kept.

2. **AC2: Dry-run reports without deleting.** `DRY_RUN=1
   learning-candidates-prune.sh` lists candidates that *would* be
   pruned (slug + age + patterns) without removing any file or
   touching the journal.

3. **AC3: Live run deletes and journals.** With `DRY_RUN=0` (default),
   pruned drafts are removed from disk AND a corresponding journal
   note exists at `~/brain/journal/YYYY-MM-DD.md` for each.

4. **AC4: Journal note format is greppable.** Every prune note matches
   the regex `^- prune: [0-9TZ]+ — patterns: .+ — age: \d+d$`. This
   lets future Claudes count and characterize lost signal.

5. **AC5: Lazy heading creation.** The `## learning-candidate prune`
   heading is added to today's journal **only** if at least one prune
   happens; if nothing was pruned, the journal is not touched.

6. **AC6: Configurable inputs.** Overriding
   `LEARNING_CANDIDATES_MAX_AGE_DAYS=1`, `LEARNING_CANDIDATES_DIR`,
   and `BRAIN_JOURNAL_DIR` via env all take effect (test by setting
   to a tmp directory + 1-day threshold + tmp journal).

7. **AC7: Summary line is accurate.** The final stderr/stdout line
   reports the count of pruned + kept drafts; sums to the count of
   `.md` files in the directory before the run.

## Out of scope

- Systemd-user timer or /self-review wiring — separate follow-up once
  the script is proven manually.
- "Soft delete" (move to an archive directory instead of `rm`) — `rm`
  is fine; the journal note is the audit trail.
- Pattern-aware retention (e.g., never prune imperative-only drafts) —
  if the user wants to keep one, they should triage it; pruning is the
  default decay rule.

## Dependencies

None. Pure shell; no new binaries. Triage and prefilter can ship
before or after; prune is independent.
