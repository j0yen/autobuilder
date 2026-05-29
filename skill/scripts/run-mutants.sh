#!/usr/bin/env bash
# run-mutants.sh — cargo-mutants integration for the autobuilder Stage 3 gate.
#
# PRD: autobuilder-mutation-testing (Phase 1 — telemetry only, no hard gate).
#
# Runs cargo-mutants in the given crate root, parses the result, merges four
# mutation fields into target/autobuilder/metrics.json, and writes a forensic
# receipt at target/autobuilder/mutants-receipt.json.
#
# Phase 1 contract (AC8): exit 0 on success even when kill_rate < 0.60.
# Non-zero exit ONLY on infrastructure failure (can't install cargo-mutants,
# cargo-mutants crashed AND produced no parseable output, or can't write the
# receipt). A low kill rate is data, not a failure.
#
# Usage:
#   run-mutants.sh <crate_dir>
#
# Env:
#   AUTOBUILDER_MUTANTS_NO_CACHE=1   disable the src+tests+Cargo.toml cache
#   AUTOBUILDER_MUTANTS_TIMEOUT=1800 overall wall-time cap in seconds (default 1800 = 30m)
#
# A file named `.no-mutation-cache` in the crate root also disables the cache.

set -uo pipefail

# cargo installs binaries to CARGO_HOME/bin (default ~/.cargo/bin). That dir is
# on an interactive shell's PATH but NOT necessarily on the autobuilder/build-tick
# environment's PATH, so a freshly `cargo install`-ed cargo-mutants would be
# invisible to `command -v`. Prepend the cargo bin dir defensively.
CARGO_BIN="${CARGO_HOME:-$HOME/.cargo}/bin"
case ":$PATH:" in
  *":$CARGO_BIN:"*) : ;;
  *) [ -d "$CARGO_BIN" ] && PATH="$CARGO_BIN:$PATH" && export PATH ;;
esac

CRATE_DIR="${1:?usage: run-mutants.sh <crate_dir>}"
cd "$CRATE_DIR" || { echo "run-mutants: cannot cd to $CRATE_DIR" >&2; exit 1; }

OUTDIR=target/autobuilder
METRICS="$OUTDIR/metrics.json"
RECEIPT="$OUTDIR/mutants-receipt.json"
CACHE_DIR="$OUTDIR/mutants-cache"
LOG="$OUTDIR/mutants.log"
WALL_CAP="${AUTOBUILDER_MUTANTS_TIMEOUT:-1800}"
mkdir -p "$OUTDIR" || { echo "run-mutants: cannot mkdir $OUTDIR" >&2; exit 1; }
: > "$LOG"

# --- 1. Ensure cargo-mutants is installed (once per autobuilder install) ---
if ! command -v cargo-mutants >/dev/null 2>&1; then
  echo "run-mutants: cargo-mutants not found; installing (cargo install cargo-mutants --locked)" | tee -a "$LOG"
  if ! cargo install cargo-mutants --locked >>"$LOG" 2>&1; then
    echo "run-mutants: FAILED to install cargo-mutants; see $LOG" >&2
    exit 1
  fi
  # The install dir may not have existed (hence not added to PATH above); add now.
  case ":$PATH:" in
    *":$CARGO_BIN:"*) : ;;
    *) [ -d "$CARGO_BIN" ] && PATH="$CARGO_BIN:$PATH" && export PATH ;;
  esac
fi
if ! command -v cargo-mutants >/dev/null 2>&1; then
  echo "run-mutants: cargo-mutants still not on PATH after install" >&2
  exit 1
fi

# --- 2. Cache key over src/ + tests/ + Cargo.toml ---
cache_enabled=1
[ "${AUTOBUILDER_MUTANTS_NO_CACHE:-0}" = "1" ] && cache_enabled=0
[ -f .no-mutation-cache ] && cache_enabled=0

cache_key=""
if [ "$cache_enabled" = "1" ]; then
  # Hash file contents deterministically (sorted path list piped through sha256sum).
  cache_key=$(
    { find src tests Cargo.toml -type f \( -name '*.rs' -o -name 'Cargo.toml' \) 2>/dev/null \
        | LC_ALL=C sort \
        | while IFS= read -r f; do sha256sum "$f"; done; } | sha256sum | awk '{print $1}'
  )
fi

# cargo-mutants (>=24) writes its JSON under <--output>/mutants.out/; older
# layouts dropped it directly under <--output>. We resolve the real path from
# a candidate list AFTER the run (see "Resolve the outcomes file" below) rather
# than assuming one location. CACHE_DEST is the fixed path a cache-hit restores
# the prior outcomes to (and which the resolution list always includes first).
MUTANTS_JSON=""
CACHE_DEST="$OUTDIR/mutants/outcomes.json"
start=$(date +%s)
used_cache=0

