# PRD: almanac-acknowledge — hear "I took it," snooze "later," catch silence

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-brain
Vision: visions/almanac.md

## TL;DR

A spoken reminder she can't answer is a one-way announcement, not a
companion. This PRD closes the loop: after almanac speaks a prompt, the
next thing she says is treated as an acknowledgment — "I took it" → done,
"in a minute" → snooze and re-ask later, and silence past earshot's
patience window → marked *missed* with one gentle re-ask. The outcome is
published as `wm.almanac.ack {id, state}`.

## Why this exists

- **speak-bridge (its dependency) only emits; nothing listens for the
  reply.** Without this, a missed dose is indistinguishable from a taken
  one — the exact failure mode that makes a medication reminder unsafe.
- **earshot already defined the patience window; reuse it.** earshot's
  dialog-timing PRD moves the FSM timeouts (`wintermute-dialog/src/fsm.rs`
  `CONFIRM_TIMEOUT_MS`, `MAX_REPROMPTS`) into config. The almanac
  acknowledgment wait must read that same patience, not invent a new
  deadline (scope boundary: almanac owns *when to prompt*, earshot owns
  *how long to wait*).
- **The brain already sees `wm.stt.final`.** wmd's subscribe loop routes
  transcripts today; correlating the next transcript after a due-prompt is
  an in-place extension of that dispatch, not new audio plumbing.

## What this builds

Extends `wintermute-brain`:

- A short-lived `PendingAck { id, category, asked_ms, snoozes_used }` on
  `DaemonState`, set when `handle_almanac_due` speaks a prompt (so this PRD
  depends on speak-bridge having published the prompt).
- In the `wm.stt.final` handler, if a `PendingAck` is open and within the
  earshot patience window, classify the transcript (simple keyword tiers,
  no LLM needed for v0.1):
  - **done** — "took it", "okay", "done", "yes", "I did" → clear pending,
    emit `wm.almanac.ack {id, state:"done"}`.
  - **snooze** — "later", "in a minute", "not yet", "soon" → if
    `snoozes_used < max_snoozes`, emit `{state:"snoozed"}` and publish a
    request for the tick-daemon to re-arm at `now + snooze_min` (topic
    `wm.almanac.snooze {id, resume_ts}`); else treat as missed.
  - **unrelated** — transcript doesn't match either tier → leave pending
    open (she may answer next turn) until the window expires.
- A timeout path: if the patience window elapses with no qualifying
  acknowledgment, emit `wm.almanac.ack {id, state:"missed"}` and speak one
  gentle re-ask (a single proactive reply via the speak-bridge path); a
  second timeout finalizes as missed without further re-asking.
- All thresholds (`max_snoozes`, `snooze_min`) come from the entry (carried
  on the due/ack envelopes); the *wait duration* comes from earshot's
  timing config. No new timing constants in this crate.

## Acceptance criteria

1. After a due prompt sets `PendingAck`, a `wm.stt.final` of "I took it" emits `wm.almanac.ack {id, state:"done"}` and clears the pending state.
2. "in a minute" with `snoozes_used < max_snoozes` emits `{state:"snoozed"}` and publishes `wm.almanac.snooze {id, resume_ts ≈ now + snooze_min}`; the same input once `snoozes_used == max_snoozes` emits `{state:"missed"}` instead.
3. An unrelated transcript while a `PendingAck` is open leaves it open (no ack emitted) and the prompt is not double-counted.
4. When the earshot patience window elapses with no qualifying reply, the daemon emits `{state:"missed"}` and speaks exactly one gentle re-ask via the speak-bridge reply path; a second elapse finalizes missed with no further re-ask.
5. The acknowledgment wait duration is sourced from earshot's dialog-timing config (assert the value tracks that config, not a literal in this crate's diff).
6. Classification is deterministic keyword tiers (no network/LLM call) and is unit-tested across done / snooze / unrelated transcripts including case and punctuation variants.
7. With no `PendingAck` open, an ordinary `wm.stt.final` is handled exactly as today (regression: existing transcript-handling tests pass unchanged).
8. `cargo test` green.
