# Vision: handshake

**Status:** active
**Created:** 2026-05-25
**Seed:** reflection (today's self-review caught a new agorabus failure mode)

## TL;DR

The agorabus bus brings up reliably 99% of the time, but it loses
under specific conditions — most recently, the post-2026-05-25 reboot
produced an orphan subscriber pair for the interactive session
PID 917, attributed to a daemon-not-ready race at boot. This vision
covers handshake reliability: the moment a Claude session attaches
to the bus and announces itself. The goal is "no orphans, ever, even
under heavy boot load."

## End-state

When this is done:

- Every Claude session that enters the bus is verifiably visible in
  `agorabus peers` within ~3s of `SessionStart` firing, or the
  failure is loud (logged + counted), not silent.
- Re-attach paths exist for sessions that did get orphaned (today
  the recovery requires `kill <sid> ; rerun script`, and the user
  has to do it manually from the affected terminal).
- Boot-time races are observable in `~/.cache/agorabus/handshake/`
  so future self-reviews can distinguish "race fired and recovered"
  from "race fired and we lost it."

## Components

- **PRD-agorabus-boot-handshake.md** (Fleet 1, this pass) — verified
  handshake in `agorabus-session-start.sh`: extended socket-wait,
  peer-record polling after subscribe, structured handshake log,
  retry-on-missing-peer. Single shell PRD; no new Rust.

## Order

Single PRD this pass; no internal ordering.

## Fleet 2 (NOT drafted — captured here per dream rule 6)

Draft after Fleet 1 ships AND at least one race is observed +
recovered in `~/.cache/agorabus/handshake/` logs.

- **handshake-reannounce-on-watch-loss** — daemon-side: when a peer's
  socket goes away (subscriber crash / kill), broadcast a
  `peer.lost` event so workers can rejoin. Today a dead subscriber's
  worker keeps publishing into a void.
- **handshake-daemon-ready-fd** — daemon writes a marker file
  (`~/.cache/agorabus/ready`) on socket-bind; hook waits on the
  marker rather than the socket file's existence. Less racy than
  `[ -S sock ]` which can be true before the daemon is actually
  accepting connections.
- **handshake-reattach-cli** — `agorabus reattach <sid>`: programmatic
  recovery from an orphan state; today recovery is "kill the orphan,
  rerun the hook in the affected terminal," which is manual.
- **handshake-startup-race-pevent** — `pevent`-style supervised launch
  so the daemon's startup is bounded + retried by something other
  than the hook script.
- **handshake-prom-counters** — daemon exports race/orphan/recover
  counters via a stable JSON endpoint for the daily self-review.

## Evidence log

- **2026-05-25 (today's self-review)** — interactive Claude PID 917
  attached two subscribers (1888, 2091) that ran but never
  registered with the daemon (daemon binary post-fix, so this is a
  startup race not the pre-fix collision bug). The
  `agorabus_orphan_subscriber` playbook's auto-fix path detected
  the condition but escalated — killing 1888/2091 from a different
  session has no re-attach path, so the user has to fix it manually
  in PID 917's terminal. Cited verbatim in
  `~/brain/journal/2026-05-25.md §Notable` with the proposed
  remedies that this fleet implements.
- **2026-05-25 ~/.claude/scripts/agorabus-session-start.sh** —
  current socket-wait is `0.1s × 5 = 0.5s` max; subscriber spawn
  has a `sleep 0.2` then assumes success with no peer-record
  verification. Under heavy boot load (kernel build PID 12146 at
  load 10.42 today), this is reliably too short.

## Open questions

- Should the hook's peer-record verification block SessionStart
  indefinitely on failure, or fail-open after N retries with a
  loud log entry? **Default in PRD:** fail-open after 10×0.3s
  with a banner — never block Claude startup. Same posture as the
  existing `[ -x agorabus ] || exit 0` line at the top of the
  script.
- Should the daemon binary itself signal "ready" (file marker /
  socket option) so future hooks don't have to poll the socket?
  Captured as Fleet 2 (`handshake-daemon-ready-fd`); not in scope
  for this pass.

## Cross-fleet coordination

- **chord vision** — chord-async-delegate is the only other
  shell-target PRD in flight; both touch `~/.claude/scripts/`. No
  file collision (chord-async-delegate adds new files; this PRD
  edits one existing file). If both ship close together, sequence
  them so the agorabus reliability fix lands first — chord's
  async-delegate assumes the bus is reliable.
- **continuity vision** — when the kernel boots, agentns session
  ids become the stable handles agorabus subscribes with. The
  handshake verification logic should be agnostic to which id
  scheme is in use (today: PID-based; future: 128-bit agentns sid).
- **freshness vision** — none directly.
