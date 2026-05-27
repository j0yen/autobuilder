#!/bin/bash
# wm-publish — narrow wrapper around `gh repo create j0yen/<slug>`.
#
# /build reference list — keep in sync with ~/wintermute/REPOS.md.
# Slug must match the regex AND (be in ALLOW or match wintermute-*).
#
# This wrapper is the substantive safety boundary; the corresponding
# settings.json allow rule `Bash(wm-publish:*)` just lets it run without
# re-prompting. Audit this whole file in one read before extending.
#
# Per PRD-build-publish-allowlist.md (archived after this lands).

set -uo pipefail

# --- arg parse ------------------------------------------------------------
slug=
description=
source="$PWD"

while [ $# -gt 0 ]; do
  case "$1" in
    --slug)        slug="${2:-}"; shift 2;;
    --description) description="${2:-}"; shift 2;;
    --source)      source="${2:-}"; shift 2;;
    -h|--help)
      sed -n '2,12p' "$0" >&2
      echo "" >&2
      echo "usage: wm-publish --slug <s> --description \"<d>\" [--source <path>]" >&2
      exit 0;;
    *)
      echo "wm-publish: unknown flag: $1" >&2
      exit 2;;
  esac
done

if [ -z "$slug" ] || [ -z "$description" ]; then
  echo "wm-publish: --slug and --description are required" >&2
  exit 2
fi

# --- slug regex -----------------------------------------------------------
if ! [[ "$slug" =~ ^[a-z][a-z0-9-]{1,40}$ ]]; then
  echo "wm-publish: slug '$slug' fails regex ^[a-z][a-z0-9-]{1,40}\$" >&2
  exit 2
fi

# --- allow-list (j0yen org only) ------------------------------------------
# Explicit allow-list of slugs that wm-publish may create. Anything outside
# this set (and not matching wintermute-*) is rejected. Keep in sync with
# REPOS.md as the build skill mints new repos.
ALLOW=(
  # wintermute fleet (also covered by wintermute-* glob below)
  wintermute-bootstrap
  wintermute-platform
  wintermute-tts
  wintermute-stt
  wintermute-audio
  wintermute-brain
  wintermute-dialog
  wintermute-kernel
  # PRD-named primitives
  recall
  agorabus
  episodic-observer
  baton
  agentsh
  agentns
  memlog
  provfs
  learning-db
  # already-shipped names from REPOS.md (defensive — wm-publish refuses
  # re-publish anyway via the origin-exists check, but keeping the names
  # here makes the allow story explicit)
  autobuilder
  autobuilder-metric-harness
  agent-pipe
  ambient
  claude-self
  confidant
  conversations-zine
  daily-receipt
  fsstory
  letters-we-never-sent
  mcp-autotuner
  memory-reliquary
  mirror
  morsel-bake
  recall-doctor
  recall-io
  recall-memory-linter
  recall-ops
  repo-as-landscape
  self-portrait
  session-index
  session-trace-receipt
  skill-manifest
  skill-telemetry
  tide-chart
)

allowed=0
if [[ "$slug" == wintermute-* ]]; then
  allowed=1
else
  for s in "${ALLOW[@]}"; do
    if [ "$s" = "$slug" ]; then allowed=1; break; fi
  done
fi
if [ "$allowed" -ne 1 ]; then
  echo "wm-publish: slug '$slug' is not in the allow-list (see ~/.local/bin/wm-publish)" >&2
  exit 2
fi

# --- source must be a git repo with ≥1 commit, no existing origin ---------
if [ ! -d "$source" ]; then
  echo "wm-publish: --source path '$source' is not a directory" >&2
  exit 2
fi
if [ ! -d "$source/.git" ] && ! git -C "$source" rev-parse --git-dir >/dev/null 2>&1; then
  echo "wm-publish: '$source' is not a git repository" >&2
  exit 2
fi
if ! git -C "$source" rev-parse HEAD >/dev/null 2>&1; then
  echo "wm-publish: '$source' has no commits yet" >&2
  exit 2
fi
if git -C "$source" remote get-url origin >/dev/null 2>&1; then
  echo "wm-publish: repo already published (origin exists)" >&2
  exit 2
fi

# --- ship -----------------------------------------------------------------
exec gh repo create "j0yen/$slug" \
  --public \
  --source="$source" \
  --remote=origin \
  --push \
  --description="$description"
