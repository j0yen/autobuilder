#!/usr/bin/env bash
# set-rollback-model.sh — idempotently stamp `rollback_model` onto a repo's
# `agent/intent-card.json` (PRD-rollback-redeploy-tag-onboard AC5).
#
# `resolve_rollback_model` (autobuilder/src/rollback.rs) already reads an
# explicit `rollback_model` key from `agent/intent-card.json`; the gap this
# script closes is that nothing writes that key except a hand edit. A
# design choice worth naming: this stamps the key onto an EXISTING card
# rather than folding into `~/.claude/skills/build/scripts/
# intent-card-refresh.sh` (the PRD's other suggested location) — that
# script lives outside this repo, is shared live build-loop infra, and
# regenerates the whole card from a PRD (a bigger, riskier surface to
# extend mid-tick). A small standalone script scoped to one key is safer
# to land and easier to call from either intent-card-refresh.sh or by hand
# later, if consolidation ever makes sense.
#
# Usage: set-rollback-model.sh <repo> [<model>]
#   <model> defaults to "redeploy-tag"; the only other accepted value is
#   "revert-commits" (the two values RollbackModel::parse in rollback.rs
#   accepts). <repo>/agent/intent-card.json must already exist — this
#   script sets one key on an existing card; minting a whole new card from
#   a PRD is intent-card-refresh.sh's job, not this one's.
#
# Idempotent by content, not just by outcome: when the card already carries
# the requested rollback_model value, NOTHING is written (no temp file, no
# mtime bump) — a second run is a true no-op, not a no-op-shaped rewrite.
# Every other key in the card is passed through unmodified.
#
# Exit: 0 ok (written or already-set) | 2 usage/missing jq | 3 invalid model
#       | 4 repo or card not found
set -uo pipefail

usage() { echo "usage: set-rollback-model.sh <repo> [<model>]" >&2; }

repo="${1:-}"
model="${2:-redeploy-tag}"
[ -n "$repo" ] || { usage; exit 2; }

case "$model" in
  redeploy-tag | revert-commits) ;;
  *)
    echo "set-rollback-model: invalid model '$model' (expected redeploy-tag or revert-commits)" >&2
    exit 3
    ;;
esac

command -v jq >/dev/null 2>&1 || { echo "set-rollback-model: jq not found" >&2; exit 2; }

[ -d "$repo" ] || { echo "set-rollback-model: repo not found: $repo" >&2; exit 4; }

card_path="$repo/agent/intent-card.json"
[ -f "$card_path" ] || { echo "set-rollback-model: no intent-card.json at $card_path" >&2; exit 4; }

current=$(jq -r '.rollback_model // empty' "$card_path") || {
  echo "set-rollback-model: could not parse $card_path as JSON" >&2
  exit 4
}

if [ "$current" = "$model" ]; then
  echo "set-rollback-model: $card_path already rollback_model=$model (no change)"
  exit 0
fi

tmp="$card_path.tmp.$$"
jq --arg m "$model" '.rollback_model = $m' "$card_path" > "$tmp"
mv "$tmp" "$card_path"
echo "set-rollback-model: $card_path rollback_model -> $model"
