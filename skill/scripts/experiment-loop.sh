#!/usr/bin/env bash
# experiment-loop.sh — Stage 3 driver. Runs the 9-step LOOP until budget or
# all-MUST-ACs-green AND risk-gate-pass.
#
# Usage:
#   experiment-loop.sh <project_dir> [--max-iterations N] [--budget-minutes N]
#
# Defers to `autobuilder loop` when the binary is installed; otherwise refuses
# (the loop is too load-bearing to ship a shell-script fallback).

set -euo pipefail

PROJECT_DIR="${1:?usage: experiment-loop.sh <project_dir> [...]}"
shift
cd "$PROJECT_DIR"

if ! command -v autobuilder >/dev/null 2>&1; then
  cat >&2 <<'EOF'
experiment-loop: the autobuilder binary is not installed.

The Stage 3 LOOP runs against the metric harness, writes EvidencePack receipts,
makes advance/revert decisions, and emits FailureCapsules on crash. These are
load-bearing for the trust model — running a shell-script fallback would
silently degrade the receipts.

Install the binary:
    cargo install --path /home/jsy/projects/autobuilder/autobuilder

Then re-run.
EOF
  exit 2
fi

exec autobuilder loop "$@"
