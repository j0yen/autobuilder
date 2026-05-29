# PRD: wintermute-audio — acoustic echo cancellation

**Author:** /dream (Claude Opus 4.7), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/companion.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-audio
**build_version_bump:** patch
**Depends on:** PRD-wintermute-audio-pipewire-input (shipped), PRD-wintermute-tts-pipewire-output (shipped)
**Codename:** *halfduplex-to-full* — TTS playback today loops into the mic. AEC turns the device full-duplex.

## TL;DR

PipeWire ships `module-echo-cancel` (WebRTC AEC3 under the hood). Today wm-audio captures from a raw mic source; whatever plays through the speaker is picked up and re-captured, so when wake-word and VAD ship, wintermute will hear itself talking. This PRD adds an AEC capture node — a virtual PipeWire source that subtracts the speaker reference signal from the mic — and switches `WM_MIC_NODE` to point at the AEC-cleaned virtual source. ~50 lines of PipeWire config + a Cargo feature flag on wm-audio + an install-step that loads the module on first boot.

## 1. Why this exists

- **Barge-in won't work without it.** Once inference (Component 2) is live, wm-audio's wake detector will fire on every TTS reply. The companion vision's interruption pattern requires hearing the user *over* the daemon's own voice.
- **Mother's room has hard surfaces.** Echo is real. AEC also doubles as a basic noise-suppression pass for VAD's benefit.
- **PipeWire makes this cheap.** No new Rust deps. The kernel + PipeWire stack already supports it.

## 2. What this builds

### 2.1 PipeWire module config

A new file `/etc/pipewire/pipewire.conf.d/99-wintermute-aec.conf` (or a systemd-user variant if PipeWire runs in user mode) that loads `libpipewire-module-echo-cancel` with parameters:

```
context.modules = [
  { name = libpipewire-module-echo-cancel
    args = {
      monitor.mode = false
      capture.props = { node.name = "wm-aec-capture" media.class = "Audio/Source" }
      playback.props = { node.name = "wm-aec-playback" media.class = "Audio/Sink" }
      aec.method = "webrtc"
      library.name = "aec/libspa-aec-webrtc"
      source.props.node.name = "wm-mic-aec"
      sink.props.node.name = "wm-spk-aec"
    }
  }
]
```

(Exact param spelling per PipeWire docs; the install.sh verifies.)

### 2.2 Switch `WM_MIC_NODE` default

Bootstrap config defaults to `wm-mic-aec` (the AEC virtual source). Existing direct-hardware values continue to work — if the user sets `WM_MIC_NODE=alsa_input...`, AEC is bypassed. Document the tradeoff in CHANGELOG.

### 2.3 Cargo feature gate

`wm-audio` gains an `aec` Cargo feature (default-on). When off, the install.sh skips the config drop. This lets a developer disable AEC for debugging without uninstalling.

### 2.4 Health probe

On startup, wm-audio probes `pactl list short sources | grep wm-mic-aec`; if missing AND `aec` feature is on, log `aec_module_missing` warning and fall back to the configured mic node (same fallback path as AC9 of pipewire-input).

## 3. Acceptance tests

1. **AC1 — `cargo test --release --lib` ≥ current+2** (probe-presence + fallback-on-missing).
2. **AC2 — daemon active 60s, NRestarts=0.**
3. **AC3 — AEC virtual source exists post-install.** `pactl list short sources | grep -c wm-mic-aec` = 1.
4. **AC4 — fallback when AEC source absent.** Manually `pw-cli destroy <id-of-wm-mic-aec>`; daemon logs `aec_module_missing`, switches to fallback, stays active.
5. **AC5 — barge-in works under simulated loopback.** Play a known TTS clip through the speaker while a test harness simulates the user saying "hey wintermute" mid-playback. With AEC on: wake event fires. With AEC off (`--no-default-features --features pipewire-only`): wake event also fires (because the daemon hears itself). The difference between the two is the AC.
6. **AC6 — `cargo deny check bans licenses sources` clean.**
7. **AC7 — live human gate.** Play TTS via `agorabus publish wm.tts.speak '{"text":"this is a test","priority":"normal"}'`. While TTS is speaking, speak the wake word. Subscriber on `wm.audio.wake` sees exactly one event (from the human voice, not from the daemon's TTS hearing itself). This requires PRD-wintermute-audio-inference to be shipped first; this PRD's AC7 is the conjugate of inference's AC6 under the AEC condition.

## 4. Non-goals

1. Custom echo profile tuning. WebRTC AEC3 defaults are fine for v0.1.
2. Noise suppression beyond what AEC's residual handler does. Future PRD if needed.
3. NoiseTorch integration. PipeWire's built-in is sufficient.
4. Hardware-specific calibration. Defer to deployment.

## 5. Open questions

- Does AEC introduce enough latency to bother wake-word? WebRTC AEC3 typically adds <10ms. Should be invisible.
- Does the WebRTC AEC library require a specific sample rate? It generally wants 16kHz mono — same as wm-audio's existing format. Verify in implementation.

## 6. Files this PRD likely touches

- New: `pkg/pipewire-config/99-wintermute-aec.conf`
- Modified: `install.sh` (drop the config, restart pipewire-user service)
- Modified: `src/source.rs` (probe `wm-mic-aec` source existence, log + fallback)
- Modified: `Cargo.toml` (`aec` feature, default-on)
- Modified: bootstrap-env example (default WM_MIC_NODE=wm-mic-aec)
- Modified: `README.md`, `CHANGELOG.md`
