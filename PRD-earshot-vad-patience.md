# PRD: earshot-vad-patience — don't mistake a pause for the end of her turn

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-audio
Vision: visions/earshot.md

## TL;DR

`wintermute-audio` ends an utterance when it detects "confirmed silence,"
but the silence-hangover window that defines "confirmed" is not
configurable and is tuned for ordinary speech. An elder who pauses
mid-sentence to find a word trips that window and gets her turn ended
underneath her. This PRD exposes the VAD silence-hangover as config with
a longer, elder-friendly default, so a natural pause is heard as a pause,
not as the end of speech.

## Why this exists

Phase-1 source reading (2026-05-29) of `wintermute-audio`:

- `wm.audio.speech.end` is documented as the "falling-edge (after
  confirmed silence)" (events.rs:27). The window of silence that must
  elapse before that edge fires is the hangover; it is the single
  parameter that decides whether a mid-sentence pause ends the turn.
- The Silero VAD plugs in behind `VadDetector` (lib.rs:24-25, 37). The
  hangover is set for normal-cadence speech and is not surfaced as a
  tunable.

For "a non-technical elder, jsy's mother" (companion.md seed) the pause
inside a sentence — "I'd like to call my… my daughter" — is normal
speech, not its end. A hangover tuned for a developer's clipped phrasing
cuts her off, the partial transcript goes to the brain, and she gets a
reply to half a thought. `earshot-dialog-timing` makes the dialog FSM
patient on the *capture/confirm* side; this PRD makes the *audio* side
patient at the source, so the speech boundary itself is forgiving. The
two are independent (different repos, different layer) and compose.

## What this builds

- A VAD/segmentation config field for the silence-hangover duration
  (e.g. `speech_end_silence_ms`), deserialized from `wintermute-audio`'s
  existing config surface, read by the VAD/segmentation path that emits
  `wm.audio.speech.end`.
- An elder-friendly default longer than today's effective hangover, with
  the rationale documented inline. Absent config → that default; existing
  deployments keep working.
- A documented floor/ceiling so the value can't be set so long that
  barge-in/turn-taking breaks (the window must stay well under the
  dialog confirm timeout) or so short that it regresses today's behavior.
- A unit test driving the segmentation logic with a synthetic
  pause-then-resume PCM/energy sequence: with the longer hangover the
  pause does **not** emit `speech.end`; the utterance continues to
  `speech.chunk` and only ends after the full configured silence.

## Acceptance criteria

1. The silence-hangover before `wm.audio.speech.end` is read from a
   config field, not a hardcoded literal; the default is longer than the
   current effective value and documented as elder-friendly.
2. With no config override, the daemon starts and uses the default; a
   provided override changes the effective hangover.
3. A test feeds a speech→short-pause→speech sequence and asserts that
   with the elder default no premature `speech.end` is emitted during the
   pause, and exactly one `speech.end` fires after sustained silence.
4. The configured hangover is validated against a documented floor and
   ceiling (rejected or clamped with a logged warning if out of range),
   so it cannot be set longer than is safe for turn-taking.
5. The wm-audio self-emitted-topic filter and existing event shapes are
   unchanged; no new topic is introduced (this tunes timing of an
   existing event).
6. `cargo test` and `cargo clippy` (repo's existing lint bar) pass.
