# PRD: wintermute-audio — mic, AEC, NS, wake, VAD → events

**Author:** /dream (Claude Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-24
**Vision:** `visions/wintermute.md`
**Builds on:** PipeWire 1.6.5 + PulseAudio compat (already on this laptop)
**Required by:** `PRD-wintermute-stt.md`, `PRD-wintermute-dialog.md`, `PRD-wintermute-tts.md` (barge-in cancel signal)
build_auto: true
build_target: mixed
build_priority: high
deferred_acs: [1, 2, 5, 8]

---

## TL;DR

Everything from the raw microphone PCM to "ready-for-STT speech
chunks" lives here. PipeWire's `module-echo-cancel` removes the
laptop's own TTS from the mic signal; NoiseTorch-ng suppresses
keyboard / fan / room noise; **microWakeWord** ONNX runs on the
cleaned stream for low-CPU wake detection; **Silero VAD** detects
utterance boundaries; the `wm-audio` Rust daemon orchestrates all
of this and publishes events on agorabus. Plan-agent collapsed what
were two PRDs (audio-pipeline + wake) into this one — wake and VAD
are thin consumers of the same mic stream, so the process boundary
was unnecessary.

---

## 1. Why this exists

Four observations:

1. **AEC is non-negotiable for always-on voice.** Without echo
   cancellation, every TTS utterance retriggers the wake word and
   the laptop talks to itself. PipeWire ships
   `module-echo-cancel`; we just need to wire it correctly.

2. **microWakeWord is the right wake engine in 2026.** Plan-agent's
   challenge: HA Voice Preview Edition shipped microWakeWord (not
   openWakeWord) precisely because the false-accept profile is
   better for always-on use and CPU draw is much lower. openWakeWord's
   custom-wake-word recipe is also notoriously finicky — synthetic
   TTS training data → real-voice mismatch — and our non-literate
   user can't retrain. Ship a *pretrained* wake word and let her
   learn it.

3. **Silero VAD is the standard.** ONNX, ~1 MB, well-understood
   turn-end detection. Nothing in the 2026 landscape replaces it
   for this job.

4. **One mic, many consumers.** STT, wake, VAD, and any future
   speaker-diarization service all want the same 16 kHz mono PCM.
   Fanout from one canonical capture stream over a Unix socket
   beats N consumers all opening the device.

---

## 2. What this builds

### 2.1 PipeWire configuration

A drop-in at `~/.config/pipewire/pipewire.conf.d/99-wintermute.conf`:

```
context.modules = [
  {
    name = libpipewire-module-echo-cancel
    args = {
      monitor.mode = true
      capture.props = {
        node.name = "wm-mic-cancelled"
        node.description = "wintermute echo-cancelled microphone"
      }
      playback.props = {
        node.name = "wm-mic-playback-ref"
        node.description = "wintermute AEC playback reference"
      }
      aec.method = webrtc
      aec.args = {
        webrtc.gain_control = true
        webrtc.noise_suppression = false  # we use NoiseTorch instead
        webrtc.aec3 = true                # preferred — see Risk
      }
    }
  }
]
```

If the system's `pipewire` package wasn't built with AEC3 support,
fall back to `webrtc.aec3 = false`. The `wm-audio` daemon detects
this at startup by probing module load output and logs a clear
warning.

### 2.2 NoiseTorch-ng

Documented install (AUR `noisetorch-ng-bin`) and a small helper
script `wm-noise on|off` that toggles the NoiseTorch virtual source.
`wm-audio` consumes the NoiseTorch source if it's loaded; falls back
to `wm-mic-cancelled` otherwise.

### 2.3 Binary: `wm-audio`

A long-running Rust daemon. On startup:

1. Open the configured input node (`WM_MIC_NODE` env from
   bootstrap, fed through AEC and NS).
2. Resample to 16 kHz mono PCM if needed (most mics already are).
3. Spawn three async tasks that consume the same shared ring buffer:
   - **socket fanout** — accepts UDS connections at
     `$XDG_RUNTIME_DIR/wintermute/mic.sock` and pushes PCM frames
     to each subscriber (currently only `wm-stt`)
   - **wake** — runs **microWakeWord** ONNX inference every 80 ms
     on a 1280-sample window (matches the model's expected input)
   - **VAD** — runs **Silero VAD** ONNX every 32 ms; emits
     `speech.start` on rising edge with hangover, `speech.end` on
     500-ms-confirmed silence
4. On `wake.detected` or `speech.start` events, publish on agorabus.

Events published:

| Topic | Payload |
|---|---|
| `wm.audio.wake` | `{wake_word, confidence, ts}` |
| `wm.audio.speech.start` | `{ts}` |
| `wm.audio.speech.chunk` | `{seq, pcm_b64, ts}` (chunks during active speech) |
| `wm.audio.speech.end` | `{duration_ms, ts}` |
| `wm.audio.mute` / `wm.audio.unmute` | `{ts}` |

Events subscribed:

| Topic | Behavior |
|---|---|
| `wm.tts.start` | mute wake detection (avoid double-fire if AEC has a 50ms tail) |
| `wm.tts.end` | unmute wake detection |
| `wm.dialog.mute_request` | mute mic entirely (`wm mute`) |
| `wm.dialog.unmute_request` | unmute |

### 2.4 Configuration

Reads from `/etc/wintermute/conf.d/00-bootstrap.env`:
- `WM_MIC_NODE` — input device name
- `WM_WAKE_WORD` — one of `hey_jarvis`, `okay_nabu`, `hey_mycroft`
- (optional) `WM_WAKE_THRESHOLD` — float 0..1, default 0.6

Hot-swap of wake word: send `wm.audio.reload` on agorabus; daemon
re-reads env and swaps the ONNX model without restarting.

### 2.5 Pretrained model bundle

A small `wm-models` AUR-style package installs:
- microWakeWord ONNX for the three pretrained wake words
- Silero VAD ONNX
- Default whisper.cpp model (used by `wm-stt`)

Models go to `/usr/share/wintermute/models/`. Hash-pinned in the
package.

---

## 3. Open-source dependencies

| Crate / tool | Version | Purpose | License |
|---|---|---|---|
| PipeWire `module-echo-cancel` | system | AEC | LGPL |
| NoiseTorch-ng | AUR `noisetorch-ng-bin` | noise suppression | GPL-3 |
| `microWakeWord` ONNX models | upstream | wake detection | Apache-2.0 |
| `Silero VAD` ONNX | upstream | turn-end detection | MIT |
| `ort` (Rust) | ^2 | ONNX Runtime bindings | MIT/Apache-2.0 |
| `pipewire-rs` | ^0.8 | PipeWire client | MIT |
| `tokio` | ^1.40 | async runtime | MIT |
| `ringbuf` | ^0.4 | shared PCM ring buffer | MIT |
| `agorabus` client | local | event pub/sub | local |

ONNX Runtime CPU build is sufficient; GPU is opt-in via the `ort`
features.

---

## 4. Acceptance criteria

1. With `wm-tts` actively playing a 5-second sentence over speakers
   (not headphones), the wake word does NOT fire even once across
   30 repetitions. (AEC working.)
2. Typing on the laptop keyboard while the mic is open reduces input
   level by ≥10 dB compared to AEC-only mode (NoiseTorch verified).
3. Wake → `wm.audio.wake` event published latency: <200 ms.
4. End-of-speech → `wm.audio.speech.end` event latency: <500 ms
   (after 500 ms of confirmed silence).
5. False-accept rate on a 60-minute recording of ambient living-room
   speech (TV at conversational volume): <0.5/hr.
6. Wake-word hot-swap via `wm.audio.reload` completes in <2 s without
   dropping mic capture.
7. Two simultaneous mic.sock subscribers can consume the PCM stream
   for 60 minutes without dropouts (verified by checksum on captured
   PCM at each consumer).
8. Daemon recovers from PipeWire restart (`systemctl --user restart
   pipewire`) without manual intervention within 5 s.

## 5. Out of scope (Fleet 2 / 3)

- Speaker diarization / voice profile (Fleet 3: only respond to her).
- Custom-trained wake words (Fleet 3 if microWakeWord training
  improves; otherwise punt indefinitely).
- Bluetooth audio routing — Fleet 2; needs separate logic for
  hearing-aid pairing.
- Multi-mic beamforming — Fleet 3 if her room is noisy.

## 6. Risks

- **AEC3 build flag.** Arch's `pipewire` package historically has
  built with `aec3=true` but the flag isn't guaranteed. Mitigation:
  detect at startup, fall back to classic webrtc-aec, log the
  warning. Repo README documents the rebuild path.
- **microWakeWord pretrained accuracy variance** by accent. Three
  wake words give some choice; if all three fail for her, Fleet 3
  has to revisit (custom training or always-on diarization).
- **NoiseTorch GPL-3.** We depend on the binary as an external tool,
  not link against it — so our MIT/Apache-2 repo licensing is fine.
  Document the system dependency.
- **PipeWire module-echo-cancel CPU.** Older laptops can hit
  noticeable load. Profile during /build's iteration and document
  CPU floor.

## 7. Open questions

- Should NoiseTorch-ng be hard-required, or fall back to webrtc-ns
  (lower quality, no extra dep)? Leaning: hard-required because the
  quality difference is large for an always-on use case.
- Is there value in a `wm-audio dump` subcommand that records the
  last 10 s of PCM for debugging false wakes? Probably yes; cheap to
  add in iter-2.
