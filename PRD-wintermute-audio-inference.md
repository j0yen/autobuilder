# PRD: wintermute-audio — wake-word + VAD inference

**Author:** /dream (Claude Opus 4.7), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/companion.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-audio
**build_version_bump:** minor
**Depends on:** PRD-wintermute-audio-pipewire-input (shipped 2026-05-28T19:05Z, commit f86ced9)
**Codename:** *attention* — the daemon was structurally listening; now it actually pays attention.

## TL;DR

`wm-audio` v0.2.0 (shipped today) streams 16 kHz mono PCM from the mic onto a UDS fanout. Subscribers can read it. Nobody decides when speech starts, when it ends, or whether the user said the wake word. This PRD attaches two inference passes to the existing PCM broadcast: **microWakeWord** for wake-word detection (publishes `wm.audio.wake`) and **Silero VAD** for speech boundary detection (publishes `wm.audio.speech.start` / `wm.audio.speech.end`). Both run on the same broadcast stream wm-audio already publishes; both consume frames, neither modifies them.

Without this PRD, "always listening" is a fantasy — the bytes flow, but nothing in the fleet can act on them.

## 1. Why this exists

- **The mic stream exists** (`/run/user/1000/wintermute/mic.sock`, 16k mono i16, verified by AC4 of pipewire-input).
- **No consumer turns it into events.** `journalctl --user -u wm-audio.service` shows zero `wm.audio.wake` and zero `wm.audio.speech.*` envelopes since startup.
- **The fleet downstream is waiting for those events.** wm-stt subscribes to `wm.audio.speech.` (per its config); without speech.start/end it never transcribes. wm-dialog's turn FSM waits on wake; without wake it never advances out of idle.
- **The vision (visions/companion.md) names this as Component 2,** gating dialog and STT.

## 2. What this builds

### 2.1 Two inference workers, one stream

A new module `inference.rs` (or split into `wake.rs` + `vad.rs`) subscribes to the existing `fanout::channel()` broadcast — two more `broadcast::Receiver<PcmFrame>` subscribers, no contention with UDS clients. Each worker runs its own model:

- **`wake_word_detector`** — loads a microWakeWord ONNX model (`hey_jarvis_v2.onnx` or `okay_nabu_v2.onnx`, configurable via `WM_WAKE_WORD`). For each incoming frame, runs inference; on detection, publishes `wm.audio.wake` and emits a structured log line.
- **`vad_detector`** — loads Silero VAD ONNX model. Maintains a small state machine (silence → voice → silence) and publishes `wm.audio.speech.start` on rising edge, `wm.audio.speech.end` on falling edge after a configurable hangover (default 500ms).

Both consume the same PCM frames. Neither blocks UDS subscribers. If model loading fails, the daemon falls back to a `null_engine` that emits no events and logs a warning — same pattern as the AC9 fallback in pipewire-input.

### 2.2 ONNX runtime

Use `ort` crate (the active community-supported binding to ONNX Runtime). Wintermute already has `onnxruntime-sys` as an indirect dep (per the bootstrap config). If not, `ort` brings it cleanly.

### 2.3 Models on disk

Models live at `/usr/share/wintermute/models/wake/` and `/usr/share/wintermute/models/vad/`. Drop them as part of the install step (~5MB total). Make them downloadable via an `install.sh --download-models` flag.

### 2.4 Bus envelopes

- `wm.audio.wake` — `{"word": "<configured-wake-word>", "confidence": <0..1>, "ts_unix_ms": <i64>}`
- `wm.audio.speech.start` — `{"ts_unix_ms": <i64>}`
- `wm.audio.speech.end` — `{"ts_unix_ms": <i64>, "duration_ms": <u32>}`

All three are in the daemon's outbound publish set; ADD to the self-emitted-topic filter so they don't re-trigger the decode-storm class.

## 3. Acceptance tests

1. **AC1 — `cargo test --release --lib` ≥ current+8** (state machine, frame slicing, fallback-on-missing-model, detector idle/active counts, three integration tests for the three new envelopes).
2. **AC2 — daemon active 60s, NRestarts=0** after restart.
3. **AC3 — wake-word fires on a synthetic test sample.** Pipe a known-positive WAV (canned wake-word recording at `tests/fixtures/hey_jarvis.wav`) into the broadcast channel via a test harness; subscriber sees one `wm.audio.wake` with confidence ≥ 0.8 within 500ms.
4. **AC4 — VAD speech window round-trip.** Pipe a 2-second speech clip with leading + trailing silence. Subscriber sees `wm.audio.speech.start` within 100ms of speech-onset and `wm.audio.speech.end` within 100ms of (speech-offset + hangover).
5. **AC5 — silence is silence.** Pipe 5 seconds of room-tone noise; no speech.start, no wake events. Threshold can be tuned via `WM_VAD_THRESHOLD` env var.
6. **AC6 — live human gate.** Speak "hey wintermute" (or the configured wake) into the mic; subscriber sees `wm.audio.wake` within 500ms. Speak a sentence; subscriber sees speech.start/end pair. This is the deployment-style smoke check — the AC8 of pipewire-input but for inference output.
7. **AC7 — fallback on missing models.** With `WM_WAKE_MODEL=/nonexistent`, daemon starts, logs `wake_model_missing`, no wake events fire, daemon does not crash. VAD path independent.
8. **AC8 — wm-audio binary unchanged on the consumer side.** UDS subscribers still read full-rate PCM with no quality loss; verified via the existing AC4 harness from pipewire-input.
9. **AC9 — `cargo deny check bans licenses sources` clean.**
10. **AC10 — self-emitted-topic filter covers all three new topics.** Lock by a test that subscribes to `wm.audio.` while publishing each new topic and asserts the daemon doesn't re-decode.

## 4. Non-goals

1. Speaker identification (who is talking — mother vs. visitor).
2. Custom wake-word training. Use stock microWakeWord models.
3. AEC — separate PRD (companion vision Component 4).
4. Cloud-based wake detection.
5. Continuous transcription. STT runs on the speech.start/end windows, not on raw stream.

## 5. Open questions

- Confidence threshold for wake — start at 0.7, tune in the field.
- Hangover for VAD — 500ms is conventional. Mother may pause mid-sentence; ship with that, tune at deploy.
- One worker or two? One state machine simpler; two more separable. Implementation choice.

## 6. Files this PRD likely touches

- New: `src/inference.rs` (or `src/wake.rs` + `src/vad.rs`)
- Modified: `src/daemon.rs` (spawn the workers from the main loop)
- Modified: `src/events.rs` (new Topics enum variants + payloads + self-emitted filter entries)
- Modified: `src/config.rs` (WM_WAKE_MODEL, WM_VAD_MODEL, WM_WAKE_WORD, WM_VAD_THRESHOLD)
- Modified: `Cargo.toml` (`ort` dep)
- New: `tests/fixtures/hey_jarvis.wav`, `tests/fixtures/speech_with_silence.wav` (small, MIT-licensed)
- Modified: `install.sh` (--download-models flag)
- Modified: `README.md`, `CHANGELOG.md`
