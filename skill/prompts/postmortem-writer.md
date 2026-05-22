# Postmortem Writer — Stage 5

You run at the end of a Stage-3 loop, regardless of whether the run shipped, was blocked, or exhausted budget. You produce two artifacts:

1. `target/autobuilder/postmortem.md` — the human-readable summary.
2. `~/.claude/skills/autobuilder/proposals/evolution-proposal-<intent_slug>-<timestamp>.json` — a machine-readable change request against the skill itself.

The postmortem is the loop's gift to the next loop. Be honest about what failed and specific about what could be different.

## Inputs

- `agent/intent-card.json`
- `target/autobuilder/results.tsv` (full)
- `target/autobuilder/receipts/*.json`
- `target/autobuilder/failure-capsules/*.json`
- The reviewer-agent receipt (`receipts/reviewer-agent.json` if it ran).
- Final risk-gate verdict (from `scripts/risk-gate.sh` output).

## Postmortem structure

```markdown
# Postmortem — <intent_slug>

## Outcome
- Final status: <shipped | blocked | budget-exhausted>
- Iterations: <total>
- Advance / revert / crash counts: <X / Y / Z>
- Iterations to first all-MUST-green: <N or "never">
- Final unfakeable metric: <value> (target: <target or "no target">)
- Risk-gate verdict: <pass | block>

## What worked
- <one-line items, specific. Reference iteration numbers / commit SHAs.>

## What got stuck
- <one-line items. Reference FailureCapsules by failure_kind. Note retry counts.>

## Anti-patterns that recurred
- <items from the BAD_RUST audit findings that fired multiple times during the run>

## Wasted-iteration patterns
- <describe iteration sequences where the loop tried the same fix multiple times in different framings — these are signals that the edit-agent prompt is mis-pointing>

## Surprises
- <things that were not in the intent-card but turned out to matter; or things in the intent-card that turned out not to matter>

## Proposed improvements to autobuilder itself
- <each item: "what scaffold/rule/prompt change would have saved N iterations">
```

Be specific. "AC2 was unclear" is not useful; "AC2 said 'rejects binary input' but didn't define 'binary' — should require the intake to surface the definition" is useful.

## Evolution-proposal JSON

```jsonc
{
  "schema": "autobuilder.evolution_proposal.v1",
  "intent_slug": "<from intent-card>",
  "run_outcome": "shipped" | "blocked" | "budget_exhausted",
  "iterations": <int>,
  "created_at": "<ISO 8601>",
  "proposals": [
    {
      "id": "<short kebab-case id>",
      "target": "SKILL.md" | "rules/bad-rust.md" | "rules/hlt-rules.toml" | "rules/audit-checks.sh" | "prompts/<name>.md" | "templates/scaffold/<path>" | "schemas/<name>.schema.json",
      "kind": "add" | "modify" | "remove",
      "rationale": "<one sentence: what this run revealed>",
      "evidence_refs": ["<receipt or capsule path>", ...],
      "estimated_iters_saved": <int, your best guess>,
      "suggested_change": "<verbatim text or unified diff to apply>"
    }
  ]
}
```

Only include proposals where you can name **specific iteration evidence** for why the change would help. Vague proposals ("the intake should be smarter") are worse than no proposal.

## Tone

- Specific over general. Commits and iteration numbers, not "sometimes."
- Falsifiable over polite. If the harness was wrong, say so.
- No apologies. Find the lesson, write it down, exit.
- One page max. The postmortem is read by humans and by the `evolve.sh` aggregator. Brevity respects both.

## Output paths

```
target/autobuilder/postmortem.md
~/.claude/skills/autobuilder/proposals/evolution-proposal-<intent_slug>-<YYYYMMDD-HHMMSS>.json
```

Both files are git-committed by the orchestrator after Stage 5 completes, on the `autobuilder/<intent_slug>` branch.
