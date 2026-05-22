#!/usr/bin/env bash
# postmortem.sh — Stage 5 driver. Aggregates results.tsv + capsules + receipts
# and prepares the Claude orchestrator to run the postmortem-writer prompt.
#
# Usage:
#   postmortem.sh <project_dir>
#
# Emits target/autobuilder/postmortem.md (via Claude) and queues an
# evolution-proposal.json in ~/.claude/skills/autobuilder/proposals/.

set -euo pipefail

PROJECT_DIR="${1:?usage: postmortem.sh <project_dir>}"
cd "$PROJECT_DIR"

if command -v autobuilder >/dev/null 2>&1; then
  exec autobuilder postmortem --project "$PROJECT_DIR"
fi

# Fallback: print a pre-flight summary that the Claude orchestrator (or the
# user) can feed into the postmortem-writer prompt.
RESULTS=target/autobuilder/results.tsv
RECEIPTS=target/autobuilder/receipts
CAPSULES=target/autobuilder/failure-capsules

echo "# Postmortem preflight"
echo
echo "## results.tsv"
if [ -f "$RESULTS" ]; then
  wc -l "$RESULTS"
  echo
  head -1 "$RESULTS"
  tail -n 10 "$RESULTS"
else
  echo "(missing)"
fi

echo
echo "## Receipts"
if [ -d "$RECEIPTS" ]; then
  ls -la "$RECEIPTS"
else
  echo "(missing)"
fi

echo
echo "## FailureCapsules"
if [ -d "$CAPSULES" ]; then
  ls -la "$CAPSULES"
  for f in "$CAPSULES"/*.json; do
    [ -f "$f" ] || continue
    echo
    echo "### $(basename "$f")"
    jq -r '"  failure_kind: \(.failure_kind)\n  stage: \(.stage)\n  retry_count: \(.retry_count)\n  summary: \(.summary)"' "$f"
  done
else
  echo "(none)"
fi

cat <<EOF

---

Next step: feed this summary into the postmortem-writer prompt:

    cat ~/.claude/skills/autobuilder/prompts/postmortem-writer.md

Then emit:
    target/autobuilder/postmortem.md
    ~/.claude/skills/autobuilder/proposals/evolution-proposal-<intent_slug>-<timestamp>.json
EOF
