# PRD: wintermute-family — distress fast-path (reach jsy now)

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/kin.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-dialog
**build_version_bump:** minor
**Depends on:** PRD-wintermute-family-intents
**Codename:** *lifeline* — the one path that must work when nothing else does.

## TL;DR

The load-bearing reason kin exists: if mother says she's fallen or unwell,
wintermute must reach jsy *immediately* — not after a Claude round-trip, not
batched behind a digest, and it must say out loud that it's doing so. This
PRD adds a deterministic distress phrase bank to the dialog FSM that fires
`wm.family.distress` (highest priority) on the non-API path, speaks an
assurance ("I'm reaching Joe right now") via the degrade-phrase mechanism,
and handles the hard-vs-soft distress distinction (immediate vs confirm).

## 1. Why this exists

- **kin vision Component 2; the safety-critical one.** Open question #3 in
  the vision draws the immediate-vs-confirm line; this PRD implements it.
- **Must survive a degraded brain.** companion-degrade
  (`PRD-wintermute-companion-degrade.md`) established that wmd goes silent
  when the API key is missing or the network is down. A distress path gated
  on Claude would fail exactly when it matters. So distress detection is a
  deterministic phrase match in the FSM, identical in spirit to that PRD's
  reasoning that "silence is a failure mode."
- **The assurance phrase is a degrade phrase.** `wintermute-brain/src/degrade.rs`
  already maps a kind → a static spoken phrase. "I'm reaching Joe right now"
  is the same mechanism triggered by success-intent rather than failure —
  reuse it, don't invent a second TTS path.
- **`wm.family.distress` is already declared** as a topic constant by
  `PRD-wintermute-family-intents` (`src/family.rs`); this PRD is its first
  publisher.

## 2. What this builds

### 2.1 Distress phrase bank (`src/distress.rs`)

A static, ordered table of phrases with a severity tag:

```rust
enum Severity { Hard, Soft }   // Hard => fire immediately; Soft => confirm first

fn classify(transcript: &str) -> Option<Severity> {
    // Hard: "i've fallen", "i fell", "i need help", "call an ambulance", "emergency"
    // Soft: "i don't feel well", "i'm not well", "something's wrong", "i'm worried"
}
```

Matching is case-insensitive, substring-tolerant (STT rarely emits clean
punctuation), and ordered so Hard wins over Soft when both match.

### 2.2 FSM distress branch

- Checked **before** the normal family-message matcher and before the brain
  path — distress short-circuits everything.
- **Hard** → immediately publish `wm.family.distress { phrase, ts }`, speak
  the assurance ("I'm reaching Joe right now"), no confirmation step.
- **Soft** → speak "Should I let Joe know?", enter a one-turn confirm wait;
  "yes" (or silence-timeout configurable default) → fire distress; "no" →
  return to listening with "Okay, I won't."
- On `wm.family.ack { delivered: true }` for a distress ref → speak "Joe
  knows, he'll be in touch"; on delivery failure → speak "I couldn't reach
  Joe — try calling him directly" (the failure case is itself spoken, never
  silent).

### 2.3 Priority signaling

The `wm.family.distress` envelope carries no urgency field (its existence
*is* the urgency); `wintermute-reach` treats this topic as bypass-batching,
highest-priority. This PRD's contract: distress is published on its own
topic, never folded into `wm.family.message`.

## 3. Acceptance criteria

1. `classify("I've fallen and I can't get up")` returns `Some(Hard)`;
   `classify("I don't feel well today")` returns `Some(Soft)`;
   `classify("what time is it")` returns `None` (unit tests).
2. When both a Hard and a Soft phrase appear, Hard is returned (ordering test).
3. Hard distress publishes `wm.family.distress` with **no** intervening
   confirmation step and **no** Claude API call (verified with API mocked
   unreachable) — bus smoke test asserts the envelope is emitted.
4. Hard distress emits a `wm.tts.say` assurance containing "Joe" within the
   same FSM step as the distress publish (no await on ack first).
5. Soft distress emits a confirmation prompt and does **not** publish
   `wm.family.distress` until a "yes" arrives; a "no" returns to listening
   without publishing (two smoke tests).
6. The assurance phrase is sourced from the degrade-phrase mechanism
   (`degrade.rs` or its equivalent), not a new ad-hoc string table — verified
   by the phrase being registered there.
7. On `wm.family.ack { delivered: false }` for a distress ref, a spoken
   failure phrase is emitted (distress failure is never silent).
8. The distress check precedes the family-message matcher and the brain path
   in the FSM ordering (test that a transcript matching both distress and a
   family verb takes the distress branch).
9. Latency: from transcript-final to `wm.family.distress` publish, no network
   or API call occurs (assert via a no-egress test harness / mocked clients).
10. `cargo test` green; `cargo clippy` clean; family-intents tests still pass.
