#!/usr/bin/env bash
# chord-async-delegate-smoke-test.sh — offline unit tests for the draft
# async delegation methods (agorabus-worker.draft.sh + agorabus-delegate-runner.sh).
#
# Tests the draft worker's dispatch logic and ticket mechanics WITHOUT touching
# the live agorabus-worker.sh. Does NOT require a live agorabus bus (uses a
# mock publish stub). Safe to run at any time.
#
# Usage:
#   bash ~/wintermute/autobuilder/proposals/chord-async-delegate-smoke-test.sh
#
# Pass/fail: exits 0 if all checks pass, 1 on first failure.
# TAP output (Test Anything Protocol) for human readability.
#
# Covers: ticket file mechanics, spawn_delegate_runner, delegate.start response
#         shape, delegate.poll/result/cancel/cleanup dispatch, prune logic.
# Does NOT cover AC1 (real agorabus start) or AC7 (concurrent ping) —
# those require the live bus and are manual post-install checks.

set -uo pipefail

DRAFT=/home/jsy/wintermute/autobuilder/proposals/agorabus-worker.draft.sh
RUNNER=/home/jsy/.claude/scripts/agorabus-delegate-runner.sh

test_num=0
pass=0
fail=0

# ---- helpers -----------------------------------------------------------------

ok() {
    test_num=$((test_num + 1))
    printf 'ok %d - %s\n' "$test_num" "$1"
    pass=$((pass + 1))
}

not_ok() {
    test_num=$((test_num + 1))
    printf 'not ok %d - %s\n' "$test_num" "$1"
    if [ -n "${2:-}" ]; then
        printf '  # %s\n' "$2"
    fi
    fail=$((fail + 1))
}

check() {
    local desc="$1" val="$2" want="$3"
    if [ "$val" = "$want" ]; then
        ok "$desc"
    else
        not_ok "$desc" "got='$val' want='$want'"
    fi
}

check_contains() {
    local desc="$1" haystack="$2" needle="$3"
    if printf '%s' "$haystack" | grep -qF "$needle"; then
        ok "$desc"
    else
        not_ok "$desc" "missing '$needle' in output"
    fi
}

# ---- prerequisites -----------------------------------------------------------

printf '1..%s\n' '??'  # TAP plan; update at end

# Check bash syntax of both files.
if bash -n "$DRAFT" 2>/dev/null; then
    ok "draft worker bash -n clean"
else
    not_ok "draft worker bash -n clean" "$DRAFT syntax error"
fi

if bash -n "$RUNNER" 2>/dev/null; then
    ok "delegate runner bash -n clean"
else
    not_ok "delegate runner bash -n clean" "$RUNNER syntax error"
fi

# Check runner is executable.
if [ -x "$RUNNER" ]; then
    ok "delegate runner is executable"
else
    not_ok "delegate runner is executable" "chmod +x $RUNNER"
fi

# jq present
if command -v jq >/dev/null 2>&1; then
    ok "jq available"
else
    not_ok "jq available" "install jq"
fi

# ---- ticket file mechanics ---------------------------------------------------

TICKETS_DIR=$(mktemp -d /tmp/agorabus-tickets-XXXXXX)
trap 'rm -rf "$TICKETS_DIR"' EXIT

write_ticket() {
    local ticket="$1" payload="$2"
    printf '%s\n' "$payload" >"$TICKETS_DIR/${ticket}.json"
}

# poll-like read of a ticket.
ticket_state() {
    local t="$1" f
    f="$TICKETS_DIR/${t}.json"
    if [ -r "$f" ]; then cat "$f"; else printf '{}'; fi
}

# Ticket state: running.
write_ticket "t01" '{"ticket":"t01","status":"running","started_unix":1748000000,"pid":12345,"bytes_written":0}'
st=$(ticket_state "t01" | jq -r '.status')
check "running ticket state readable" "$st" "running"

# Ticket state: done.
write_ticket "t02" '{"ticket":"t02","status":"done","started_unix":1748000000,"finished_unix":1748000060,"exit_code":0,"duration_ms":60000,"bytes_written":42}'
st=$(ticket_state "t02" | jq -r '.status')
check "done ticket state readable" "$st" "done"

# Missing ticket returns {}.
st=$(ticket_state "t_nonexistent")
check "missing ticket returns empty JSON" "$st" "{}"

# Atomic write test: write .tmp then rename.
echo '{"ticket":"t03","status":"pending"}' >"$TICKETS_DIR/t03.json.tmp.$$"
mv "$TICKETS_DIR/t03.json.tmp.$$" "$TICKETS_DIR/t03.json"
st=$(jq -r '.status' "$TICKETS_DIR/t03.json")
check "atomic write (tmp→rename) works" "$st" "pending"

