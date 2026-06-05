# PRD: lucid-explain — wintermute narrates a turn in plain language

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-lucid
Vision: visions/lucid.md

## TL;DR

`lucid trace` and `lucid mind` produce structured views for a developer.
`lucid explain <turn_id>` composes them into one plain-language sentence or
two that any human can understand — and can *speak* it through `wm-tts`:
"I didn't answer because I never heard a wake word," or "I heard you say 'what
time is it' and replied in about two seconds." Debuggability for jsy;
legibility for the non-technical elder this is ultimately for.

## Why this exists

jsy's seed was "a way for wintermute to share their thoughts and inner mechanics
so I can understand and debug." Traces and reasoning dumps serve the developer,
but the deeper end-state is a machine that can *tell you what it just did* in
words — both so jsy can debug conversationally ("why didn't you answer?") and so
the elder (the `companion`/`earshot` end-user) is never left guessing whether
the device heard her.

Evidence from Phase 1:
- The structured inputs exist after the rest of the fleet: lucid-trace `--json`
  (stages, stall point, latencies) and lucid-mind `--json` (route, reason,
  context, tools).
- The failure topics that explain *why* a turn died are concrete and nameable:
  `wm.stt.uncertain` ("I wasn't sure what you said"), `wm.dialog.timeout`
  ("you went quiet before I could confirm"), `wm.dialog.unheard`, `wm.brain.error`
  ("my thinking step failed"), `wm.tts.error` ("I had the answer but couldn't
  speak it").
- `wm-tts` already exposes `wm.tts.say`/`wm.tts.speak` — explain can route its
  narration to the existing speech path; no new audio plumbing.
- The `hearth` persona work means there's already a warm voice to borrow for the
  elder-facing variant (vision cross-link).

## What this builds

Extends `wintermute-lucid` with a narration layer over trace + mind:

- **`lucid explain <turn_id>`** — produce a short plain-language account of the
  turn from the structured trace/mind data:
  - **Success:** "You said 'what time is it'. I routed it to Sonnet and answered
    'It's about 3:45' in 2.1 seconds."
  - **Stall:** map the stall stage to a human cause via a small table —
    no-wake → "I never heard my name, so I didn't start listening";
    stt-stalled → "I started listening but couldn't make out any words";
    dialog-timeout → "I heard you but you went quiet before I could confirm";
    brain-error → "I understood you but my thinking step failed."
- **`--voice` (default off for jsy, on for the elder variant):** publish the
  narration to `wm.tts.say` so wintermute *speaks* the explanation. A `--persona`
  flag selects the warm (`hearth`) voice vs. a flat diagnostic voice.
- **`lucid explain --last`** — narrate the most recent turn (the conversational
  "what just happened?").
- **Deterministic, not generative:** the narration is templated from the
  structured trace/mind fields, NOT a fresh LLM call — so explaining a turn never
  itself spends a brain turn, never adds latency to the live loop, and never
  depends on cloud credit. (This keeps explain usable even when the brain is the
  thing that failed.)
- Composes the existing `--json` outputs of lucid-trace and lucid-mind; does not
  re-parse the raw log itself.

Non-goals: new inference, persistence, live view. lucid-explain is the
prose/voice layer at the top of the fleet.

## Acceptance criteria

1. `lucid explain <turn_id>` prints a short (1–2 sentence) plain-language
   account of a successful turn naming what was heard, the route, the reply, and
   the latency — sourced from lucid-trace/mind `--json`, with no LLM call.
2. A stalled/dead turn is explained by mapping its stall stage to a human cause
   via a documented table (no-wake, stt-stall, dialog-timeout, brain-error,
   tts-error each produce a distinct sentence). Tested with one synthetic turn
   per failure mode.
3. The narration is deterministic given the same structured input (same trace →
   same sentence); proven by a golden-output test.
4. `lucid explain --voice <id>` publishes the narration text to `wm.tts.say`
   (verifiable by capturing the bus event); without `--voice` nothing is spoken.
5. `--persona hearth|flat` selects the voice register of the spoken/printed
   narration.
6. `lucid explain --last` narrates the most recent recorded turn with no id
   argument.
7. Explaining a turn never triggers a `wm.dialog.turn.user` or `wm.brain.*`
   request — confirmed by asserting no brain-request event is emitted during an
   explain (it must not consume a brain turn or cloud credit).
