# PRD: rouse-voice-selftest — the fleet answers "am I actually hearing?"

Status: landed v0.6.0
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-audio
Vision: visions/rouse.md
Depends on: PRD-wintermute-audio-inference (real detectors to assert against),
  PRD-rouse-wake-vad-models (models on disk)
deferred_acs: [7]
deferred_ac_reasons: {"7": "live-mode smoke requires a human to speak the wake word in the deployed fleet — hardware-gated, not automatable in CI"}
Codename: *audiometry* — a hearing test for the machine.

## TL;DR

On 2026-05-29 we tried to talk to wintermute and nothing happened.
Diagnosing it took a long manual session: hand-rolled `agorabus
subscribe` loops, journal greps, a direct `pw-record` amplitude check,
and finally reading `daemon.rs` to discover the detectors are no-ops.
The fleet had no way to answer the one operational question — *is the
voice path actually producing events right now?* — short of that
investigation. This PRD adds `wm-audio selftest`: a runtime command that
drives a known wake-word fixture (or `--live` mic) through the **real**
inference pipeline and asserts `wm.audio.wake` and
`wm.audio.speech.{start,end}` appear on the bus, with an exit-code
contract. It is the voice-path analogue of the `agorabus doctor`
subcommand shipped the same day.

## Why this exists

- **This session is the evidence.** Verifying voice was broken required
  ~30 minutes of ad-hoc tooling (multiple `agorabus subscribe`
  windows, a `pw-record` peak/RMS measurement showing real signal, a
  self-test publish/subscribe round-trip to rule out the bus, and source
  reading). A single `wm-audio selftest` would have returned
  `stale/deaf: wake never fired` immediately. Recorded in memory
  `project_voice_input_null_detectors.md`.
- **Existing smoke tests don't cover this.** `tests/wake_bus_smoke.rs`
  and `tests/vad_bus_smoke.rs` inject an `AlwaysWake`-style **stub**
  detector (wake_bus_smoke.rs:66-70 forces a `WakeOutcome`) to exercise
  the *bus topology*. They prove the publish path with a fake detector —
  they do **not** run real ONNX inference, do **not** read the mic, and
  do **not** verify the live daemon. There is no runtime, real-detector,
  end-to-end check.
- **"active" ≠ "hearing".** This session also showed wm-audio can be
  `systemctl is-active` = active while publishing into a dead bus
  connection (it logged `publish failed ... error=send_line` only on
  shutdown). Unit health and systemd state both lied; only observing
  actual events told the truth. A selftest observes actual events.
