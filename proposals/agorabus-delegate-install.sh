#!/usr/bin/env bash
# agorabus-delegate-install.sh — one-shot, reviewable install of the async
# delegation surface from PRD-chord-async-delegate.
#
# This is the SINGLE user-gated step for PRD-chord-async-delegate. Everything
# else (worker draft, runner scaffold, AGORABUS_RPC.md v0.2 docs, 27/27
# offline smoke-test) already shipped in prior /build ticks. This helper
# installs the live worker — which is on Claude's SessionStart spawn path —
# so it MUST be reviewed + run in one supervised session, per the
# critical-startup-path doctrine.
#
# Usage:
#   bash proposals/agorabus-delegate-install.sh --dry-run   # show plan, write nothing
#   bash proposals/agorabus-delegate-install.sh             # install + self-check
#
# What it does (in order):
#   1. bash -n both the worker draft and the runner (refuse to install broken).
#   2. Track the runner in the dotfiles repo if it isn't already:
#      the live runner is currently a plain file in ~/.claude/scripts/ and is
#      NOT in the dotfiles repo, yet the worker draft references it by that
#      live path. To keep the dotfiles repo the single source of truth, copy
#      the runner into the repo and replace the live file with a symlink
#      (matching how agorabus-worker.sh is already wired). Snapshot first.
#   3. Snapshot the live (symlinked-into-dotfiles) worker, then install the
#      441-LOC async draft over the real dotfiles file (preserving the symlink).
#   4. Re-run bash -n on both installed files.
#   5. Restart the worker for the current session so the new methods take
#      effect: kill the old worker; SessionStart re-spawns it on next hook,
#      or it is re-spawned here if a session_id is derivable.
#   6. Scriptable self-check: METHODS now advertises the 5 async methods;
#      print the manual AC1-10 verification recipe for the live bus.
#
# Rollback (if anything looks wrong):
#   cp <printed worker backup>  ~/wintermute/dotfiles/.claude/scripts/agorabus-worker.sh
#   cp <printed runner backup>  ~/.claude/scripts/agorabus-delegate-runner.sh   # if step 2 ran
#
# ACs (verifiable against the live bus AFTER this install — see recipe at end):
#   AC1  start returns <500ms, worker loop stays free (concurrent ping <100ms).
#   AC2  poll: running -> done across a 60s-sleep delegation.
#   AC3  result carries stdout + exit_code + duration_ms.
#   AC4  delegate.progress.<ticket> fires start + done events.
#   AC5  cancel: SIGTERM child, state->cancelled, result event published.
#   AC6  delegate.run back-compat envelope unchanged (5s prompt).
#   AC7  no head-of-line: concurrent ping from another sid replies <200ms.
#   AC8  --ttl 5 vs 60s sleep -> state:timeout at ~T+5s, SIGTERM then SIGKILL.
#   AC9  AGORABUS_DELEGATE_DEPTH>0 recursion guard preserved.
#   AC10 cleanup removes ticket files; 24h prune of terminal tickets on poll.

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
WORKER_DRAFT="$HERE/agorabus-worker.draft.sh"
RUNNER_LIVE="$HOME/.claude/scripts/agorabus-delegate-runner.sh"
WORKER_LIVE="$HOME/.claude/scripts/agorabus-worker.sh"
DOTFILES_SCRIPTS="$HOME/wintermute/dotfiles/.claude/scripts"
RUNNER_REPO="$DOTFILES_SCRIPTS/agorabus-delegate-runner.sh"

DRY=0
[ "${1:-}" = "--dry-run" ] && DRY=1

die() { printf 'ERROR: %s\n' "$1" >&2; exit 1; }

[ -f "$WORKER_DRAFT" ] || die "worker draft not found: $WORKER_DRAFT"
[ -e "$RUNNER_LIVE" ]  || die "live runner not found: $RUNNER_LIVE"
[ -L "$WORKER_LIVE" ]  || die "live worker is not a symlink (expected symlink into dotfiles): $WORKER_LIVE"
[ -d "$DOTFILES_SCRIPTS" ] || die "dotfiles scripts dir not found: $DOTFILES_SCRIPTS"

WORKER_TARGET="$(readlink -f "$WORKER_LIVE")"
TS=$(date +%s)

printf '%s\n' '--- agorabus-delegate-install ---'
printf 'worker draft:   %s\n' "$WORKER_DRAFT"
printf 'worker live:    %s -> %s\n' "$WORKER_LIVE" "$WORKER_TARGET"
printf 'runner live:    %s%s\n' "$RUNNER_LIVE" "$([ -L "$RUNNER_LIVE" ] && printf ' (symlink -> %s)' "$(readlink -f "$RUNNER_LIVE")" || printf ' (plain file, NOT yet in dotfiles)')"
printf 'runner repo:    %s\n\n' "$RUNNER_REPO"

# Step 1: syntax gate.
printf '=== step 1: bash -n gate ===\n'
bash -n "$WORKER_DRAFT" || die "worker draft failed bash -n"
bash -n "$RUNNER_LIVE"  || die "runner failed bash -n"
printf 'bash -n: worker draft OK, runner OK\n\n'

