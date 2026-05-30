# PRD: agorabus-client-reconnect — the subscriber survives a daemon bounce

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/agorabus
Vision: visions/vigil.md

## TL;DR

The long-lived `agorabus subscribe` client has no reconnect logic. When
the daemon dies (a restart to roll fresh code in), the subscriber's
socket returns EOF and the process exits — permanently. The session it
represents vanishes from `peers` and stops receiving events until the
*entire Claude session* restarts and re-runs the SessionStart hook. This
PRD makes the `subscribe` loop reconnect: on EOF / connection-reset it
re-opens the socket with bounded backoff + jitter, re-announces,
re-subscribes to the same prefix(es), and resumes appending to the same
inbox ndjson — so a daemon bounce is a blip, not a session death.

## Why this exists

- **Root-cause of the carried-forward stale-binary debt.** Self-review
  has flagged "agorabus daemon stale binary" for 4+ consecutive runs
  (`~/brain/journal/2026-05-{27,28,29}.md`; recall `01KSRV7R4FERPP…`).
  It escalates instead of auto-fixing whenever subscribers > 5
  (`self-review/SKILL.md:259`) because the bounce is destructive:
  `SKILL.md:270` — "other live Claude sessions will need to re-run their
  SessionStart hook to reattach after restart, and that's a user-visible
  disruption." The disruption exists *only* because the client can't
  reconnect.
- **Confirmed absent.** Phase 1 (2026-05-29) grepped
  `agorabus/src/client.rs` for `reconnect|resubscribe|EOF|loop` — the
  subscribe path (`client.rs:271` `subscribe`, `client.rs:278`
  `next_event` "Returns `Ok(None)` on EOF") reads until EOF and returns;
  there is no re-open. `main.rs:319` `Command::Subscribe` consumes that
  stream and exits when it ends.
- **The hook can't cover it.** `agorabus-session-start.sh` re-registers a
  session, but it is a *SessionStart* hook — it fires once when a Claude
  session begins, never when the daemon dies mid-session. The
  `agorabus_orphan_subscriber` playbook (`SKILL.md:272`) can only
  re-trigger the hook for *this* self-review session, not for other live
  sessions (`SKILL.md:285`).
- **Resolves vigil Open Question #3** ("Restart vs reload … is a brief
  peer-drop acceptable?"). The answer is "yes, *if* clients reconnect" —
  this PRD supplies that.

## What this builds

Extends `~/wintermute/agorabus/` (rust-extend; preserves all existing
behavior, adds reconnect to the long-lived subscribe path only). Current
version 0.4.0.

- A reconnect wrapper around the subscribe loop. The existing
  `BusClient` connect → announce → subscribe → `next_event` sequence
  (`src/client.rs`) is wrapped so that when the inner loop returns on EOF
  or errors with a connection-level error (`ECONNRESET`,
  `BrokenPipe`, `ConnectionRefused`, socket missing), the client:
  1. logs a structured `reconnecting` line to stderr (sid, attempt#,
     delay_ms),
  2. sleeps `min(cap, base * 2^attempt) + jitter` (defaults: base 100ms,
     cap 5s, full jitter),
  3. re-opens the socket, re-`announce`s with the same session_id / pid /
     cwd / intent, re-`subscribe`s to the same prefix(es),
  4. continues streaming into the same output sink (the same ndjson the
     SessionStart hook redirects to).
- A `--max-reconnect-attempts <n>` flag (default: unbounded / `0` = forever)
  and `--reconnect-base-ms` / `--reconnect-cap-ms` overrides, so tests can
  bound the loop. Reconnect is **on by default** for the long-lived
  subscribe command; a `--no-reconnect` flag restores the old exit-on-EOF
  behavior for one-shot/scripted use.
- If a `bus.draining` notice is in scope (PRD-agorabus-drain-notice, may
  ship after this), the loop honors `resume_after_ms` as the first
  backoff; until that PRD lands, the loop simply uses its own backoff
  schedule. This PRD does **not** depend on drain.
- The reconnect attempt counter resets to 0 after any successful
  re-subscribe that survives ≥ `reconnect-cap-ms`, so a flapping daemon
  doesn't escalate backoff forever but a clean reconnect starts fresh.

No protocol change. No daemon change. No new dependency beyond what's
already in the tree (`tokio` time for the sleep is already a dep).

## Acceptance criteria

1. **AC1 — reconnect on daemon death.** With a daemon running and a
   `subscribe` client attached, killing and relaunching the daemon
   results in the *same* client process re-appearing in `agorabus peers`
   (same session_id) within `reconnect-cap-ms + bind_time`, without the
   client process exiting. Integration test:
   `tests/acceptance_reconnect_survives_restart.rs`.
2. **AC2 — events resume after reconnect.** A `publish` to the
   subscribed prefix issued *after* the daemon relaunch is delivered to
   the reconnected client and appended to its output sink. Test asserts
   the post-restart event line appears in the captured stdout/ndjson.
3. **AC3 — backoff is bounded and jittered.** With
   `--reconnect-base-ms 50 --reconnect-cap-ms 400`, observed inter-attempt
   delays are non-decreasing up to the cap and never exceed
   `cap + jitter`; a unit test over the delay schedule asserts the
   `min(cap, base*2^n)` shape.
4. **AC4 — `--max-reconnect-attempts` terminates.** With
   `--max-reconnect-attempts 2` and no daemon ever coming back, the
   client exits non-zero after exactly 2 failed reconnects (test uses a
   socket path with no daemon).
5. **AC5 — `--no-reconnect` preserves old behavior.** With
   `--no-reconnect`, the client exits 0 on EOF exactly as 0.4.0 does
   (regression guard for one-shot callers).
6. **AC6 — no behavior change for non-subscribe ops.** `peers`,
   `publish`, `heartbeat`, `claim`, `intent`, and the fail-open
   no-daemon path (existing AC6 of agorabus) are unchanged — the existing
   acceptance suite still passes.
