# PRD: wintermute fleet — bus heartbeat keepalive

**Author:** Claude (for the user)
**Status:** Draft v0.1
**Date:** 2026-05-28
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute  # multi-repo; see §3 per-target
**build_version_bump:** patch
**Depends on:** PRD-wintermute-fleet-bus-startup-defect (shipped 2026-05-28T20:36Z)
**Codename:** *quietkill* — the daemon stays connected, but vanishes from `agorabus peers` after 60s.

---

## TL;DR

The bus-startup-defect PRD shipped the stale-binary fix (v0.1.1 across tts/stt/dialog/brain). Live verification revealed the next-shelf issue: **the four daemons never send heartbeats after their initial announce**, so the bus daemon's `Peers {}` handler prunes them at 60s heartbeat-age even though their UDS connections are alive and they're still receiving + dispatching events. AC3 of the bus-startup PRD ("agorabus peers list shows the daemon after the 60s window") fails strictly for all four; it passes only within the first 60s.

agorabus already shipped `c3777ca fix: subscribe keeps last_heartbeat fresh` providing the machinery (`agorabus::client::send_heartbeat()` + `Client::into_halves()`), and the `agorabus subscribe` CLI uses it. The four daemons need to spawn the same heartbeat ticker on their subscribe-client.

wm-audio is the reference baseline and shows the same defect — it's not exempt from this fix.

---

## 1. Observed behaviour (2026-05-28T20:42 local)

```
$ systemctl --user restart wm-tts.service wm-stt.service wm-dialog.service wmd.service
$ sleep 5 && agorabus peers | jq -r '.[].session_id' | grep -E 'wm-(tts|stt|dialog|brain)' | wc -l
8                                  # 2 peers each × 4 daemons; AC3 within-window PASS

$ sleep 65 && agorabus peers | jq -r '.[].session_id' | grep -E 'wm-(tts|stt|dialog|brain)' | wc -l
0                                  # AC3 strict (after 60s window) FAIL

$ systemctl --user is-active wm-tts.service wm-stt.service wm-dialog.service wmd.service
active active active active        # daemons still alive, just invisible to peers query
```

The daemons keep receiving broadcast events fine — the bus only evicts from the peers *snapshot*, not from the bcast distribution. So `wm.tts.speak` → wm-tts → `wm.tts.start` round-trip keeps working past the 60s eviction; only `agorabus peers` lies.

---

## 2. What's already in place (don't redo)

1. `agorabus::client::send_heartbeat(write, tool)` — free fn that sends a `Heartbeat { tool, skill: None, prd_slug: None, working_paths: None }` over an `OwnedWriteHalf` without a reply read.
2. `agorabus::Client::into_halves()` — splits a client into `(OwnedWriteHalf, Lines<...>)` for half-duplex use.
3. `agorabus::Client::next_event()` — already skips `InboundLine::Reply` so heartbeat replies on a subscribed wire don't crash the stream.
4. The bus daemon's `Peers {}` handler at `agorabus/src/daemon.rs:407` prunes by `now - last_heartbeat_unix_secs > DEFAULT_HEARTBEAT_TIMEOUT_SECS (60)`.

So all the machinery is there; the four daemons just need to opt in.

---

## 3. Per-repo targets

| Repo | Where the subscribe loop lives | `build_into` |
|---|---|---|
| wintermute-tts | `src/daemon.rs:854` `while let Some(ev) = sub_client.next_event()` | `/home/jsy/wintermute/wintermute-tts` |
| wintermute-stt | `src/daemon.rs:251` same shape | `/home/jsy/wintermute/wintermute-stt` |
| wintermute-dialog | `src/daemon.rs:494` `tokio::select! { next = sub_client.next_event() ... }` | `/home/jsy/wintermute/wintermute-dialog` |
| wintermute-brain | (same pattern; check `src/daemon.rs` subscribe loop) | `/home/jsy/wintermute/wintermute-brain` |
| wintermute-audio | `src/daemon.rs` control loop in `run_control_loop` | `/home/jsy/wintermute/wintermute-audio` |

This one DOES include wintermute-audio — wm-audio has the same defect, and there's no reference baseline to preserve (the audio binary at ~/.cargo/bin/wm-audio is currently the same shape as the others).

---

## 4. Recommended implementation shape

For each daemon, transform the subscribe client into a heartbeat-driven dual-task pattern. Sketch:

