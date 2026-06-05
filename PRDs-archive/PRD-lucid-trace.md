# PRD: lucid-trace — reconstruct one turn as a latency timeline

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-lucid
Vision: visions/lucid.md

## TL;DR

Given the recorded bus (lucid-tap) and a shared correlation id (lucid-turn-id),
this PRD adds `lucid trace <turn_id>` and `lucid last` — commands that
reconstruct a single turn as a stage-by-stage timeline with per-stage latency
and an explicit marker for *where the turn stalled or died*. This is the direct
answer to "I'm talking and nothing is happening."

## Why this exists

The single most expensive question of the 2026-06-03/04 session was "where did
the turn die?" — and it had no one-command answer. The wake-never-fired bug was
misdiagnosed as overfitting through three retrains and 120 recordings because
nobody could see that wake *scored* but the score was dropped. A timeline view
keyed on `turn_id` collapses that autopsy into one command.

Evidence from Phase 1:
- The stage events all exist and (post lucid-turn-id) share a key:
  `wm.audio.wake` → `wm.audio.speech.start` → `wm.audio.speech.end` →
  `wm.stt.final` → `wm.dialog.turn.user` → `wm.brain.route` → `wm.brain.reply`
  → `wm.tts.start` → `wm.tts.end` (grep of `wintermute-*/src/*.rs`).
- `wm.brain.route` already carries `latency_ms` (`router.rs:502`); other stage
  latencies are derivable from `ts_received` deltas recorded by lucid-tap.
- Failure topics exist and mark dead turns: `wm.stt.uncertain`, `wm.stt.error`,
  `wm.dialog.timeout`, `wm.dialog.unheard`, `wm.brain.error`, `wm.tts.error`.

## What this builds

Extends `wintermute-lucid` with a read-side that queries the recorded log:

- **`lucid trace <turn_id>`** — load every record for that id, order by
  `ts_received`, and render a timeline:
  ```
  turn 1780616644-3f2a   "what time is it"
    +0ms     wm.audio.wake            score=0.99
    +18ms    wm.audio.speech.start
    +1240ms  wm.audio.speech.end      (1.24s capture)
    +3950ms  wm.stt.final             "what time is it"  (2.71s stt)
    +3962ms  wm.dialog.turn.user
    +3970ms  wm.brain.route           tier=sonnet
    +6115ms  wm.brain.reply           (2.15s brain)      "It's about 3:45..."
    +6140ms  wm.tts.start
    +8400ms  wm.tts.end               ✓ completed
  ```
  Each row shows the absolute offset from turn start and, where it bounds a
  stage, the stage duration.
- **Stall/death detection.** If the turn ends on a failure topic
  (`*.error`, `stt.uncertain`, `dialog.timeout`, `dialog.unheard`) or simply
  stops progressing before `tts.end`, mark the last-reached stage and label the
  missing next stage: `✗ stalled after stt.final — no dialog.turn.user (dialog
  never picked it up)`. This is the load-bearing feature.
- **`lucid last [N]`** — trace the most recent turn (or last N turns,
  one-line-summary each), so the common case ("what just happened?") needs no id.
- **`--json`** for machine consumption (feeds lucid-explain) and `--full` to
  dump raw payloads per row.
- Stage model is a small ordered table (expected topic sequence) so "missing
  next stage" is computed, not hardcoded per failure.

Non-goals: brain-internal reasoning detail (lucid-mind), live updating
(lucid-live), prose narration (lucid-explain).

## Acceptance criteria

1. `lucid trace <turn_id>` prints an ordered, offset-annotated timeline of all
   recorded events for that turn, oldest first.
2. Stage durations are computed and shown for the bounded stages (capture =
   speech.start→speech.end, stt = speech.end→stt.final, brain =
   dialog.turn.user→brain.reply, tts = tts.start→tts.end).
3. A turn that ends on a failure topic or stops before `tts.end` is reported as
   stalled/dead, naming the last-reached stage and the expected-but-absent next
   stage (test with a synthetic recorded turn truncated after `stt.final`).
4. A fully successful turn is reported as completed with an end-to-end total
   latency.
5. `lucid last` traces the most recent turn with no id argument; `lucid last N`
   lists the last N turns as one-line summaries.
6. `lucid trace --json <id>` emits a structured timeline (stages, offsets,
   durations, terminal status) suitable for downstream tooling.
7. Tracing an unknown/never-recorded id exits non-zero with a clear "no records
   for turn <id>" message, not a panic.
