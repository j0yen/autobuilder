# PRD: lucid-tap — the flight recorder for the whole bus

Status: Draft v0.1
build_target: rust-cli
build_into: /home/jsy/wintermute/wintermute-lucid
Vision: visions/lucid.md

## TL;DR

Every thought wintermute has already flows over the agorabus bus — ~120 topics
across the voice stack and action layer — but nothing records it. This PRD
introduces `wm-lucid`, a recorder daemon that subscribes to the entire `wm.`
prefix and persists every event to a rotating, turn-keyed structured log that
survives daemon and reboot death. It is the flight recorder the rest of the
`lucid` fleet reads from.

## Why this exists

Throughout the 2026-06-03/04 session I repeatedly hand-rolled taps to see what
the system was doing: one-shot `agorabus subscribe wm.brain.reply --max-events
1`, `journalctl --user -u wmd | grep -oE 'tier=...'`, ad-hoc `/tmp/reply.json`
captures. Each was throwaway, none was correlated, and the journal's clock skew
(Jun 04/05/06 in one capture) made post-hoc reconstruction unreliable. A
first-class recorder would have made every one of those greps a single command.

Evidence from Phase 1:
- The bus carries ~120 `wm.*` topics today (grep of `wintermute-*/src/*.rs`),
  including the high-signal `wm.brain.route`, `wm.brain.tool.call/result`,
  `wm.dialog.state`, `wm.stt.final`, `wm.audio.wake`.
- `agorabus subscribe <prefix>` already streams one JSON line per event to
  stdout and reconnects across daemon restarts (`agorabus --help`) — the
  recorder builds directly on this; it does not need new bus plumbing.
- No persistence layer exists; events are ephemeral once printed.
- The `agorabus` recorder must itself appear as a well-behaved peer (intent tag),
  per the `~/.claude/AGORABUS_RPC.md` convention and existing daemon practice.

## What this builds

A new repo `~/wintermute/wintermute-lucid` shipping the `wm-lucid` binary:

- **Subscribe** to the whole `wm.` prefix via the agorabus client (reconnecting),
  registering as a peer with `intent=wm-lucid recorder`.
- **Record** each event as a structured row: `{ts_received, topic, turn_id (if
  present), from, raw_payload}`. Use the event's `turn_id` (from lucid-turn-id)
  as the primary correlation key; events without one are still recorded,
  bucketed by a synthetic `untagged-<ts>` key.
- **Persist** to an append-only log under `~/.cache/wintermute/lucid/` (or
  `$XDG_CACHE_HOME`), with **rotation** by size and/or age so it is bounded —
  the recorder must never fill the disk. A small index (turn_id → byte offsets,
  or a sidecar) makes per-turn lookup cheap for `lucid trace`.
- **Survive restarts:** on start, open/append the current segment; do not
  truncate prior segments. Records survive process death (file-backed) and
  reboot.
- **Ship a systemd-user unit** `wm-lucid.service` (enabled, `WantedBy`
  wintermute.target, `After`/`Wants` agorabus) consistent with the other voice
  daemons' unit conventions.
- **A minimal `lucid tap` foreground mode** (`wm-lucid tap [--topic <prefix>]`)
  that tails live events to stdout as structured JSON — the first-class
  replacement for the ad-hoc `agorabus subscribe` one-shots, useful before the
  trace/mind readers exist.
- SIGPIPE safety: call `sigpipe::reset()` first thing in `main()` (per the
  toolkit convention) so `wm-lucid tap | head` doesn't coredump.

Non-goals: trace reconstruction (lucid-trace), brain-reasoning rendering
(lucid-mind), live TUI (lucid-live). This PRD only records and tails.

## Acceptance criteria

1. `wm-lucid` starts, registers as an agorabus peer with a `wm-lucid` intent
   tag, and subscribes to the full `wm.` prefix (verifiable via `agorabus
   peers`).
2. A burst of N published `wm.*` events results in N persisted records under the
   cache dir, each carrying `{ts_received, topic, turn_id?, from, raw_payload}`.
3. Records are keyed/indexed by `turn_id` when present; an event lacking a
   `turn_id` is still recorded under a synthetic key and never dropped.
4. Log rotation bounds total on-disk size: writing past the configured cap
   rotates/prunes oldest segments, and prior (unrotated) segments are never
   truncated — proven by a test that overflows a small cap.
5. After a `wm-lucid` restart, records written before the restart are still
   present and readable (file-backed persistence survives process death).
6. `wm-lucid tap` streams live events to stdout as one JSON object per line,
   honors an optional `--topic <prefix>` filter, and exits cleanly (no panic)
   when its stdout pipe closes.
7. A `wm-lucid.service` systemd-user unit installs, enables, and comes up
   `active` with the correct ordering deps.
