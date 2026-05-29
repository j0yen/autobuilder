# Vision: continuity-of-conversation — wintermute remembers

**Authored by:** /dream (Claude Opus 4.8), with jsy
**Seed:** companion vision OQ#5 — "wmd today is stateless across turns.
For 'what did I just say?' / 'say that again louder' to work, the daemon
needs short-term turn memory. Possibly a recall integration. Deferred to
a future vision: *continuity-of-conversation*." Also dialog-turn-fsm
non-goal #1 ("Multi-turn memory ... require wmd state, separate PRD").
**Status:** active

## TL;DR

`wmd` (wintermute-brain) is the conversation loop, and today it forgets
everything the instant a reply is published. `handle_turn_user` builds a
request from exactly one string — `compose_request(model, &persona,
&turn.transcript)` — and the test suite pins it: `assert_eq!(req.messages
.len(), 1)` (daemon.rs:1585). It queries recall per-turn and splices hits
into the *system prompt*, but it never accumulates conversation history,
never writes anything back, and never reopens a thread it had yesterday.
The recall-subject scaffolding for this already exists and sits unused:
`THREAD_SUBJECT_PREFIX = "wintermute-thread-"` and `thread_subject_for(
date)` in lib.rs are defined, tested, and called by nothing in the turn
path. This vision wires continuity end to end: a turn remembers the turn
before it, a session knows when it ends, what mattered in a session is
written back to recall, and the next session opens by remembering it.

For a companion at jsy's mother's side this is not a nicety. "Say that
again, louder." "What did you just tell me?" "Did I already ask you
about my pills?" "You said my daughter visits Sunday." None of these
work against a brain that resets every turn.

## End-state

When this vision is fulfilled:

1. **Within a conversation, turns chain.** wmd carries a bounded rolling
   history of the last N turns into each Anthropic request, so the model
   can resolve "say that again", "what did you mean", "and the second
   one?" against real prior context — not a cold single string.
2. **Conversations have edges.** wmd knows when a session begins and ends
   — an idle gap (no turn for T minutes) or an explicit close ("goodbye",
   "never mind", "that's all") ends it. History is bounded by the session,
   not by a fixed ring that bleeds across unrelated conversations.
