# PRD: wintermute-tts — text to speech with barge-in

**Author:** /dream (Claude Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-24
**Vision:** `visions/wintermute.md`
**Builds on:** PipeWire (default sink)
**Sibling:** `peon-ping/docs/prds/PRD-003-tts-spoken-feedback.md` (already designed; not built)
**Consumed by:** `PRD-wintermute-dialog.md` (which sends speak / cancel requests)
build_auto: true
build_target: rust-cli
build_priority: high
deferred_acs: [1, 3, 5, 7]

---

## TL;DR

Text in, audio out, fast. **Piper** (CPU-only, MIT, ~10× real-time
on a desktop) is the primary backend; ElevenLabs is opt-in for
natural quality. Streaming PCM to the default sink so the first
syllable lands within 300 ms; cancellable mid-utterance for barge-in;
pre-cached ~50 common short phrases serve in <50 ms.

This PRD has a deliberate sibling: peon-ping's PRD-003 specs TTS for
the trainer + dynamic notification templates. The contract here is
explicit — whoever ships first defines the voice-pack resolver, and
the other adopts it. No duplicate engines.

---

## 1. Why this exists

Three observations:

1. **Piper is the obvious 2026 choice.** MIT, CPU-only, broad voice
   library, real-time on a Pi5, ~10× real-time on a desktop CPU. The
   peon-ping PRD-003 design also lands on Piper as the Linux backend.

2. **Barge-in requires mid-utterance cancel within 100 ms.** If she
   interrupts to correct the laptop, the cancellation latency *is*
   the conversational feel. Slow cancel = the laptop talks over her
   = she gets frustrated.

3. **Common phrases should be free.** "Yes." "One moment." "I didn't
   catch that." "I'm here." A handful of pre-cached WAVs serve
   instantly — useful both for snappy feel and for the offline
   fallback when Piper can't be loaded.

---

## 2. What this builds

### 2.1 Binary: `wm-tts`

A long-running Rust daemon. On startup:

1. Load the configured Piper voice (`WM_TTS_VOICE`, default
   `en_US-lessac-medium`) and its ONNX runtime context.
2. Pre-render the cache phrases (see 2.3) to
   `~/.cache/wintermute/tts/<voice>/<phrase-hash>.wav` if not
   already present.
3. Open `default-sink` via PipeWire client and subscribe to agorabus
   for `wm.tts.*` requests.

Events subscribed:

| Topic | Payload |
|---|---|
| `wm.tts.speak` | `{text, priority, cancel_previous}` |
| `wm.tts.cancel` | `{}` (cancel current utterance) |
| `wm.tts.reload_voice` | `{voice}` (hot-swap) |

Events published:

| Topic | Payload |
|---|---|
| `wm.tts.start` | `{text, source, ts}` |
| `wm.tts.cancel.ack` | `{ts, drained_ms}` |
| `wm.tts.end` | `{text, duration_ms, ts}` |
| `wm.tts.error` | `{kind, message, ts}` |

### 2.2 Streaming pipeline

Text → Piper-rs (or `piper` subprocess if Rust binding immature) →
streaming PCM into a small ring buffer → PipeWire stream consumer.

Key: do NOT buffer the full WAV before playing. Push frames as Piper
produces them. First-audio target is 300 ms from request to first
sample at the speaker.

Cancellation:
- Set an `AtomicBool::cancel`
- The audio task drops the ring buffer and signals PipeWire to flush
- Piper subprocess is killed with SIGTERM (or its async task
  cancelled if using `piper-rs`)
- `wm.tts.cancel.ack` emitted within 100 ms with the number of ms of
  speech that were already played

### 2.3 Pre-cache

A YAML config at `/etc/wintermute/tts-cache.yaml` lists the phrases:

```yaml
phrases:
  - "yes"
  - "no"
  - "one moment"
  - "I didn't catch that"
  - "I'm here"
  - "I'm thinking"
  - "let me check"
  - "sorry, the internet is down right now"
  # ...
```

On startup, `wm-tts` renders each via Piper to a WAV file. Future
`wm.tts.speak` calls compare text against the cache; an exact match
plays the pre-rendered WAV in <50 ms (just a PipeWire enqueue).

Cache files are voice-keyed: changing voice re-renders.

### 2.4 ElevenLabs opt-in

When `WM_CLOUD_TTS_QUALITY=true`:
- Streaming TTS via ElevenLabs WebSocket API
- Voice ID from `WM_TTS_VOICE_ID_CLOUD` (set in bootstrap)
- Falls back to Piper on any cloud error
- Bootstrap displays a "cost: ~$X per million characters" disclosure

### 2.5 Coordination with peon-ping PRD-003

Both this PRD and peon-ping PRD-003 want a TTS engine on Linux.
Concrete contract:

**Shared voice-pack resolver:** a Rust library `wm-voicepack` (lives
in `wintermute/wm-voicepack/`) that maps a voice-pack identifier to
a backend (Piper file path / ElevenLabs voice ID / espeak-ng args)
and renders.

**Whoever ships first** publishes the crate and the other consumes
it. Concretely:
- If `wm-tts` ships first → peon-ping PRD-003 calls `wm-voicepack`
  in its hook pipeline for TTS-enabled notifications.
- If peon-ping PRD-003 ships first → `wm-tts` calls into the same
  resolver crate (factored out of peon-ping if needed).

`/build` should pick the order based on dependency priority — likely
`wm-tts` first because Fleet 1 needs it for the greeting.

---

## 3. Open-source dependencies

| Crate / tool | Version | Purpose | License |
|---|---|---|---|
| `piper` (binary) or `piper-rs` (preferred if mature) | ^1 | TTS inference | MIT |
| Piper voice models | upstream | voices | MIT |
| `pipewire-rs` | ^0.8 | PipeWire client | MIT |
| `tokio` | ^1.40 | async | MIT |
| `serde_yaml` | ^0.9 | cache config | MIT/Apache-2.0 |
| `reqwest` + `tokio-tungstenite` (optional) | ^0.12, ^0.23 | ElevenLabs cloud | MIT/Apache-2.0 |
| `agorabus` client | local | pub/sub | local |

---

## 4. Acceptance criteria

1. `wm.tts.speak` → first audio at speaker latency: ≤300 ms for a
   short phrase using Piper warm.
2. `wm.tts.cancel` → audio silenced at speaker: ≤100 ms; ack
   includes correct drained_ms.
3. Pre-cached phrase ("yes") → speaker: ≤50 ms.
4. Voice hot-swap via `wm.tts.reload_voice` completes in <5 s and
   does not interrupt any non-cancellable in-flight speech.
5. ElevenLabs path (when enabled) first-audio latency ≤400 ms over
   a typical broadband connection.
6. Cloud failure during ElevenLabs streaming falls back to Piper
   mid-sentence (or cleanly restarts the utterance if mid-stream
   cutover is too jarring — implementer's choice; document the
   behavior).
7. 60-minute steady-state run: no audio glitches, RSS growth <30 MB.
8. The `wm-voicepack` crate is published (either as part of this
   PRD's repo or as a shared crate) and peon-ping PRD-003 has a
   tracked integration ticket.

## 5. Out of scope (Fleet 2 / 3)

- Voice cloning of a family member (Fleet 3 `wintermute-voice-clone`
  — license-constrained).
- SSML / prosody markup — Fleet 2 if needed.
- Sound effects mixed with speech — peon-ping's domain; not here.

## 6. Risks

- **piper-rs maturity** — if the binding isn't stable, subprocess
  the `piper` binary. Performance impact minor (Piper is fast either
  way); complexity reduction is real.
- **Pre-cache disk usage** — ~50 phrases × ~50 KB × N voices = small
  (~5 MB). Documented.
- **ElevenLabs cost** — explicitly disclosed in bootstrap UI before
  the user opts in.

## 7. Open questions

- Should we ship multiple Piper voices in the model bundle or pull
  on demand? Leaning: ship 3-5 popular voices in `wm-models`
  package, document how to add more.
- Should `wm-voicepack` live in this PRD's repo or be its own crate
  immediately? Leaning: in-repo until peon-ping PRD-003 needs it,
  then extract.
- Long utterances (>30 s) — pre-emptive chunking with silence
  markers? Leaning: yes, but iter-2.
