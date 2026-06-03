# PRD: wintermute-reach — the daily digest to jsy

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/kin.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-reach
**build_version_bump:** minor
**Depends on:** PRD-wintermute-reach, PRD-wintermute-presence
**Codename:** *bulletin* — one calm note a day, so you don't have to wonder.

## TL;DR

Per-interaction notifications would make jsy's phone buzz all day and teach
him to ignore it. The digest is the opposite: `wintermute-reach` aggregates
`wm.presence.*` events across the day and delivers a single calm summary at a
configured time — "Mom talked to wintermute 4 times today, last at 6:12pm" —
plus a line if a silence window was flagged. Reassurance, batched, opt-in.

## 1. Why this exists

- **kin vision Component 5.** It joins the two runtime daemons: presence
  emits the raw signal, reach owns the off-device transport, and the digest
  is where they meet.
- **Notification fatigue is a real failure mode.** A peace-of-mind feature
  that fires per-interaction trains the recipient to mute it, which defeats
  the point — and worse, buries a real silence-alert in noise. Batching is
  the design.
- **The transport and the presence signal already exist** once
  `PRD-wintermute-reach` and `PRD-wintermute-presence` ship. This PRD adds no
  new boundary — it's aggregation + cadence over capabilities already built.

## 2. What this builds

Extends `wintermute-reach` (does not create a new repo).

### 2.1 Presence aggregation

- The reach daemon additionally subscribes `wm.presence.summon` and
  `wm.presence.silence`.
- Maintains a rolling per-day tally: interaction count, first/last
  interaction timestamps, and whether a silence window fired.
- State persisted so a restart mid-day keeps the running tally.

### 2.2 Digest emission

- Config: a digest send time (e.g. 20:00 local) and an enable toggle.
- At the configured time, formats a short human line and delivers it through
  the existing `Transport` (same email/ntfy/webhook backend reach already
  uses) — **not** a new transport.
- Example bodies:
  - `Mom talked to wintermute 4 times today; last at 6:12pm.`
  - `Quiet day — Mom hasn't talked to wintermute since yesterday 7:40pm.`
- A flagged silence window escalates the digest to a slightly more prominent
  line but is still part of the daily send (distress remains the only
  instant path — the digest never tries to be an alarm).

### 2.3 Opt-in + reset

- No digest unless enabled in `/etc/wintermute/conf.d/` (family-enroll).
- The tally resets at the configured day boundary; the reset is logged.

## 3. Acceptance criteria

1. reach subscribes `wm.presence.summon` and `wm.presence.silence` in addition
   to its family topics (subscription test).
2. Four `wm.presence.summon` events across a simulated day produce a tally of
   4 with correct first/last timestamps (aggregation unit test).
3. At the configured digest time, exactly one digest delivery occurs through
   the configured transport, with a body containing the count and the
   last-interaction time (integration test with the fake transport).
4. A day with zero summons produces a "quiet day" digest body (test).
5. A flagged `wm.presence.silence` is reflected in that day's digest line
   (test) and does **not** trigger an immediate separate delivery (it waits
   for the digest — only distress is instant).
6. The per-day tally survives a daemon restart (state round-trip test).
7. The tally resets at the day boundary and the reset is logged (test).
8. With digest disabled in config, no digest is ever delivered (opt-in gate).
9. The digest reuses reach's existing `Transport` impl (no second transport
   code path — verified by construction/test).
10. `cargo test` green; `cargo clippy` clean; reach's existing family-message
    and ack tests still pass (no regression).