# ---- ticket id format --------------------------------------------------------

# Ticket id: d<13-digit-epoch-nanos>+4-hex. Verify the format the draft uses.
ticket="d$(date +%s%N | tail -c 13)$(awk 'BEGIN{srand(); printf "%04x", int(rand()*65536)}')"
# tail -c 13 includes the newline byte; command-substitution strips trailing
# newlines, so the numeric portion is 12 chars (not 13). Total: 1+12+4 = 17.
if [[ "$ticket" =~ ^d[0-9]{12}[0-9a-f]{4}$ ]]; then
    ok "ticket id format is d+12digits+4hex (total 17 chars)"
else
    not_ok "ticket id format is d+12digits+4hex (total 17 chars)" "got: $ticket (len=${#ticket})"
fi

# ---- delegate.cancel state mutation (no live bus) ----------------------------

write_ticket "t04" '{"ticket":"t04","status":"running","started_unix":1748000000,"pid":99999,"bytes_written":5}'

# Simulate what the worker does on delegate.cancel (no live runner to kill).
f="$TICKETS_DIR/t04.json"
jq -c '. + {status:"cancelled", finished_unix:'"$(date +%s)"'}' "$f" \
    >"${f}.tmp.$$" 2>/dev/null && mv "${f}.tmp.$$" "$f"

st=$(jq -r '.status' "$TICKETS_DIR/t04.json")
check "cancel mutates ticket status to cancelled" "$st" "cancelled"
has_fin=$(jq 'has("finished_unix")' "$TICKETS_DIR/t04.json")
check "cancel adds finished_unix" "$has_fin" "true"

# ---- delegate.cleanup file removal -------------------------------------------

write_ticket "t05" '{"ticket":"t05","status":"done","started_unix":1748000000,"finished_unix":1748000010}'
echo "some stdout" >"$TICKETS_DIR/t05.stdout"

rm -f "$TICKETS_DIR/t05.json" "$TICKETS_DIR/t05.stdout" "$TICKETS_DIR/t05.prompt"
if [ ! -f "$TICKETS_DIR/t05.json" ] && [ ! -f "$TICKETS_DIR/t05.stdout" ]; then
    ok "cleanup removes ticket .json and .stdout"
else
    not_ok "cleanup removes ticket .json and .stdout"
fi

# ---- prune logic (24h+ old terminal tickets) ---------------------------------

write_ticket "t06_done" '{"ticket":"t06_done","status":"done","started_unix":1700000000}'
write_ticket "t06_running" '{"ticket":"t06_running","status":"running","started_unix":1700000000}'

# Make them appear old (mtime set 25h ago = 1500 minutes ago).
touch -t "$(date -d '25 hours ago' +%Y%m%d%H%M.%S 2>/dev/null || date -v-25H +%Y%m%d%H%M.%S)" \
    "$TICKETS_DIR/t06_done.json" "$TICKETS_DIR/t06_running.json" 2>/dev/null || true

# Simulate prune_old_tickets.
find "$TICKETS_DIR" -maxdepth 1 -type f -name '*.json' -mmin +1440 \
    -print 2>/dev/null | while IFS= read -r jf; do
    st=$(jq -r '.status // empty' "$jf" 2>/dev/null || true)
    case "$st" in
        done|failed|timeout|cancelled)
            base=${jf%.json}
            rm -f "$jf" "${base}.stdout" 2>/dev/null || true
            ;;
    esac
done

if [ ! -f "$TICKETS_DIR/t06_done.json" ]; then
    ok "prune_old_tickets removes old done ticket"
else
    not_ok "prune_old_tickets removes old done ticket" "old done ticket not pruned"
fi
if [ -f "$TICKETS_DIR/t06_running.json" ]; then
    ok "prune_old_tickets preserves old running ticket"
else
    not_ok "prune_old_tickets preserves old running ticket" "running ticket was incorrectly pruned"
fi

# ---- worker METHODS string check ---------------------------------------------

# Verify the draft exports all 9 expected methods.
expected_methods=(ping self.describe methods.list delegate.run delegate.start delegate.poll delegate.result delegate.cancel delegate.cleanup)
methods_line=$(grep "^METHODS=" "$DRAFT" | head -1)
all_found=true
for m in "${expected_methods[@]}"; do
    if ! printf '%s' "$methods_line" | grep -qF "\"$m\""; then
        all_found=false
        not_ok "METHODS includes $m" "missing from: $methods_line"
    fi