3. **What mattered is written back.** At session close, wmd extracts the
   durable facts from the conversation and writes them to recall (the
   `embed`/`write` path the recall client deliberately deferred —
   recall_client.rs: "embed remains intentionally omitted ... it lands
   when the brain starts writing memories back, which is a separate
   iter"). Tomorrow's wmd can recall them.
4. **The next session continues the last.** When a new conversation opens
   and a recent thread was flushed, wmd recalls it and can open with
   continuity — "Earlier you mentioned your daughter visits Sunday" —
   rather than greeting a stranger. This is the difference between
   per-turn retrieval (already shipped) and conversational memory.
5. **Repair affordances work.** "Say that again", "say it louder", "what
   did you just say?", "I didn't ask that" resolve against the in-session
   history without a round-trip to the model when they're pure replay.

## Components (PRD-sized pieces)

Decomposed in dependency order. Each line is a future PRD; the bolded
ones are drafted this dream pass. All are `rust-extend` into
`/home/jsy/wintermute/wintermute-brain`.

1. **PRD-wmd-turn-history** (drafted) — the foundation. Accumulate a
   bounded `Vec<Message>` of recent turns and feed them to
   `compose_request` instead of the single transcript. Today
   `req.messages.len() == 1` is pinned by test; this makes it `≤ 2N+1`.
2. **PRD-wmd-session-boundary** (drafted) — give conversations edges.
   Infer session start/end from `TurnUserEvent.ts` gaps (no upstream
   protocol change) plus explicit close phrases; emit `wm.brain.session.
   {start,end}`; bound the history ring to the live session.
3. **PRD-wmd-repair-affordances** (drafted) — "say that again", "louder",
   "what did you just say?" Recognized brain-side against the in-session
   history; replay the last reply (with a `loudness` hint envelope for
   "louder") without a model round-trip when the request is pure replay.
4. **PRD-wmd-memory-writeback** (drafted) — at session end, summarize the
   session into durable facts and write them to recall under
   `thread_subject_for(date)` (the unused lib.rs convention) + a
   profile/semantic subject for standing facts. Implements the deferred
   recall `embed`/`write` client path.
5. **PRD-wmd-session-recap** (drafted) — at session start, query recall
   for the most recent flushed thread(s) and surface a continuity opener.
   The retrieval counterpart of writeback; turns per-turn recall into
   genuine cross-session memory.

## Order

```
PRD-wmd-turn-history  (foundation — everything reads the history buffer)
        │
        ├──────────────► PRD-wmd-repair-affordances  (in-session replay)
        │
        ▼
PRD-wmd-session-boundary  (edges + session-scoped history)
        │
        ▼
PRD-wmd-memory-writeback  (flush at session end → recall write/embed)
        │
        ▼
PRD-wmd-session-recap  (next session opens with recalled context)
```

- turn-history is the gate: it introduces the history buffer every other
  PRD reads or bounds.
- repair-affordances needs only the in-session buffer, so it can ship
  right after turn-history, in parallel with session-boundary.
- session-boundary must land before writeback (writeback fires *on*
  session end) and before recap (recap fires *on* session start).
- memory-writeback and session-recap are the long-term arm: writeback
  stores, recap retrieves. recap can ship first against manually-seeded
  or writeback-produced `wintermute-thread-*` memories, but it's only
  meaningful once writeback populates them — so writeback leads.

## Open questions

1. **Session-id provenance.** v0.1 infers sessions from `ts` gaps
   brain-side because `TurnUserEvent` (bus.rs:64) carries no session id.
   Cleaner long-term: `wm-dialog`'s turn FSM mints a session id and
   stamps it on `wm.dialog.turn.user`. That's a dialog-side PRD (sibling
   to companion's dialog-turn-fsm) — defer until the gap-inference
   approach shows its seams.
2. **What counts as "durable"?** Writeback must not flood recall with
   conversational chaff. Heuristic v0.1: model-extracted facts only
   (a dedicated extraction prompt), gated by a confidence floor, written
   as proposals (recall `observe`/`proposals`) rather than committed
   memories — so the existing triage queue reviews them. Auto-promote is
   a later decision.
3. **History token budget.** Rolling N turns competes with the recall
   context splice and the persona for the prompt-cache window. Tune N at
   deployment; cap by token estimate, not turn count, if it bites.
4. **Privacy of writeback.** A companion that writes everything its user
   says into a searchable store is a privacy surface. Mother's
   conversations are not jsy's to query casually. Out of scope for the
   mechanism PRDs; a real concern before deployment — sibling vision
   *family-boundaries*.
5. **Cross-session vs cross-day.** `thread_subject_for(date)` is per-day.
   A conversation that spans midnight, or a "continue what we discussed
   last week", needs subject ranging. Defer; v0.1 recaps today + the
   most recent prior day.

## Notes for /build

- Every PRD is `rust-extend` into `~/wintermute/wintermute-brain`,
  single-target, same shape as the companion fleet that shipped via
  parallel autobuilder agents. They serialize, though — they all touch
  `daemon.rs` / `handle_turn_user`, so dispatching two in parallel will
  collide. Build in dependency order, one at a time.
- turn-history changes the test `dispatch_turn_user` suite pins
  (`req.messages.len() == 1` at daemon.rs:1585). That assertion must be
  rewritten, not deleted — the PRD specifies the new invariant.
- memory-writeback is the first consumer of recall's write path from
  wmd; recall_client.rs currently wires only ping/query/touch. The PRD
  adds the `write`/`embed` client method and must mirror recall's
  length-prefixed framing (MAX_FRAME_BYTES = 4 MiB).
- The unused `THREAD_SUBJECT_PREFIX` / `thread_subject_for` in lib.rs is
  the intended home for written-back thread memories — writeback and
  recap should both route through it rather than inventing a new subject.
