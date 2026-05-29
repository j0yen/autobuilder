# PRD: wintermute-audio — PipeWire input (mic actually streams)

**Author:** Claude (for the user)
**Status:** Draft v0.1
**Date:** 2026-05-28
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-audio
**build_version_bump:** minor
**Depends on:** PRD-wintermute-tts-pipewire-output (shipped, sibling pattern reference)
**Codename:** *listen* — symmetric to *sayit*; the daemon claims a mic, this PRD makes the bytes actually flow.

---

## TL;DR

`wm-audio` is bus-healthy and structurally wired (mic UDS fanout module, config parsing, broadcast channel skeleton). But when the user asks "can you hear me?" the answer is no — `/run/user/1000/wintermute/mic.sock` does not exist, no `wm.audio.*` events ever fire after startup, and `lib.rs` documents this gap with `// inference, plus the PipeWire capture implementation, are deferred`. Pure mirror of where `wm-tts` was before today's pipewire-output ship. This PRD closes the input side: `pw-record` subprocess captures from `WM_MIC_NODE`, streams 16kHz mono i16 frames into the existing `tokio::sync::broadcast` → UDS fanout pipeline, and emits a `wm.audio.capture.{start,end}` envelope pair around the capture span.

Wake-word detection (microWakeWord), VAD (Silero), AEC, and `wm.audio.{wake,speech.start,speech.end}` segmentation are all explicit non-goals — they consume the PCM stream once it exists. This PRD just gets the stream existing.

---

## 1. Current state (evidence)

1. **mic.sock missing.** `ls /run/user/1000/wintermute/mic.sock` → no such file. The fanout's `UnixListener::bind` was never called by the daemon's main loop.
2. **No bus events.** `journalctl --user -u wm-audio.service --since "2 min ago"` → "No entries" once startup quiets. `agorabus subscribe wm.audio.` for 3s → silence.
3. **lib.rs:** `// A MicSource trait abstracting the capture device so PipeWire, … inference, plus the PipeWire capture implementation, are deferred`. Stale comment that this PRD ships.
4. **config.rs:** `WM_MIC_NODE` env var is parsed (defaults to empty / PW default) but no consumer.
5. **fanout.rs is fully written and tested** — broadcast channel, UDS listener, drop-on-Lagged policy. Just nothing publishes into it.
6. **`pw-record` is installed** at `/usr/sbin/pw-record` (ships with `pipewire`, zero new deps).
7. **Config drift to flag:** bootstrap config sets `WM_AUDIO_MIC_NODE=…HiFi__Mic1__source` but `pactl list short sources` only shows `…HiFi__Mic2__source`. Either the user's hardware enumerates differently than the install assumed, or PRD-wintermute-bootstrap had a stale default. The fix landing here should validate the configured node exists at startup and either fall back to PW default with a warning, or refuse to start with a clear error.

---

## 2. Functional requirements

### 2.1 Default capture backend: `pw-record` subprocess (iter-1)

```
pw-record --target "$WM_MIC_NODE" --rate 16000 --channels 1 --format s16 -
```

Output goes to stdout (the `-` argument), which the daemon reads as a stream of little-endian i16 samples. Each 320-sample chunk (20ms at 16kHz) is wrapped in a `PcmFrame` and published on the existing broadcast channel.

If `WM_MIC_NODE` is empty or unset, omit `--target` so PipeWire routes to the default source. If `WM_MIC_NODE` is set but `pactl list short sources` doesn't have it, log a warning and fall back to default (don't refuse to start — this matches the `WM_SINK_NODE` fallback behavior shipped in pipewire-output AC9).

Why `pw-record` and not pipewire-rs: same logic as pipewire-output — zero new Rust deps, identical install footprint, gets the byte stream flowing today. The pipewire-rs streaming consumer is a separate future PRD.

### 2.2 Event lifecycle

When the daemon starts (or after a `wm.audio.reload` that changes the mic node):

