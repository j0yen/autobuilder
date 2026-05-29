# PRD: earshot-gentle-reprompt — patient, spoken, more-than-once

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-dialog
Vision: visions/earshot.md
Depends-on: PRD-earshot-dialog-timing.md

## TL;DR

When an elder doesn't answer in time, the dialog FSM reprompts exactly
once and then walks silently back to idle. That reads as the companion
losing interest. This PRD turns the single-shot reprompt into a
configurable, patient sequence of warm check-ins, and makes the eventual
return-to-idle a kind spoken line rather than a silent state flip.

## Why this exists

Phase-1 source reading (2026-05-29) of `wintermute-dialog/src/fsm.rs`:

- The confirm-timeout path is `(State::Confirming(ctx),
  Event::ConfirmTimeout) → DenyReason::Silence → Idle` (fsm.rs:236-252).
  When the timer fires, the turn ends.
- A reprompt path exists — `ConfirmDecision::Reprompt` emits a
  `reprompt_text` and restarts the timer (fsm.rs:402-415) — but
  `MAX_REPROMPTS = 1` (fsm.rs:31) caps it: one nudge, then silence.
- The silence branch returns to idle without saying anything to her.

The companion is for "a non-technical elder, jsy's mother" (companion.md
seed). She may be slow to respond, mishear the prompt, or need a moment.
One reprompt and a silent exit is exactly the cold behavior earshot
exists to fix. `earshot-dialog-timing` makes `max_reprompts` and the
confirm window configurable; this PRD spends those knobs on *warmth*:
escalating, gentle prompts and a spoken, kind close.

### Scope boundary — earshot vs hearth (load-bearing)

This PRD owns the **silence / no-response** path in `fsm.rs`. It does
**not** touch `degrade.rs`, which `hearth-dialog-degrade-warmth` owns —
that bank is for *fault* phrasing ("I didn't catch that," STT-uncertain /
transcribe-timeout). The distinction: hearth answers *"I didn't
understand you"*; earshot answers *"I'm still waiting for you."* Same
wm-tts output path, different FSM trigger, different module. Both modify
`wintermute-dialog`; serialize this PRD after `earshot-dialog-timing`
(both edit `fsm.rs`) and expect a possible rebase against in-flight
`hearth-dialog-degrade-warmth`.

## What this builds

- A small, ordered phrase set for the no-response sequence — escalating
  patience, e.g. attempt 1: "I'm still here — take your time," attempt 2:
  "Whenever you're ready," final/close: "I'll be right here when you need
  me." Phrases live in this module (the silence bank), not in
  `degrade.rs`. Defaults provided; optionally overridable via config
  alongside the `[timing]` table.
- The reprompt logic reads `max_reprompts` from `DialogTimingConfig`
  (from `earshot-dialog-timing`) instead of the `MAX_REPROMPTS` const,
  and selects the phrase for the current attempt index.
- On the final timeout (attempts exhausted), the FSM emits a spoken
  close line via the TTS path **before** transitioning to Idle — the
  return-to-idle is announced, not silent. `DenyReason::Silence` is still
  the recorded reason; the change is that a warm utterance accompanies it.
- Attempt counting and timer restart reuse the existing
  `ConfirmContext`/attempt machinery (fsm.rs:402-415); no new state is
  added to the FSM graph.

## Acceptance criteria

1. With `max_reprompts >= 2` (from dialog-timing config), a Confirming
   turn that times out reprompts that many times, emitting a distinct,
   escalating phrase per attempt, before ending — asserted by driving
   repeated `ConfirmTimeout` events and checking the emitted texts.
2. The phrase selected matches the attempt index (attempt 1 ≠ attempt 2),
   pulled from the silence bank, not from `degrade.rs`.
3. On the final timeout the FSM emits a spoken close line through the TTS
   output path *before* the `Confirming → Idle` transition; the recorded
   `DenyReason` is still `Silence`.
4. `max_reprompts` is sourced from `DialogTimingConfig`, not the
   `MAX_REPROMPTS` const; setting it to 1 reproduces today's single-shot
   behavior (regression guard).
5. No code in `degrade.rs` is added or modified by this PRD
   (grep-confirmable); the silence phrases live in the FSM/silence module.
6. Existing confirm-timeout tests (fsm.rs:236-252 path) are updated to the
   new multi-attempt + spoken-close behavior and pass; none deleted to go
   green.
7. `cargo test` and `cargo clippy` (repo's existing lint bar) pass.