```rust
// Before
let mut sub_client = agorabus::Client::try_connect(&sock).await?...;
sub_client.announce(...).await?;
sub_client.subscribe(...).await?;
while let Some(ev) = sub_client.next_event().await? { ... }

// After
let mut sub_client = agorabus::Client::try_connect(&sock).await?...;
sub_client.announce(...).await?;
for prefix in PREFIXES { sub_client.subscribe(prefix).await?; }
let (mut write_half, lines) = sub_client.into_halves();
let hb_task = tokio::spawn(async move {
    let mut iv = tokio::time::interval(Duration::from_secs(
        agorabus::DEFAULT_HEARTBEAT_TIMEOUT_SECS / 2,
    ));
    iv.tick().await;  // skip first
    loop {
        iv.tick().await;
        if let Err(e) = agorabus::client::send_heartbeat(&mut write_half, TOOL_NAME).await {
            tracing::warn!(error=%e, "heartbeat failed; subscriber wire likely dead");
            break;
        }
    }
});
// Rewrap the read half into something that emits events without the original Client.
// (Or keep sub_client alive but spawn the heartbeat using a *clone* — not possible with
// OwnedWriteHalf. The cleanest path is into_halves + a custom read loop that uses
// InboundLine to filter Reply lines.)
```

If `into_halves` makes the rewrite too invasive, a simpler v1: spawn a second `Client::connect` (a third connection, one for HEARTBEAT only) that issues `heartbeat` every 30s and discards the reply. The bus's `Heartbeat` op updates the `peers` entry keyed by `session_id`, so the third connection just needs to announce with the same `session_id` as the publish-client and tick.

(Actually no — a fresh announce on the same `session_id` from a different conn becomes a *guest* per `agorabus/src/daemon.rs:306` and won't refresh the original record. So heartbeats must travel on the *announce-owner* connection. Use `into_halves` on the publish client, which is mostly idle in three of the four daemons.)

---

## 5. Acceptance tests

For each daemon (including wm-audio this time):

1. **AC1 — local cargo test green.** `cd $build_into && cargo test --release --lib` exits 0.
2. **AC2 — daemon survives 5min under systemd.** `systemctl --user restart $svc.service && sleep 300 && systemctl --user is-active $svc.service` → `active`, `NRestarts <= 1`.
3. **AC3 — agorabus peers list still shows the daemon after 70s.** `systemctl --user restart $svc.service && sleep 70 && agorabus peers | jq -r '.[].session_id' | grep -c "$svc"` ≥ 1.
4. **AC4 — bus round-trip still works after 70s.** Use the same per-daemon publish from the bus-startup-defect PRD, but fire it ≥ 70s after restart. Log shows dispatch within 5s.
5. **AC5 — `cargo deny check bans licenses sources` green.**
6. **AC6 — heartbeat does not interfere with event delivery.** Pre-flight smoke: subscribe to the daemon's outbound prefix, fire 10 inbound events at 1Hz, observe all 10 arrive without loss (the heartbeat ticker is on a separate task so it shouldn't preempt event reads).
7. **AC7 — fail-open preserved.** Same shape as bus-startup-defect AC7 — bus down → daemon exits status 0.

---

## 6. Non-goals

1. **No agorabus protocol change.** The Heartbeat op already exists and works.
2. **No bus daemon change.** The prune logic is correct — daemons should heartbeat.
3. **No fix to the wm-stt → wm.stt.error → wm-stt feedback loop.** That's a separate PRD (the publish_error path republishes onto a subscribed prefix; each error generates another error, ad infinitum). Raise as `PRD-wintermute-stt-error-loop-suppress` if not already queued.
4. **No fix to the bus daemon's single-`subscribed_prefix` slot.** Each `Subscribe` op overwrites the prior prefix; only the LAST `subscribe()` call survives. wm-dialog, wm-audio, and wm-brain all call subscribe in a loop and so only effectively see their last prefix. That's a real defect but agorabus-side; raise as `PRD-agorabus-multi-prefix-subscribe` if not already queued.

---

## 7. Ordering / dependency notes

- Per-repo patches sibling fixes; any order is fine.
- All five (including wm-audio) should land before the announce-fix and bus-startup-defect PRDs are considered fully closed at the AC3 strict gate.
- Suggested order: audio → tts → stt → dialog → brain (start with the reference and propagate the pattern outward).
