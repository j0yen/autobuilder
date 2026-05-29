# PRD: wintermute-family — voice intents to reach jsy

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/kin.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-dialog
**build_version_bump:** minor
**Depends on:** PRD-wintermute-dialog-turn-fsm
**Codename:** *messenger* — she speaks, jsy hears.

## TL;DR

A device at mother's home can hear, think, and speak, but she has no way to
reach jsy *through* it. This PRD adds a Family branch to the dialog FSM:
when she says "tell Joe …" / "message Joe …" / "let Joe know …", wintermute
recognizes the intent, emits a `wm.family.message` envelope onto the bus,
and — when the delivery daemon acks — speaks a confirmation ("I let Joe
know"). It also defines the `wm.family.*` topic contract that the rest of
the kin fleet keys on, and routes inbound `wm.family.reply` envelopes to the
TTS path so jsy's reply is spoken back to her.

## 1. Why this exists

- **kin vision Component 1** — the contract-defining root of the fleet. Every
  other kin PRD declares topic constants matching the table this PRD ships.
- **No `wm.family.*` topic exists today.** Phase 1 grep of `wm\.[a-z.]+`
  across `~/wintermute/**/*.rs` found `wm.audio.*`, `wm.tts.*`,
  `wm.stt.final`, `wm.brain.reply`, `wm.browser.*` — and nothing family.
- **The request/reply shape already has a precedent.** `wm.browser.cmd` →
  `wm.browser.reply` (`wintermute-browser/src/protocol.rs:73,85`) is a
  working bus round-trip; `wm.family.message` → `wm.family.reply` mirrors it.
- **The dialog FSM is the right home.** `wintermute-dialog` owns the turn
  state machine (Listen → Wake → Capturing → … → Speaking); a Family branch
  is a natural new transition off the transcribed-intent state, and it keeps
  intent recognition deterministic rather than gated on the Claude API.

## 2. What this builds

### 2.1 Topic + envelope module (`src/family.rs`)

```rust
pub const TOPIC_FAMILY_MESSAGE:  &str = "wm.family.message";
pub const TOPIC_FAMILY_DISTRESS: &str = "wm.family.distress"; // defined here, fired by family-distress PRD
pub const TOPIC_FAMILY_ACK:      &str = "wm.family.ack";
pub const TOPIC_FAMILY_REPLY:    &str = "wm.family.reply";

#[derive(Serialize, Deserialize)]
pub struct FamilyMessage { pub to: String, pub body: String, pub urgency: Urgency, pub ts: i64 }
#[derive(Serialize, Deserialize)]
pub struct FamilyAck   { pub r#ref: String, pub delivered: bool, pub transport: String, pub ts: i64 }
#[derive(Serialize, Deserialize)]
pub struct FamilyReply { pub from: String, pub body: String, pub ts: i64 }
```

### 2.2 Intent recognition (deterministic, API-independent)

A matcher over the final transcript: leading verb + recipient token.

- Triggers: `tell|message|let … know|send … to|call` + an enrolled recipient
  name (default "Joe"; recipient set comes from family-enroll config, with a
  hard-coded "Joe" fallback so this PRD is testable standalone).
- Extracts the message body (everything after the recipient token) and emits
  `wm.family.message { to, body, urgency: Normal, ts }`.
- "call Joe" with no body emits `wm.family.message { body: "<she asked you
  to call>", urgency: Normal }` — reach decides how a call-request renders.
- Matching is case-insensitive, tolerant of STT punctuation, and MUST NOT
  require the Claude API (so it works when the brain is degraded).

### 2.3 FSM wiring

- New FSM state/branch `Family` reachable from the post-transcription state.
- On match → publish `wm.family.message`, transition to a `FamilyPending`
  wait for `wm.family.ack` (timeout → spoken "I couldn't reach Joe just now").
- On `wm.family.ack { delivered: true }` → speak confirmation via existing
  TTS path; on `delivered: false` → speak the failure phrase.
- Subscribe `wm.family.reply` independently of turn state → speak it ("Joe
  says: …") when it arrives, prefixed so she knows it's from jsy.

### 2.4 Self-emitted-topic filter

Apply the wm-* sibling filter: dialog must not re-consume its own
`wm.family.message`. Reuse the existing filter pattern in the repo.

## 3. Acceptance criteria

1. `src/family.rs` defines all four `wm.family.*` topic constants and the
   `FamilyMessage` / `FamilyAck` / `FamilyReply` types; each round-trips
   through serde_json (unit test).
2. Given transcript "tell Joe the heating is broken", the matcher produces
   `FamilyMessage { to: "Joe", body: "the heating is broken", urgency: Normal }`
   (unit test, no bus, no API).
3. Given transcript "what's the weather" (no family verb), the matcher
   produces `None` and the FSM takes its normal brain path (unit test).
4. Matching runs with the Claude API mocked-unreachable and still produces
   the envelope (proves API-independence).
5. Bus smoke test: a published `wm.family.ack { delivered: true }` drives the
   FSM to emit a TTS `wm.tts.say` confirmation containing "Joe".
6. Bus smoke test: a published `wm.family.reply { from: "Joe", body: "ok" }`
   produces a `wm.tts.say` whose text contains both "Joe" and "ok".
7. A `wm.family.ack` timeout (no ack within configurable window) produces the
   "couldn't reach Joe" spoken failure.
8. The dialog daemon does not re-consume its own `wm.family.message`
   (self-emitted-topic filter verified by smoke test).
9. Recipient name set is read from family-enroll config when present and
   falls back to "Joe" when absent; both paths covered by test.
10. `cargo test` green; `cargo clippy` clean; the daemon still passes its
    existing turn-FSM smoke tests (no regression).
