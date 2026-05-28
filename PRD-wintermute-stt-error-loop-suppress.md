# PRD: wintermute-stt — break the wm.stt.error feedback loop

**Author:** Claude (for the user)
**Status:** Draft v0.1
**Date:** 2026-05-28
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-stt
**build_version_bump:** patch
**Codename:** *snake-eats-tail* — wm-stt publishes its own decode errors onto the prefix it subscribes to.

---

## TL;DR

wm-stt subscribes to `wm.stt.` (via `STT_COMMAND_PREFIX`) AND publishes its `wm.stt.error` topic onto the same prefix. The bus broadcasts the error back to wm-stt, which tries to decode it, gets `unknown topic: wm.stt.error` (the error topic isn't an inbound command), publishes a new `wm.stt.error` describing the decode failure — and the cycle continues. In production today the loop saturates at >19,000 events/s and is masked only by the bus's broadcast-channel slot pressure (`broadcast::error::RecvError::Lagged` swallows most).

Confirmed live during bus-startup-defect verification (2026-05-28T20:31Z):

```
$ agorabus subscribe 'wm.stt.error' & sleep 0.3
$ agorabus publish wm.audio.speech.end '{"duration_ms":100,"ts":2}'
# 29,591 wm.stt.error events captured in 1.5s, all
# {"kind":"bus","message":"decode: unknown topic: wm.stt.error", ...}
```

The same pattern exists in wm-tts (`wm.tts.error` on the `wm.tts.` prefix it subscribes to) — confirmed but lower volume.

---

## 1. Per-repo targets

| Repo | Inbound prefix | Outbound error topic | Effect |
|---|---|---|---|
| wintermute-stt | `wm.stt.` | `wm.stt.error` | LOOP — high volume |
| wintermute-tts | `wm.tts.` | `wm.tts.error` | LOOP — confirmed but lower volume (request types are tagged enums, so a re-decode of `wm.tts.error` fails fast at the schema gate, still publishes a `wm.tts.error` for the second-order failure) |

wm-dialog and wm-brain don't publish errors on a prefix they themselves subscribe to (they each subscribe to upstream topics only), so they're untouched.

---

## 2. Recommended fix

Two equivalent options; pick one:

1. **Suppress decode failures for known-outbound topics.** In `wm-stt`'s subscribe loop, add a list of "topics this daemon emits" (`wm.stt.error`, `wm.stt.partial`, `wm.stt.final`, `wm.stt.uncertain`, `wm.stt.model_loaded`) and silently skip any event whose topic is in that set. Same for `wm-tts`. Cheap (a single `if` in the decode path); doesn't touch agorabus.
2. **Add a `from`-field filter.** `ServerEvent` carries `from: String`; skip events where `from == own_session_id`. Slightly cleaner but requires the daemon to know its own session id and read every event's `from` field.

Recommend #1: explicit topic-allow-list. Most defensive, no `from`-field roundtrip.

---

## 3. Acceptance tests

1. **AC1 — cargo test --release --lib green.** New unit test in each crate's daemon.rs that asserts decode_request silently skips the daemon's outbound topics OR the subscribe loop's filter does.
2. **AC2 — no recursive storm.** Live test: after restart, fire one `wm.audio.speech.end` (or `wm.tts.speak` for tts), then subscribe to the matching `wm.X.error` topic for 3 seconds — see ≤ 5 events (one expected first-order error if any, no recursion).
3. **AC3 — legitimate errors still propagate.** Live test: fire a genuinely malformed message on a known topic (e.g. `wm.audio.speech.start` with a missing field) — see exactly one `wm.X.error` event with the right `decode:` message; no follow-up storm.
4. **AC4 — `cargo deny check bans licenses sources` green.**
5. **AC5 — fail-open preserved** (existing AC7 from sibling PRDs).

---

## 4. Non-goals

- Don't change the error envelope schema.
- Don't filter at the bus daemon — the subscribe semantics should stay broad; subscribers know best what to ignore.
