# PRD: agorabus-drain-notice — the bus says goodbye before it exits

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/agorabus
Vision: visions/vigil.md

## TL;DR

When the agorabus daemon is asked to shut down for a code roll, it
should not just vanish — it should tell its subscribers it is going and
when to come back. This PRD adds a graceful-drain step to the daemon's
existing SIGTERM/SIGINT handler: broadcast a
`{"op":"bus.draining","resume_after_ms":N}` notice to every subscriber,
flush state, close the listener, then exit. Reconnecting clients
(PRD-agorabus-client-reconnect) use `resume_after_ms` as their first
backoff so the whole fleet doesn't thundering-herd the socket the instant
the new daemon binds.

## Why this exists

- **The bounce should be coordinated, not abrupt.** The daemon already
  installs a clean shutdown path — `main.rs:237-243` waits on
  `SignalKind::terminate()` / `interrupt()` and drives a
  `tokio::sync::oneshot` `shutdown` channel that `run_daemon`
  (`daemon.rs:100`, `daemon.rs:145` `_ = &mut shutdown =>`) selects on.
  Today that path closes silently: subscribers learn the bus is gone only
  by reading EOF. There is no "draining" signal — Phase 1 (2026-05-29)
  grepped `signal|shutdown|drain` and found only the bare oneshot.
- **Avoids a reconnect stampede.** Once PRD-agorabus-client-reconnect
  ships, every live subscriber will retry on EOF. Without a server-
  suggested delay they all retry on the same backoff schedule and hit the
  rebinding socket simultaneously. A `resume_after_ms` hint (covering the
  expected rebuild+rebind window) staggers them.
- **Makes `agorabus reload` observable.** PRD-agorabus-reload needs a way
  to ask the daemon to leave gracefully and know subscribers were
  notified; the drain broadcast is that mechanism.
- Serves vigil's end-state: "gracefully restarts → polls agorabus
  `peers` to confirm re-registration" (visions/vigil.md). "Gracefully"
  means the drain notice, not a bare `kill`.

## What this builds

Extends `~/wintermute/agorabus/` (rust-extend; augments the existing
shutdown path, adds one protocol event). Current version 0.4.0.

- A new server→client event `bus.draining` carrying `resume_after_ms`
  (u64). Added to the `ServerEvent` enum in `src/protocol.rs` as a new
  variant so existing clients that don't recognize it ignore it (the
  reconnect loop and any future consumer match on it; older one-shot
  clients never see it because they exit before drain).
- The SIGTERM/SIGINT handler (driven from `main.rs:237`) first sends the
  drain broadcast through the daemon-wide broadcast channel
  (`BroadcastMsg`, `daemon.rs:~95`) to all subscribers, waits a short,
  bounded grace period (default 200ms, `--drain-grace-ms`) for the writes
  to flush, *then* resolves the existing `shutdown` oneshot to stop
  accepting and exit. Existing behavior (clean exit) is preserved; only
  the pre-exit notice is new.
- `resume_after_ms` value: a daemon flag `--drain-resume-hint-ms`
  (default 3000) so `agorabus reload`/`rollout` can pass a window sized to
  the expected rebuild. The daemon does not compute it; it relays the
  configured hint.
- The notice is best-effort: if a subscriber's write would block past the
  grace period, the daemon proceeds to exit anyway (drain must never
  hang shutdown — a stuck subscriber cannot wedge a roll).

No change to `peers`/`publish`/`heartbeat`/`claim`/`intent` semantics. No
new dependency.

## Acceptance criteria

1. **AC1 — drain notice delivered.** A subscriber attached to any prefix
   receives exactly one `{"op":"bus.draining","resume_after_ms":N}` line
   when the daemon is sent SIGTERM, *before* its connection reaches EOF.
   Integration test: `tests/acceptance_drain_notice.rs` sends SIGTERM and
   asserts the drain line precedes EOF on the subscriber stream.
2. **AC2 — resume hint is configurable.** Launching the daemon with
   `--drain-resume-hint-ms 1500` yields `resume_after_ms: 1500` in the
   delivered notice.
3. **AC3 — shutdown still terminates promptly.** With a subscriber whose
   read side is stalled, SIGTERM still causes the daemon process to exit
   within `drain-grace-ms + small_margin` (drain never wedges shutdown).
   Test uses a subscriber that never reads and asserts daemon exit within
   the bound.
4. **AC4 — clean-exit semantics preserved.** After SIGTERM the daemon
   exits 0 and the socket file is gone / a fresh daemon can bind the same
   path — matching pre-PRD shutdown behavior.
5. **AC5 — backward compatible.** A 0.4.0-style one-shot client
   (`publish`, `peers`) that connects and disconnects before any drain is
   unaffected; the existing acceptance suite passes unchanged.
6. **AC6 — unknown-event tolerance documented.** The README notes that
   `bus.draining` is an advisory event clients may ignore; a subscriber
   built before this PRD (no match arm) treats it as an unrecognized line
   and continues — test asserts a minimal line-reader does not crash on
   the new event.
