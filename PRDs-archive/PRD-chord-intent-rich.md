# PRD-chord-intent-rich

Status: Draft v0.1
build_auto: false
build_target: rust-extend
build_into: /home/jsy/wintermute/agorabus
build_version_bump: minor
Vision: visions/chord.md

## TL;DR

Concurrent Claude sessions show up on agorabus's peer list with an
`intent` field that only ever says `subscriber` or `worker`, plus a
`last_tool` string updated by heartbeat. That's not enough to reason
about what each session is *doing* right now. This PRD extends the
heartbeat envelope and adds two CLI subcommands so a session can
publish structured intent (skill, PRD slug, working paths) and any
other session can read structured intent for all peers in one call.

## Why this exists

Right now, when three sessions are alive simultaneously (the steady
state per recent self-reviews 2026-05-23 through 2026-05-24), nothing
distinguishes them on the bus beyond the worker/subscriber split and
the most recent tool name. Evidence:

- `agorabus peers` output (verified 2026-05-25 this session) returns
  entries with `intent: "subscriber"` for every peer — the field
  exists but is never set meaningfully.
- `~/.claude/scripts/agorabus-worker.sh` writes only `tool` on each
  heartbeat (see `agorabus-worker.sh` line ~64 dispatching on
  per-line `method`; the heartbeat producer elsewhere does not pass
  skill/PRD).
- `visions/wintermute.md` describes Claude-on-claude collaboration as
  a future capability; without structured intent there's nothing to
  collaborate over.
- `feedback_delegate_run_300s_cap.md` recommends "use baton (visible
  window) or single-tool ops only" partly because the caller can't
  tell what the target session is currently doing — a richer intent
  envelope addresses the underlying visibility gap.

The chord vision (visions/chord.md §End-state #1) names this as the
prerequisite for everything else in the fleet: claims, cross-session
episodes, and async-delegate routing all assume structured intent
exists.

## What this builds

### Wire-level changes (`agorabus` daemon + protocol)

Heartbeat envelope grows three optional fields:

```json
{
  "op": "heartbeat",
  "tool": "Bash",
  "skill": "/build",                  // new — optional
  "prd_slug": "recall-daemon",        // new — optional
  "working_paths": ["~/wintermute/recall"]  // new — optional, max 8
}
```

Existing clients that send only `tool` still work; new fields are
additive. The daemon stores them on the peer record alongside
`last_heartbeat_unix_secs`.

`agorabus peers` JSON output gains the same three fields when set;
absent fields remain absent (no `null` filler — keep the wire small).

### CLI surface (new subcommands)

```sh
# Set intent for the calling session. All flags optional.
# Writes a single heartbeat with the new fields populated.
agorabus intent set \
  --skill /build \
  --prd  recall-daemon \
  --paths ~/wintermute/recall,~/wintermute/autobuilder

# Read structured intent for all peers as JSON (alias for `peers` but
# filtered to peers that have any intent field set, and only the
# intent fields plus session_id).
agorabus intent list
```

Both subcommands accept `--session-id <sid>` (mirrors existing
agorabus client convention) and the same `--socket` path argument as
other subcommands. Fail-open: no daemon ⇒ exit 0, `[]` on stdout
(matches agorabus AC6 in its own README).

### Hook integration (out of scope for this PRD)

The natural caller for `intent set` is a SessionStart hook + skill
entry/exit hooks. Wiring those is **out of scope** for this PRD —
the PRD ships the primitive; hooks land separately so the user can
review the surface first. (Reasonable next step: extend
`agorabus-session-start.sh` with an `intent set --skill ${CLAUDE_SKILL}`
call when `$CLAUDE_SKILL` is set.)

## Acceptance criteria

1. **AC1 — protocol back-compat.** Existing
   `{"op":"heartbeat","tool":"Bash"}` continues to work; daemon
   replies `{"ok":true}`; peer record updates `last_heartbeat`
   without touching skill/prd_slug/working_paths.

2. **AC2 — new fields stored.** A heartbeat with all four optional
   fields populated results in those fields appearing in the next
   `agorabus peers` JSON output on that peer. Fields not sent in a
   later heartbeat retain their last-set value (sticky); a heartbeat
   with explicit empty string or empty array clears the field.

3. **AC3 — `intent set` writes one heartbeat.** Running
   `agorabus intent set --session-id sid-X --skill /build` writes a
   single heartbeat for `sid-X` carrying `skill="/build"` and
   leaves `tool` unset. Subsequent `agorabus peers` shows `skill`
   populated for `sid-X`.

4. **AC4 — `intent list` filters and projects.** With three peers on
   the bus where two have intent fields set, `agorabus intent list`
   returns exactly those two peers, each with only `session_id` +
   set intent fields (no `pid`, `cwd`, etc.). The third peer is
   omitted.

5. **AC5 — `working_paths` cap.** A heartbeat with
   `working_paths` of length 9+ is rejected with
   `{"ok":false,"error":"too_many_paths","detail":"max 8"}`. A
   heartbeat with `working_paths` of length 0 clears the field
   (treated same as absent on next read).

6. **AC6 — fail-open on no daemon.** `agorabus intent list` with no
   daemon running exits 0 and emits `[]`. `agorabus intent set …`
   with no daemon running exits 0 silently (matches client
   convention).

7. **AC7 — version + changelog.** Cargo.toml bumps to next minor
   (0.1.0 → 0.2.0). CHANGELOG.md gains a `## v0.2.0` section
   describing the protocol additions. `~/wintermute/REPOS.md` is
   not touched (per build-rust-extend AC8).

## Risks / trade-offs

- **Sticky vs per-heartbeat fields.** AC2 makes new fields sticky
  (set once, persist until cleared). The alternative — clear on every
  heartbeat that omits them — would be more "live" but force every
  heartbeat to repeat all fields. Sticky matches how skills/PRDs
  actually behave (long-running for a session) and keeps the wire
  smaller. Trade-off documented; revisit if it causes confusion.
- **No auth on `intent set`.** Anyone on the local socket can set
  intent for any session_id. Fine for single-user trust model;
  documented in agorabus README. Don't add signing.
- **Schema growth.** This is the first time the heartbeat envelope
  grows beyond `op` + `tool`. Three new fields is fine; future PRDs
  must resist piling more on. If a 4th vision wants a new field, that
  vision's PRD should justify it.

## Out of scope

- SessionStart hook auto-population. (Land separately after the
  primitive is reviewed.)
- A `intent get --self` lookup. (Trivial: `agorabus intent list |
  jq '.[] | select(.session_id == "...")'`. Add only if friction
  shows up.)
- Capability/method registration. (See Fleet 2 bullet
  `chord-method-discovery` in visions/chord.md.)

## Provenance

- Vision doc: `visions/chord.md`
- Research evidence cited above; further detail in vision §Evidence.
- /dream session 2026-05-25, seed: reflection (bare `/dream`,
  cross-session orchestration picked as biggest white space among
  the four candidate seeds; vision §TL;DR for rationale).
