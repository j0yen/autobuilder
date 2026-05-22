#!/usr/bin/env bash
# install.sh — symlink ~/.claude/skills/autobuilder → this repo's skill/.
#
# Run from the repo root after pulling new commits to pick up updates to
# prompts/, rules/, schemas/, scripts/, templates/, and SKILL.md without
# manually copying. proposals/ lives inside the symlink target after
# install — git-ignored via skill/.gitignore so runtime state never lands
# in commits.

set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
TARGET="$HOME/.claude/skills/autobuilder"

# Rescue existing proposals/ (runtime state) before we relink.
RESCUE_PROPOSALS=""
if [ -d "$TARGET/proposals" ] && [ ! -L "$TARGET" ]; then
  RESCUE_PROPOSALS=$(mktemp -d)
  cp -a "$TARGET/proposals/." "$RESCUE_PROPOSALS/"
  echo "rescued existing proposals → $RESCUE_PROPOSALS"
fi

# If the target is a real directory (not a symlink), back it up before linking.
if [ -e "$TARGET" ] && [ ! -L "$TARGET" ]; then
  BACKUP="$TARGET.backup.$(date -u +%Y%m%dT%H%M%SZ)"
  mv "$TARGET" "$BACKUP"
  echo "backed up existing skill → $BACKUP"
fi

mkdir -p "$(dirname "$TARGET")"
ln -snf "$SCRIPT_DIR" "$TARGET"
echo "symlinked $TARGET → $SCRIPT_DIR"

# Re-plant proposals/ inside the now-linked dir (git-ignored).
mkdir -p "$TARGET/proposals"
if [ -n "$RESCUE_PROPOSALS" ]; then
  cp -a "$RESCUE_PROPOSALS/." "$TARGET/proposals/"
  rm -rf "$RESCUE_PROPOSALS"
  echo "restored proposals → $TARGET/proposals"
fi

echo "done."
