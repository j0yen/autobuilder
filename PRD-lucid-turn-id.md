# PRD: lucid-turn-id — one correlation id threads a turn across all daemons

Status: Draft v0.1
build_target: mixed
Vision: visions/lucid.md

## TL;DR

A single spoken turn passes through five daemons (`wm-audio` → `wm-stt` →
`wm-dialog` → `wm-brain` → `wm-tts`) but **no shared identifier ties their
events together**. Today `turn_id` exists only inside `wm-brain`, minted locally
as `now_ms`. This PRD mints a `turn_id` at the moment of wake and propagates it
through every downstream event, so any consumer can reconstruct one turn by id
instead of by guessing at wall-clock timestamps. This is the spine the rest of
the `lucid` fleet stands on.

## Why this exists

During the 2026-06-03/04 voice-bringup session, answering "where did this turn
die?" required joining events across five separate journals on wall-clock time —
and the journal showed mutual clock skew (Jun 04/05/06 timestamps in a single
capture). Sub-second races between `speech.end`, `stt.final`, and
`dialog.turn.user` made timestamp-join unreliable.

Evidence from Phase 1:
- `turn_id` appears in exactly five lines, all in `wm-brain`:
  `wintermute-brain/src/router.rs:507` (`pub turn_id: u64`),
  `daemon.rs:2050,2119,2176` (`turn_id: now_ms`), `bus.rs:54`. It is minted
  locally per turn, **not** received from upstream.
- `wm.brain.route` already carries this `turn_id`
  (`{turn_id, tier, reason, latency_ms, model, ts}`, `router.rs:502`) — so the
  brain is *ready* to be correlated; it just isn't fed a shared id.
- `wm-audio` emits `wm.audio.wake`, `wm.audio.speech.start/end` with no id;
  `wm-stt` emits `wm.stt.final/partial/uncertain` with no id; `wm-dialog` emits
  `wm.dialog.turn.user`, `wm.dialog.state` with no id; `wm-tts` emits
  `wm.tts.start/end` with no id (grep of `wintermute-*/src/*.rs`).

## What this builds

A `turn_id` convention plus its propagation:

- **Mint point:** `wm-audio` generates a `turn_id` when a wake fires
  (`wm.audio.wake`) — a monotonic-ish, collision-resistant token (e.g.
  `<unix_ms>-<4-hex>`; the hex avoids the same-millisecond collision that a bare
  `now_ms` risks). The same id is attached to that turn's `speech.start`,
  `speech.chunk`, and `speech.end`.
- **A tiny shared helper** (in whichever crate the daemons already share, or a
  new leaf `wm-turnid` lib if none fits) to mint and parse the id, so every
  daemon uses identical semantics. Keep it dependency-free.
- **Propagation rule:** each daemon copies the `turn_id` from the event that
  triggered its work onto every event it emits in response.
  - `wm-stt`: `speech.end{turn_id}` → `stt.final{turn_id}` / `stt.partial` /
    `stt.uncertain`.
  - `wm-dialog`: `stt.final{turn_id}` → `dialog.turn.user{turn_id}`,
    `dialog.state{turn_id}`.
  - `wm-brain`: stop minting `now_ms`; adopt the inbound `turn_id` from
    `dialog.turn.user` for `wm.brain.route`, `wm.brain.reply`, `wm.brain.tool.*`.
    Fall back to a freshly minted id only when no inbound id is present
    (e.g. system-injected turns), tagged so consumers can tell.
  - `wm-tts`: `brain.reply{turn_id}` → `tts.start{turn_id}` / `tts.end`.
- **Backward compatibility:** the field is additive and optional in every
  envelope; a daemon that hasn't adopted it yet, or an event with no upstream id,
  must not break any consumer. Events without a `turn_id` are still valid.

This is `mixed` (rust-extend across five repos), so it ships repo-by-repo.
Order the work mint-first (`wm-audio`), then downstream, so a half-applied
state still degrades gracefully (later daemons simply lack the id until adopted).

## Acceptance criteria

1. A reusable mint/parse helper exists with unit tests proving two ids minted in
   the same millisecond differ, and that parse round-trips a minted id.
2. `wm-audio` attaches a `turn_id` to `wm.audio.wake`, `speech.start`,
   `speech.chunk`, and `speech.end` for a given utterance, and all four share the
   same id (test via a captured event sequence).
3. `wm-stt`, `wm-dialog`, `wm-brain`, and `wm-tts` each copy the inbound
   `turn_id` onto their emitted events for that turn (one test per daemon
   asserting in-id == out-id).
4. `wm-brain` adopts the inbound `turn_id` from `dialog.turn.user` for its
   `wm.brain.route` / `wm.brain.reply` envelopes instead of minting `now_ms`,
   and falls back to a freshly minted, distinctly-flagged id only when no inbound
   id exists.
5. Every `turn_id` field is optional/additive: a consumer fed an event with no
   `turn_id` (legacy or upstream not-yet-adopted) behaves exactly as before —
   proven by a test deserializing a pre-PRD envelope.
6. A live end-to-end capture of one spoken (or injected) turn shows a single
   shared `turn_id` present on the wake, stt.final, dialog.turn.user,
   brain.route, brain.reply, and tts.end events.
