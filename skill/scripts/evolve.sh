#!/usr/bin/env bash
# evolve.sh — aggregate evolution-proposal-*.json files and prepare a diff
# bundle against the skill itself. Never auto-applies.
#
# Usage:
#   evolve.sh [--since YYYY-MM-DD] [--max N]

set -euo pipefail

SKILL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROPOSALS_DIR="$SKILL_DIR/proposals"

if command -v autobuilder >/dev/null 2>&1; then
  exec autobuilder evolve "$@"
fi

# Fallback: enumerate proposals and produce a one-page summary the Claude
# orchestrator can hand to the evolve prompt.

if [ ! -d "$PROPOSALS_DIR" ]; then
  echo "evolve: no proposals directory at $PROPOSALS_DIR" >&2
  exit 0
fi

PROPOSAL_COUNT=$(find "$PROPOSALS_DIR" -name 'evolution-proposal-*.json' -type f 2>/dev/null | wc -l)
APPLIED_LOG="$PROPOSALS_DIR/applied.log"

echo "# Evolve preflight"
echo
echo "Proposals on disk: $PROPOSAL_COUNT"
echo "Applied log: $([ -f "$APPLIED_LOG" ] && wc -l < "$APPLIED_LOG" || echo 0) entries"
echo

if [ "$PROPOSAL_COUNT" -eq 0 ]; then
  echo "No proposals to aggregate."
  exit 0
fi

echo "## Proposals by target"
find "$PROPOSALS_DIR" -name 'evolution-proposal-*.json' -type f -print0 \
  | xargs -0 jq -s '
      [.[] | .proposals[]?]
      | group_by(.target)
      | map({
          target: .[0].target,
          count: length,
          total_iters_saved: ([.[].estimated_iters_saved // 0] | add),
          kinds: ([.[].kind] | unique),
          rationales: ([.[].rationale] | unique | .[0:3])
        })
      | sort_by(-.total_iters_saved)
    '

cat <<EOF

---

Next step: feed this into the evolve prompt:

    cat ~/.claude/skills/autobuilder/prompts/evolve.md

Output (do not auto-apply):
    ~/.claude/skills/autobuilder/proposals/evolve-report-\$(date -u +%Y%m%d).md
    ~/.claude/skills/autobuilder/proposals/evolve-diff-\$(date -u +%Y%m%d).patch
EOF
