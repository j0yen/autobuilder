# Vision: earshot — tuned to the person who will actually live with it

**Authored by:** /dream (Claude Opus 4.8), with jsy
**Created:** 2026-05-29
**Status:** active
**Seed:** companion.md's recurring frame — *"a non-technical elder, jsy's
mother"* — and `hearth`'s own scope boundary. `hearth` made the
companion's **words** warm. It said nothing about whether she can **hear
them**, or whether the device **waits** for someone who speaks slowly and
pauses mid-sentence. Caught live this pass by reading the fleet source
(citations below), not predicted.

## TL;DR

The voice loop works and `hearth` is giving it a personality. But every
timing and audio constant in that loop was tuned by a developer testing
it at his own desk, at his own pace, at his own hearing. The person it is
*for* is an elder who speaks more slowly, pauses inside a sentence,
hears less, and is unsettled when a machine cuts her off or talks too
fast to follow. earshot is the discipline of meeting her where she is:
the device waits long enough, reprompts gently instead of giving up after
one try, and speaks slowly and loudly enough to actually land.

Concretely, all confirmed by reading source in Phase 1:

- **Conversation tempo is compile-time.** `wintermute-dialog/src/fsm.rs`
  defines `CONFIRM_TIMEOUT_MS = 30_000` (fsm.rs:28) and
  `MAX_REPROMPTS = 1` (fsm.rs:31) as `const`s; the wider timing family
  (`CAPTURE_TIMEOUT_MS`, `TRANSCRIBE_TIMEOUT_MS`, `THINK_TIMEOUT_MS`,
  `STATE_HEARTBEAT_MS`) is re-exported from the same module (lib.rs:34-35).
  None of it is in a config table. An elder who pauses to think gets
  cut off by a deadline chosen for a developer's cadence — the *exact*
  `const`-not-config problem `hearth`'s persona-config just solved for
  the persona string, here for tempo.
- **One reprompt, then cold silence.** When the confirm timer fires the
  FSM walks `Confirming → ConfirmTimeout → DenyReason::Silence → Idle`
  (fsm.rs:236-252). The reprompt path exists (fsm.rs:402-415 emits a
  `reprompt_text`) but `MAX_REPROMPTS = 1` caps it at a single retry
  before the device just stops. For someone slow to respond, that reads
  as the companion losing interest.
