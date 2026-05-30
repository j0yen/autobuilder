# Vision: rouse — the deaf loop wakes, and can prove it

**Authored by:** /dream (Claude Opus 4.8), with jsy
**Created:** 2026-05-29
**Status:** active
**Seed:** explicit `/dream` from jsy after a live voice-debug session
(2026-05-29). We tried to talk to wintermute and nothing happened. The
investigation (memory `project_voice_input_null_detectors.md`) found the
voice loop is plumbed end-to-end but **deaf**: wm-audio ships no-op
detectors, so no wake word ever fires and no speech segment is ever cut.

## TL;DR

wintermute's voice input is plumbed but not alive. `wm-audio` v0.2.0
captures the mic and broadcasts PCM on a UDS fanout + the bus — verified
working with real signal (peak 20%, RMS 1284) this session. But the
two stages that turn sound into *meaning* are placeholder no-ops:
`daemon.rs:86` wires a `NullWakeDetector` (`process()` always returns
`NotDetected`) and `daemon.rs:93` a `NullVadDetector`. There is no
ONNX inference in the crate and `/usr/share/wintermute/models/{wake,vad}/`
are root-owned and **empty**. So "always listening" is, today, a fiction:
the bytes flow and nothing in the fleet can act on them.

The center of the fix already exists as a queued, well-formed PRD —
`PRD-wintermute-audio-inference.md` (microWakeWord + Silero VAD via
`ort`). rouse does **not** re-draft it. rouse builds the two pieces that
bracket it and that it hand-waves: the **floor** (actually get the models
onto disk — the dir is root-owned and there is no install.sh) and the
**ceiling** (a runtime command that proves the live capture→wake→VAD→STT
chain emits events, so "is voice working?" is one command instead of the
30-minute manual bus-subscribe investigation it took this session).

When rouse + audio-inference land, every queued `earshot-*` PRD finally
has a real loop to tune — today they tune a "Silero VAD silence-hangover"
that does not exist (earshot.md:69-72 assumes a working VAD; it's the
null detector).

## End-state

When this vision is fulfilled:

1. **The models are on disk, reproducibly.** A first-class
   `wm-audio fetch-models` action downloads, checksum-verifies, and
   installs the microWakeWord wake models (hey_jarvis / okay_nabu /
   hey_mycroft — the three `WakeWord` enum variants, config.rs:14-18)
   and the Silero VAD model into the model dirs, idempotently, with
   provenance recorded. The `wm-models bundle` that config.rs:11 already
   *names* becomes real.
2. **The detectors actually fire.** (Owned by the existing
   `audio-inference` PRD — rouse depends on it, does not duplicate it.)
   Real ONNX wake + VAD inference replaces the nulls via the hot-swap
   hook already present at `daemon.rs:122` `with_wake_detector()`.
3. **The fleet can answer "am I actually hearing?"** A
   `wm-audio selftest` runtime check injects a known wake-word fixture
   (or `--live` reads the mic) through the *real* inference path and
   asserts `wm.audio.wake` + `wm.audio.speech.{start,end}` appear on the
   bus, with an exit-code contract — the voice-path analogue of the
   `agorabus doctor` shipped 2026-05-29. Operationally: the thing that
   would have answered this session's question in one command.

## Components (PRD-sized pieces)

1. **PRD-rouse-wake-vad-models** (drafted) — the floor. `rust-extend`
   wm-audio: a `wm-audio fetch-models` subcommand + module that
   provisions the microWakeWord + Silero VAD ONNX models into
   `/usr/share/wintermute/models/{wake,vad}/` (sudo) or a `--prefix`
   dir, idempotent, checksum-verified, provenance-logged. Realizes the
   `wm-models bundle` named at config.rs:11. Independent of inference —
   can ship first.
2. **PRD-wintermute-audio-inference** (ALREADY DRAFTED, queued — do NOT
   re-draft) — the center. microWakeWord wake + Silero VAD inference
   replacing the null detectors. rouse cites it as the gating dependency
   for component 3 and the consumer of component 1's models.
3. **PRD-rouse-voice-selftest** (drafted) — the ceiling. `rust-extend`
   wm-audio: a `wm-audio selftest` runtime command that drives a fixture
   (or live mic) through the real pipeline and asserts the event chain,
   exit-code contract like `agorabus doctor`. Depends on both the models
   (1) and real inference (2) to assert against something that fires.

## Order

```
PRD-rouse-wake-vad-models   (floor — models on disk; independent, ship first)
        │
        ▼
PRD-wintermute-audio-inference  (center — EXISTING queued PRD; real detectors)
        │
        ▼
PRD-rouse-voice-selftest    (ceiling — proves the live chain; needs 1 + 2)
```

## Open questions

1. **Canonical wake word.** config defaults to `hey_jarvis`
   (config.rs:101) and the daemon logs `wake=hey-jarvis`, but the
   audio-inference PRD's AC6 says speak *"hey wintermute"* and offers
   `okay_nabu` too. Three names float across the fleet. rouse-wake-vad-
   models provisions all three enum variants; settling the *default* the
   product ships with is a product call, noted not built.
2. **System vs user model dir.** wm-stt hardcodes
   `models_root:"/usr/share/wintermute/models"` (root-owned). Provisioning
   there needs sudo; a user-dir (`~/.local/share/wintermute/models`)
   avoids privilege but diverges from wm-stt's path. rouse-wake-vad-models
   defaults to the system dir (consistency) with `--prefix` for
   unprivileged/test installs; unifying the path convention is an
   `onramp`/`homestead` deployment concern.
3. **selftest: live vs fixture.** Fixture injection is deterministic and
   CI-able; `--live` mic is the real operational check but needs a human
   to speak. Ship both modes; default to fixture.
4. **Model licensing/source pinning.** microWakeWord models are
   Apache-2.0 (esphome ecosystem); Silero VAD is MIT. rouse-wake-vad-
   models pins exact source URLs + sha256; if upstream moves, the
   provisioning step fails loud rather than installing an unverified blob.

## Notes for /build

- **CRITICAL ordering correction for the earshot fleet:** all four
  `earshot-*` PRDs (queued) tune timing/VAD/TTS knobs of a voice loop
  that does not yet detect anything. `earshot-vad-patience` in particular
  tunes a VAD silence-hangover that is currently a `NullVadDetector`.
  Do not ship/verify earshot's VAD or reprompt PRDs as "working" until
  `audio-inference` (real detectors) ships — their human-gate ACs will
  silently pass against a loop that never fires. earshot's dialog-timing
  + tts-legibility PRDs are unaffected (pure config/synth, no detector
  dependency).
- rouse-wake-vad-models is independent — dispatch first; it de-risks
  audio-inference (which can then assume models present rather than
  carrying its own download path).
- rouse-voice-selftest must build LAST (needs real detectors + models).
- `ort`/onnxruntime is already a fleet-wide dep (agorabus, cadence,
  atlas, ac-judge, ambient, …) — the inference runtime is proven; no new
  vendoring risk.
- Both rouse PRDs are `rust-extend` single-target into
  `~/wintermute/wintermute-audio`, same shape as the rest of the fleet.