# --- merge_metrics_null: merge null mutation fields (used on timeout) ---
merge_metrics_null() {
  [ -f "$METRICS" ] || return 0
  local tmp; tmp=$(mktemp "$OUTDIR/.metrics.XXXXXX")
  jq '. + {mutants_total:null, mutants_killed_count:null,
           mutants_alive_count:null, mutation_kill_rate:null}' \
     "$METRICS" > "$tmp" && mv "$tmp" "$METRICS"
}

if [ "$cache_enabled" = "1" ] && [ -n "$cache_key" ] && [ -f "$CACHE_DIR/$cache_key.json" ]; then
  echo "run-mutants: cache hit ($cache_key); reusing prior outcomes" | tee -a "$LOG"
  mkdir -p "$OUTDIR/mutants"
  cp "$CACHE_DIR/$cache_key.json" "$CACHE_DEST"
  used_cache=1
else
  # --- 3. Run cargo-mutants ---
  echo "run-mutants: running cargo mutants (cap ${WALL_CAP}s, jobs $(nproc))" | tee -a "$LOG"
  verdict_timeout=0
  # cargo-mutants exit codes are version-dependent and DO NOT cleanly separate
  # "ran but found survivors" from "could not run". Empirically (cargo-mutants
  # 27.0.0): 0=all mutants caught, 1=baseline/build broken, 2=some mutants
  # MISSED (survived), 124=our timeout wrapper. Crucially exit 2 — a surviving
  # mutant — is the NORMAL telemetry case this PRD exists to measure, NOT a
  # failure. So we do NOT decide success from the exit code. Instead we run,
  # remember rc only for the timeout case, and below treat "no parseable
  # outcomes file was produced" (resolved from the candidate list) as the real
  # infrastructure failure. This is robust across cargo-mutants versions.
  #
  # NOTE: cargo-mutants forbids --in-place with --jobs (parallel mutation needs
  # isolated copied trees; in-place mutates the single source tree serially). We
  # want the parallelism (PRD asks for --jobs $(nproc)), so we use the default
  # copy-tree mode (no --in-place) which is cargo-mutants' standard parallel path.
  timeout "${WALL_CAP}s" \
    cargo mutants --no-shuffle --jobs "$(nproc)" \
      --output "$OUTDIR/mutants" >>"$LOG" 2>&1
  rc=$?
  if [ "$rc" = "124" ]; then
    echo "run-mutants: wall-time cap (${WALL_CAP}s) hit; verdict=timeout, not blocking" | tee -a "$LOG"
    verdict_timeout=1
  fi
  if [ "$verdict_timeout" = "1" ]; then
    # Emit a timeout verdict and merge null mutation fields; do not block.
    write_timeout_receipt() {
      local now; now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
      jq -n --arg ts "$now" --argjson wall "$(( $(date +%s) - start ))" \
        '{schema:"autobuilder.mutants-receipt.v1", verdict:"timeout",
          generated_at:$ts, mutation_wall_seconds:$wall, outcomes:[]}' > "$RECEIPT"
    }
    write_timeout_receipt || { echo "run-mutants: cannot write timeout receipt" >&2; exit 1; }
    merge_metrics_null
    exit 0
  fi
fi

# Resolve the outcomes file cargo-mutants produced. Candidate order:
#   CACHE_DEST                              — populated only on a cache hit
#   <output>/mutants.out/outcomes.json      — cargo-mutants >=24 layout
#   <output>/mutants.out/mutants.json       — older >=24 alias
#   <output>/outcomes.json, <output>/mutants.json — flat/legacy layout
for cand in \
    "$CACHE_DEST" \
    "$OUTDIR/mutants/mutants.out/outcomes.json" \
    "$OUTDIR/mutants/mutants.out/mutants.json" \
    "$OUTDIR/mutants/outcomes.json" \
    "$OUTDIR/mutants/mutants.json"; do
  [ -f "$cand" ] && MUTANTS_JSON="$cand" && break
done

# Infrastructure-failure guard (replaces the old, version-fragile exit-code
# check): cargo-mutants ran to completion only if it produced a parseable
# outcomes file. If none of the candidates exist, cargo-mutants could not run
# the mutation set (build broken / crashed / bad usage) — surface as a non-zero
# infra failure rather than silently scoring 0 caught. rc is shown for context.
if [ -z "$MUTANTS_JSON" ] || [ ! -f "$MUTANTS_JSON" ]; then
  echo "run-mutants: cargo-mutants produced no outcomes file (rc=${rc:-?}); treating as infrastructure failure; see $LOG" >&2
  exit 1
fi

wall=$(( $(date +%s) - start ))

