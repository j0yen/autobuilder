# PRD: wmd-repair-affordances — "say that again, louder"

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/continuity-of-conversation.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-brain
**build_version_bump:** minor
**Depends on:** PRD-wmd-turn-history
**Codename:** *come-again* — the smallest, most human payoff of having a history buffer.

## TL;DR

Once wmd holds a turn-history buffer (PRD-wmd-turn-history), the most
common companion repair requests become near-free: "say that again",
"what did you just say?", "louder", "I didn't ask that." These are the
exact phrases companion.md OQ#5 and dialog-turn-fsm non-goal #1 call out
as broken today. This PRD recognizes them brain-side and replays the last
assistant turn from the history buffer — without a model round-trip when
the request is pure replay — optionally tagging the reply with a
`loudness` hint for wm-tts.

## 1. Why this exists

- **They're the failure mode you hit first.** A companion mishears or
  speaks too quietly constantly. "Say that again, louder" is the single
  most likely follow-up an older user gives, and today it produces a
  fresh model call that may answer a *different* question, because wmd
  has no idea "that" refers to its last reply.
- **The history buffer makes it trivial.** PRD-wmd-turn-history stores
  the last assistant turn. Replay is reading `history.last().assistant` —
  no API call, no latency, no token cost, no risk of a divergent answer.
- **Latency matters for repair.** A round-trip to Claude to re-say what
  was just said is wasteful and slow; the user is already mildly
  frustrated. Local replay answers in milliseconds.

## 2. What this builds

### 2.1 Repair-intent matcher

A matcher over the trimmed transcript, run *before* the LLM dispatch in
`handle_turn_user`, classifying into:

```rust
enum Repair {
    RepeatLast,         // "say that again", "what did you say", "come again"
    RepeatLouder,       // "louder", "say that again louder", "speak up"
    None,               // fall through to normal LLM turn
}
```

Phrase lists are config (`repair_repeat_phrases`, `repair_louder_phrases`)
with defaults; matched case-insensitively on a punctuation-stripped
transcript. Conservative by design: only short, unambiguous utterances
match (a long sentence containing "louder" is a real question, not a
repair) — gate on transcript word-count ≤ a small threshold.

### 2.2 Replay path

- `RepeatLast` → re-publish the stored last assistant text as a fresh
  `wm.brain.reply` (new `ts`). No LLM call. If history is empty (nothing
  to repeat), fall through to a degrade phrase ("I haven't said anything
  yet") via the normal reply path.
- `RepeatLouder` → same, but the reply envelope carries `loudness:
  "loud"` (a new optional field on `ReplyEvent`, default absent). wm-tts
  may honor it (volume bump); if it ignores it, behavior degrades to a
  plain repeat — no hard dependency on a wm-tts change.
- The replayed turn is **not** re-pushed into history (it's the same
  turn, not a new one) — prevents "say that again" × 3 from filling the
  ring with duplicates.

### 2.3 Interaction with session-boundary

Repair requests extend the current session like any turn (they reset the
idle clock) but never *open* a session on their own meaningfully — if
there's no history, there's nothing to repeat. A repair as the first
utterance of a session degrades gracefully (2.2).

## 3. Acceptance tests

1. **AC1 — repeat replays last reply, no LLM call.** With a prior turn in
   history, "say that again" publishes a `wm.brain.reply` whose text
   equals the last assistant text, and the mocked LLM's
   `collect_messages` is **not** invoked (assert call count unchanged).
2. **AC2 — louder sets the loudness hint.** "say that louder" publishes a
   reply with `loudness == "loud"` and text == last assistant text.
3. **AC3 — empty history degrades.** "say that again" as the first turn
   of a session publishes a graceful degrade phrase, not an empty reply
   or a crash.
4. **AC4 — replay doesn't grow history.** After a real turn then two
   "say that again"s, history holds exactly one turn.
5. **AC5 — long sentence containing a keyword is NOT a repair.** "Could
   you speak a bit about why the radio is louder at night?" goes to the
   LLM (word-count + ambiguity guard), not the replay path.
6. **AC6 — loudness field is backward-compatible.** `ReplyEvent` without
   `loudness` serializes/deserializes unchanged (`#[serde(skip_serializing
   _if = "Option::is_none")]`); existing wm-tts consumers unaffected.
7. **AC7 — config round-trips.** Repair phrase lists persist through
   brain.toml.
8. **AC8 — `cargo test --release --lib` ≥ current+8.**
9. **AC9 — daemon active 60s, NRestarts=0.**
10. **AC10 — `cargo deny check bans licenses sources` clean.**

## 4. Non-goals

1. **Paraphrase / "say it simpler".** That genuinely needs the model;
   only verbatim replay is local here. A `Rephrase` variant is a future
   PRD.
2. **wm-tts volume implementation.** This PRD only *emits* the loudness
   hint; honoring it is a wm-tts concern (sibling PRD if it doesn't
   already support per-utterance volume).
3. **Multi-turn "what did you say before that?"** v0.1 repeats only the
   immediately-last turn.
4. **"I didn't ask that" / correction routing.** Recognized as a future
   `Repair::Reject` variant; out of scope for v0.1.

## 5. Open questions

- Word-count threshold for the ambiguity guard — 4? 5? Tune against real
  transcripts once dialog-turn-fsm is live.
- Should "louder" persist for the rest of the session (the user is hard
  of hearing) rather than one turn? Likely a wm-tts session-volume
  concern, not brain-side. Note for the wm-tts sibling PRD.

## 6. Files this PRD likely touches

- New: `src/repair.rs` (the matcher + `Repair` enum)
- Modified: `src/daemon.rs` (`handle_turn_user` checks repair before LLM
  dispatch; replay path)
- Modified: `src/bus.rs` (`ReplyEvent.loudness: Option<String>`)
- Modified: `src/lib.rs` (`BrainConfig` repair phrase lists)
- Modified: `src/persist.rs`, `tests/`, `README.md`, `CHANGELOG.md`
