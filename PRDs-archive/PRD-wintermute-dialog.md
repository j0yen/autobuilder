# PRD: wintermute-dialog — turn-taker, barge-in, verbal confirmation

**Author:** /dream (Claude Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-24
**Vision:** `visions/wintermute.md`
**Builds on:** `PRD-wintermute-audio.md`, `PRD-wintermute-stt.md`, `PRD-wintermute-tts.md`
**Used by:** `PRD-wintermute-brain.md` (which sends destructive intents through dialog's confirmation flow)
build_auto: true
build_target: rust-cli
build_priority: high

---

## TL;DR

The conversational state machine. `wm-dialog` is the arbiter between
the audio layer, STT, TTS, and the brain — it owns turn-taking,
barge-in, the verbal-confirmation protocol for destructive intents,
and the mute / child-lock surfaces. Plan-agent split this out of the
brain explicitly: bundling sub-200 ms timing-critical arbitration
with the Claude API loop was wrong. Different debugging surfaces,
different latency budgets.

---

## 1. Why this exists

Three observations:

1. **The Claude API loop and the conversational state machine have
   different latency budgets.** Brain calls can take 500-5000 ms
   (network + inference); barge-in cancellation must complete in
   <200 ms. Bundling them in one daemon makes the wrong tradeoffs
   for each.

2. **Verbal confirmation is its own protocol.** "You want me to
   delete the email from your sister — say 'yes delete' if that's
   right." The flow has timeouts, ambiguous-response handling,
   re-prompt rules, and an explicit reject-on-silence default. It
   doesn't belong in the brain; it belongs in the dialog layer.

3. **Mute and child-lock are dialog concerns.** `wm mute` halts all
   speech regardless of what the brain is doing. Child-lock blocks
   destructive intents entirely even if approved verbally. These are
   dialog-level policies, not brain-level.

---

## 2. What this builds

### 2.1 Binary: `wm-dialog`

A long-running Rust daemon. State machine:

```
┌──────┐  wake.detected      ┌───────────┐
│ idle ├────────────────────▶│ listening │
└──┬───┘                     └─────┬─────┘
   │                               │ stt.partial / stt.final
   │ tts.start                     ▼
   ▼                         ┌──────────────┐
┌─────────┐  tts.end         │ transcribing │
│ speaking├─────┐            └──────┬───────┘
└────┬────┘    │                   │ stt.final
     │ wake    ▼                   ▼
     │      ┌──────┐            ┌────────┐
     └─────▶│ idle │◀──────────┤thinking│
            └──────┘  brain.   └────┬───┘
                     reply         │ destructive
                                   ▼
                            ┌────────────┐
                            │ confirming │
                            └────────────┘
```

Plus `muted` (top-level orthogonal state) and `child_locked`
(blocks any transition into `confirming → execute`).

### 2.2 Events

Subscribed:

| Topic | From | Behavior |
|---|---|---|
| `wm.audio.wake` | wm-audio | enter listening; if speaking, cancel TTS first |
| `wm.audio.speech.start` | wm-audio | enter transcribing if idle/listening |
| `wm.audio.speech.end` | wm-audio | wait for stt.final |
| `wm.stt.partial` | wm-stt | informational; ignored unless debug |
| `wm.stt.final` | wm-stt | forward to brain (wm.brain.utterance) |
| `wm.stt.uncertain` | wm-stt | say "Sorry, could you repeat that?" |
| `wm.brain.reply` | wmd | enter speaking; route text to wm-tts |
| `wm.brain.reply.destructive` | wmd | enter confirming; run verbal-confirm protocol |

Published:

| Topic | Payload |
|---|---|
| `wm.dialog.state` | `{state, prior_state, since_ms, ts}` |
| `wm.dialog.turn.user` | `{transcript, confidence, ts}` |
| `wm.dialog.turn.system` | `{text, ts}` |
| `wm.dialog.confirm.granted` | `{intent_id, ts}` |
| `wm.dialog.confirm.denied` | `{intent_id, reason, ts}` |
| `wm.dialog.mute_request` / `unmute_request` | `{ts}` |

### 2.3 Barge-in handling

When `wake.detected` fires while in `speaking`:
1. Publish `wm.tts.cancel` → wm-tts kills audio within 100 ms
2. Transition to `listening`
3. Mute is *not* affected — barge-in is normal use

Total barge-in budget: 200 ms wake-to-cancel-ack.

### 2.4 Verbal confirmation protocol

When brain emits `wm.brain.reply.destructive` with payload
`{intent_id, summary, confirm_keyword}`:

1. Enter `confirming` state with 30-s timeout
2. Speak: `"You want me to <summary>. Say 'yes <confirm_keyword>'
   if that's right."` via wm-tts
3. Wait for next `wm.stt.final`
4. Match the transcript against the expected pattern:
   - Exact match `yes <confirm_keyword>` → grant
   - "yes" alone → re-prompt once for the keyword
   - "no" / "cancel" / "stop" → deny
   - silence (30 s) → deny
   - anything else → re-prompt with one more attempt, then deny
5. Emit `wm.dialog.confirm.granted` or `denied`; brain executes (or
   doesn't) accordingly

`confirm_keyword` is short and content-specific (e.g., for an email
delete: "delete-email"). Reduces accidental triggers.

### 2.5 Mute / child-lock / quiet

- `wm.dialog.mute_request` → mute on; wm-tts cancels current
  speech; wm-audio gates wake (`wm.audio.mute`); state machine
  ignores stt.final until unmute
- `child_lock = true` (configured in bootstrap or via `wm
  child-lock on`) → all destructive intents are auto-denied
  silently (or with a configurable "I can't do that for you" reply)
- Quiet hours (`WM_QUIET_HOURS=22:00-07:00`) — Fleet 3, not v1

### 2.6 CLI

- `wm-dialog state` — current state JSON
- `wm-dialog mute` / `unmute` — same as `wm mute`
- `wm-dialog child-lock on|off`
- `wm-dialog say <text>` — debug: drive `speaking` from CLI

---

## 3. Open-source dependencies

| Crate / tool | Version | Purpose | License |
|---|---|---|---|
| `tokio` | ^1.40 | async | MIT |
| `serde` + `serde_json` | ^1 | event payloads | MIT |
| `statig` or hand-rolled state-machine | ^0.3 | type-safe FSM | MIT |
| `agorabus` client | local | pub/sub | local |

---

## 4. Acceptance criteria

1. Wake during `speaking` cancels TTS and enters `listening` within
   200 ms (measured wake-event to cancel-ack).
2. `stt.uncertain` triggers a re-prompt without leaving the state
   machine in a stuck state (verified by injecting 5 sequential
   uncertains).
3. Verbal-confirm protocol grants on `"yes <keyword>"` exactly;
   denies on silence, "no", "cancel", or ambiguous after one re-prompt.
4. `wm-dialog mute` silences current TTS and gates wake within
   200 ms; `unmute` restores both within 200 ms.
5. `child_lock = true` causes 100% of destructive intents in a
   10-scenario test suite to be denied without verbal prompt.
6. State machine transitions are logged with prior_state, new_state,
   trigger event, and elapsed-ms; logs queryable via
   `wm-dialog state --history N`.
7. 60-minute steady-state run with 50 simulated turns shows no
   state-machine wedges (deadlocks, stuck-in-`confirming`, etc.).

## 5. Out of scope (Fleet 2 / 3)

- Quiet hours scheduling (Fleet 3 `wintermute-quiet-hours`).
- Multi-user disambiguation ("who said that?") — Fleet 3.
- Conversational repair beyond the one re-prompt — Fleet 2 polish.
- TTS streaming with mid-sentence interjection ("excuse me, before
  you finish that...") — Fleet 3 if natural conversation demands it.

## 6. Risks

- **State machine bugs in edge cases.** Mitigation: write the
  exhaustive test suite as part of acceptance — every state × every
  event = a covered case. Use `statig` (or similar) for compile-time
  exhaustiveness if practical.
- **Confirm-keyword discoverability.** She has to learn that "yes"
  alone isn't enough. Mitigation: the spoken prompt always says the
  keyword out loud, and the re-prompt repeats it. After 30 days of
  use we can revisit whether the keyword friction is worth the
  safety.
- **Race between wake.detected and brain.reply.destructive** — what
  if she interrupts a confirm prompt? Mitigation: barge-in during
  `confirming` cancels the confirm flow entirely (treated as deny);
  test explicitly.

## 7. Open questions

- Should `wm-dialog` have a notion of "topic" / "thread" so multi-
  turn conversations don't get mixed up? Leaning: no, that lives in
  the brain — dialog just routes utterances. Revisit if the brain
  ends up needing a topic-id at this layer.
- Verbal cancel during `speaking`: should "stop talking" be a
  recognized phrase even without wake? Leaning: yes, but iter-2 —
  needs a small always-listening intent classifier and that's its
  own complexity.
