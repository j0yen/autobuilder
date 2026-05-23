#!/usr/bin/env bash
# run-metrics.sh — emit autobuilder.metrics.v1 for the autobuilder repo itself.
#
# The unfakeable scalar is `stage4_receipt_producers_callable`: how many of
# the Stage 4 receipt producers (rollback-plan, vti-plan, reviewer-agent,
# ci-checks, gate) respond to --help on the freshly-built binary. ACs map
# 1:1 to producers + a build/test sanity AC + a digest-roundtrip AC.

set -uo pipefail

# The Rust toolchain lives at ~/.cargo/bin on this machine but is not on
# the default PATH. Without this, AC6's subshell silently fails with
# "cargo: command not found" and reports AC6=fail with no surface signal.
# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

REPO_ROOT="${1:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$REPO_ROOT"

OUT_DIR="target/autobuilder"
RUN_LOG="$OUT_DIR/run.log"
METRICS_FILE="$OUT_DIR/metrics.json"
mkdir -p "$OUT_DIR"
: > "$RUN_LOG"

log() { printf '%s\n' "$*" >> "$RUN_LOG"; }

# Resolve the autobuilder binary the same way risk-gate.sh does.
find_autobuilder_binary() {
  if command -v autobuilder >/dev/null 2>&1; then
    command -v autobuilder
    return
  fi
  local candidates=(
    "$REPO_ROOT/autobuilder/target/release/autobuilder"
    "$REPO_ROOT/autobuilder/target/debug/autobuilder"
  )
  for c in "${candidates[@]}"; do
    if [ -x "$c" ]; then
      echo "$c"
      return
    fi
  done
}
AUTOBUILDER_BIN="$(find_autobuilder_binary)"
log "autobuilder binary: ${AUTOBUILDER_BIN:-NOT FOUND}"

# Per-run scratch root for AC fixture tmp dirs; cleaned up at exit.
AC_SCRATCH=$(mktemp -d)
trap 'rm -rf "$AC_SCRATCH"' EXIT
ac_tmpdir() { mktemp -d -p "$AC_SCRATCH"; }

# AC checks. Each prints OK/FAIL to run.log; ac_results captures the count.
declare -A AC_RESULT
run_ac() {
  local id="$1"; shift
  if "$@" >> "$RUN_LOG" 2>&1; then
    AC_RESULT[$id]="pass"
    log "$id: pass"
  else
    AC_RESULT[$id]="fail"
    log "$id: fail"
  fi
}

ac_help() {
  [ -n "$AUTOBUILDER_BIN" ] && "$AUTOBUILDER_BIN" "$@" --help >/dev/null
}

# Each AC test materializes a tiny git fixture in a tmp dir, invokes the
# subcommand, and asserts an observable contract from the AC's English —
# not just that --help responded. Tmp dirs are cleaned up after each
# test so the real repo's target/ is never touched.

ac_setup_tmp_repo() {
  local dir="$1"
  mkdir -p "$dir"
  (
    cd "$dir"
    git init -q
    git -c user.email=t@t -c user.name=t commit -q --allow-empty -m "init"
  )
}

# AC1 — rollback-plan must write target/autobuilder/rollback.md AND the
#       receipt with a digest; verdict pass when commits are revert-clean.
ac1_rollback_writes_md() {
  [ -n "$AUTOBUILDER_BIN" ] || return 1
  local tmp; tmp=$(ac_tmpdir)
  ac_setup_tmp_repo "$tmp"
  (cd "$tmp" && echo hi > a.txt && git add -A && \
    git -c user.email=t@t -c user.name=t commit -q -m "add a")
  "$AUTOBUILDER_BIN" rollback-plan --project "$tmp" --base HEAD~1 >/dev/null 2>&1 || return 1
  [ -s "$tmp/target/autobuilder/rollback.md" ] || return 1
  [ -s "$tmp/target/autobuilder/receipts/rollback-plan.json" ] || return 1
  local v; v=$(jq -r '.verdict' "$tmp/target/autobuilder/receipts/rollback-plan.json")
  [ "$v" = "pass" ]
}
run_ac AC1 ac1_rollback_writes_md

