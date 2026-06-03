# PRD: wintermute-stt — whisper.cpp model + real transcription

**Author:** /dream (Claude Opus 4.7), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/companion.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-stt
**build_version_bump:** minor
**Depends on:** PRD-wintermute-audio-inference (the speech.start/end envelopes that gate STT windows)
**Codename:** *transcribe* — wm-stt currently echoes a stub; this PRD makes it hear words.

## TL;DR

`wm-stt` config says `model: "distil-small.en"` but it actually runs `StubEngine` and emits nothing. This PRD wires the existing `whisper` Cargo feature (already declared in the crate per the install-flow notes from PRD-wintermute-stt acceptance work), drops the actual model bytes at `/usr/share/wintermute/models/whisper/distil-small.en.bin` via the install step, and routes the `wm.audio.speech.start` → `wm.audio.speech.end` PCM windows from the mic UDS fanout into whisper.cpp. Output: `wm.stt.final` envelopes with transcribed text + confidence.

## 1. Why this exists

- **The infrastructure is ready.** wm-audio captures PCM (shipped 19:05Z); inference PRD will emit speech.start/end (this vision Component 2); wm-stt is bus-healthy and subscribes to the right prefix. The only missing piece is the model.
- **Stub is silent.** `wm-stt start: config resolved cfg=SttConfig { model: "distil-small.en", ..., cloud_fastpath: false }` then nothing. No `wm.stt.final` events fire because no inference runs.
- **The vision needs words.** dialog FSM (Component 5) routes the transcribed text to brain; without text, no thinking; without thinking, no reply. STT is the throat of the loop.

## 2. What this builds

### 2.1 Enable the whisper feature

The crate's `Cargo.toml` declares `whisper` as an opt-in feature (per the v0.1.1 changelog note "feature-gated whisper-rs"). Default builds compile without it. This PRD adds `--features whisper` to the install step and verifies the build succeeds (whisper.cpp pulls in `cmake` + a C++ toolchain — handle in install.sh).

### 2.2 Connect window → model

When wm-stt receives `wm.audio.speech.start`, it begins copying PCM frames from `/run/user/1000/wintermute/mic.sock` into a ring buffer. On `wm.audio.speech.end`, it runs whisper.cpp on the captured window, publishes `wm.stt.final` with text + confidence, clears the buffer.

If the window is < 200ms or > 30s, skip (likely false-positive or stuck) and publish `wm.stt.uncertain` with `reason: "window_invalid"`.

### 2.3 Model on disk

`distil-small.en` is ~250MB. Install via `install.sh --download-model distil-small.en` (or `--download-models all`). Cached under `/usr/share/wintermute/models/whisper/`. The model is HuggingFace-hosted (Apache 2.0 license — verify).

### 2.4 Bus envelopes (already declared, just emit them)

- `wm.stt.final` — `{"text": "<string>", "confidence": <0..1>, "duration_ms": <u32>, "ts_unix_ms": <i64>}`
- `wm.stt.uncertain` — `{"text": "<string>", "confidence": <0..1>, "reason": "<string>"}`
- `wm.stt.error` — `{"kind": "model_missing|window_invalid|inference_failed", "detail": "<string>"}`

Self-emitted-topic filter MUST cover all three (per the wm-tts pattern).

## 3. Acceptance tests

1. **AC1 — `cargo test --release --lib --features whisper` ≥ current+5** (ring buffer, window slicing, model load, two integration tests).
2. **AC2 — daemon active 60s, NRestarts=0** with the model present.
3. **AC3 — transcribe a known sample.** Feed `tests/fixtures/hello_world.wav` via a test harness that simulates the audio fanout. Daemon publishes one `wm.stt.final` with text matching `/^hello[, ]+world/i` and confidence ≥ 0.7.
4. **AC4 — live human gate.** With the rest of the fleet running, speak a clear sentence into the mic after a wake event. Subscriber on `wm.stt.final` sees the transcribed text within 1.5s of speech.end.
5. **AC5 — empty window doesn't crash.** Feed a speech.start immediately followed by a speech.end (zero PCM). Daemon publishes `wm.stt.uncertain` with `reason: "window_invalid"`, daemon doesn't crash.
6. **AC6 — fail-soft on missing model.** With `WM_STT_MODEL=/nonexistent`, daemon starts, publishes `wm.stt.error` with `kind: "model_missing"` on first speech.end, doesn't crash.
7. **AC7 — `cargo deny check bans licenses sources` clean** (whisper-rs has known unmaintained-transitive warnings; document the deny.toml exceptions in CHANGELOG).
8. **AC8 — model license tracked.** README "Recent" section lists the model + license + download URL. Don't ship the model bytes in the repo (gitignore it).
9. **AC9 — confidence threshold.** Below `WM_STT_CONFIDENCE` (default 0.45), publish `wm.stt.uncertain` not `wm.stt.final`. Test via a known-marginal sample.

## 4. Non-goals

1. **Streaming partial results.** Only emit final once speech.end fires. Streaming partials is its own PRD (PRD-wintermute-stt-streaming) and probably waits for whisper.cpp's `--no-context` mode improvements.
2. **Multi-language.** English only for v0.1. The model is `distil-small.en`.
3. **Cloud fastpath.** `WM_CLOUD_STT_FASTPATH` already exists in config; not wired here. Future PRD.
4. **Speaker labeling, sentiment, punctuation tuning.** All future.

## 5. Open questions

- Model size vs. latency tradeoff. `distil-small.en` is the sweet spot (~250MB, ~1.5s for 5s audio on this laptop's CPU). `tiny.en` is faster but worse; `medium.en` is slower but better. PRD ships small; deployment can swap.
- whisper.cpp vs. whisper-rs vs. distil-whisper via candle. Use `whisper-rs` (bindings to whisper.cpp) — already gated in the crate. Avoid the candle path until that ecosystem matures.

## 6. Files this PRD likely touches

- Modified: `src/engine.rs` (replace StubEngine with WhisperEngine; gated by `whisper` feature)
- Modified: `src/daemon.rs` (subscribe to speech.start/end, ring-buffer mic.sock PCM, run inference on speech.end)
- Modified: `src/config.rs` (WM_STT_MODEL path, WM_STT_CONFIDENCE)
- Modified: `Cargo.toml` (whisper-rs as optional dep + feature gate confirmed)
- Modified: `install.sh` (--download-model, --features whisper)
- Modified: `deny.toml` (whisper-rs transitive exceptions)
- New: `tests/fixtures/hello_world.wav`
- Modified: `README.md`, `CHANGELOG.md`
