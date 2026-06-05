# PRD: lucid-live — watch the pipeline light up as you speak

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-lucid
Vision: visions/lucid.md

## TL;DR

`lucid trace` and `lucid mind` are post-hoc. `lucid watch` is live: a terminal
view that shows the pipeline stages lighting up in real time as you speak, plus
the current dialog FSM state, so a stall is visible the instant it happens
instead of after a five-journal autopsy.

## Why this exists

The session's recurring failure mode was jsy saying "I'm talking and nothing is
happening" with no way to see, *in the moment*, which stage was alive and which
was silent. A live monitor would have shown instantly: wake fired (green) but
stt produced nothing (stuck) — collapsing minutes of "why would it work before
and not now?" into a glance.

Evidence from Phase 1:
- The dialog FSM has clean, nameable states (`wintermute-dialog/src/fsm.rs`:
  Idle, Listening, Transcribing, Thinking, Speaking, Confirming) and already
  publishes `wm.dialog.state` — but nothing renders it for a watching human.
- All stage topics stream live over agorabus, which `agorabus subscribe`
  follows with auto-reconnect (`agorabus --help`); lucid-tap already taps them.
- This is the live counterpart to lucid-trace: same stage model, real-time.

## What this builds

Extends `wintermute-lucid` with a live TUI subscribed to the bus:

- **`lucid watch`** — a full-terminal view with two regions:
  - **Pipeline row:** the ordered stages `[wake] → [capture] → [stt] → [dialog]
    → [brain] → [tts]`, each lighting up as its event arrives for the current
    turn, with the live elapsed time in the active stage. Stages dim/grey when
    idle, highlight when active, flash red on a failure topic.
  - **State line:** the current `wm.dialog.state` (Idle/Listening/Transcribing/
    Thinking/Speaking/Confirming) and the active `turn_id` + partial transcript
    (`wm.stt.partial`) as it streams.
- **Stall surfacing:** if a stage stays active past a configurable threshold
  with no successor event, mark it amber ("stt active 6s — no final yet"); this
  is the live version of trace's stall detection.
- **Scrollback:** a compact log of recent completed turns (one line each:
  `✓ "what time is it" 8.4s` / `✗ stalled @ stt`), so `watch` doubles as a feed.
- **Plain fallback:** `lucid watch --plain` emits a line-per-event stream (no
  TUI control codes) for piping/logging or dumb terminals.
- Use a lightweight TUI crate already acceptable to the workspace (e.g.
  `ratatui` + `crossterm`); keep the binary's non-watch paths free of TUI deps
  via a feature or module boundary so lucid-tap/trace/mind stay slim.
- Clean teardown: restore the terminal on `q`/Ctrl-C/SIGTERM; never leave the
  terminal in raw mode.

Non-goals: persistence (lucid-tap), post-hoc reconstruction (lucid-trace),
reasoning detail (lucid-mind), prose (lucid-explain). lucid-live is the
real-time monitor only.

## Acceptance criteria

1. `lucid watch` renders a pipeline row of the six ordered stages and updates a
   stage's state as its bus event arrives for the active turn (verifiable by
   replaying a recorded turn into a test harness / mock bus).
2. The active dialog FSM state from `wm.dialog.state` is shown live and updates
   on each transition.
3. Streaming `wm.stt.partial` transcript text is displayed and updated as
   partials arrive.
4. A stage that stays active past the stall threshold with no successor event is
   visually flagged (amber); a failure topic flashes the stage red.
5. Completed turns accumulate in a compact scrollback with terminal status and
   total latency, one line each.
6. `lucid watch --plain` produces a TUI-free line-per-event stream suitable for
   piping.
7. Exiting (`q`, Ctrl-C, or SIGTERM) restores the terminal to a sane state with
   no leftover raw-mode or alternate-screen artifacts.
