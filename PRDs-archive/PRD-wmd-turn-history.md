# PRD: wmd-turn-history — the brain remembers the last few turns

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/continuity-of-conversation.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-brain
**build_version_bump:** minor
**Codename:** *short-memory* — the foundation the whole continuity vision reads.

## TL;DR

`wmd` forgets everything between turns. `handle_turn_user` (daemon.rs:1030)
builds its Anthropic request with `compose_request(model, &persona,
&turn.transcript)` — a single transcript string — and the test suite
pins the consequence: `assert_eq!(req.messages.len(), 1)` (daemon.rs:1585),
`req.messages[0].role == Role::User`. So "say that again", "what did you
mean", "and the second one?" all hit a model that has never seen the
turn before. This PRD adds a bounded in-memory rolling history of recent
`(user, assistant)` turn pairs and feeds it into the request, so the
conversation actually chains.

## 1. Why this exists

- **The brain is provably stateless.** daemon.rs:1057 constructs the
  request from one transcript; daemon.rs:1585 asserts exactly one message
  reaches the API. There is no `Vec<Message>` accumulation anywhere in
  the turn path (verified: `grep history src/daemon.rs` → nothing in the
  handler).
- **`anthropic::Message` already supports it.** anthropic.rs models
  `messages: Vec<Message>` with `Role::{User,Assistant}` and
  `MessageRequest::streaming(model, max_tokens, messages: Vec<Message>)`.
  The wire shape is ready; only the daemon's single-string habit blocks
  multi-turn. This PRD is a behavior change, not a protocol change.
- **The companion needs it.** companion.md OQ#5 and dialog-turn-fsm
  non-goal #1 both name multi-turn memory as the missing piece for
  "what did you just say?" / "say that again." This PRD is the floor
  those affordances stand on.

## 2. What this builds

### 2.1 A bounded turn-history buffer

A new `History` type (likely `src/history.rs`), held in `DaemonState`:

```rust
struct Turn { user: String, assistant: String, ts: u64 }

struct History {
    turns: VecDeque<Turn>,   // oldest first
    max_turns: usize,        // config: history_turns, default 6
}
```

- After a successful reply, push `Turn { user: transcript, assistant:
  reply_text, ts }`. On overflow, pop_front.
- Destructive intents: push the *spoken* prefix as the assistant turn
  (the user heard it), not the JSON block.
- Errors / empty replies / dropped turns: **do not** push (the user got
  no usable answer, so it isn't a real turn). Document this choice.

### 2.2 Feed history into the request

`compose_request` gains a history argument. The message list becomes,
oldest to newest: `[user, assistant, user, assistant, …, current_user]`.
The recall-context splice and persona stay where they are (system
prompt) — history is conversation, not context. Invariant:
`req.messages.len() == 2 * history.len() + 1` and the last message is the
current `Role::User` transcript.

### 2.3 Config knob

`BrainConfig` gains `history_turns: usize` (default 6), persisted through
the existing `brain.toml` atomic-write path (persist.rs). `history_turns
= 0` disables history (restores today's single-message behavior) — useful
as a kill switch and as the cheap path for the repair-affordances PRD's
pure-replay case.

### 2.4 Token guard

Cap the rendered history by an estimated token budget (chars/4 heuristic
is fine for v0.1), trimming oldest turns first, so a long history never
crowds out the persona + recall context in the prompt-cache window. Log
at DEBUG when trimming fires.

## 3. Acceptance tests

1. **AC1 — multi-turn request shape.** Drive three sequential
   `turn.user` events through a mocked LLM that returns a distinct reply
   each time. On the 3rd turn, assert the request carries the prior two
   user+assistant pairs in order, ending with the 3rd user transcript:
   `messages.len() == 5`, alternating roles, last == current user.
   (This is the rewrite of the daemon.rs:1585 single-message assertion —
   rewrite it, don't delete it.)
2. **AC2 — ring bound.** With `history_turns = 2`, after 5 turns the
   request carries at most `2*2+1 = 5` messages; the oldest turns are
   evicted oldest-first.
3. **AC3 — failures don't pollute history.** A turn whose LLM call
   errors (or returns empty) adds nothing to history: the next
   successful turn's request shows the prior *successful* turn, not the
   failed one.
4. **AC4 — `history_turns = 0` restores single-message behavior.** Request
   carries exactly one message (today's invariant) when disabled.
5. **AC5 — destructive intent stores spoken text.** After a destructive
   reply, the stored assistant turn is the spoken prefix, not the fenced
   JSON.
6. **AC6 — token guard trims.** With an artificially low token cap and a
   full ring, the oldest turns are dropped and a DEBUG log records the
   trim; the current user turn is never dropped.
7. **AC7 — config round-trips.** `history_turns` persists through
   brain.toml write/reload (persist.rs path).
8. **AC8 — `cargo test --release --lib` ≥ current+10.**
9. **AC9 — daemon active 60s, NRestarts=0.**
10. **AC10 — `cargo deny check bans licenses sources` clean.**

## 4. Non-goals

1. **Session boundaries.** The ring is a fixed last-N here; bounding
   history to a *conversation* is PRD-wmd-session-boundary.
2. **Persistence across daemon restart.** In-memory only; restart starts
   fresh. (Writeback is the durable arm — separate PRD.)
3. **Writing history to recall.** PRD-wmd-memory-writeback.
4. **Repair phrase recognition.** PRD-wmd-repair-affordances.

## 5. Open questions

- Default `history_turns`: 6 is a guess. Tune once dialog-turn-fsm
  produces real multi-turn traffic.
- Should the assistant's *recalled context* for a past turn be re-sent?
  No — context is recomputed per turn from recall; only the spoken
  user/assistant text chains. Stated to prevent context bloat.

## 6. Files this PRD likely touches

- New: `src/history.rs` (the `History`/`Turn` types + token-guard logic)
- Modified: `src/daemon.rs` (`DaemonState` holds `History`;
  `handle_turn_user` reads/writes it; `compose_request` takes history)
- Modified: `src/lib.rs` (`BrainConfig.history_turns`, default + serde)
- Modified: `src/persist.rs` (round-trip the new field — likely no change
  if it's `#[serde(default)]`)
- Modified: `tests/` + the in-module `dispatch_turn_user` suite
- Modified: `README.md`, `CHANGELOG.md`