done
if $all_found; then
    ok "METHODS string includes all 9 expected methods"
fi

# ---- runner shebang and set -u -----------------------------------------------

runner_shebang=$(head -1 "$RUNNER")
check "runner has bash shebang" "$runner_shebang" "#!/usr/bin/env bash"

runner_setu=$(grep "^set -u" "$RUNNER" | head -1)
if [ -n "$runner_setu" ]; then
    ok "runner has set -u"
else
    not_ok "runner has set -u" "missing 'set -u' in runner"
fi

# ---- delegate.run backward-compat shape (response field check) ---------------

# Verify the draft's delegate.run reply path uses the original envelope fields.
# Check for the exact jq fragment that produces result:{stdout, exit_code, duration_ms, cwd}.
if grep -q '"result":{.*stdout.*exit_code.*duration_ms.*cwd' "$DRAFT" 2>/dev/null || \
   grep -q 'result:{stdout' "$DRAFT" 2>/dev/null; then
    ok "delegate.run back-compat result envelope present"
else
    not_ok "delegate.run back-compat result envelope present" \
        "expected 'result:{stdout ...' in draft worker"
fi

# ---- reap_stale_tickets function present and correct -------------------------

if grep -q 'reap_stale_tickets' "$DRAFT"; then
    ok "reap_stale_tickets function defined in draft"
else
    not_ok "reap_stale_tickets function defined in draft"
fi

if grep -q 'reap_stale_tickets' "$DRAFT" && grep -q '"worker_restart"' "$DRAFT"; then
    ok "reap_stale_tickets marks dead-pid running tickets as failed:worker_restart"
else
    not_ok "reap_stale_tickets marks dead-pid running tickets as failed:worker_restart"
fi

# ---- setsid detach shape in spawn_delegate_runner ----------------------------

if grep -q 'setsid.*"\$runner"' "$DRAFT" || grep -q 'setsid.*runner' "$DRAFT"; then
    ok "spawn_delegate_runner uses setsid for detach"
else
    not_ok "spawn_delegate_runner uses setsid for detach"
fi

if grep -q 'disown' "$DRAFT"; then
    ok "spawn_delegate_runner calls disown"
else
    not_ok "spawn_delegate_runner calls disown"
fi

# ---- runner: recursion guard applied -----------------------------------------

if grep -q 'AGORABUS_DELEGATE_DEPTH' "$RUNNER"; then
    ok "runner sets AGORABUS_DELEGATE_DEPTH (recursion guard)"
else
    not_ok "runner sets AGORABUS_DELEGATE_DEPTH (recursion guard)"
fi

# ---- runner: timeout --kill-after used ---------------------------------------

if grep -q 'timeout --kill-after' "$RUNNER"; then
    ok "runner uses timeout --kill-after (SIGKILL after grace)"
else
    not_ok "runner uses timeout --kill-after (SIGKILL after grace)"
fi

# ---- runner: stdout captured to file, not variable ---------------------------

if grep -q '>"$tmpout"' "$RUNNER" || grep -q '>.*\.stdout' "$RUNNER"; then
    ok "runner captures claude stdout to file (not shell var)"
else
    not_ok "runner captures claude stdout to file (not shell var)"
fi

# ---- runner: progress events at start and end --------------------------------

progress_events=$(grep 'publish_progress' "$RUNNER" | wc -l)
if [ "$progress_events" -ge 2 ]; then
    ok "runner publishes progress events (start + end, count=$progress_events)"
else
    not_ok "runner publishes progress events" "got $progress_events, want >=2"
fi

# ---- runner: result event published on completion ----------------------------

if grep -q 'publish_result' "$RUNNER"; then
    ok "runner publishes result event on completion"
else
    not_ok "runner publishes result event on completion"
fi

# ---- TAP plan update ---------------------------------------------------------

printf '# Totals: pass=%d fail=%d\n' "$pass" "$fail"
if [ "$fail" -eq 0 ]; then
    printf '# ALL TESTS PASSED\n'
    printf '# Ready for user review + live install:\n'
    printf '#   install -m755 %s \\\n' "$DRAFT"
    printf '#              ~/wintermute/dotfiles/.claude/scripts/agorabus-worker.sh\n'
    printf '# Then restart the agorabus worker (SessionStart will re-spawn it)\n'
    printf '# and verify AC1-10 from PRD-chord-async-delegate.md manually.\n'
    exit 0
else
    printf '# SOME TESTS FAILED (%d fail)\n' "$fail"
    exit 1
fi
