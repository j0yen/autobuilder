# PRD: lucid-mind-brain-context — publish the brain's injected context digest

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-brain
build_priority: normal
build_version_bump: minor
Vision: visions/lucid.md

## TL;DR

`lucid mind <turn_id>` (shipped this tick into wintermute-lucid v0.2.0)
reconstructs the brain's decision from `wm.brain.route`, `wm.brain.tool.call`,
and `wm.brain.tool.result`. But the one thing jsy most wanted to see — *what
recall context the brain actually injected* and *how the prompt was assembled* —
is computed inside the brain and never published, so lucid-mind can only show a
gap there. This PRD adds a thin `wm.brain.context` digest event to
wintermute-brain so lucid-mind's "inner mechanics" view is complete.

## Why this exists

This is the deferred publish-side of PRD-lucid-mind. During the parallel /build
tick that shipped lucid-mind, the read-side was implemented against the events
already on the bus, but the brain's injected recall context and assembled prompt
are not on the bus today (noted in lucid-mind's "Gap" section). Without this
event, `lucid mind` cannot answer "was the wrong reply caused by bad recall
context?" — the exact failure mode from the 2026-06-03/04 session where
"what time is it" got a persona non-answer at tier=sonnet.

## What this builds

Extends `wintermute-brain` with a single new published topic, emitted once per
turn right after the brain assembles its prompt and before/alongside
`wm.brain.route`:

- **`wm.brain.context`** — a *digest*, not the full prompt:
  `{ turn_id, recall_hits: [{id, subject}], persona_tier, history_turns, ts }`
  - `recall_hits` — the ids + one-line subjects of the recall memories actually
    injected into the prompt (NOT their bodies).
  - `persona_tier` — which persona/system tier was used.
  - `history_turns` — count of prior dialog turns included as context.
  - keyed on the same `turn_id` lucid-turn-id threads through the pipeline, so
    lucid-mind joins it to the existing route/tool events with no extra wiring.

Keep the payload small and privacy-light (subjects + ids, never bodies).

## Acceptance

1. wintermute-brain publishes `wm.brain.context` exactly once per turn, carrying
   `turn_id`, `recall_hits`, `persona_tier`, `history_turns`, `ts`.
2. The `turn_id` equals the turn's correlation id (same key as `wm.brain.route`),
   verified by a unit test that drives one turn and asserts the two events share
   the id.
3. `recall_hits` lists only id+subject pairs (no memory bodies); a unit test
   asserts no body text leaks into the payload.
4. The event serializes to stable JSON (serde) with a versioned/typed struct; a
   round-trip (serialize→deserialize) unit test passes.
5. `cargo test --release` green in wintermute-brain; no clippy regressions under
   the repo's existing gate.
6. (deferred — runtime) lucid-mind's `lucid mind <turn_id>` renders the recall
   context section from a real recorded `wm.brain.context` event after a live
   turn. Documents the post-wiring validation; not agent-verifiable offline.

deferred_acs: [6]

## Notes

Companion read-side already shipped: wintermute-lucid v0.2.0 `lucid mind`/`lucid why`.
Once this lands and a live turn is recorded, the lucid-mind PRD's runtime ACs can
be closed. Triggered by the lucid-mind /build branch on the 2026-06-04 tick.
