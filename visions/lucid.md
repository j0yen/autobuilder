# Vision: lucid — wintermute's inner life, made legible

**Authored by:** /dream (Claude Opus 4.8), with jsy
**Created:** 2026-06-04
**Status:** active
**Seed:** jsy, mid-session, after hours of debugging the voice stack blind:
*"a way for wintermute to share their thoughts and inner mechanics so I can
understand and debug."* Motivated directly by the lived pain of the
2026-06-03/04 voice-bringup session (citations below), not predicted.

## TL;DR

The voice loop now works end-to-end, but bringing it up cost a full session of
debugging *blind*. Five daemons (`wm-audio` → `wm-stt` → `wm-dialog` →
`wm-brain` → `wm-tts`) gossip over the agorabus bus, and **every thought the
system has already flows across that bus** — ~120 topics, including
`wm.brain.route` which carries the brain's literal tier decision
(`{turn_id, tier, reason, latency_ms, model}`, `wintermute-brain/src/bus.rs:54`).
But nothing **records, correlates, or explains** that stream. When jsy said
"I'm talking and nothing is happening," there was no way to answer *where the
turn died* without hand-grepping five separate journals with mutual clock skew
(Jun 04/05/06 timestamps in one capture). The wake-never-fired bug was a
1-line tensor-extraction error misdiagnosed as "overfitting" through **three
retrains and 120 user recordings** — because the actual signal (wake score
computed but dropped) was never surfaced.

lucid is the discipline of making the machine transparent: a tap that records
the whole bus, a correlation id that threads one utterance through all five
daemons, a timeline that shows each stage and its latency, a window into the
brain's actual reasoning, a live monitor that lights up as you speak, and a
plain-language self-explanation any human (including the non-technical elder
this is ultimately *for*) can understand.

## Why now (Phase 1 evidence)

- **No end-to-end correlation.** `turn_id` exists in exactly one place —
  inside `wm-brain`, minted as `now_ms` (`daemon.rs:2050,2119,2176`) and never
  shared upstream. `wm-audio`, `wm-stt`, `wm-dialog`, `wm-tts` emit their
  events with no shared id. Correlating a single turn means joining on
  wall-clock timestamps across daemons — which this session proved unreliable
  (clock skew, sub-second races).
- **The brain's decision is already published but never surfaced.**
  `wm.brain.route` ships `{turn_id, tier, reason, latency_ms, model, ts}` on
  every turn (`router.rs:502`, "operator observability"). Live this session it
  showed `tier=sonnet latency_ms=2145` — but only because I hand-grepped the
  journal. No user-facing surface consumes it.
- **The bus is the nervous system, and I kept hand-rolling taps.** Throughout
  the session I repeatedly ran ad-hoc `agorabus subscribe wm.brain.reply
  --max-events 1` one-shots and `journalctl | grep -oE 'tier=...'` to see what
  was happening. That tooling should exist as a first-class artifact.
- **The dialog FSM has clean, nameable states** (`fsm.rs`: Idle → Listening →
  Transcribing → Thinking → Speaking → Confirming) but `wm.dialog.state`
  events aren't rendered anywhere a human watches.
- **No PRD in the queue touches observability** (grep-confirmed across all 12
  queued PRDs and 27 visions). `earshot` is about conversation *tempo*;
  `scribe` is journaling. This domain is unclaimed.

## End-state

When lucid ships:

1. **One id threads a turn.** Speaking "wintermute, what time is it" mints a
   `turn_id` at wake; it rides every downstream event through to `wm.tts.end`.
   `wm.brain.route`'s existing `turn_id` joins cleanly to the same turn.
2. **The whole bus is recorded.** A `wm-lucid` daemon persists every event to a
   rotating, turn-keyed structured log that survives daemon and reboot death —
   the flight recorder.
3. **`lucid trace <id>` / `lucid last`** reconstructs one turn as a latency
   timeline: `wake 0.99 → speech.start → speech.end (1.2s) → stt.final "what
   time is it" (2.7s) → dialog.turn.user → brain.route tier=sonnet → brain.reply
   (2.1s) → tts.end`. The "where did it die and how long did each stage take"
   answer, in one command.
4. **`lucid mind <id>` / `lucid why`** surfaces the brain's actual reasoning:
   route + reason, which recall context was injected, which tools it called
   (`wm.brain.tool.call`/`result` already exist), the model and latency.
5. **`lucid watch`** is a live TUI: a row of pipeline stages lighting up in
   real time as you speak, plus the current dialog FSM state — so a stall is
   visible the instant it happens, not after a five-journal autopsy.
6. **`lucid explain`** turns a trace into plain language wintermute can print
   or *speak*: "I didn't answer because I never heard a wake word" / "I heard
   you but the brain timed out after 30s." Debuggability for jsy; legibility
   for the elder.

## Components (one bullet per PRD)

- **lucid-turn-id** — mint a shared `turn_id` at wake and propagate it through
  every daemon's events. The correlation spine; everything else needs it.
- **lucid-tap** — the `wm-lucid` recorder daemon: subscribe the whole `wm.`
  bus, persist turn-keyed structured records with rotation, survive restarts.
- **lucid-trace** — `lucid trace <id>` / `lucid last`: reconstruct one turn as
  a stage-by-stage latency timeline; flag the stall stage.
- **lucid-mind** — `lucid mind <id>` / `lucid why`: surface the brain's route,
  reasoning, recall context, and tool calls for a turn.
- **lucid-live** — `lucid watch`: real-time TUI of the pipeline stages and the
  dialog FSM state lighting up as you speak.
- **lucid-explain** — `lucid explain <id>`: natural-language self-narration of a
  turn, printable and speakable via wm-tts.

## Order

```
lucid-turn-id ──► lucid-tap ──► lucid-trace ──► lucid-explain
                          ├──► lucid-mind  ──┘
                          └──► lucid-live
```

turn-id is the foundation (without a shared id, trace/mind/live correlate on
unreliable timestamps). tap is the recorder everything reads. trace, mind, and
live each consume tap independently. explain composes trace + mind into prose.

## Open questions

- Should `wm-lucid` be a new repo (`~/wintermute/wintermute-lucid`) or a
  subcommand surface on an existing observability tool? Leaning new repo: it's
  a daemon + CLI with its own lifecycle, and the toolkit convention favors
  small focused binaries.
- Should the recorder also stamp records with the `provfs`/`agentns` session id
  when present, so a turn ties back to which Claude/agent session was driving?
  (Kernel surface exists — Phase 1.5.) Deferred to a future lucid-extend.
- `lucid-mind` wants the brain's *injected recall context* and *prompt
  summary*, which aren't fully on the bus today — the brain may need to publish
  a `wm.brain.context` digest. Captured as an AC dependency in lucid-mind.
- Does `lucid explain` speak through the same persona as the companion
  (`hearth`), or a flatter "diagnostic voice"? Probably persona for the elder,
  flat for jsy — a `--voice` flag.
