#!/usr/bin/env bash
# scaffold.sh — Stage 2 entry point. Materializes the project tree from
# templates/scaffold/ using the intent-card.
#
# Usage:
#   scaffold.sh <intent_card_path> <project_dir>
#
# Defers to `autobuilder scaffold` if the binary is installed; otherwise
# performs a minimum-viable manual copy of templates/scaffold/.

set -euo pipefail

SKILL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INTENT_CARD="${1:?usage: scaffold.sh <intent_card_path> <project_dir>}"
PROJECT_DIR="${2:?usage: scaffold.sh <intent_card_path> <project_dir>}"

if [ ! -f "$INTENT_CARD" ]; then
  echo "scaffold: $INTENT_CARD not found" >&2
  exit 1
fi

if command -v autobuilder >/dev/null 2>&1; then
  exec autobuilder scaffold --intent-card "$INTENT_CARD" --out "$PROJECT_DIR"
fi

echo "scaffold: autobuilder binary not installed; falling back to template copy" >&2

if [ -e "$PROJECT_DIR" ]; then
  echo "scaffold: $PROJECT_DIR already exists; refusing to overwrite" >&2
  exit 1
fi

mkdir -p "$PROJECT_DIR"
cp -r "$SKILL_DIR/templates/scaffold/." "$PROJECT_DIR/"
mkdir -p "$PROJECT_DIR/agent"
cp "$INTENT_CARD" "$PROJECT_DIR/agent/intent-card.json"

# Naive substitution of {{intent_slug}} and {{target_kind}} in template files.
INTENT_SLUG=$(jq -r '.intent_slug // "untitled"' "$INTENT_CARD")
TARGET_KIND=$(jq -r '.hard_constraints.target_kind // "cli"' "$INTENT_CARD")

find "$PROJECT_DIR" -type f \( -name '*.toml' -o -name '*.md' -o -name '*.rs' -o -name '*.sh' -o -name '*.yml' \) -print0 \
  | xargs -0 sed -i "s/{{intent_slug}}/$INTENT_SLUG/g; s/{{target_kind}}/$TARGET_KIND/g"

echo "scaffold: project materialized at $PROJECT_DIR (intent_slug=$INTENT_SLUG, target=$TARGET_KIND)"