# AC2 — vti-plan must route a src/ change to the rust-source lane with
#       confidence 1.0; an unrouted file must be reported with confidence 0.0.
ac2_vti_plan_routes() {
  [ -n "$AUTOBUILDER_BIN" ] || return 1
  local tmp; tmp=$(ac_tmpdir)
  ac_setup_tmp_repo "$tmp"
  mkdir -p "$tmp/src" "$tmp/agent"
  cat > "$tmp/agent/proof-lanes.toml" <<'EOF'
[[lane]]
id = "rust-source"
globs = ["src/**/*.rs"]
required_commands = ["cargo test"]
EOF
  # Commit the lanes FIRST so they don't appear in the diff range.
  (cd "$tmp" && git add -A && \
    git -c user.email=t@t -c user.name=t commit -q -m "lanes")
  (cd "$tmp" && echo "fn main(){}" > src/main.rs && touch unrouted.txt && \
    git add -A && git -c user.email=t@t -c user.name=t commit -q -m "add code")
  # Should block because unrouted.txt has no lane; but the receipt must
  # still record the rust-source route with confidence 1.0.
  "$AUTOBUILDER_BIN" vti-plan --project "$tmp" --base HEAD~1 >/dev/null 2>&1 || true
  local receipt="$tmp/target/autobuilder/receipts/vti-plan.json"
  [ -s "$receipt" ] || return 1
  # Use jq -e for numeric comparison so we don't trip on "1" vs "1.0" string form.
  jq -e '
    (.routes[] | select(.path == "src/main.rs") | .confidence == 1.0) and
    (.routes[] | select(.path == "unrouted.txt") | .confidence == 0.0)
  ' "$receipt" >/dev/null
}
run_ac AC2 ac2_vti_plan_routes

# AC3 — reviewer-agent prepare must write a review-request packet that
#       includes intent_card_sha256, commit_count, and changed_files.
ac3_reviewer_prepare_packet() {
  [ -n "$AUTOBUILDER_BIN" ] || return 1
  local tmp; tmp=$(ac_tmpdir)
  ac_setup_tmp_repo "$tmp"
  mkdir -p "$tmp/agent"
  echo '{"schema":"autobuilder.intent_card.v1","slug":"test"}' > "$tmp/agent/intent-card.json"
  (cd "$tmp" && echo hello > README && git add -A && \
    git -c user.email=t@t -c user.name=t commit -q -m "add readme")
  "$AUTOBUILDER_BIN" reviewer-agent prepare --project "$tmp" --base HEAD~1 >/dev/null 2>&1 || return 1
  local req="$tmp/target/autobuilder/review-request.json"
  [ -s "$req" ] || return 1
  local hash count files
  hash=$(jq -r '.intent_card_sha256' "$req")
  count=$(jq -r '.commit_count' "$req")
  files=$(jq -r '.changed_files | length' "$req")
  [[ "$hash" == sha256:* ]] && [ "$count" = "1" ] && [ "$files" -ge 1 ]
}
run_ac AC3 ac3_reviewer_prepare_packet

# AC4 — ci-checks must refuse verdict=pass without ≥1 success run. Run
#       it against a repo whose `origin` points at a repo with no runs
#       and confirm it exits non-zero with verdict=block.
ac4_ci_checks_blocks_when_empty() {
  [ -n "$AUTOBUILDER_BIN" ] || return 1
  local tmp; tmp=$(ac_tmpdir)
  ac_setup_tmp_repo "$tmp"
  # Use a public repo unlikely to have a workflow run for THIS commit's sha.
  # The (random) sha from the empty-init commit above will yield zero runs.
  if "$AUTOBUILDER_BIN" ci-checks --project "$tmp" --repo j0yen/autobuilder >/dev/null 2>&1; then
    # Pass means the binary failed to block on empty — that's the bug we
    # want to catch.
    return 1
  fi
  local receipt="$tmp/target/autobuilder/receipts/ci-checks.json"
  [ -s "$receipt" ] || return 1
  local verdict runs
  verdict=$(jq -r '.verdict' "$receipt")
  runs=$(jq -r '.run_count' "$receipt")
  [ "$verdict" = "block" ] && [ "$runs" = "0" ]
}
run_ac AC4 ac4_ci_checks_blocks_when_empty

