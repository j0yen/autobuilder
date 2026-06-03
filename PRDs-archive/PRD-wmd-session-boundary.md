# PRD: wmd-session-boundary — conversations have edges

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/continuity-of-conversation.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-brain
**build_version_bump:** minor
**Depends on:** PRD-wmd-turn-history
**Codename:** *edges* — without a notion of "this conversation," history bleeds and writeback has nothing to fire on.

## TL;DR

PRD-wmd-turn-history gives the brain a rolling last-N buffer, but a fixed
ring has no idea where one conversation ends and the next begins — turn 7
about pills sits in the same buffer as turn 1 about the weather an hour
earlier. This PRD gives conversations edges: wmd infers a *session* from
gaps in `TurnUserEvent.ts` (no upstream protocol change — bus.rs:64 shows
the event carries only transcript/confidence/ts) plus explicit close
phrases, emits `wm.brain.session.{start,end}`, and scopes the history
buffer to the live session. These edges are what PRD-wmd-memory-writeback
fires on (flush at end) and what PRD-wmd-session-recap fires on (recall at
start).

## 1. Why this exists

- **History without edges is incoherent.** The turn-history ring mixes
  unrelated conversations. A user who says "say that again" after a
  20-minute gap means their *last* turn, not whatever happened to still
  be in a fixed ring.
- **Writeback and recap need a trigger.** Both downstream PRDs key off
  "a session ended" / "a session started." Without an explicit boundary
  there's nothing to hang them on. This PRD is the shared edge.
- **The brain already has no session concept.** `grep session
  src/daemon.rs` finds only agorabus *peer* session ids (daemon.rs:1361),
  not conversation sessions. `TurnUserEvent` has no session field. The
  boundary must be derived here, brain-side.

## 2. What this builds

### 2.1 Session inference

`DaemonState` tracks `current_session: Option<Session>` where:

```rust
struct Session {
    id: String,        // minted: "wmd-sess-{first_ts}"
    started_ms: u64,
    last_turn_ms: u64,
    turn_count: u32,
}
```

On each `turn.user`:
- If no current session, or `now_ms - last_turn_ms > idle_gap_ms`
  (config, default 300_000 = 5 min): **close** any existing session
  (emit `wm.brain.session.end`), **open** a new one (emit
  `wm.brain.session.start`), and reset the turn-history ring.
- Otherwise extend the current session (`last_turn_ms = now_ms`,
  `turn_count += 1`).

### 2.2 Explicit close phrases

A small matcher recognizes end-of-conversation utterances — "goodbye",
"that's all", "never mind", "thanks, that's it", "go to sleep" — as the
*final* turn of a session. The turn is still answered normally (the model
gets to say goodbye), then the session closes immediately after the reply
rather than waiting for the idle gap. Phrase list is config
(`session_end_phrases`, with sensible defaults), matched case-insensitively
on a trimmed, punctuation-stripped transcript.

### 2.3 Session envelopes

- `wm.brain.session.start` → `{ session_id, ts }`
- `wm.brain.session.end` → `{ session_id, ts, turn_count, reason:
  "idle" | "explicit" | "shutdown" }`

Add both to wm-brain's self-emitted-topic allow-list (same pattern the
companion fleet used for `wm.*` self-suppress). On clean daemon shutdown,
close any open session with `reason: "shutdown"` so writeback gets a
chance to fire (best-effort; not guaranteed on SIGKILL).

### 2.4 History scoping

The turn-history ring from PRD-wmd-turn-history is cleared on
`session.start`. The ring never carries turns across a boundary.

## 3. Acceptance tests

1. **AC1 — gap opens a new session.** Two turns 6 minutes apart (idle_gap
   = 5 min) produce: session.start, session.end (reason=idle),
   session.start. The second turn's request history is empty of the
   first turn.
2. **AC2 — turns within the gap stay in one session.** Three turns 1
   minute apart emit exactly one session.start and no session.end; all
   three share one session_id; history chains per turn-history rules.
3. **AC3 — explicit close ends the session after the reply.** A turn
   "goodbye" is answered (reply published), then session.end
   (reason=explicit) fires; the next turn opens a fresh session.
4. **AC4 — session.end carries turn_count.** A 4-turn session emits
   session.end with turn_count=4.
5. **AC5 — shutdown closes the open session.** Sending the daemon a
   shutdown signal with an open session emits session.end
   (reason=shutdown) before exit (best-effort; test via the graceful
   shutdown path, not SIGKILL).
6. **AC6 — config round-trips.** `idle_gap_ms` and `session_end_phrases`
   persist through brain.toml (persist.rs).
7. **AC7 — self-emitted filter.** wm-brain does not re-ingest its own
   `wm.brain.session.*` envelopes (no feedback loop).
8. **AC8 — `cargo test --release --lib` ≥ current+8.**
9. **AC9 — daemon active 60s, NRestarts=0.**
10. **AC10 — `cargo deny check bans licenses sources` clean.**

## 4. Non-goals

1. **Dialog-minted session ids.** v0.1 infers brain-side; a
   `wm-dialog`-stamped session id on `turn.user` is a sibling dialog PRD
   (vision OQ#1).
2. **Writing the session to recall.** PRD-wmd-memory-writeback consumes
   `session.end`; this PRD only emits it.
3. **Recapping at start.** PRD-wmd-session-recap consumes
   `session.start`.
4. **Multi-speaker sessions.** Single-speaker assumption inherited from
   the companion vision.

## 5. Open questions

- 5-minute idle gap is a guess; a companion left running all day may want
  longer. Tune at deployment.
- Should a barge-in (`wm.tts.cancel`) reset the idle clock? Probably yes —
  it's active engagement. Wire if dialog surfaces the event to brain;
  otherwise the next `turn.user` resets it anyway.

## 6. Files this PRD likely touches

- New: `src/session.rs` (the `Session` type + inference + phrase matcher)
- Modified: `src/daemon.rs` (`DaemonState.current_session`; boundary
  logic in the turn handler; shutdown hook)
- Modified: `src/bus.rs` (session.start / session.end event types +
  outgoing topics + self-emitted filter)
- Modified: `src/lib.rs` (`BrainConfig.idle_gap_ms`,
  `session_end_phrases`)
- Modified: `src/persist.rs` (round-trip new fields)
- Modified: `tests/`, `README.md`, `CHANGELOG.md`
