#!/usr/bin/env bash
# agorabus-session-start.sh — SessionStart hook for cross-session pub/sub.
#
# PRD-agorabus-boot-handshake v0.1 — verified subscribe with retry.
#
# Provenance / why the wait windows are what they are
# ---------------------------------------------------
# From ~/brain/journal/2026-05-25.md §Notable (verbatim):
#
#   Post-reboot startup race produced an agorabus orphan for PID 917.
#   New pattern that's worth a playbook tweak: the
#   agorabus_orphan_subscriber playbook assumes pre-fix daemon
#   collision bug, but with a post-fix daemon the orphan can still
#   arise from a daemon-not-ready race at boot if the SessionStart
#   hook fires before the daemon's socket is bound. The current hook
#   script does a 0.1s x 5 poll for the socket, which can lose under
#   heavy boot load. Future-Claude: consider extending the hook's
#   socket-wait to ~0.5s x N or adding a peer-record-explicit re-
#   announce after the subscribe handshake.
#
# This script implements both: 10 x 0.3s socket-wait (~3s max) plus
# explicit peer-record polling after subscribe + worker spawns, with
# a single re-spawn on missing peer. Tuned for 2026-05-25 boot load
# (~10.42). Re-tune from self-review observations after kernel
# upgrades.
#
# Idempotently:
#   - Starts the agorabus daemon if not already running.
#   - Spawns a long-lived `subscribe` so this session appears in `peers`
#     and receives published events.
#   - Appends incoming events to ~/.cache/agorabus/sessions/<sid>.ndjson
#     so the model can tail its own inbox.
#   - Verifies each phase landed in the daemon's peer record. Logs one
#     append per attempt to ~/.cache/agorabus/handshake/<sid>.log as
#     JSON: {ts, sid, phase, attempt_n, ok|fail, elapsed_ms}.
#
# Emits a brief banner listing other peers on the bus (if any). Never
# blocks Claude startup; always exits 0.

set -u

agorabus=/home/jsy/.local/bin/agorabus
cache=/home/jsy/.cache/agorabus
sessions="$cache/sessions"
handshake="$cache/handshake"
sock="$cache/sock"
daemon_log="$cache/daemon.log"

[ -x "$agorabus" ] || exit 0
mkdir -p "$sessions" "$handshake" 2>/dev/null || exit 0

# Log-rotation: prune handshake logs older than 14 days at start.
find "$handshake" -maxdepth 1 -type f -name '*.log' -mtime +14 -delete 2>/dev/null || true

# Find Claude's PID early so we have $sid available for the handshake log.
root="$PPID"
if [ -r "/proc/$PPID/comm" ]; then
    parent_comm=$(cat "/proc/$PPID/comm" 2>/dev/null || true)
    if [ "$parent_comm" != "claude" ]; then
        grand=$(awk '{print $4}' "/proc/$PPID/stat" 2>/dev/null || true)
        if [ -n "$grand" ] && [ "$grand" != "1" ]; then
            root="$grand"
        fi
    fi
fi

cwd="${CLAUDE_PROJECT_DIR:-$PWD}"
project=$(basename "$cwd")
sid="claude-${root}-${project}"
sid_worker="${sid}-worker"
hslog="$handshake/${sid}.log"

# epoch_ms — millisecond unix timestamp via date.
epoch_ms() {
    local s ns
    read -r s ns < <(date +'%s %N')
    # %N can be "N" on busybox-style date; guard.
    case "$ns" in
        ''|*[!0-9]*) printf '%s000' "$s" ;;
        *) printf '%s%03d' "$s" "$((10#${ns:0:3}))" ;;
    esac
}

# hslog_append phase attempt_n status elapsed_ms [note]
hslog_append() {
    local phase="$1" attempt="$2" status="$3" elapsed="$4" note="${5:-}"
    local ts
    ts=$(epoch_ms)
    if [ -n "$note" ]; then
        printf '{"ts":%s,"sid":"%s","phase":"%s","attempt_n":%s,"status":"%s","elapsed_ms":%s,"note":"%s"}\n' \
            "$ts" "$sid" "$phase" "$attempt" "$status" "$elapsed" "$note" >>"$hslog" 2>/dev/null || true
    else
        printf '{"ts":%s,"sid":"%s","phase":"%s","attempt_n":%s,"status":"%s","elapsed_ms":%s}\n' \
            "$ts" "$sid" "$phase" "$attempt" "$status" "$elapsed" >>"$hslog" 2>/dev/null || true
    fi
}

# peer_present sid — returns 0 if `agorabus peers` lists the given session_id.
peer_present() {
    local target="$1" out
    out=$("$agorabus" peers 2>/dev/null) || return 1
    [ -n "$out" ] && [ "$out" != "[]" ] || return 1
    printf '%s' "$out" \
        | jq -e --arg s "$target" 'any(.[]?; .session_id == $s)' >/dev/null 2>&1
}

# Bring up the daemon if missing, with extended socket-wait.
daemon_start_ms=$(epoch_ms)
if ! pgrep -f 'agorabus daemon' >/dev/null 2>&1; then
    nohup "$agorabus" daemon >"$daemon_log" 2>&1 &
    disown
    daemon_ok=0
    for n in $(seq 1 10); do
        if [ -S "$sock" ]; then
            daemon_ok=1
            hslog_append daemon_up "$n" ok "$(( $(epoch_ms) - daemon_start_ms ))"
            break
        fi
        sleep 0.3
    done
    if [ "$daemon_ok" -eq 0 ]; then
        hslog_append daemon_up 10 fail "$(( $(epoch_ms) - daemon_start_ms ))" "socket-wait-exhausted"
        # Fail open — let subsequent steps try anyway; subscriber retries internally.
    fi