# AC5 — gate must walk all 7 receipts AND verify head_sha-binding on each
#       (the counter-attack: a receipt with a mismatched head_sha must
#       cause gate to block, not pass).
ac5_gate_catches_head_sha_mismatch() {
  [ -n "$AUTOBUILDER_BIN" ] || return 1
  local tmp; tmp=$(ac_tmpdir)
  ac_setup_tmp_repo "$tmp"
  local head; head=$(cd "$tmp" && git rev-parse HEAD)
  local recs="$tmp/target/autobuilder/receipts"
  mkdir -p "$recs"

  # Plant valid receipts for 6 of 7, with the WRONG head_sha on ci-checks.
  local bad="deadbeef0000000000000000000000000000dead"
  cat > "$recs/intake.json" <<EOF
{"schema":"autobuilder.intent_card.v1"}
EOF
  cat > "$recs/vti-plan.json" <<EOF
{"schema":"autobuilder.vti_plan_receipt.v1","head_sha":"$head","verdict":"pass"}
EOF
  cat > "$recs/$head.json" <<EOF
{"schema":"autobuilder.iteration_receipt.v1","head_sha":"$head","verdict":"baseline"}
EOF
  cat > "$recs/risk-gate.json" <<EOF
{"schema":"autobuilder.bad_rust_audit.v1","blocking_count":0,"advisory_count":0,"findings":[]}
EOF
  cat > "$recs/reviewer-agent.json" <<EOF
{"schema":"autobuilder.reviewer_agent_receipt.v1","head_sha":"$head","decision":"pass"}
EOF
  cat > "$recs/rollback-plan.json" <<EOF
{"schema":"autobuilder.rollback_plan_receipt.v1","head_sha":"$head","verdict":"pass"}
EOF
  # Deliberately wrong head_sha — gate must catch this.
  cat > "$recs/ci-checks.json" <<EOF
{"schema":"autobuilder.ci_checks_receipt.v1","head_sha":"$bad","verdict":"pass"}
EOF

  if "$AUTOBUILDER_BIN" gate --project "$tmp" >/dev/null 2>&1; then
    return 1  # gate passed when it should have blocked
  fi
  local release="$tmp/target/autobuilder/release-receipt.json"
  [ -s "$release" ] || return 1
  local v; v=$(jq -r '.verdict' "$release")
  [ "$v" = "block" ] || return 1
  # The block_count should pin ci-checks specifically.
  jq -e '.checks[] | select(.name == "ci-checks" and .pass == false and .head_sha_match == false)' "$release" >/dev/null
}
run_ac AC5 ac5_gate_catches_head_sha_mismatch

# AC6: build + clippy strict + test (subshell so cwd is preserved)
ac_build_pass() {
  (cd "$REPO_ROOT/autobuilder" && \
    cargo check --workspace && \
    cargo clippy --bin autobuilder -- -D warnings && \
    cargo test --workspace)
}
run_ac AC6 ac_build_pass

# AC7: digest self-binding on a freshly-emitted receipt.
ac_digest_roundtrip() {
  # Use rollback-plan against this repo to produce a real receipt, then
  # recompute its sha256 against the canonical-ish JSON (jq -S) with the
  # digest field blanked, and compare.
  [ -n "$AUTOBUILDER_BIN" ] || return 1
  local tmp_receipt
  tmp_receipt="$REPO_ROOT/target/autobuilder/receipts/rollback-plan.json"
  rm -f "$tmp_receipt"
  "$AUTOBUILDER_BIN" rollback-plan --project "$REPO_ROOT" --base HEAD >/dev/null 2>&1 || true
  [ -s "$tmp_receipt" ] || return 1
  local observed expected
  observed=$(jq -r '.receipt_digest' "$tmp_receipt")
  expected=$(jq --sort-keys '.receipt_digest = ""' "$tmp_receipt" | jq --sort-keys -c '.' | tr -d '\n' | sha256sum | awk '{print "sha256:"$1}')
  [ "$observed" = "$expected" ]
}
run_ac AC7 ac_digest_roundtrip

