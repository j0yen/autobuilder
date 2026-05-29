# PRD: wintermute-companion — graceful degradation (say what's wrong)

**Author:** /dream (Claude Opus 4.7), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/companion.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-brain
**build_version_bump:** minor
**Depends on:** PRD-wintermute-dialog-turn-fsm
**Codename:** *say-so* — when the daemon can't think or hear or speak, it says so out loud rather than going silent.

## TL;DR

Today when wmd (brain) loses its API key, the network is down, STT misfires, or the mic disappears, wintermute goes silent and waits. For a developer this is fine: check the journal, find the error, fix the config. For mother this is unusable: she said something, nothing happened, she has no recourse. This PRD ships a small phrase bank ("I can't reach my brain right now," "I lost my microphone," "Hold on, I'm reconnecting"), a `wm.health.*` envelope set, and the routing rules that turn a failure into spoken output through the existing wm-tts path.

## 1. Why this exists

- **Silence is a failure mode.** A device that doesn't respond to a wake word is broken to the user, regardless of which component failed.
- **The companion vision names this as Component 7.** Without it, the deployment is unsafe — mother can't tell whether wintermute is thinking, broken, or off.
- **Every component already publishes errors.** wm-stt → wm.stt.error, wm-tts → wm.tts.error, wm-audio → wm.audio.error, wmd → wm.brain.error. They're emitted; nothing aggregates them into voice output.

## 2. What this builds

### 2.1 Phrase bank in wm-brain

A new module `src/degrade.rs` in wintermute-brain with a static lookup table:

```rust
fn degrade_phrase(kind: &str) -> &'static str {
    match kind {
        "brain_unreachable"      => "I can't reach my brain right now. Try again in a moment.",
        "brain_api_key_missing"  => "I'm not configured to think yet. Could you ask jsy?",
        "stt_window_invalid"     => "Sorry, I didn't catch that.",
        "stt_model_missing"      => "My ears aren't installed yet.",
        "audio_mic_missing"      => "I lost my microphone. Hold on.",
        "audio_aec_missing"      => "My echo cancellation isn't working; I might hear myself.",
        "tts_pw_cat_missing"     => /* publishable only via UI/log; can't speak this */ "",
        "network_down"           => "I can't reach the network. I'll wait.",
        "general_error"          => "Something went wrong. Let me try again.",
        _ => "Something I haven't seen before just happened.",
    }
}
```

### 2.2 Aggregator subscription

wmd subscribes to `wm.stt.error`, `wm.tts.error`, `wm.audio.error`, `wm.brain.error`. On any error, look up the `kind` field, fetch the phrase, publish `wm.tts.speak` with `priority: "system"` to interrupt anything in progress.

### 2.3 Rate limiting

A failing component can spam errors at high rate. The aggregator must NOT speak the same degrade phrase more than once per 30 seconds. Track per-kind last-spoken timestamp.

### 2.4 Health envelope set

Publish `wm.health.snapshot` every 60 seconds with a struct describing each component's last-known state (`{component: "audio|stt|tts|brain", state: "ok|degraded|down", last_error: "...", last_seen_ts: ...}`). External tools (status display, jsy's dashboard) can consume this.

### 2.5 Self-emitted-topic filter

`wm.health.*` topics MUST be in wm-brain's self-emitted allow-list.

## 3. Acceptance tests

1. **AC1 — `cargo test --release --lib` ≥ current+8** (phrase lookup, rate limit, aggregator, health snapshot, fallback for unknown kind, integration tests).
2. **AC2 — daemon active 60s, NRestarts=0.**
3. **AC3 — degrade phrase publishes on simulated error.** Test harness publishes `wm.stt.error {"kind":"stt_window_invalid"}`. wmd publishes `wm.tts.speak` with text matching the phrase bank within 100ms.
4. **AC4 — rate limit.** Same `wm.stt.error` published 10 times in 5s. wmd publishes `wm.tts.speak` exactly once (the first); subsequent firings within the 30s window are suppressed.
5. **AC5 — different errors don't suppress each other.** stt error + tts error in same window both fire the appropriate phrases.
6. **AC6 — unknown kind falls back to generic phrase.** Publish `wm.stt.error {"kind":"unknown_specific"}`. wmd publishes "Something I haven't seen before just happened."
7. **AC7 — health snapshot envelope.** Subscribe to `wm.health.snapshot` for 65s; receive ≥1 envelope with all four components reported.
8. **AC8 — priority routes through TTS.** Publishing `wm.tts.speak` with `priority: "system"` while regular speech is in progress: TTS cancels in-progress speech (using the AC5 cancel hook from pipewire-output) and speaks the degrade phrase.
9. **AC9 — `cargo deny check bans licenses sources` clean.**
10. **AC10 — live human gate.** Stop wm-stt service (simulate model gone). Speak wake-word + sentence. Within 5s, hear the degrade phrase through the speaker. Restart wm-stt; conversation returns to normal.

## 4. Non-goals

1. **Detailed error narration.** "I can't connect because port 443 to api.anthropic.com is blocked" is too technical. The phrase bank is intentionally simple.
2. **Multi-language.** English only.
3. **Personality variation.** Each phrase is fixed for v0.1. Tone tuning is a future PRD.
4. **Automatic recovery actions.** wmd says what's wrong but doesn't try to fix anything. Recovery is each component's own concern (retry-backoff, etc.).
5. **A status UI.** `wm.health.snapshot` is published; consumers (a dashboard, a status LED) are sibling PRDs.

## 5. Open questions

- Should mother be able to ask "wintermute, what's wrong?" and get the current health summary as a spoken sentence? Likely yes; that's an extension of this PRD into a `wm.dialog.health_query` intent. Deferred.
- Should the degrade phrases be configurable per deployment? `/etc/wintermute/phrases.yaml`? Future.
- Tone: "I can't reach my brain right now" might worry mother. "Hold on, I'm thinking..." is gentler. PRD ships the blunt version; deployment tunes.

## 6. Files this PRD likely touches

- New: `src/degrade.rs` (phrase bank + aggregator + rate limit)
- Modified: `src/daemon.rs` (subscribe to error topics; spawn the health-snapshot ticker)
- Modified: `src/events.rs` (Topics for wm.health.snapshot; self-emitted filter)
- Modified: `src/bus.rs` (TTS publish path with `priority: "system"`)
- Modified: `README.md`, `CHANGELOG.md`
- New: `tests/integration/degrade.rs`
