# PRD: wintermute-dialog — turn-taking state machine

**Author:** /dream (Claude Opus 4.7), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/companion.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-dialog
**build_version_bump:** minor
**Depends on:** PRD-wintermute-audio-inference, PRD-wintermute-stt-whisper-model
**Codename:** *take-turns* — wm-dialog is alive on the bus but the actual turn FSM is partial.

## TL;DR

`wm-dialog` is bus-healthy (heartbeat shipped today). Its README says "conversational FSM, turn-taker, barge-in." Its source has the state enum and the subscribe loop. What's missing is the *transitions* — code that says: when in Listening + wake → enter Capturing; when in Capturing + speech.end → enter Transcribing; when in Transcribing + stt.final → enter Thinking; when in Thinking + brain.reply → enter Speaking; when in Speaking + tts.end → return to Listening. This PRD ships those transitions, plus a barge-in path (Speaking + wake → publish wm.tts.cancel → enter Capturing).

## 1. Why this exists

- **Upstream events finally exist.** Before today nothing emitted wake / speech / stt.final / brain.reply / tts.end as a real signal stream. Now (after audio-inference + stt-whisper ship) they will. The FSM has been waiting for inputs; this PRD wires it.
- **Without the FSM, the fleet has no conversation loop.** Even if every other component works, there's no orchestrator that says "she just spoke; transcribe; think; reply; listen again."
- **Barge-in is the user-experience differentiator.** A companion that can't be interrupted mid-sentence is alienating. The hook is already in wm-tts (AC5 of pipewire-output verified the cancel path); wm-dialog has to *invoke* it.

## 2. What this builds

### 2.1 The state machine

```rust
enum DialogState {
    Listening,           // wake-word armed; mic events pass through
    Capturing,           // wake fired; awaiting speech.end
    Transcribing,        // speech.end fired; awaiting stt.final
    Thinking,            // stt.final emitted; awaiting brain.reply
    Speaking { reply_id }, // brain.reply received; awaiting tts.end
}
```

Transitions:

| From | Event | To | Side effect |
|---|---|---|---|
| Listening | `wm.audio.wake` | Capturing | publish `wm.dialog.attention` (UI hook) |
| Capturing | `wm.audio.speech.end` | Transcribing | — |
| Capturing | timeout 8s | Listening | publish `wm.dialog.timeout` |
| Transcribing | `wm.stt.final` | Thinking | publish `wm.dialog.heard` with text |
| Transcribing | `wm.stt.uncertain` | Listening | publish `wm.dialog.unheard` |
| Transcribing | timeout 3s | Listening | publish `wm.dialog.timeout` |
| Thinking | `wm.brain.reply` | Speaking { reply_id } | publish `wm.tts.speak` with reply text |
| Thinking | `wm.brain.error` | Listening | publish via `wm.tts.speak` from degrade phrase bank |
| Thinking | timeout 10s | Listening | publish degrade phrase |
| Speaking | `wm.tts.end` outcome=ok | Listening | — |
| Speaking | `wm.tts.end` outcome=cancelled | Capturing | — (barge-in continued) |
| Speaking | `wm.audio.wake` | Capturing | publish `wm.tts.cancel` first |

### 2.2 Barge-in path

In `Speaking` state, a wake event triggers: publish `wm.tts.cancel`, transition to `Capturing`. The wm-tts daemon's cancel hook (verified by AC5 of pipewire-output) ends playback within 200ms; wm-dialog is already in capture mode and ready to hear the next utterance.

### 2.3 Health envelopes

Every state transition logs at INFO level (`dialog: transition from=X to=Y on=event`). Publish `wm.dialog.state` periodically (every 5s while not in Listening) so external observers can introspect.

### 2.4 Self-emitted-topic filter

Add `wm.dialog.{attention, heard, unheard, timeout, state}` to wm-dialog's self-emitted allow-list (same pattern as the wm-tts error-loop-suppress fix).

## 3. Acceptance tests

1. **AC1 — `cargo test --release --lib` ≥ current+10** (one per transition + barge-in + timeout + degrade routes).
2. **AC2 — daemon active 60s, NRestarts=0.**
3. **AC3 — happy-path round trip (mocked).** Test harness emits wake, then speech.end, then stt.final, then brain.reply, then tts.end. FSM transitions through Listening → Capturing → Transcribing → Thinking → Speaking → Listening within 500ms.
4. **AC4 — barge-in transition.** From Speaking state, harness emits wake. Within 50ms, wm-dialog publishes `wm.tts.cancel`. FSM is in Capturing.
5. **AC5 — capture timeout.** Wake without speech.end; after 8s, FSM returns to Listening and publishes `wm.dialog.timeout`.
6. **AC6 — transcription failure routes through degrade.** Harness emits `wm.stt.uncertain`; FSM publishes a degrade phrase via wm.tts.speak ("Sorry, I didn't catch that") then returns to Listening.
7. **AC7 — live human gate.** With the full fleet running (audio-inference, stt-whisper, dialog, tts), speak: "hey wintermute, what time is it?" The expected sequence in journalctl: wm.audio.wake → wm.audio.speech.start → wm.audio.speech.end → wm.stt.final (text="what time is it") → wm.brain.reply (text contains a time) → wm.tts.start → wm.tts.end. User hears the answer through the speaker.
8. **AC8 — `cargo deny check bans licenses sources` clean.**
9. **AC9 — state introspection.** `agorabus subscribe wm.dialog.state` shows a heartbeat envelope every 5s while non-idle and goes silent when Listening.

## 4. Non-goals

1. **Multi-turn memory.** Each conversation is one round-trip; "what did you just say?" / "say that louder" require wmd state, separate PRD.
2. **Mood / personality model.** The phrases are blunt for v0.1.
3. **User identification.** Single-speaker assumption.
4. **Concurrent conversations.** One FSM at a time. Two people talking is undefined behavior.
5. **Wake-word disable mode.** "wintermute, mute yourself" is its own command pattern, sibling PRD.

## 5. Open questions

- Timeout values (8s capture, 3s transcribe, 10s think) — tune at deployment.
- Should `wm.dialog.attention` trigger a UI indicator (LED on the device, sound)? Likely yes; out of scope here, sibling PRD.
- Should the FSM persist across daemon restart? Probably not for v0.1.

## 6. Files this PRD likely touches

- Modified: `src/daemon.rs` (the state machine + transitions)
- Modified: `src/state.rs` or new module `src/fsm.rs` (state enum + transition table)
- Modified: `src/bus.rs` (subscribe to wake/speech/stt/brain/tts envelopes; self-emitted filter)
- Modified: `src/events.rs` (Topics enum for new wm.dialog.* envelopes)
- New: `src/degrade.rs` (small phrase bank — see also companion-degrade PRD)
- Modified: `tests/` integration tests
- Modified: `README.md`, `CHANGELOG.md`
