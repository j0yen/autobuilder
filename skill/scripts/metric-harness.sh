#!/usr/bin/env bash
# metric-harness.sh — wrapper around the per-project scripts/run-metrics.sh.
#
# Normalizes the metrics.json output and writes target/autobuilder/metrics.json.
# Used by the Stage 3 LOOP to obtain the unfakeable scalar.
#
# Usage:
#   metric-harness.sh <project_dir>
#
# Defers to `autobuilder metric-harness` (the v1 dogfood target) when available.

set -euo pipefail

PROJECT_DIR="${1:?usage: metric-harness.sh <project_dir>}"
cd "$PROJECT_DIR"

if [ ! -x scripts/run-metrics.sh ]; then
  echo "metric-harness: scripts/run-metrics.sh missing or not executable" >&2
  exit 1
fi

mkdir -p target/autobuilder

if command -v autobuilder >/dev/null 2>&1; then
  exec autobuilder metric-harness --project "$PROJECT_DIR"
fi

# Fallback: directly invoke the project harness and capture output.
LOG=target/autobuilder/run.log
METRICS=target/autobuilder/metrics.json

set +e
scripts/run-metrics.sh > "$LOG" 2>&1
RC=$?
set -e

if [ $RC -ne 0 ]; then
  echo "metric-harness: run-metrics.sh exited $RC; see $LOG" >&2
  exit $RC
fi

if [ ! -f "$METRICS" ]; then
  echo "metric-harness: run-metrics.sh succeeded but produced no $METRICS" >&2
  exit 1
fi

# Validate it's at least parseable JSON.
if ! jq empty "$METRICS" 2>/dev/null; then
  echo "metric-harness: $METRICS is not valid JSON" >&2
  exit 1
fi

cat "$METRICS"
