# PRD: almanac-speak-bridge — a due entry becomes spoken words

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-brain
Vision: visions/almanac.md

## TL;DR

`wm.almanac.due` is published but nothing speaks it. This PRD teaches
`wintermute-brain` (wmd) to subscribe to `wm.almanac.due` and speak the
prompt by reusing the *exact* proactive publish path `recap_opener`
already uses — so the reminder comes out in hearth's persona, at earshot's
pace, through wm-tts, with no new speech mechanism invented.

## Why this exists

- **The proactive reply path already exists; we reuse it, not rebuild it.**
  `handle_session_start` (`wintermute-brain/src/daemon.rs:1352-1377`) builds
  a `ReplyEvent { text, ts }` and calls `publish(outgoing::REPLY, …)` when
  `recap_opener` is on. wm-tts already consumes `wm.brain.reply`. An almanac
  prompt is structurally the same proactive turn — a different trigger
  (a bus event, not session start), same output. Inventing a second
  proactive path would fragment the one place persona + TTS converge.
- **hearth and earshot calibrate that path, not almanac.** Speaking through
  brain's reply means the prompt inherits hearth's persona and earshot's
  legibility automatically. almanac must not embed phrasing or timing here
  (scope boundary in visions/almanac.md).
- **The subscribe loop is the established integration seam.**
  `wintermute-brain` already runs a live agorabus subscribe loop
  (`daemon.rs:1` module doc); adding a `wm.almanac.due` handler is an
  in-pattern extension, not new plumbing.

## What this builds

Extends `wintermute-brain`:

- A new bus event type `AlmanacDueEvent { id, label, say, category,
  fire_ts }` (deserialized from the `wm.almanac.due` envelope defined by
  PRD-almanac-tick-daemon).
- A handler `handle_almanac_due(state, publish, ev, now_ms)` that builds a
  `ReplyEvent { text: ev.say, ts: now_ms }` and publishes it to
  `outgoing::REPLY` — the identical call `handle_session_start` makes.
  (v0.1 speaks `ev.say` verbatim; the persona wrapper is hearth's job. If a
  later PRD wants the brain to rephrase via the LLM, that is an explicit
  opt-in, not v0.1.)
- Wire the handler into the subscribe loop's topic dispatch so
  `wm.almanac.due` envelopes route to it.
- Config gate `almanac_speak: bool` on `BrainConfig` (default `true` — the
  companion deployment wants it; tests and a developer desk can disable it),
  mirroring how `recap_opener` is a bool gate (`lib.rs:100`). Honor a
  `WM_BRAIN_ALMANAC_SPEAK` env override like the other brain config knobs
  (`lib.rs:273`).
- Respect `child_lock` and existing reply invariants — an almanac prompt is
  a plain reply, subject to the same publish path guarantees.

## Acceptance criteria

1. A `wm.almanac.due` envelope with `say="time for your blue pill"` causes wmd to publish exactly one `wm.brain.reply` whose `text` is `"time for your blue pill"` (verbatim in v0.1), via the same `publish(outgoing::REPLY, …)` path `recap_opener` uses.
2. The handler is exercised through the subscribe-loop dispatch in a test using the existing `EventSink` test double — no live bus required (mirror the `handle_session_start` test at `daemon.rs:2384`-style assertions).
3. With `almanac_speak=false` (config or `WM_BRAIN_ALMANAC_SPEAK=0`), a `wm.almanac.due` envelope publishes **no** reply (AC mirrors `recap_opener=false` → no greeting, `daemon.rs:1377`).
4. A malformed `wm.almanac.due` envelope (missing `say`) logs a WARN and publishes nothing — it never panics the subscribe loop (degrade discipline).
5. The default `BrainConfig` has `almanac_speak=true`; round-trips through serde and the env-override path (`WM_BRAIN_ALMANAC_SPEAK`) like the sibling knobs.
6. No persona string, phrase bank, or timing constant is added in this crate by this PRD (the prompt text comes from the envelope; persona/pace come from the existing reply path). Reviewer confirms the diff adds no such literals.
7. `cargo test` green; existing recap/persona tests still pass unchanged.
