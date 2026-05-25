#!/usr/bin/env bash
# install.sh — install the autobuilder Claude Code skill.
#
# Two modes:
#   1. Repo-local: invoked as `./skill/install.sh` from a checkout of
#      j0yen/autobuilder. Symlinks ~/.claude/skills/autobuilder/ to
#      this script's parent dir.
#   2. Curl-piped: invoked as `curl ... | bash`. No checkout exists;
#      script self-clones the repo into ~/.local/share/autobuilder/
#      via sparse-checkout (skill/ only by default; full clone with
#      AUTOBUILDER_FULL=1), then runs mode 1 against that clone.
#
# proposals/ inside the skill is runtime state and stays git-ignored;
# install.sh rescues any existing proposals/ across re-installs.

set -euo pipefail

TARGET="$HOME/.claude/skills/autobuilder"

# --- Mode detection ---------------------------------------------------
# If $0 resolves to a real file inside a checkout of j0yen/autobuilder,
# we're in mode 1. Otherwise (e.g. piped from curl), we're in mode 2
# and need to self-clone.
SCRIPT_PATH="${BASH_SOURCE[0]:-$0}"
SCRIPT_DIR=""
if [ -f "$SCRIPT_PATH" ]; then
  SCRIPT_DIR=$(cd "$(dirname "$SCRIPT_PATH")" && pwd)
fi

if [ -z "$SCRIPT_DIR" ] || [ ! -f "$SCRIPT_DIR/SKILL.md" ]; then
  # Mode 2: curl|bash. Self-clone.
  echo "→ no local checkout detected; self-cloning j0yen/autobuilder..."
  command -v git >/dev/null 2>&1 || { echo "fatal: git not found"; exit 1; }

  CLONE_ROOT="${AUTOBUILDER_CLONE_ROOT:-$HOME/.local/share/autobuilder}"
  mkdir -p "$(dirname "$CLONE_ROOT")"

  if [ -d "$CLONE_ROOT/.git" ]; then
    echo "→ existing clone at $CLONE_ROOT — refreshing"
    git -C "$CLONE_ROOT" fetch --depth 1 origin main
    git -C "$CLONE_ROOT" reset --hard origin/main
  elif [ "${AUTOBUILDER_FULL:-0}" = "1" ]; then
    echo "→ full clone into $CLONE_ROOT (AUTOBUILDER_FULL=1)"
    git clone --depth 1 https://github.com/j0yen/autobuilder.git "$CLONE_ROOT"
  else
    echo "→ sparse clone (skill/ + LICENSE) into $CLONE_ROOT"
    git clone --depth 1 --filter=blob:none --sparse \
      https://github.com/j0yen/autobuilder.git "$CLONE_ROOT"
    git -C "$CLONE_ROOT" sparse-checkout set skill LICENSE-MIT LICENSE-APACHE README.md
  fi

  SCRIPT_DIR="$CLONE_ROOT/skill"
fi

# --- Mode 1: symlink the skill into ~/.claude/skills/ ----------------

# Rescue existing proposals/ (runtime state) before relinking.
RESCUE_PROPOSALS=""
if [ -d "$TARGET/proposals" ] && [ ! -L "$TARGET" ]; then
  RESCUE_PROPOSALS=$(mktemp -d)
  cp -a "$TARGET/proposals/." "$RESCUE_PROPOSALS/"
  echo "→ rescued existing proposals → $RESCUE_PROPOSALS"
fi

# If the target is a real directory (not a symlink), back it up.
if [ -e "$TARGET" ] && [ ! -L "$TARGET" ]; then
  BACKUP="$TARGET.backup.$(date -u +%Y%m%dT%H%M%SZ)"
  mv "$TARGET" "$BACKUP"
  echo "→ backed up existing skill → $BACKUP"
fi

mkdir -p "$(dirname "$TARGET")"
ln -snf "$SCRIPT_DIR" "$TARGET"
echo "→ symlinked $TARGET → $SCRIPT_DIR"

# Re-plant proposals/ inside the now-linked dir (git-ignored).
mkdir -p "$TARGET/proposals"
if [ -n "$RESCUE_PROPOSALS" ]; then
  cp -a "$RESCUE_PROPOSALS/." "$TARGET/proposals/"
  rm -rf "$RESCUE_PROPOSALS"
  echo "→ restored proposals → $TARGET/proposals"
fi

echo
echo "✓ autobuilder skill installed."
echo
echo "Next steps:"
echo "  1. Open Claude Code; on next session start, /autobuilder is available."
echo "  2. For Stages 3-5, build the companion binary:"
echo "     cd \"$(dirname "$SCRIPT_DIR")\" && cargo install --path autobuilder"