1. **`wm.audio.capture.start`** — publish before spawning `pw-record`: `{"mic": "<resolved-node-name-or-default>", "rate": 16000, "channels": 1}`. Sent ONCE per capture lifetime, not per frame.
2. **`wm.audio.capture.end`** — publish if `pw-record` exits (clean or error): `{"outcome": "ok" | "error", "dur_ms": <wall-time>, "reason": "<exit-code-or-signal>"}`. The daemon then retries the spawn with exponential backoff (1s, 2s, 4s, capped at 30s) — capture is a persistent service, not one-shot.
3. **`wm.audio.error`** — on configuration failures (node not found and no fallback, permissions denied, etc.).

These three topics are in the daemon's outbound publish set; they MUST be added to the self-emitted-topic filter (sibling pattern from `bus.rs` — same defect template as wm-tts had before the error-loop-suppress fix). Without this, the daemon will recurse on any inbound `wm.audio.error` envelope.

### 2.3 UDS fanout (existing module, just plug it in)

The `fanout::channel()` returns a `(Sender, Receiver)` pair. The daemon's main loop should:
1. Build the channel on startup.
2. Spawn `fanout::serve(sender, sock_path)` task — the UDS listener.
3. For each `PcmFrame` from `pw-record`, call `sender.send(frame)`. Lagged subscribers self-drop via existing logic.

This is the integration step the existing fanout module was written for; nothing in `fanout.rs` should change.

### 2.4 Capture metric

Add (or fix, if already declared) a `captured_bytes` counter that ticks per frame. Symmetric to `played_bytes` in pipewire-output. Surface via whichever metric path the daemon already exposes.

---

## 3. Acceptance tests

All ACs verified live, not just locally. Don't park at user-gate-blocker.