# Step 2: bring the runner under dotfiles version control.
printf '=== step 2: track runner in dotfiles repo ===\n'
if [ -L "$RUNNER_LIVE" ] && [ "$(readlink -f "$RUNNER_LIVE")" = "$(readlink -f "$RUNNER_REPO")" ]; then
    printf 'runner already symlinked into dotfiles repo; nothing to do.\n\n'
else
    RUNNER_BAK="${RUNNER_LIVE}.bak.${TS}"
    printf 'plan: cp runner -> %s ; backup live -> %s ; symlink live -> repo\n' "$RUNNER_REPO" "$RUNNER_BAK"
    if [ "$DRY" -eq 0 ]; then
        cp "$RUNNER_LIVE" "$RUNNER_BAK"
        install -m755 "$RUNNER_LIVE" "$RUNNER_REPO"
        ln -sfn "$RUNNER_REPO" "$RUNNER_LIVE"
        printf 'runner now: %s -> %s (backup: %s)\n' "$RUNNER_LIVE" "$(readlink -f "$RUNNER_LIVE")" "$RUNNER_BAK"
        printf 'remember to: git -C ~/wintermute/dotfiles add .claude/scripts/agorabus-delegate-runner.sh\n'
    fi
    printf '\n'
fi

# Step 3: install the async worker draft over the dotfiles-tracked file.
printf '=== step 3: install async worker ===\n'
WORKER_BAK="${WORKER_TARGET}.bak.${TS}"
printf '=== diff (current worker -> async draft) ===\n'
diff "$WORKER_TARGET" "$WORKER_DRAFT" || true
printf '=== /diff ===\n'
if [ "$DRY" -eq 1 ]; then
    printf '\n[dry-run] no files written. Re-run without --dry-run to install.\n'
    exit 0
fi
cp "$WORKER_TARGET" "$WORKER_BAK"
install -m755 "$WORKER_DRAFT" "$WORKER_TARGET"
printf 'worker installed: %s (via %s); backup: %s\n\n' "$WORKER_TARGET" "$WORKER_LIVE" "$WORKER_BAK"

# Step 4: post-install syntax gate on the live files.
printf '=== step 4: bash -n installed live files ===\n'
bash -n "$WORKER_LIVE"  || die "installed worker failed bash -n — ROLLBACK: cp $WORKER_BAK $WORKER_TARGET"
bash -n "$RUNNER_LIVE"  || die "installed runner failed bash -n"
printf 'bash -n: OK\n\n'

# Step 5: restart the worker so new methods take effect this session.
printf '=== step 5: restart worker ===\n'
sid="${CLAUDE_SESSION_ID:-}"
if [ -n "$sid" ]; then
    pkill -f "agorabus-worker.sh $sid\$" 2>/dev/null || true
    printf 'killed worker for sid=%s; SessionStart will re-spawn, or run:\n' "$sid"
    printf '  setsid %s %s &\n' "$WORKER_LIVE" "$sid"
else
    printf 'CLAUDE_SESSION_ID unset; restart the worker by re-running the\n'
    printf 'SessionStart hook (start a fresh session) or:\n'
    printf '  pkill -f "agorabus-worker.sh <sid>$" ; setsid %s <sid> &\n' "$WORKER_LIVE"
fi
printf '\n'

# Step 6: scriptable check + manual AC recipe.
printf '=== step 6: self-check ===\n'
if grep -q 'delegate.start' "$WORKER_LIVE" && grep -q 'delegate.cleanup' "$WORKER_LIVE"; then
    printf 'METHODS advertises async surface: OK\n'
else
    die "installed worker missing async methods — ROLLBACK: cp $WORKER_BAK $WORKER_TARGET"
fi
printf '\nInstall complete. Verify AC1-10 against the live bus:\n'
printf '  # AC1 start returns fast + AC7 no head-of-line:\n'
printf '  agorabus rpc <sid>-worker delegate.start --arg cwd=$PWD --arg prompt="sleep 60; echo hi" | jq .\n'
printf '  agorabus rpc <sid>-worker ping | jq .   # must reply <100-200ms while above runs\n'
printf '  # AC2/AC3 poll then result:\n'
printf '  agorabus rpc <sid>-worker delegate.poll   --arg ticket=<ticket> | jq .\n'
printf '  agorabus rpc <sid>-worker delegate.result --arg ticket=<ticket> | jq .\n'
printf '  # AC5 cancel / AC8 ttl / AC10 cleanup:\n'
printf '  agorabus rpc <sid>-worker delegate.cancel  --arg ticket=<ticket> | jq .\n'
printf '  agorabus rpc <sid>-worker delegate.cleanup --arg ticket=<ticket> | jq .\n'
printf '  # AC6 back-compat:\n'
printf '  agorabus rpc <sid>-worker delegate.run --arg cwd=$PWD --arg prompt="echo ok" | jq .\n'
printf '\nIf any AC fails, rollback:\n'
printf '  cp %s %s\n' "$WORKER_BAK" "$WORKER_TARGET"
