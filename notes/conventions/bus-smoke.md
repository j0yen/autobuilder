# Convention: bus-smoke tests for wm-* daemons

**Established:** 2026-05-28 via PRD-wintermute-fleet-bus-smoke-convention
**Status:** active — applies to every wm-* daemon that links `agorabus`
**Canonical prior art:** `wintermute-audio/tests/wake_bus_smoke.rs` (lines 82–165)

## Why this exists

Four fleet daemons shipped 2026-05-27/28 with the same protocol bug:
they called `agorabus::Client::connect()` and then `.subscribe()`
without first calling `.announce()`. The agorabus daemon enforces
"first message must be `Announce`" at
`agorabus/src/daemon.rs:315` — it replies `announce_required` and
closes the connection. All four daemons exited within ~1 s of
contacting the real bus.

`wintermute-audio` already did this right because it had
`wake_bus_smoke.rs`, `vad_bus_smoke.rs`, and `reload_bus_smoke.rs`
exercising the daemon against an in-process agorabus before merge.
The other four repos had only `acceptance_template.rs` placeholders.

This convention closes that gap. Every fleet daemon that links
agorabus must ship at least one `tests/bus_smoke.rs` that exercises
its real-protocol wire-up against a live in-process bus before its
PRD can archive.

## Where

`tests/bus_smoke.rs` in the repo root. One per repo. Canonical name
for the announce-ordering regression check.

Per-feature smoke files (e.g., `wake_bus_smoke.rs`, `vad_bus_smoke.rs`)
are fine alongside and additive — they cover specific publish paths.
`bus_smoke.rs` covers the minimum: connect + announce + subscribe +
≥1 publish-through.

## Shape

1. Spawn an in-process `agorabus` daemon via `agorabus::run_daemon` on
   a per-test temp socket. Use `wm-audio`'s `tmp_path` helper or
   equivalent: the socket's parent directory must be a fresh
   `pid+nanos` subdir (agorabus chmods the parent to 0700 on bind;
   pointing it at `/tmp` directly silently fails).
2. Wait on the `ready_tx: oneshot::Sender<()>` that `run_daemon`
   signals once the listener is bound.
3. Connect a test subscriber with `Client::connect`. **Call
   `.announce(...)` before any `.subscribe(...)` or `.publish(...)`.**
4. Subscribe to the daemon-under-test's topic prefix.
5. Start the daemon-under-test pointed at the same socket.
6. Drive the daemon with whatever scripted input the repo's other
   smoke tests use.
7. Assert: (a) the daemon stays up for ≥2 s without an
   `announce_required` error anywhere in its anyhow chain; (b) at
   least one expected event publishes through the bus.
8. Shut down via `oneshot::Sender<()>` — never let the daemon task
   leak. Use a Drop guard if the test panics mid-flight.

## CI behavior

- `cargo test --release --test bus_smoke` runs in regular CI.
- **No `#[ignore]`.** No env witness. A bus-client misuse must
  surface as a compile/test failure, not a runtime surprise after
  ship.
- agorabus runs in-process — no hardware, no external services, no
  network. The test is fully self-contained.

## Anti-cargo-cult gate

The test body must contain at least one explicit `client.announce(...)`
call **before** any `client.subscribe(...)` or `client.publish(...)`.
This is positive evidence that the author understood the ordering.
A test that connects without announcing reproduces the bug it's
supposed to catch.

## 30-line skeleton

```rust
use std::path::PathBuf;
use std::time::Duration;
use agorabus::{Client, DaemonConfig, run_daemon};
use tokio::time::timeout;

fn tmp_path(tag: &str, ext: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("wm-XXX-test-{pid}-{nanos}"));
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{tag}.{ext}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bus_smoke() {
    let bus_sock = tmp_path("bus", "sock");
    let bus_cfg = DaemonConfig {
        socket_path: bus_sock.clone(),
        heartbeat_timeout: Duration::from_secs(60),
        broadcast_capacity: 1024,
    };
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let bus_task = tokio::spawn(async move {
        let _ = run_daemon(bus_cfg, Some(ready_tx), shutdown_rx).await;
    });
    timeout(Duration::from_secs(2), ready_rx).await.unwrap().unwrap();

    let mut sub = Client::connect(&bus_sock).await.unwrap();
    sub.announce("XXX-bus-smoke-sub", std::process::id(), "", "test-sub").await.unwrap();
    sub.subscribe("wm.XXX").await.unwrap();

    // ... start daemon-under-test, drive it, assert event arrives ...

    let _ = shutdown_tx.send(());
    let _ = bus_task.await;
}
```

Replace `wm-XXX` / `wm.XXX` with the daemon under test. Add a Drop
guard around `shutdown_tx` if the assertions can panic.

## Promotion path

- `bus_smoke.rs` is the **minimum** every daemon must have.
- Per-feature smoke files (`wake_bus_smoke.rs`, `vad_bus_smoke.rs`,
  `reload_bus_smoke.rs`) are additive, not replacements. Use them
  when the daemon has multiple distinct publish paths.
- If 8+ repos start consuming this pattern and the skeleton drifts,
  factor into a `wintermute-bus-test` crate. Premature today with
  ~4–10 consumers.

## Fleet 2 hook

Every new `wm-*` daemon PRD — browser, desktop, screen-narrate,
mail, calendar, music, and any future Fleet 2 addition — must
include a `tests/bus_smoke.rs` before its PRD can archive. /dream's
PRD-drafting checklist references this convention. /build's
verified-completed check #5 already accepts named cargo tests as AC
pairing surface; the new `bus_smoke.rs` is a regression gate for
the class of bug, not paired to any specific AC.

## Related

- `PRD-wintermute-fleet-agorabus-announce-fix.md` — patches the four
  daemons that shipped with the bug.
- `agorabus/src/daemon.rs:315` — the source of truth for the
  protocol invariant.
- `wintermute-audio/tests/{wake,vad,reload}_bus_smoke.rs` —
  canonical prior art.
