# Vision: companion — wintermute at her side

**Authored by:** /dream (Claude Opus 4.7), with jsy
**Seed:** 2026-05-28T19:18 PT — "for this to work with my mother, voice will need to be the primary mode of interaction. you will need to always be listening, ready to respond."
**Status:** active

## TL;DR

Wintermute today is a developer's voice toy: mic captures PCM (shipped 19:05Z), speakers play TTS (shipped 18:31Z), bus carries envelopes, daemons survive heartbeat windows. None of this is usable as a companion. The vision: wintermute on a desk at jsy's mother's home, always-listening, summoned by a wake word, transcribes what she says, sends it to Claude, says the reply, returns to listening. No keyboard. No surprises. No silent failures. When something breaks, wintermute says so out loud rather than going still.

## End-state

When this vision is fulfilled:

1. **She says "hey wintermute"** (or whatever wake word the deployment picks). Within ~300ms the daemon emits `wm.audio.wake`.
2. **She speaks her message.** VAD catches the speech boundary and emits `wm.audio.speech.start` then `wm.audio.speech.end`. wm-stt transcribes from the PCM window and publishes `wm.stt.final`.
3. **wm-dialog routes** the transcript through wmd (brain) to Claude; the response comes back as `wm.brain.reply`.
4. **wm-tts speaks the reply** through the speaker. Wake-word listening resumes when playback ends.
5. **She can interrupt mid-reply** — saying the wake word again triggers `wm.tts.cancel`; the daemon stops talking and listens.
6. **Echo cancellation is on** — TTS playback doesn't loop into the mic.
7. **The device boots on power** to wintermute.target with no manual login; it survives power loss; it recovers from network blips and Claude outages by saying what's wrong.
8. **There is no keyboard.** Every operating mode is reachable by voice or by power-cycle.

## Components (PRD-sized pieces)

Decomposed in dependency order. Each line is a future PRD; the bolded ones are drafted this dream pass.

1. **PRD-agorabus-multi-prefix-subscribe** — already queued (commit a5f19bb). wm-audio must hear `wm.tts.start` to do barge-in; today its subscribe loop only honors the last prefix. **Blocks barge-in.**
2. **PRD-wintermute-audio-inference** (drafted) — microWakeWord + Silero VAD wired onto the existing fanout PCM stream; emits `wm.audio.wake`, `wm.audio.speech.start`, `wm.audio.speech.end`. The mic stream exists; we just need to attach the brains.
3. **PRD-wintermute-stt-whisper-model** (drafted) — replace the stub STT engine with whisper.cpp + a downloaded model, transcribing the PCM window between speech.start/end. Today wm-stt has `model: "distil-small.en"` in config but the bytes aren't there.
4. **PRD-wintermute-audio-aec** (drafted) — bind PipeWire's `module-echo-cancel`; without it, every TTS reply loops into the mic and wake-word fires on the daemon's own voice. Pure config + a Cargo.toml feature flag on wm-audio.
5. **PRD-wintermute-dialog-turn-fsm** (drafted) — wm-dialog's actual state machine. Listen → Wake → Capturing → Transcribing → Thinking → Speaking → Listen. Today wm-dialog is up but the FSM is partial; the events upstream of it have been the gate. Now they exist.
6. **PRD-wintermute-companion-boot** (drafted) — boot-on-power, autologin into wintermute.target, no greeter, recovery from power loss. The platform PRD already laid the supervisor; this PRD turns it into a kiosk.
7. **PRD-wintermute-companion-degrade** (drafted) — when STT fails, brain is unreachable, network is down, mic disappears: wintermute says so. A small TTS phrase bank ("I can't reach my brain right now", "I lost my microphone", "Hold on, I'm reconnecting") + retry semantics + a `wm.health.*` envelope set.

## Order

```
PRD-agorabus-multi-prefix-subscribe (already queued)
        │
        ▼
PRD-wintermute-audio-inference  ── PRD-wintermute-audio-aec (parallel)
        │
        ▼
PRD-wintermute-stt-whisper-model
        │
        ▼
PRD-wintermute-dialog-turn-fsm  ── PRD-wintermute-companion-degrade (parallel)
        │
        ▼
PRD-wintermute-companion-boot  (deployment capstone)
```

- The agorabus fix is the gate for barge-in; the rest can ship without it but barge-in won't work.
- Inference and AEC are independent — one is signal extraction, the other is signal cleaning.
- STT model depends on inference because it consumes the speech.start/end windows.
- Dialog FSM ties STT and brain and TTS together; it depends on STT being real.
- Degradation depends on the failure modes existing in the FSM.
- Boot is last because deploying a half-functional kiosk is worse than a working laptop.

## Open questions

1. **Local vs cloud STT.** whisper.cpp `distil-small.en` runs locally; ElevenLabs / OpenAI cloud is faster + bigger but adds a key, a network dep, and a privacy story. PRD-wintermute-stt-whisper-model picks local-first; cloud-fastpath is a Cargo feature flag.
2. **Wake word choice.** "hey wintermute" is on-brand but two-syllable wake words have higher false-positive rates than three. "okay nabu" is the default microWakeWord model and well-trained. Deferred to the deployment moment; PRD wires both.
3. **Form factor.** Laptop, RPi Zero, RPi 5, mini-PC. The PRDs target laptop because that's where we test; deployment is a sibling concern. wintermute-bootstrap's mDNS caregiver-setup flow already assumes a headless device.
4. **What does she hear when she summons it for the first time at her home?** Not in this vision's scope, but worth thinking about — a too-technical greeting will be alienating. Companion has a personality question lurking under it.
5. **Multi-turn memory.** wmd today is stateless across turns. For "what did I just say?" / "say that again louder" to work, the daemon needs short-term turn memory. Possibly a recall integration. Deferred to a future vision: *continuity-of-conversation*.
6. **Family routing.** Long-term: does jsy get notifications when mother summons wintermute? Does mother have a way to call jsy through it? Sibling vision.

## Notes for /build

- Order matters. inference and aec can run in parallel agents; everything else is gated downstream. Don't dispatch dialog-turn-fsm before stt-whisper-model is verified-completed.
- Each PRD is rust-extend, single-target, same shape as the bus-startup-defect / heartbeat-keepalive / pipewire-output / pipewire-input series that all shipped today.
- The install-path drift (cargo install → ~/.cargo/bin; systemd → ~/.local/bin) bit four PRDs in a row today. Companion-boot should fix it at the systemd unit level (point at /usr/local/bin/ with a system-wide install, or symlink-during-install).
- The `wm-audio` self-emitted-topic filter (sibling pattern from wm-tts) is critical for any new `wm.audio.*` topic this vision adds — every PRD in the fleet must apply it.
