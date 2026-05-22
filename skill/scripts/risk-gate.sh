#!/usr/bin/env bash
# risk-gate.sh — Stage 4 driver. Checks that all 7 receipts are present and
# digest-bound to HEAD, then emits a pass/block verdict.
#
# Usage:
#   risk-gate.sh <project_dir>
#
# Defers to `autobuilder gate` when the binary is installed. Fallback is a
# minimum-viable check that all 7 receipt files exist and are non-empty.

set -euo pipefail

PROJECT_DIR="${1:?usage: risk-gate.sh <project_dir>}"
cd "$PROJECT_DIR"

find_autobuilder_binary() {
  # 1. anything already on PATH wins.
  if command -v autobuilder >/dev/null 2>&1; then
    command -v autobuilder
    return
  fi
  # 2. well-known build outputs of the autobuilder workspace.
  local candidates=(
    "$HOME/projects/autobuilder/autobuilder/target/release/autobuilder"
    "$HOME/projects/autobuilder/autobuilder/target/debug/autobuilder"
  )
  for c in "${candidates[@]}"; do
    if [ -x "$c" ]; then
      echo "$c"
      return
    fi
  done
}

AUTOBUILDER_BIN="$(find_autobuilder_binary)"
if [ -n "$AUTOBUILDER_BIN" ]; then
  exec "$AUTOBUILDER_BIN" gate --project "$PROJECT_DIR"
fi

# Fallback: structural-presence check only.
RECEIPTS_DIR=target/autobuilder/receipts
REQUIRED=(
  intake.json
  vti-plan.json
  proof-receipt.json
  risk-gate.json
  reviewer-agent.json
  rollback-plan.json
  ci-checks.json
)

VERDICT_FILE=target/autobuilder/risk-gate-verdict.json
MISSING=()
EMPTY=()
HEAD_SHA=$(git rev-parse HEAD 2>/dev/null || echo "unknown")

for r in "${REQUIRED[@]}"; do
  if [ ! -f "$RECEIPTS_DIR/$r" ]; then
    MISSING+=("$r")
  elif [ ! -s "$RECEIPTS_DIR/$r" ]; then
    EMPTY+=("$r")
  fi
done

if [ ${#MISSING[@]} -eq 0 ] && [ ${#EMPTY[@]} -eq 0 ]; then
  VERDICT=pass
else
  VERDICT=block
fi

mkdir -p target/autobuilder
jq -n \
  --arg verdict "$VERDICT" \
  --arg head "$HEAD_SHA" \
  --argjson missing "$(printf '%s\n' "${MISSING[@]}" | jq -R . | jq -s .)" \
  --argjson empty "$(printf '%s\n' "${EMPTY[@]}" | jq -R . | jq -s .)" \
  '{
    schema: "autobuilder.risk_gate_verdict.v1",
    head_sha: $head,
    verdict: $verdict,
    missing_receipts: $missing,
    empty_receipts: $empty,
    note: "fallback check only — install autobuilder binary for digest-binding"
  }' > "$VERDICT_FILE"

cat "$VERDICT_FILE"
[ "$VERDICT" = pass ] || exit 1