1. **AC1 — `cargo test --release --lib` exits 0.** Current count varies (run `cargo test` to find it); expect +3 to +5 new tests (capture spawn, fallback-on-bad-node, capture.start/end envelope, fanout-integration, retry-backoff). Final ≥ current + 3.
2. **AC2 — daemon survives 60s under systemd post-bump.** `systemctl --user restart wm-audio.service && sleep 60 && systemctl --user is-active wm-audio.service` → `active`, NRestarts=0.
3. **AC3 — `mic.sock` exists after startup.** `[ -S /run/user/1000/wintermute/mic.sock ]` succeeds within 5s of service start.
4. **AC4 — UDS subscriber reads non-zero PCM.** A test subscriber that connects to `mic.sock` and reads 16000 bytes (1s of audio) gets exactly 16000 bytes (or more) within 2s. Specifically: `socat - UNIX-CONNECT:/run/user/1000/wintermute/mic.sock | head -c 16000 | wc -c` returns 16000. (The bytes don't need to be sensible audio — silence-from-muted-mic is fine for this test; AC8 is the human-audibility gate.)
5. **AC5 — `wm.audio.capture.start` envelope fires once at startup.** `agorabus subscribe wm.audio.capture.start` running before the daemon restart sees exactly one event within 5s of start.
6. **AC6 — `wm.audio.capture.end` fires on graceful stop.** `systemctl --user stop wm-audio.service` → subscriber sees a single `wm.audio.capture.end` event with `outcome: "ok"`.
7. **AC7 — `captured_bytes` metric > 0 after 5s of capture.** Whatever metric surface exists, it reports a non-zero value.
8. **AC8 — human-audible gate (best-effort).** Pipe the live mic into `pv | head -c 32000 > /tmp/cap.raw && play -t raw -r 16000 -c 1 -e signed -b 16 /tmp/cap.raw` (or `ffplay` equivalent). User confirms they hear themselves (or microphone-room ambient) on playback. This is the "I hear myself" capstone — same shape as the pipewire-output capstone where the user said "I hear audio."
9. **AC9 — fallback on missing node.** `WM_MIC_NODE=alsa_input.does-not-exist.source` → daemon starts anyway, logs `mic_node_fallback` warning, captures from PipeWire default. Test via `systemctl --user set-environment WM_MIC_NODE=alsa_input.fake && systemctl --user restart wm-audio.service` and verify it stays active.
10. **AC10 — fail-soft: pw-record missing.** With `pw-record` not on `$PATH` (override via `WM_PW_RECORD_BIN=/nonexistent`), daemon emits `wm.audio.error` with `kind: "pw_record_missing"`, retries with backoff, does NOT crash.
11. **AC11 — `cargo deny check bans licenses sources` clean** (CVSS4 workaround).

---

## 4. Non-goals

1. **Wake-word detection.** microWakeWord integration is a separate future PRD (PRD-wintermute-audio-wake-detect or similar). This PRD ships the PCM stream that wake-detect will consume.
2. **VAD.** Silero VAD likewise consumes the stream; not in scope.
3. **AEC.** Echo cancellation (PipeWire echo-cancel module) — separate.
4. **`wm.audio.{wake, speech.start, speech.end}` segmentation envelopes.** These come from wake-detect + VAD; deferred.
5. **`wm.audio.reload`-driven mic switching.** The reload event currently only hot-swaps wake_word; teaching it to also re-spawn pw-record with a new `--target` is a follow-on.
6. **Multi-channel mics.** Mono 16kHz only; multi-channel is whisper.cpp's downstream concern.
7. **pipewire-rs streaming.** Separate future PRD, symmetric to PRD-wintermute-tts-pipewire-streaming.

---

## 5. Configuration

`/etc/wintermute/conf.d/00-bootstrap.env` (already exists, just consumed now):

```
WM_MIC_NODE=alsa_input.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__Mic2__source
WM_PW_RECORD_BIN=/usr/sbin/pw-record       # optional; defaults to "pw-record" on $PATH
```

**Config-drift note (worth a one-line CHANGELOG mention):** the bootstrap install wrote `…HiFi__Mic1__source` but this laptop's PipeWire enumerates `…HiFi__Mic2__source`. AC9's fallback covers this gracefully; the bootstrap PRD should learn to probe `pactl list short sources` and pick the first available input rather than hardcoding Mic1. That's a sibling PRD: PRD-wintermute-bootstrap-mic-autodetect.

---

## 6. Future PRDs (do not implement here)

- **PRD-wintermute-audio-wake-detect:** microWakeWord on the PCM stream, emit `wm.audio.wake`.
- **PRD-wintermute-audio-vad:** Silero VAD → `wm.audio.speech.start` / `wm.audio.speech.end`.
- **PRD-wintermute-audio-aec:** PipeWire echo-cancel module for full-duplex (so TTS playback doesn't loop into the mic).
- **PRD-wintermute-audio-pipewire-streaming:** replace `pw-record` subprocess with pipewire-rs (lower TTFB, fewer process boundaries).
- **PRD-wintermute-bootstrap-mic-autodetect:** bootstrap probes pactl + picks first usable input rather than hardcoding `Mic1`.

---

## 7. Investigation hints

1. **Read `daemon.rs`** to find where startup happens. The fanout init + pw-record spawn should land there, in that order.
2. **`fanout.rs` is the integration target**, NOT something to modify. `channel()` returns the sender; `serve(sender, path)` opens the UDS listener. Wire both, drop frames in, done.
3. **`source.rs`** likely has the `PcmFrame` type. Use it.
4. **`pw-record --help`** for exact flag spelling. On this laptop: `--target`, `--rate`, `--channels`, `--format`, output to stdout via `-`.
5. **Apply the self-emitted-topic filter pattern** from `wintermute-tts/src/bus.rs` (shipped today in PRD-wintermute-tts-error-loop-suppress). Without it, the new `wm.audio.{capture.start, capture.end, error}` envelopes will recurse on the wm.audio. subscribe prefix.

---

## 8. Definition of done

- All 11 ACs pass live.
- Diff includes deleting the stale `lib.rs` "deferred" comment about PipeWire capture.
- CHANGELOG v0.2.0 entry summarizes: pw-record subprocess shipped, fanout integrated, capture envelopes emitted, fallback on missing node, fail-soft on missing pw-record.
- README "Recent" section appended with a bullet for live capture.
- Service restarted; user-side smoke (AC8): user confirms they can hear themselves on the playback of the captured stream.
- PRD moves to PRDs-archive/ once verified-completed.
