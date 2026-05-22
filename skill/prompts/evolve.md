# Evolve — gated self-improvement aggregator

You are invoked when the user runs `/autobuilder --evolve` (or `scripts/evolve.sh`). You aggregate the `evolution-proposal-*.json` files in `~/.claude/skills/autobuilder/proposals/` from the last K runs and surface a diff against the skill itself for the user's review.

You **never** auto-apply changes to `SKILL.md`, `rules/`, `templates/`, `schemas/`, or `prompts/`. The whole point of Stage 5 is to keep the self-modification loop honest.

## Inputs

- All `evolution-proposal-*.json` in `~/.claude/skills/autobuilder/proposals/` newer than the last applied change.
- The current state of `~/.claude/skills/autobuilder/{SKILL.md,rules/,templates/,schemas/,prompts/}`.
- An optional `~/.claude/skills/autobuilder/proposals/applied.log` recording prior accepted/rejected proposals (so you don't re-propose what the user already rejected).

## Aggregation rules

1. **Group by `target`.** Multiple runs proposing the same `target` change should collapse into one ranked recommendation.
2. **Rank by total `estimated_iters_saved`** across proposals targeting the same file/section, then by number of distinct runs proposing it (more runs = stronger signal).
3. **Drop proposals contradicted by later runs.** If run A proposed adding rule X and run B proposed removing rule X, surface both and let the user decide; don't try to resolve.
4. **Discard proposals previously rejected by user.** The applied.log tracks `{proposal_id, decision, reason}`. Respect prior decisions unless the user explicitly says "re-evaluate."
5. **Cap output at 10 recommendations.** If more, group by category and summarize the long tail.

## Output

A markdown report and a unified-diff bundle:

```
~/.claude/skills/autobuilder/proposals/evolve-report-<YYYYMMDD>.md
~/.claude/skills/autobuilder/proposals/evolve-diff-<YYYYMMDD>.patch
```

The report:

```markdown
# Evolve Report — <date>

Considered <N> proposals from <M> runs since <last-applied-date>.

## Recommendations (ranked)

### 1. <short title> — <total estimated iters saved> over <K> runs

**Target:** `<path>`
**Kind:** add | modify | remove
**Evidence:** <commit/iter refs across runs>
**Rationale:** <2-3 sentences>
**Proposed diff:** see hunk 1 in `evolve-diff-<YYYYMMDD>.patch`

### 2. ...

## Long tail (not in top 10)
- <one-line summaries of proposals that didn't make the cut>

## How to apply

The diff is NOT auto-applied. Review each hunk:

    git -C ~/.claude/skills/autobuilder apply --check evolve-diff-<date>.patch
    # then, for accepted hunks:
    git -C ~/.claude/skills/autobuilder apply --include='<path>' evolve-diff-<date>.patch

After applying, record decisions in `proposals/applied.log` so future evolve runs don't re-surface them.
```

The patch is a standard unified diff that `git apply` accepts.

## Things to never do

- **Never** write directly to `SKILL.md` or any other skill file. Only produce the diff.
- **Never** include a hunk you cannot trace to specific run evidence in the proposals.
- **Never** modify `~/.claude/skills/autobuilder/proposals/applied.log` automatically — that file records user decisions only.
- **Never** discard proposals because they're inconvenient. Surface; the user decides.

## Safety check before exit

Before writing the report and patch, verify:
- The patch applies cleanly with `git apply --check` against the current skill state. If it doesn't, regenerate the affected hunks against current content.
- No hunk would delete a schema field referenced by an existing receipt. (If you propose schema changes, mark them `kind: "modify"` and note the backwards-compat consideration.)
- No hunk would silently weaken a `BLOCKING` BAD_RUST item. Demotion from blocking → advisory requires explicit user note in the proposal evidence.
