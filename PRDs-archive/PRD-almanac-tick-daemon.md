# PRD: almanac-tick-daemon — fire the right entry at the right local time

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-almanac
Vision: visions/almanac.md

## TL;DR

The schedule store knows *what* and *when*; nothing yet *fires* it. This
PRD adds a `wm-almanac daemon` mode that, in each entry's IANA timezone,
sleeps until the next due entry and at that instant publishes
`wm.almanac.due {id, label, say, category}` to agorabus — plus a
systemd-user timer fallback for hosts that prefer oneshot ticks. This is
the heartbeat that turns a static list into spoken time.

## Why this exists

- **The store (PRD-almanac-schedule-store) is inert without a clock.** Its
  `wm-almanac next` already computes the next fire instant; this PRD turns
  that computation into a live publish loop.
- **The fleet has a proven poll-and-publish precedent to mirror.**
  `wintermute-calendar`'s `run_daemon` (`caldav.rs:292`) polls and emits a
  JSON envelope (`"type": "wm.cal.event.upcoming"`, `caldav.rs:303`). almanac
  uses the same publish shape but is driven by *local recurrence* computed
  in-process, not by a CalDAV fetch — no network, no credentials.
- **agorabus publish is the standard transport.** Daemons publish via an
  `agorabus::Client` (`wintermute-brain/src/daemon.rs:883-886`
  `client.publish(topic, data)`); the tick-daemon opens the same client.

## What this builds

Extends `wintermute-almanac`:

- `src/daemon.rs`: `run_daemon(store_path, client) -> Result<()>` loop:
  1. Load store; compute the soonest enabled entry's next fire instant
     (reuse the `next`-subcommand logic — factor it into a shared fn).
  2. Sleep until that instant (tokio timer; wake early if interrupted).
  3. On fire, publish `wm.almanac.due` with `{id, label, say, category,
     fire_ts}`; then advance that entry's recurrence and recompute.
  4. `Once` entries are marked fired (not re-armed); `Daily`/`Weekly`
     re-arm to their next occurrence.
- A `--once` flag for a single tick (compute → if anything is due within a
  small window, publish it, exit) so a systemd-user timer
  (`wm-almanac-tick.timer`, `OnCalendar=*:0/1`) can drive it without a
  long-lived process. Ship the `.timer` + `.service` units under the crate's
  `contrib/systemd/`.
- Degrade-out-loud hooks: if the store is unreadable or the clock looks
  wrong (next fire in the past by > 1 day), publish `wm.health.almanac`
  with a diagnostic rather than silently skipping (companion-degrade
  discipline).

`wm.almanac.due` envelope (documented in crate README so speak-bridge and
missed-to-kin agree): `{ "type": "wm.almanac.due", "id": "...", "label":
"...", "say": "...", "category": "med", "fire_ts": <unix> }`.

## Acceptance criteria

1. `wm-almanac daemon --once` with an entry due now publishes exactly one `wm.almanac.due` envelope carrying `id`, `label`, `say`, `category`, `fire_ts`; with nothing due it publishes nothing and exits 0.
2. Next-fire computation is DST-correct: a `Daily 08:00 America/Los_Angeles` entry fires at the local 08:00 across a DST boundary (test with chrono-tz against a date pair spanning the transition).
3. A `Weekly{Mon,Wed,Fri}` entry fires only on those weekdays; a `Once` entry fires once and is then marked fired (never re-armed); a disabled (`opt_in=false`) entry never fires.
4. In long-running `daemon` mode, after firing an entry the loop re-arms and the *same* daily entry fires again on the next day's tick (simulated by advancing the injected clock), proving re-arm works.
5. Publishing goes through `agorabus::Client::publish` to topic `wm.almanac.due`; the publish sink is behind a trait so tests assert envelope contents without a live bus (mirror `wintermute-brain`'s `EventSink` test pattern).
6. An unreadable store or a next-fire computed > 1 day in the past emits `wm.health.almanac` with a diagnostic and does not skip silently.
7. `contrib/systemd/wm-almanac-tick.{timer,service}` are valid units (`systemd-analyze verify` clean) driving `wm-almanac daemon --once` every minute.
8. `cargo test` green including the injected-clock fire tests; `wm-almanac daemon --help` documents `--once`.