# --- 4. Parse outcomes ---
# cargo-mutants JSON shapes vary by version. Two known forms:
#   (a) summary object: { "caught": N, "missed": N, "unviable": N, "timeout": N }
#   (b) outcomes array : [ { "summary": "CaughtMutant"|"MissedMutant"|... }, ... ]
caught=0; missed=0; unviable=0; mtimeout=0
if [ -f "$MUTANTS_JSON" ]; then
  if jq -e 'type=="object" and (has("caught") or has("missed"))' "$MUTANTS_JSON" >/dev/null 2>&1; then
    caught=$(jq -r '.caught // 0' "$MUTANTS_JSON")
    missed=$(jq -r '.missed // 0' "$MUTANTS_JSON")
    unviable=$(jq -r '.unviable // 0' "$MUTANTS_JSON")
    mtimeout=$(jq -r '.timeout // 0' "$MUTANTS_JSON")
  elif jq -e 'type=="array"' "$MUTANTS_JSON" >/dev/null 2>&1; then
    caught=$(jq -r '[.[]|select(.summary=="CaughtMutant" or .summary=="Caught")]|length' "$MUTANTS_JSON")
    missed=$(jq -r '[.[]|select(.summary=="MissedMutant" or .summary=="Missed")]|length' "$MUTANTS_JSON")
    unviable=$(jq -r '[.[]|select(.summary=="Unviable")]|length' "$MUTANTS_JSON")
    mtimeout=$(jq -r '[.[]|select(.summary=="Timeout")]|length' "$MUTANTS_JSON")
  elif jq -e 'has("outcomes")' "$MUTANTS_JSON" >/dev/null 2>&1; then
    caught=$(jq -r '[.outcomes[]|select(.summary=="CaughtMutant" or .summary=="Caught")]|length' "$MUTANTS_JSON")
    missed=$(jq -r '[.outcomes[]|select(.summary=="MissedMutant" or .summary=="Missed")]|length' "$MUTANTS_JSON")
    unviable=$(jq -r '[.outcomes[]|select(.summary=="Unviable")]|length' "$MUTANTS_JSON")
    mtimeout=$(jq -r '[.outcomes[]|select(.summary=="Timeout")]|length' "$MUTANTS_JSON")
  fi
fi

total=$(( caught + missed ))
if [ "$total" -gt 0 ]; then
  kill_rate=$(awk "BEGIN{printf \"%.4f\", $caught/$total}")
else
  kill_rate="null"
fi

# --- Cache store (only for a fresh run that produced outcomes) ---
if [ "$used_cache" = "0" ] && [ "$cache_enabled" = "1" ] && [ -n "$cache_key" ] && [ -f "$MUTANTS_JSON" ]; then
  mkdir -p "$CACHE_DIR"
  cp "$MUTANTS_JSON" "$CACHE_DIR/$cache_key.json"
fi

# --- 5. Forensic receipt ---
now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
if [ -f "$MUTANTS_JSON" ]; then
  detail=$(jq -c '.' "$MUTANTS_JSON" 2>/dev/null || echo 'null')
else
  detail='null'
fi
tmp_r=$(mktemp "$OUTDIR/.receipt.XXXXXX")
jq -n \
  --arg ts "$now" \
  --argjson caught "$caught" --argjson missed "$missed" \
  --argjson unviable "$unviable" --argjson mtimeout "$mtimeout" \
  --argjson total "$total" \
  --arg kr "$kill_rate" \
  --argjson wall "$wall" \
  --argjson cached "$used_cache" \
  --argjson detail "$detail" \
  '{schema:"autobuilder.mutants-receipt.v1",
    verdict:"complete",
    generated_at:$ts,
    cached:($cached==1),
    mutants_total:$total,
    mutants_killed_count:$caught,
    mutants_alive_count:$missed,
    mutants_unviable_count:$unviable,
    mutants_timeout_count:$mtimeout,
    mutation_kill_rate:(if $kr=="null" then null else ($kr|tonumber) end),
    mutation_wall_seconds:$wall,
    detail:$detail}' > "$tmp_r" || { echo "run-mutants: cannot write receipt" >&2; exit 1; }
mv "$tmp_r" "$RECEIPT"

# --- AC4: merge the four fields into metrics.json; existing fields untouched ---
if [ -f "$METRICS" ]; then
  tmp_m=$(mktemp "$OUTDIR/.metrics.XXXXXX")
  jq \
    --argjson total "$total" \
    --argjson caught "$caught" \
    --argjson missed "$missed" \
    --arg kr "$kill_rate" \
    --argjson wall "$wall" \
    '. + {
       mutants_total:$total,
       mutants_killed_count:$caught,
       mutants_alive_count:$missed,
       mutation_kill_rate:(if $kr=="null" then null else ($kr|tonumber) end),
       mutation_wall_seconds:$wall
     }' "$METRICS" > "$tmp_m" \
    && mv "$tmp_m" "$METRICS" \
    || echo "run-mutants: WARN could not merge into $METRICS (non-fatal, telemetry only)" >&2
fi

echo "run-mutants: caught=$caught missed=$missed kill_rate=$kill_rate wall=${wall}s cached=$used_cache" | tee -a "$LOG"
# Phase 1: telemetry only. Always exit 0 on a successful run regardless of kill_rate.
exit 0
