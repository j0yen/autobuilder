# PRD: earshot-tts-legibility — speak slow enough and loud enough to land

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-tts
Vision: visions/earshot.md

## TL;DR

`wintermute-tts` speaks at piper's default rate and at whatever level the
synth emits — there is no speaking-rate control and no output gain
anywhere in the synth or playback path. For a hard-of-hearing elder
that's the difference between a companion she can follow and one she
can't. This PRD adds a speaking-rate knob (piper `--length_scale`) and an
output volume/gain knob to the TTS path, with defaults slightly slower
and louder than the developer baseline, tunable per deployment.

## Why this exists

Phase-1 source reading (2026-05-29) of `wintermute-tts/src/synth.rs`:

- `PiperSubprocess::render` invokes the piper CLI with exactly
  `--model <voice>.onnx` and `--output_file <out.wav>` and text on stdin
  (synth.rs:55, 101-105). There is **no `--length_scale`** — piper's
  native speaking-rate control (higher = slower) — and **no gain/volume**
  applied to the rendered WAV anywhere in synth or the daemon playback
  path.

The companion is for "a non-technical elder, jsy's mother" (companion.md
seed). Hearing loss is the common case in that population; the single
most effective accessibility levers for synthetic speech are *slower*
and *louder*. `hearth` makes the words kind; if she can't make them out,
the kindness doesn't arrive. piper exposes `--length_scale` for free —
the knob simply isn't wired. Gain is a trivial WAV post-process or a
playback-level setting. No PRD in the queue touches TTS rate
(grep-confirmed). This PRD is independent of the dialog earshot PRDs and
can build in parallel.

## What this builds

- A TTS voice/output config (e.g. a `[voice]` table) with
  `speaking_rate` (mapped to piper `--length_scale`) and `gain` (or
  `volume`) fields, deserialized from `wintermute-tts`'s existing config
  surface.
- `PiperSubprocess::render` passes `--length_scale <derived from
  speaking_rate>` to the piper invocation. Mapping documented (e.g.
  `speaking_rate` as a human-facing multiplier converted to piper's
  length-scale convention; rate < 1.0 = slower if that reads better to a
  caregiver — pick one and document it).
- Output gain applied on the synthesized audio (WAV sample scaling with
  clipping protection, or a pw-cat/playback volume argument — whichever
  fits the existing playback path), capped to a safe ceiling.
- Elder-friendly defaults: slightly slower than piper default, modest
  positive gain. Absent config → these defaults; existing behavior
  recovered by setting rate/gain to neutral.
- The ElevenLabs cloud path (cloud.rs / cloud_ws.rs) receives the
  equivalent where the API supports it (voice-settings stability/rate);
  if the cloud API has no rate knob, the PRD documents that and applies
  rate only on the piper path, gain on both.
- Gain must not clip: a synthetic full-scale sample at the configured
  gain stays within range (tested).

## Acceptance criteria

1. A TTS `[voice]` config exposes `speaking_rate` and `gain`; both
   deserialize and have documented elder-friendly defaults.
2. With a non-neutral `speaking_rate`, the piper subprocess argv includes
   `--length_scale <value>` derived from it; asserted by a test that
   inspects the constructed command/argv (no real piper binary required).
3. With neutral `speaking_rate`/`gain`, the rendered output matches
   today's behavior (no `--length_scale` forced to a non-default, or
   forced to piper's documented default; gain = unity) — i.e. the change
   is opt-in-by-default-but-tunable and never silently degrades.
4. Output gain is applied to synthesized audio and a full-scale input at
   max configured gain does not exceed sample range (clipping-protection
   test).
5. `gain` and `speaking_rate` are validated against documented bounds;
   out-of-range values are clamped or rejected with a logged warning.
6. The cloud path either applies the equivalent rate/gain or documents
   in code why it can't, with gain applied on both paths.
7. `cargo test` and `cargo clippy` (repo's existing lint bar) pass.