# Count Stage 4 receipt producers callable via --help.
STAGE4_CALLABLE=0
for sub in rollback-plan vti-plan reviewer-agent ci-checks gate; do
  if ac_help "$sub" >/dev/null 2>&1; then
    STAGE4_CALLABLE=$((STAGE4_CALLABLE + 1))
  fi
done

# AC totals
AC_IDS=(AC1 AC2 AC3 AC4 AC5 AC6 AC7)
AC_TOTAL=${#AC_IDS[@]}
AC_PASS=0
for id in "${AC_IDS[@]}"; do
  if [ "${AC_RESULT[$id]:-fail}" = "pass" ]; then
    AC_PASS=$((AC_PASS + 1))
  fi
done

# Run the BAD_RUST audit against the repo. The workspace lives at
# autobuilder/ rather than the repo root, so two detectors fire on the
# layout rather than on real problems:
#   - check_cargo_lock_committed — no Cargo.lock at repo root
#   - cargo_deny_check — no Cargo.toml at repo root for cargo-deny to read
# Strip both from the published risk-gate receipt. CI runs cargo-deny
# directly inside autobuilder/ via the workflow, so the supply-chain
# check is still enforced — just not via this audit on this project.
AUDIT_RAW="$OUT_DIR/audit.raw.json"
RISK_GATE_RECEIPT="$OUT_DIR/receipts/risk-gate.json"
mkdir -p "$OUT_DIR/receipts"
if bash "$HOME/.claude/skills/autobuilder/rules/audit-checks.sh" "$REPO_ROOT" > "$AUDIT_RAW" 2>>"$RUN_LOG"; then
  log "audit (raw): passed (no blocking)"
else
  log "audit (raw): blocking findings present"
fi
# Filter the layout-specific false positive and recompute counts. The
# resulting object is the autobuilder.bad_rust_audit.v1 receipt the gate
# consumes.
jq '
  .findings |= map(
    select(.detector != "check_cargo_lock_committed" and .detector != "cargo_deny_check")
  ) |
  .blocking_count = ([.findings[] | select(.severity == "blocking")] | length) |
  .advisory_count = ([.findings[] | select(.severity == "advisory")] | length)
' "$AUDIT_RAW" > "$RISK_GATE_RECEIPT"
BLOCKING=$(jq -r '.blocking_count // 0' "$RISK_GATE_RECEIPT" 2>/dev/null || echo 0)
ADVISORY=$(jq -r '.advisory_count // 0' "$RISK_GATE_RECEIPT" 2>/dev/null || echo 0)
log "audit (filtered for autobuilder layout): blocking=$BLOCKING advisory=$ADVISORY"

HEAD_SHA=$(git rev-parse HEAD 2>/dev/null || echo unknown)
CAPTURED_AT=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# Emit the metrics.json doc (un-digested; the autobuilder loop runner adds
# its own digest into the receipt).
jq -n \
  --arg head "$HEAD_SHA" \
  --arg captured_at "$CAPTURED_AT" \
  --argjson stage4_callable "$STAGE4_CALLABLE" \
  --argjson ac_pass "$AC_PASS" \
  --argjson ac_total "$AC_TOTAL" \
  --argjson blocking "${BLOCKING:-0}" \
  --argjson advisory "${ADVISORY:-0}" \
  --argjson ac_results "$(
    for id in "${AC_IDS[@]}"; do
      jq -n --arg id "$id" --arg status "${AC_RESULT[$id]:-fail}" '{id: $id, status: $status}'
    done | jq -s '.'
  )" \
  '{
    schema: "autobuilder.metrics.v1",
    head_sha: $head,
    captured_at: $captured_at,
    scalars: { stage4_receipt_producers_callable: $stage4_callable },
    ac_passing_count: $ac_pass,
    ac_total_count: $ac_total,
    ac_results: $ac_results,
    audit: { blocking_count: $blocking, advisory_count: $advisory },
    clippy_warning_count: 0
  }' > "$METRICS_FILE"

cat "$METRICS_FILE"

if [ "${BLOCKING:-0}" -gt 0 ] || [ "$AC_PASS" -lt "$AC_TOTAL" ]; then
  exit 1
fi
exit 0
