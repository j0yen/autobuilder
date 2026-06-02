# Proposal: self-review agentns block replacement
#
# Drop-in replacement for the agentns bullet in SKILL.md Phase B / B.5.
# Replace lines 235-241 (the two-line "agentns:" bullet) with the block below.
#
# Lines replaced (verbatim in SKILL.md as of 2026-05-29):
#   235: - **agentns**: `[ -f /proc/self/agent_session ]` and `cat
#   236:   /proc/self/agent_session`. If empty / file missing, the namespace
#   237:   registration failed. Recent reaper kills:
#   238:   `dmesg -t | grep -E 'agent_ns:.*reaping|agent_ns:.*budget' | tail -10`.
#   239:   Any line with `budget.*SIGKILL` is worth noting (a real session was
#   240:   budget-killed; might be intentional, might be a runaway, surface
#   241:   verbatim).
#
# Swap command (run as the user once satisfied with the proposal):
#   # replace the agentns bullet in Phase B/B.5 with this block
#   # (lines 235-241 in SKILL.md — verify line numbers before applying)
#
# PRD: PRD-agentns-doctor-self-review.md (signet vision)
# Depends on: agentns-doctor installed at ~/.local/bin/agentns-doctor
#             (degrades safely if absent — fallback branch runs instead)

## Replacement SKILL.md text (Markdown section)

- **agentns**: tri-state probe via `agentns-doctor` — classifies the surface
  as `init` (unwrapped, expected), `live` (wrapped, healthy), `absent`
  (surface missing), or `malformed`. Run the block below; journal one calm
  line for `init`/`live`, emit `PENDING:` only for `absent` on a
  `-wintermute` kernel or `malformed`.

  ```bash
  # agentns — tri-state via agentns-doctor (signet vision)
  # PRD: PRD-agentns-doctor-self-review.md
  if command -v agentns-doctor >/dev/null 2>&1; then
    ans=$(agentns-doctor status --format json 2>/dev/null)
    state=$(printf '%s' "$ans" | jq -r '.state // "malformed"')
    verdict=$(printf '%s' "$ans" | jq -r '.verdict // ""')
    case "$state" in
      init)
        # unwrapped — EXPECTED; do NOT flag; one calm journal line
        echo "agentns: init ns (unwrapped) — expected until claude-agentns-wrap routes launches through agentns-claude; not a fault"
        ;;
      live)
        # wrapped — record sid + intent_tag, healthy
        sid=$(printf '%s' "$ans" | jq -r '.session_id // "?"')
        tag=$(printf '%s' "$ans" | jq -r '.intent_tag // "?"')
        echo "agentns: live session ${sid} intent=${tag}"
        ;;
      absent)
        # only a fault on a -wintermute kernel
        if uname -r | grep -q wintermute; then
          echo "PENDING: agentns surface absent on -wintermute kernel — namespace driver may not have loaded"
        else
          echo "agentns: surface absent (stock kernel) — expected, not a fault"
        fi
        ;;
      malformed)
        echo "PENDING: agentns surface malformed: ${ans}"
        ;;
      *)
        echo "PENDING: agentns surface unknown state=${state}: ${ans}"
        ;;
    esac
  else
    # agentns-doctor not installed yet: cat, but interpret correctly
    s=$(cat /proc/self/agent_session 2>/dev/null || true)
    if [ -z "$s" ]; then
      echo "agentns surface absent (stock kernel or pre-boot) — not a fault"
    elif printf '%s' "$s" | grep -qE '^0+$'; then
      echo "agentns: init ns (unwrapped) — EXPECTED until claude-agentns-wrap routes launches through agentns-claude; NOT a kernel fault"
    else
      echo "agentns: live session ${s}"
    fi
  fi

  # dmesg: recent reaper kills (unchanged from original block)
  dmesg -t | grep -E 'agent_ns:.*reaping|agent_ns:.*budget' | tail -10 | while IFS= read -r line; do
    if printf '%s' "$line" | grep -q 'budget.*SIGKILL'; then
      echo "agentns-reap: ${line}"
    fi
  done
  ```

  Standing journal phrasing for the `init` state (copy verbatim into the
  Notable section — do NOT carry as Pending):

  > agentns: init ns (unwrapped), expected — non-zero session id arrives
  > when `claude-agentns-wrap` routes launches through `agentns-claude`.
  > Not a fault; not carried as Pending.

  **Anti-regression note**: the string "registration failed" MUST NOT appear
  in journal output for any of the four states. The old SKILL.md prose
  caused 20 consecutive mis-diagnoses by presenting the `init` (all-zeros)
  reading as "registration failed" to the reading human. This block names
  all four states explicitly.
