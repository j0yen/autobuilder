#!/usr/bin/env bash
# agorabus-boot-handshake-install.sh — one-shot install of the hardened
# session-start hook from PRD-agorabus-boot-handshake.
#
# Usage:
#   bash proposals/agorabus-boot-handshake-install.sh [--dry-run]
#
# What it does:
#   1. Snapshots the live hook to ~/.claude/scripts/agorabus-session-start.sh.bak.<ts>
#   2. Installs proposals/agorabus-session-start.draft.sh to the live path.
#   3. Runs bash -n on the installed file to confirm syntax.
#   4. Prints the diff (live-old vs installed) for quick review.
#
# After install, test manually in one session:
#   CLAUDE_PROJECT_DIR=$PWD bash ~/.claude/scripts/agorabus-session-start.sh
#   agorabus peers | jq .
#   cat ~/.cache/agorabus/handshake/claude-*.log | jq .
#
# If anything looks wrong, restore with:
#   cp ~/.claude/scripts/agorabus-session-start.sh.bak.<ts> \
#      ~/.claude/scripts/agorabus-session-start.sh
#
# ACs satisfied by this PRD after install:
#   AC1  — cold boot, daemon present: daemon_up:ok, sub_attach:ok attempt_1,
#           worker_attach:ok attempt_1 in the handshake log.
#   AC2  — cold boot, daemon absent: hook brings up daemon, waits ≤3s,
#           peers includes $sid + ${sid}-worker.
#   AC3  — slow-daemon race: daemon_up:ok elapsed_ms ≥ 2000 logged.
#   AC4  — subscriber-lose-announce (synthetic): sub_attach:fail attempt_1,
#           re-spawn, sub_attach:ok attempt_2.
#   AC5  — persistent failure: exhaustion logged, hook exits 0.
#   AC6  — idempotent re-run: already_attached:ok markers, no dup subscribers.
#   AC7  — log rotation: >14-day logs pruned on each run. [VERIFIED]
#   AC8  — no regression under load: run with stress-ng, verify peers after.
#   AC9  — replayable from journal: verbatim 2026-05-25 §Notable in header. [VERIFIED]
#   AC10 — end-to-end: reboot + observe clean peers within 5s. (user-verify)

set -euo pipefail

DRAFT="$(cd "$(dirname "$0")" && pwd)/agorabus-session-start.draft.sh"
LIVE="$HOME/.claude/scripts/agorabus-session-start.sh"
DRY=0

if [ "${1:-}" = "--dry-run" ]; then
    DRY=1
fi

if [ ! -f "$DRAFT" ]; then
    printf 'ERROR: draft not found at %s\n' "$DRAFT" >&2
    exit 1
fi

if [ ! -f "$LIVE" ]; then
    printf 'ERROR: live hook not found at %s\n' "$LIVE" >&2
    exit 1
fi

# The live path is a symlink into ~/wintermute/dotfiles (the dotfiles repo is
# the single source of truth). Resolve it so we write the real file and keep
# the symlink intact — a naive `install` over the symlink would replace it
# with a regular file and silently desync the dotfiles repo.
TARGET="$(readlink -f "$LIVE")"

bash -n "$DRAFT" || { printf 'ERROR: draft failed bash -n\n' >&2; exit 1; }

TS=$(date +%s)
BAK="${TARGET}.bak.${TS}"

printf '%s\n' '--- agorabus-boot-handshake-install ---'
printf 'draft:  %s\n' "$DRAFT"
printf 'live:   %s\n' "$LIVE"
printf 'target: %s\n' "$TARGET"
printf 'backup: %s\n' "$BAK"
printf '\n'

printf '=== diff (target → draft) ===\n'
diff "$TARGET" "$DRAFT" || true
printf '=== /diff ===\n\n'

if [ "$DRY" -eq 1 ]; then
    printf '[dry-run] no files written.\n'
    exit 0
fi

cp "$TARGET" "$BAK"
printf 'backup written: %s\n' "$BAK"

# Write into the real file (preserves the symlink at $LIVE).
install -m755 "$DRAFT" "$TARGET"
printf 'installed: %s (via %s)\n' "$TARGET" "$LIVE"

bash -n "$LIVE" && printf 'bash -n: OK\n'

printf '\nInstall complete. Test with:\n'
printf '  CLAUDE_PROJECT_DIR=$PWD bash %s\n' "$LIVE"
printf '  agorabus peers | jq .\n'
printf '  cat ~/.cache/agorabus/handshake/*.log | jq .\n'