- **Symmetry with the fleet's new self-describing pattern.** `agorabus
  doctor` (shipped 2026-05-29) lets the bus report its own currency;
  `wm-audio selftest` lets the ear report its own function. The fleet is
  growing first-class "am I working?" surfaces; voice should have one.

## What this builds

Extends `~/wintermute/wintermute-audio/` (rust-extend; adds one
subcommand + a harness module, no change to `start`):

- **New subcommand `wm-audio selftest`** (and a `src/selftest.rs`
  module). Two modes:
  1. **Fixture mode (default).** Loads a bundled known-positive wake
     fixture (`tests/fixtures/hey_jarvis.wav`, the same asset
     audio-inference AC3 introduces) and a speech-with-silence clip,
     feeds them through the **real** wake + VAD detectors (the production
     code path, not a stub), and asserts the resulting events. Runs
     self-contained — spins up the inference workers against an in-process
     bus or a scratch socket; does not require the system daemon to be
     running.
  2. **Live mode (`--live [secs]`).** Subscribes to the running daemon's
     bus and listens for `wm.audio.wake` + `wm.audio.speech.{start,end}`
     for N seconds (default 10) while a human speaks the configured wake
     word + a sentence. The operational "is the deployed fleet hearing me
     right now?" check.
- **Assertions / verdicts.** A `current`/healthy verdict requires, within
  the mode's window: ≥1 `wm.audio.wake` for the configured wake word, and
  a matched `wm.audio.speech.start`→`wm.audio.speech.end` pair. Verdicts:
  `healthy` | `deaf: no-wake` | `deaf: no-speech-segment` |
  `unreachable: <reason>` (no daemon in live mode / models missing /
  fixture unreadable).
- **Exit-code contract** (mirrors `agorabus doctor`): `0` = healthy,
  `1` = deaf (pipeline up but no events — the exact 2026-05-29 condition),
  `2` = could-not-run (models absent → point at `wm-audio fetch-models`;
  no daemon in `--live`; unreadable fixture). `--format json|text`
  (default text); JSON emits `{mode, wake_word, events_seen:
  {wake, speech_start, speech_end}, verdict, detail}`.
- **Diagnostic breadcrumbs.** On a `deaf` verdict, selftest reports
  *which* stage was silent and the likely cause — e.g. "wake never fired:
  detector is NullWakeDetector? run `wm-audio fetch-models` and confirm
  audio-inference is built" — so the verdict points at the fix, not just
  the symptom. (Directly encodes this session's diagnostic path.)
- Reuses the crate's existing bus client + fanout test harness
  (`tests/fanout_smoke.rs`, `tests/wake_bus_smoke.rs` patterns). README +
  CHANGELOG + version bump per convention.

## Acceptance criteria

1. `wm-audio selftest --help` documents `--live`, `--format`, and the
   exit-code contract; `wm-audio --help` lists it. Pre-existing
   subcommands/tests unchanged and passing; clippy clean.
2. **Healthy path (fixture):** with real detectors built and models
   present, `wm-audio selftest` feeds the wake fixture + speech clip
   through the real pipeline, observes ≥1 `wm.audio.wake` and a
   speech.start/end pair, prints `healthy`, and exits 0.
3. **Deaf path:** with the wake detector forced to the null/no-op backend
   (or models absent so the detector falls back to null per audio-
   inference AC7), `selftest` observes no wake event, prints
   `deaf: no-wake` with the diagnostic pointer, and exits 1. (This is the
   regression lock for the 2026-05-29 condition.)
4. **No-speech-segment path:** with wake firing but VAD null/silent,
   `selftest` prints `deaf: no-speech-segment` and exits 1.
5. **Could-not-run path:** with models absent AND no fallback events,
   selftest exits 2 (not 1) and its message names `wm-audio fetch-models`
   as the remedy; in `--live` with no running daemon it exits 2 with
   `unreachable: no daemon`.
6. `--format json` emits valid JSON with `mode`, `wake_word`,
   `events_seen`, `verdict`, and `detail`; `--format text` is
   human-readable.
7. **Live mode smoke (human-gated):** `wm-audio selftest --live 12`
   against the running fleet, speaking the configured wake word then a
   sentence, prints `healthy` and exits 0; staying silent prints
   `deaf: no-wake` and exits 1.
8. `cargo test --release` ≥ current+6 (healthy fixture verdict, deaf
   no-wake, deaf no-speech, exit-2 models-absent, json shape, live-mode
   no-daemon exit-2 — all using in-process bus/stub injection, no live
   mic in CI). `cargo deny check bans licenses sources` clean.

## Non-goals

1. Implementing the detectors — that's `audio-inference`. selftest
   *exercises* them; it does not provide them.
2. Provisioning models — that's `rouse-wake-vad-models`. selftest only
   *detects* their absence and points at the fix.
3. Continuous monitoring / alerting — selftest is a one-shot command. A
   periodic voice-health probe (self-review playbook calling
   `wm-audio selftest`) is a natural follow-on but out of scope here.
4. STT/dialog/brain/TTS round-trip — selftest asserts up to the
   `wm.audio.speech.*` events (the wm-audio boundary). End-to-end-to-TTS
   verification is a separate, larger harness.
