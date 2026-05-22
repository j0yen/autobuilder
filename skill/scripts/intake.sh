#!/usr/bin/env bash
# intake.sh — Stage 1 entry point.
#
# Intake is a conversational stage (the 5-Whys interview). The skill's Claude
# orchestrator handles the dialogue; this script just validates the result.
#
# Usage:
#   intake.sh <intent_card_path>
#
# Validates intent-card.json against schemas/intent-card.schema.json.
# Exit 0 if valid, 1 if not (with diagnostic on stderr).

set -euo pipefail

SKILL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCHEMA="$SKILL_DIR/schemas/intent-card.schema.json"
INTENT_CARD="${1:-agent/intent-card.json}"

if [ ! -f "$INTENT_CARD" ]; then
  echo "intake: $INTENT_CARD not found" >&2
  exit 1
fi

if [ ! -f "$SCHEMA" ]; then
  echo "intake: schema $SCHEMA not found" >&2
  exit 1
fi

# Prefer the autobuilder binary's validator if available; fall back to ajv-cli or check-jsonschema.
if command -v autobuilder >/dev/null 2>&1; then
  exec autobuilder intake --validate "$INTENT_CARD"
fi

if command -v ajv >/dev/null 2>&1; then
  ajv validate -s "$SCHEMA" -d "$INTENT_CARD" --strict=false
elif command -v check-jsonschema >/dev/null 2>&1; then
  check-jsonschema --schemafile "$SCHEMA" "$INTENT_CARD"
else
  # Minimal fallback: jq presence check.
  jq -e 'has("schema") and .schema == "autobuilder.intent_card.v1"' "$INTENT_CARD" >/dev/null || {
    echo "intake: intent-card.schema mismatch; install ajv-cli or check-jsonschema for full validation" >&2
    exit 1
  }
  echo "intake: minimal validation passed (install ajv-cli or check-jsonschema for full schema check)" >&2
fi