- **TTS has no legibility knobs.** `PiperSubprocess::render` invokes the
  piper CLI with only `--model` and `--output_file` (synth.rs:101-105) —
  no `--length_scale` (piper's native speaking-rate control) and no
  output gain anywhere in the synth or playback path. There is no way to
  make wintermute speak slower or louder for a hard-of-hearing listener.

No PRD in the queue touches dialog timing or TTS rate (grep-confirmed,
Phase 1). This domain is unclaimed.

## End-state

When this vision is fulfilled:

1. **The device waits at her pace.** Capture, confirm, and silence
   windows come from a `[timing]` config table with elder-friendly
   defaults, not from `const`s. A pause mid-sentence does not end her
   turn; a few seconds of thought before answering does not abandon it.
2. **It reprompts gently, more than once.** When she doesn't answer, the
   companion checks in warmly ("I'm still here — take your time") and
   only returns to idle after a configurable number of patient tries,
   not a hard `MAX_REPROMPTS = 1`. The return-to-idle, when it comes, is
   said out loud and kindly ("I'll be right here when you need me"),
   not a silent state flip.
3. **It speaks to be heard.** TTS exposes a speaking rate (piper
   `--length_scale`, ElevenLabs voice-settings equivalent) and an output
   volume/gain, with defaults set slightly slower and louder than the
   developer baseline, tunable per deployment for a specific person's
   hearing.
4. **It listens with patience.** The VAD silence-hangover — the
   "confirmed silence" window before `wm.audio.speech.end` fires
   (events.rs:27) — is configurable, with a longer default so a natural
   mid-utterance pause isn't mistaken for the end of her turn.
5. **One register, one config story.** earshot's timing/audio settings
   sit alongside `hearth`'s `[persona]` table in the same deployment
   config surface, so a caregiver tunes "how she sounds and how patient
   she is" in one place.

## Components (PRD-sized pieces)

Each line is a PRD; all `rust-extend` into the named fleet repo.

1. **PRD-earshot-dialog-timing** (drafted) — the foundation. Lift the
   `fsm.rs` timing `const`s into a `[timing]` config table on
   `wintermute-dialog` with elder-friendly defaults, threaded into the
   FSM/daemon so timeouts are deployment-tunable. Mirrors hearth's
   persona-config `const`→config move (fsm.rs:28,31; lib.rs:34-35).
2. **PRD-earshot-vad-patience** (drafted) — expose the Silero VAD
   silence-hangover on `wintermute-audio` as config, longer default, so a
   mid-sentence pause doesn't prematurely fire `wm.audio.speech.end`
   (events.rs:27; lib.rs:24-25). Independent of dialog-timing.
3. **PRD-earshot-tts-legibility** (drafted) — add speaking-rate (piper
   `--length_scale`) and output gain/volume to `wintermute-tts`'s synth +
   playback path, which today has neither (synth.rs:101-105). Cloud
   (ElevenLabs) path gets the voice-settings equivalent. Independent of
   the dialog PRDs.
4. **PRD-earshot-gentle-reprompt** (drafted) — raise the single-shot
   reprompt to a configurable patient sequence and make the
   silence→idle return warm and spoken, in the FSM confirm-timeout path
   (fsm.rs:236-252, 402-415). Depends on dialog-timing (reads its reprompt
   count + cadence knobs).

## Order

```
PRD-earshot-dialog-timing  (foundation — introduces the [timing] config)
        │
        ├──► PRD-earshot-vad-patience    (wm-audio, independent — parallel)
        ├──► PRD-earshot-tts-legibility  (wm-tts,   independent — parallel)
        │
        ▼
PRD-earshot-gentle-reprompt  (wm-dialog, reads dialog-timing's reprompt knobs)
```

## Scope boundary (do not merge with hearth)

`hearth-dialog-degrade-warmth` owns `wintermute-dialog/src/degrade.rs` —
the **fault** phrase bank (STT-uncertain, transcribe-timeout). earshot's
gentle-reprompt owns the **silence / no-response** path in the FSM
(`fsm.rs` Confirming → ConfirmTimeout → DenyReason::Silence). Different
module, different trigger: hearth answers "I didn't understand you,"
earshot answers "I'm still waiting for you." They share the wm-tts path
but never touch each other's source. Both modify `wintermute-dialog`, so
serialize earshot's two dialog PRDs against each other and against the
in-flight hearth-dialog-degrade-warmth (all touch the same crate; expect
rebases, not logic conflicts).

## Open questions

1. **Whose pace?** Defaults are "elder-friendly" but one elder is not
   another. v1 ships generous static defaults + a config table; a
   *learned* pace (widen the silence window if she's repeatedly cut off,
   per observed turn timings) is a later vision — note it, don't build it.
2. **Loudness vs. the AEC loop.** Raising TTS gain feeds more energy back
   toward the mic; `wintermute-audio-aec` (companion fleet) must still
   cancel it. tts-legibility should cap gain and flag the interaction;
   verifying it doesn't reopen the wake-on-own-voice problem is a
   deployment smoke test, not a unit AC.
3. **Rate via piper vs. SSML.** piper `--length_scale` is the simplest
   lever and is what tts-legibility uses; per-phrase SSML prosody (slow
   only the important clause) is richer but backend-specific — deferred.
4. **One config file or three?** dialog `[timing]`, audio `[vad]`, tts
   `[voice]` live in three repos' configs today. A unified deployment
   config (one file a caregiver edits) is a `homestead`/`onramp`-adjacent
   concern; earshot ships the three tables and leaves unification to the
   deployment vision.

## Notes for /build

- dialog-timing is the gate for gentle-reprompt; vad-patience and
  tts-legibility are fully independent and can run as parallel agents.
- dialog-timing and gentle-reprompt both edit `fsm.rs` — do not dispatch
  them concurrently; build timing first, verified, then reprompt.
- Watch `hearth-dialog-degrade-warmth`: it's in-flight on the same crate.
  No logic overlap (degrade.rs vs fsm.rs) but Cargo/lib.rs re-export
  churn may force a rebase.
- Every PRD is `rust-extend`, single-target, same shape as the companion
  + hearth fleets. Elder-friendly defaults must not break existing tests
  that pin the old `const` values — those assertions get rewritten to the
  new config-sourced invariant, not deleted (same discipline the
  continuity-of-conversation fleet used for `req.messages.len()`).
