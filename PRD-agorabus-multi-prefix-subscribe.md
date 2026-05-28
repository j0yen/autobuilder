# PRD: agorabus — multi-prefix subscribe (only last prefix wins today)

**Author:** Claude (for the user)
**Status:** Draft v0.1
**Date:** 2026-05-28
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/agorabus
**build_version_bump:** minor (semver: behaviour change, callers who relied on "last subscribe wins" will see all prior subscribes activated too)
**Codename:** *only-the-last* — `subscribe()` called N times means only the Nth prefix matters.

---

## TL;DR

The bus daemon's connection state holds a single `subscribed_prefix: Option<String>` slot (`agorabus/src/daemon.rs:184, 400-403`). Each `ClientMessage::Subscribe` op **overwrites** the prior prefix. So a client that does:

```rust
for prefix in ["wm.audio.", "wm.stt.", "wm.brain."] {
    sub_client.subscribe(prefix).await?;
}
```

only effectively subscribes to `"wm.brain."`. The first two are silently lost.

wm-audio and wm-dialog both rely on multi-prefix subscribe today. wm-audio's prefixes are `["wm.tts.", "wm.dialog.", "wm.audio.reload"]` — only `"wm.audio.reload"` actually matches (which by coincidence is what the user round-trip-tested when calling wm-audio "the reference baseline"). wm-dialog's prefixes are `["wm.audio.", "wm.stt.", "wm.brain."]` — only `"wm.brain."` is live. This is why the bus-startup-defect verification couldn't drive wm-dialog through a `wm.stt.final` round-trip even though dialog's subscribe-completion log said `subscribed prefixes=["wm.audio.", "wm.stt.", "wm.brain."]`.

---

## 1. Fix shape

Change `subscribed_prefix: Option<String>` to `subscribed_prefixes: Vec<String>` (or `BTreeSet<String>`), append on each `Subscribe`, and change `topic_matches` to "any prefix matches".

Keep backward-compatibility behaviour: clients that issue exactly one `subscribe()` call see no change. Clients that issue multiple now actually receive all matching events instead of just the last prefix's.

---

## 2. Acceptance tests

1. **AC1 — cargo test --release --lib green.** New test in agorabus daemon that opens a client conn, calls subscribe twice with disjoint prefixes, publishes one event for each prefix, asserts both arrive.
2. **AC2 — wm-dialog actually sees wm.audio.* and wm.stt.* events.** Rebuild + reinstall wm-dialog after the agorabus bump; restart it; publish `wm.stt.final` with a valid payload; observe wm-dialog's FSM snapshot_ms advance.
3. **AC3 — wm-audio sees wm.tts.* and wm.dialog.* events.** Same shape with wm-audio.
4. **AC4 — `cargo deny check bans licenses sources` green.**
5. **AC5 — single-prefix clients unaffected.** Existing agorabus acceptance tests still pass.

---

## 3. Cascade

After agorabus ships, bump the path-dep pin in tts/stt/dialog/brain/audio to the new minor, rebuild, reinstall, and run AC2/AC3 of this PRD against each.