else
    # Daemon already up — record zero-wait success for clean cold-boot baseline.
    if [ -S "$sock" ]; then
        hslog_append daemon_up 0 ok 0 already_running
    else
        hslog_append daemon_up 0 fail 0 daemon_pid_alive_but_no_socket
    fi
fi

# spawn_subscriber — fire the long-lived subscribe process.
spawn_subscriber() {
    local log="$sessions/${sid}.ndjson"
    setsid bash -c "exec '$agorabus' subscribe --session-id '$sid' '' >>'$log' 2>&1" </dev/null &
    disown
}

# Avoid double-attaching if a subscriber for this sid already exists.
sub_start_ms=$(epoch_ms)
if pgrep -f "agorabus subscribe --session-id $sid" >/dev/null 2>&1 \
   && peer_present "$sid"; then
    hslog_append sub_attach 0 ok 0 already_attached
else
    if ! pgrep -f "agorabus subscribe --session-id $sid" >/dev/null 2>&1; then
        spawn_subscriber
    fi
    # Poll for the peer record up to 10x 0.3s.
    sub_ok=0
    for n in $(seq 1 10); do
        sleep 0.3
        if peer_present "$sid"; then
            hslog_append sub_attach "$n" ok "$(( $(epoch_ms) - sub_start_ms ))"
            sub_ok=1
            break
        fi
    done
    # Re-spawn once and poll another 5x 0.3s if still missing.
    if [ "$sub_ok" -eq 0 ]; then
        hslog_append sub_attach 10 fail "$(( $(epoch_ms) - sub_start_ms ))" no_peer_after_first_window
        # Kill the dud subscriber (if any) and respawn.
        pkill -f "agorabus subscribe --session-id $sid" 2>/dev/null || true
        spawn_subscriber
        for n in 11 12 13 14 15; do
            sleep 0.3
            if peer_present "$sid"; then
                hslog_append sub_attach "$n" ok "$(( $(epoch_ms) - sub_start_ms ))"
                sub_ok=1
                break
            fi
        done
        if [ "$sub_ok" -eq 0 ]; then
            hslog_append sub_attach 15 fail "$(( $(epoch_ms) - sub_start_ms ))" exhausted
        fi
    fi
fi

# Spawn the RPC worker so this session auto-replies to ping / self.describe /
# methods.list / delegate.run on rpc.req.<sid>. Worker is idempotent.
worker="$HOME/.claude/scripts/agorabus-worker.sh"
spawn_worker() {
    local workers spawn_log
    workers="$cache/workers"
    mkdir -p "$workers" 2>/dev/null || true
    spawn_log="$workers/${sid}.spawn.log"
    setsid bash -c "exec '$worker' '$sid' '$cwd' >>'$spawn_log' 2>&1" </dev/null &
    disown
}

worker_start_ms=$(epoch_ms)
if [ -x "$worker" ]; then
    if pgrep -f "agorabus-worker.sh $sid " >/dev/null 2>&1 \
       && peer_present "$sid_worker"; then
        hslog_append worker_attach 0 ok 0 already_attached
    else
        if ! pgrep -f "agorabus-worker.sh $sid " >/dev/null 2>&1; then
            spawn_worker
        fi
        worker_ok=0
        for n in $(seq 1 10); do
            sleep 0.3
            if peer_present "$sid_worker"; then
                hslog_append worker_attach "$n" ok "$(( $(epoch_ms) - worker_start_ms ))"
                worker_ok=1
                break
            fi
        done
        if [ "$worker_ok" -eq 0 ]; then
            hslog_append worker_attach 10 fail "$(( $(epoch_ms) - worker_start_ms ))" no_peer_after_first_window
            pkill -f "agorabus-worker.sh $sid\$" 2>/dev/null || true
            spawn_worker
            for n in 11 12 13 14 15; do
                sleep 0.3
                if peer_present "$sid_worker"; then
                    hslog_append worker_attach "$n" ok "$(( $(epoch_ms) - worker_start_ms ))"
                    worker_ok=1
                    break
                fi
            done
            if [ "$worker_ok" -eq 0 ]; then
                hslog_append worker_attach 15 fail "$(( $(epoch_ms) - worker_start_ms ))" exhausted
            fi
        fi
    fi
fi

# Peer banner (other sessions only).
peers=$("$agorabus" peers 2>/dev/null)
case "$peers" in
    ""|"[]") exit 0 ;;
esac

other_count=$(printf '%s' "$peers" \
    | jq --arg sid "$sid" '[.[] | select(.session_id != $sid)] | length' 2>/dev/null)
if [ "${other_count:-0}" -gt 0 ]; then
    printf '\n=== agorabus: %s peer(s) on bus ===\n' "$other_count"
    printf '%s\n' "$peers" \
        | jq -r --arg sid "$sid" \
            '.[] | select(.session_id != $sid) | "- \(.session_id) pid=\(.pid) cwd=\(.cwd) intent=\(.intent)"' \
            2>/dev/null
    printf 'this session: %s\n' "$sid"
    printf '=== /agorabus ===\n'
fi

exit 0
