#!/usr/bin/env bash
# SessionEnd hook: stop the ctrace session we own and render its summary.
#
# Hardening over the original ctrace-session-end.sh:
#   - Uses `scribe render` when available; falls back to summarize-ctrace-session.sh.
#   - On render failure: appends a one-line diagnostic to claude-stop.err
#     (log path + exit code), so "errored" is distinguishable from "never ran".
#   - On render success: ensures claude-stop.err is empty (so absence of errors
#     is a genuine signal, not ambiguity).
#   - Preserves the always-exit-0 contract — never blocks shutdown.
#
# PRD: PRD-ctrace-session-end-resilient.md  (AC #3, #4, #5, #6)
#
# Drop-in replacement for ~/.claude/scripts/ctrace-session-end.sh once verified.

set -uo pipefail

ctrace=/home/jsy/.local/bin/ctrace
cache=/home/jsy/.cache/ctrace
marker="$cache/claude-owns.json"
summarize=/home/jsy/.claude/scripts/summarize-ctrace-session.sh
err="$cache/claude-stop.err"

# --- helpers ------------------------------------------------------------------

# Append a diagnostic line to claude-stop.err.
# Format: ISO-ts  ctrace-session-end  FIELD=value ...
diag() {
    printf '%s  ctrace-session-end  %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*" >> "$err" 2>/dev/null || true
}

# Render the log at $1, preferring scribe, falling back to summarize.
# Returns 0 on success, 1 on failure (never raises).
render_log() {
    local log_path="$1"
    local render_exit=0

    if command -v scribe >/dev/null 2>&1; then
        local summary="${log_path%.ndjson}.summary.md"
        if scribe render "$log_path" -o "$summary" 2>>"$err"; then
            return 0
        fi
        # scribe failed — record and try fallback
        diag "status=scribe-failed log=$log_path"
    fi

    if [ -x "$summarize" ]; then
        "$summarize" "$log_path" >/dev/null 2>>"$err"
        render_exit=$?
        if [ "$render_exit" -eq 0 ]; then
            return 0
        fi
        diag "status=summarize-failed log=$log_path exit=$render_exit"
        return 1
    fi

    diag "status=no-renderer log=$log_path"
    return 1
}

# --- main ---------------------------------------------------------------------

# No marker → not our session; nothing to do.
[ -f "$marker" ] || exit 0

log=$(jq -r '.log // empty' "$marker" 2>/dev/null || true)

# Stop the tracer, capturing any stop errors.
"$ctrace" stop >/dev/null 2>>"$err" || true

rm -f "$marker"

if [ -n "$log" ] && [ -f "$log" ]; then
    # Clear the error file before this render attempt so that on success
    # the absence of content genuinely means "no errors this session".
    # (Prior content from ctrace stop is preserved only on failure, below.)
    # We do NOT truncate yet — we snapshot the pre-render err length first
    # and only clear if render succeeds.
    pre_render_err_bytes=$(wc -c < "$err" 2>/dev/null || echo 0)

    if render_log "$log"; then
        # Success: remove any diagnostic content accumulated during this
        # clean render attempt (the file may have stop-output from above).
        # If claude-stop.err was previously zero, keep it zero (clean signal).
        if [ "$pre_render_err_bytes" -eq 0 ]; then
            : > "$err" 2>/dev/null || true
        fi
    else
        # render_log already appended a diagnostic; leave err file intact.
        : # no-op — error is logged
    fi
else
    diag "status=no-log-file marker_log=${log:-<empty>}"
fi

exit 0
