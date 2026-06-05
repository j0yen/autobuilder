# PRD: lucid-mind — surface the brain's actual reasoning for a turn

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-lucid
Vision: visions/lucid.md

## TL;DR

The timeline (lucid-trace) shows *that* the brain replied; lucid-mind shows
*why* it replied the way it did. `lucid mind <turn_id>` (and `lucid why` for the
last turn) surfaces the brain's route decision and reason, the model/tier and
latency, the recall context that was injected, and the tools it called — the
"inner mechanics" jsy asked to see.

## Why this exists

When jsy asked wintermute "what time is it" this session, the reply was a
persona-flavored non-answer ("Oh that sounds exciting...") served at `tier=sonnet
latency_ms=2145`. Diagnosing *why* — wrong route? bad recall context? a tool that
should have fired and didn't? — was impossible from the outside, because the
brain's decision-making is published but never assembled into a human view.

Evidence from Phase 1:
- `wm.brain.route` already publishes the decision:
  `{turn_id, tier, reason, latency_ms, model, ts}` — "operator observability"
  (`wintermute-brain/src/router.rs:159,502`, `bus.rs:54`). The `reason` field is
  a machine-readable string (`router.rs:184`) describing *why this route*.
- `wm.brain.tool.call` and `wm.brain.tool.result` are published topics — the
  brain's *actions* are already on the bus.
- The command router classifies short commands (≤`command_max_words`) onto a
  separate path with `RoutePrefer` (Auto/local-only/cloud-only) — visible in the
  route reason but not assembled anywhere.
- **Gap:** the brain's *injected recall context* and *assembled prompt* are not
  on the bus today. lucid-mind needs them; this PRD adds a thin
  `wm.brain.context` digest event (turn_id, recall-hit ids/subjects used, system
  persona tier, history-turn count) rather than dumping the full prompt.

## What this builds

Extends `wintermute-lucid` with a brain-reasoning reader, plus a small
publish-side addition to `wm-brain`:

- **`wm-brain` change (minimal):** on each turn, publish a `wm.brain.context`
  digest carrying the inbound `turn_id`, the recall hits actually injected (their
  ids/subjects and count), the persona/tier used, and the number of history turns
  in context. A *digest*, not the raw prompt — bounded and privacy-aware.
- **`lucid mind <turn_id>`** — assemble and print, for that turn:
  - **Route:** tier, model, `reason` string decoded into plain words
    (e.g. `command-router: 4 words ≤ 6 → cloud-only → sonnet`), latency_ms.
  - **Context:** which recall memories were injected (subjects), history depth,
    persona in force.
  - **Tools:** each `wm.brain.tool.call` with its `wm.brain.tool.result`
    (matched by turn_id + call order), shown as `time.now() → "15:45"`.
  - **Reply:** the final `wm.brain.reply` text and whether it was the
    `.destructive` variant.
- **`lucid why`** — `lucid mind` for the most recent turn (the conversational
  "why did you just do that?").
- **`--json`** for lucid-explain to consume.
- Degrade gracefully when `wm.brain.context` is absent (pre-adoption turns):
  show route + tools + reply, and note context unavailable rather than failing.

Non-goals: timeline/latency (lucid-trace owns that), live view (lucid-live),
prose narration (lucid-explain). lucid-mind is the structured reasoning reader.

## Acceptance criteria

1. `wm-brain` publishes a `wm.brain.context` digest per turn carrying the
   inbound `turn_id`, injected recall-hit subjects + count, persona/tier, and
   history-turn count (test on a recorded turn).
2. The `wm.brain.context` digest is a bounded digest — it does NOT contain the
   full assembled prompt or full memory bodies (assert size/shape in test).
3. `lucid mind <turn_id>` prints the route (tier, model, decoded reason,
   latency), the injected context summary, the matched tool calls+results, and
   the final reply for that turn.
4. The route `reason` machine-string is rendered into a human-readable
   explanation (e.g. word-count → router path → chosen tier).
5. Tool calls are paired with their results by `turn_id` and call order, shown
   as `call → result`; a call with no result is shown as pending/failed.
6. `lucid why` produces the same view for the most recent recorded turn with no
   id argument.
7. For a turn recorded before `wm.brain.context` adoption, `lucid mind` still
   renders route + tools + reply and explicitly notes the context digest was
   unavailable — no panic, no empty output.
