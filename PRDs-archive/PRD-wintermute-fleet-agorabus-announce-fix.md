# PRD: wintermute fleet — missing `agorabus::Client::announce()` blocks all daemons but wm-audio

**Author:** Claude Opus 4.7 (wire-up session 2026-05-27)
**Status:** Draft v0.1
**Date:** 2026-05-27
**Builds on:** PRD-wintermute-tts, PRD-wintermute-stt, PRD-wintermute-dialog, PRD-wintermute-brain (all four ship today; this is a one-line follow-up patch per repo)
build_auto: true
build_target: rust-cli
build_priority: high
build_into: wintermute-tts, wintermute-stt, wintermute-dialog, wintermute-brain
extend_target: true

---

## TL;DR

`wm-tts`, `wm-stt`, `wm-dialog`, and `wmd` (brain) all connect to
agorabus and then immediately call `.subscribe()` without first calling
`.announce()`. The agorabus daemon enforces "first message must be
`Announce`" (see `agorabus/src/daemon.rs:281` — replies with
`announce_required` and closes the connection). Result: every fleet
daemon except `wm-audio` exits within ~1s of startup with
"bus closed; daemon exiting" or a `send_line` error. Today's wire-up
session shipped systemd user units and the bootstrap env file; only
`wm-audio` actually stays running.

The fix is identical in shape across all four repos — `wm-audio`
already does it right and is the reference implementation.

## Why this exists

This bug was caught during the first end-to-end fleet bring-up
(2026-05-27, 22:45 PDT). All four PRDs passed their own unit + integration
tests because the tests stub the agorabus `Client` (no protocol
enforcement). The defect only surfaces against a real `agorabus daemon`.
Future PRDs in this fleet will share the same shape and should use
`wm-audio`'s pattern from day one.

## What to change

### Reference (correct) implementation

`wintermute-audio/src/daemon.rs` (lines ~125–157, function
`AudioPipeline::run`):

```rust
let mut pub_client = Client::connect(&config.bus_socket).await?;
pub_client
    .announce(
        &config.session_id,
        std::process::id(),
        cwd_str(),
        "wm-audio mic/wake/vad pipeline",
    )
    .await?;

let mut sub_client = Client::connect(&config.bus_socket).await?;
sub_client
    .announce(
        &format!("{}-sub", config.session_id),
        std::process::id(),
        "",
        "wm-audio control subscribe",
    )
    .await?;

for prefix in [...] { sub_client.subscribe(prefix).await?; }
```

### Per-repo edit

In each of the four repos, locate the `run()` (or equivalently named)
function in `src/daemon.rs` that calls `agorabus::Client::try_connect()`
+ `agorabus::Client::connect()`, and insert `announce()` calls
immediately after each connect. Suggested session-id shape:
`format!("wm-{name}-{}", std::process::id())` for the publisher and
`format!("wm-{name}-{}-sub", std::process::id())` for the subscriber.
Intent strings should describe what the connection does (publisher /
subscriber).

Files to edit (one each, ~10-15 line insertion):

- `wintermute-tts/src/daemon.rs` — around line 815 (function `run`)
- `wintermute-stt/src/daemon.rs` — around line 213 (function `run`)
- `wintermute-dialog/src/daemon.rs` — around line 450 (function `run`)
- `wintermute-brain/src/daemon.rs` — around line 1310 (function `run`)

## Open-source dependencies

None new. The `announce()` method already exists on
`agorabus::Client` (see `agorabus/src/client.rs:120`).

## Acceptance criteria

1. After rebuilding all four repos and running `systemctl --user start
   wm-tts.service wm-stt.service wm-dialog.service wmd.service`, all
   four units stay `active (running)` for at least 60s with no restart
   loops in `journalctl --user -xeu <unit>`.
2. `agorabus peers` shows 8 wm-* peer entries (one pub + one sub per
   daemon, plus the existing two from `wm-audio`).
3. Existing unit + integration tests continue to pass (`cargo test
   --release` green in each repo). The in-memory test sinks already
   bypass the real protocol, so the change is invisible to them.
4. Each repo's daemon, when started against an `agorabus daemon` that
   is *not* running, still logs "agorabus not reachable; exiting" and
   exits 0 — i.e. the fail-open behavior is preserved.

## Out of scope

- Refactoring the four daemons to share a `connect_pair_and_announce`
  helper. That's a follow-up cleanup, not a Fleet 1 blocker.
- Heartbeat sending (the `Client::heartbeat()` method exists but is
  unused by the fleet today; agorabus tolerates that).
- Adding a startup acceptance test that exercises a real daemon. Worth
  doing eventually but out of scope here.

## Risks

- Session-id collision if a daemon is restarted with the same PID
  (unlikely in practice; PID reuse is observable but rare). If it
  matters, append `process::id()` + a nanos-since-epoch suffix.

## Notes for the autobuilder

This is a small surgical patch, not a fresh build. The receipt set can
be lighter than the default 25-receipt release gate: a passing
`cargo test --release` plus a manual smoke (start the daemon against
the live bus, observe peer registration via `agorabus peers`) is enough
to call it done. Each repo's `target/autobuilder/receipts/` should
get one receipt per AC above.
