# PRD: wmd-memory-writeback — what mattered gets remembered

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/continuity-of-conversation.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-brain
**build_version_bump:** minor
**Depends on:** PRD-wmd-session-boundary
**Codename:** *write-it-down* — the deferred recall-write path, finally wired.

## TL;DR

Today wmd reads from recall every turn but writes nothing back —
`recall_client.rs` says so in a comment: *"`embed` remains intentionally
omitted — it lands when the brain starts writing memories back, which is
a separate iter."* This is that iter. On `wm.brain.session.end` (from
PRD-wmd-session-boundary), wmd summarizes the just-ended conversation into
durable facts and writes them to recall under the
`thread_subject_for(date)` convention that lib.rs already defines and
nothing uses. The next day's wmd — via PRD-wmd-session-recap — can recall
them.

## 1. Why this exists

- **The write path is explicitly stubbed, by design, waiting for this.**
  recall_client.rs wires `ping`/`query`/`touch` and names `embed` as
  deliberately deferred until "the brain starts writing memories back."
  This PRD is the named successor.
- **The recall subject is pre-built and unused.** lib.rs:45-47 defines
  `THREAD_SUBJECT_PREFIX = "wintermute-thread-"` and `thread_subject_for(
  date)` → `wintermute-thread-2026-05-26`, with a passing test
  (lib.rs:385). Nothing in the daemon writes to it. The home for
  written-back memories already has an address.
- **Continuity is impossible without it.** Per-turn recall retrieval
  (already shipped) surfaces *pre-existing* memories. Without writeback,
  nothing a user says in conversation ever becomes a memory — tomorrow's
  wmd meets a stranger every morning.

## 2. What this builds

### 2.1 Recall write client

Extend `recall_client.rs` with a `write`/`embed` method speaking recall's
length-prefixed framing (the file already pins `MAX_FRAME_BYTES = 4 MiB`
and mirrors the v0.5.x protocol). It submits a memory: `{ subject, body,
kind, confidence }`. Mirror the existing ping/query/touch request/response
enum style. Recall's CLI surface (`recall write`, `recall observe`)
confirms the daemon accepts writes; this is the socket-side equivalent.

### 2.2 Session summarization

On `session.end`, if the session had ≥ 1 real turn, render the session's
turn-history into a compact transcript and issue a **dedicated
extraction call** to the model (a separate, cheap, non-streaming prompt —
NOT the conversation loop) that returns durable facts as structured
lines:

```
FACT | <subject-hint> | <one-line durable fact> | <confidence 0..1>
```

Examples the prompt should extract: standing facts ("her daughter visits
on Sundays"), preferences ("she likes the radio louder in the evening"),
commitments ("she said she'd call the doctor Monday"). It should NOT
extract conversational chaff, questions, or the assistant's own replies.

### 2.3 Write as proposals, not committed memories

Per vision OQ#2 (privacy + chaff control), v0.1 writes extracted facts as
recall **proposals** (the `observe`/`proposals` path), not directly
committed memories — so the existing triage queue reviews them before
they become permanent and queryable. A `writeback_auto_commit` config
flag (default `false`) can later flip to direct `write` once trust is
established. Thread-scoped episodic summaries go under
`thread_subject_for(date)`; standing facts get a profile/semantic subject
hint from the extractor.

### 2.4 Failure tolerance

Writeback is best-effort and must never block or crash the daemon. A
recall outage, an extraction-call failure, or a malformed extractor
response logs a WARN and drops the writeback for that session — exactly
the tolerance pattern `touch_recalled_hits` already uses (daemon.rs:1092,
"recall outage degrades to no usage signal, not dropped reply").

### 2.5 Idempotence / dedup

A session that ends, then is somehow re-ended (shutdown after idle close)
must not double-write. Guard on session_id: each session writes back at
most once.

## 3. Acceptance tests

1. **AC1 — session.end triggers extraction + write.** A 3-turn session
   ending (idle or explicit) calls the extraction model once and submits
   the returned facts to the recall write client (mocked). Assert the
   write client received the expected subjects/bodies.
2. **AC2 — thread-subject routing.** Episodic session summary is written
   under `thread_subject_for(<date>)` (`wintermute-thread-YYYY-MM-DD`);
   assert the subject matches the lib.rs convention.
3. **AC3 — proposals by default.** With `writeback_auto_commit = false`,
   writes go through the proposal/observe path, not direct commit.
4. **AC4 — empty session writes nothing.** A session that ended with 0
   real turns (e.g. only a failed turn) issues no extraction call and no
   write.
5. **AC5 — recall outage is tolerated.** With the write client returning
   a transport error, the daemon logs WARN and continues; no panic, no
   restart, the next session still functions.
6. **AC6 — extraction failure is tolerated.** A malformed extractor
   response (no FACT lines, or garbage) results in zero writes and a WARN,
   not a crash.
7. **AC7 — write-once per session.** A session that closes via shutdown
   after an idle close does not double-write (session_id guard).
8. **AC8 — chaff is filtered.** Given a transcript of only questions and
   small talk, the extractor prompt yields no FACT lines and nothing is
   written. (Test the parsing/gating, with a mocked extractor returning
   no facts.)
9. **AC9 — `cargo test --release --lib` ≥ current+12.**
10. **AC10 — daemon active 60s, NRestarts=0; `cargo deny check bans
    licenses sources` clean.**

## 4. Non-goals

1. **Auto-promoting proposals.** v0.1 parks them for triage; auto-commit
   is a config flag defaulted off and a later trust decision.
2. **Reading the written memories back.** PRD-wmd-session-recap.
3. **Cross-day thread merging.** One thread subject per calendar date
   (vision OQ#5).
4. **A new extraction model / fine-tune.** Uses the same Anthropic client
   with a distinct system prompt; no new dependency.
5. **Privacy controls / consent.** A real concern (vision OQ#4) but a
   policy PRD, not this mechanism PRD. Proposals-by-default is the
   v0.1 mitigation.

## 5. Open questions

- Extraction model: Haiku is cheap and probably sufficient for fact
  extraction; the conversation loop uses Sonnet/Opus. Make the extraction
  model a config knob (`writeback_model`, default Haiku).
- Should writeback fire mid-session for very long conversations, not just
  at end? Defer; v0.1 is end-of-session only.
- Confidence floor for proposals — drop FACT lines below e.g. 0.5?
  Config `writeback_confidence_floor`, default 0.5.

## 6. Files this PRD likely touches

- Modified: `src/recall_client.rs` (add `write`/`embed`/proposal method)
- New: `src/writeback.rs` (extraction prompt + FACT parsing + dedup guard)
- Modified: `src/daemon.rs` (subscribe to / handle `session.end`; invoke
  writeback)
- Modified: `src/lib.rs` (`writeback_auto_commit`, `writeback_model`,
  `writeback_confidence_floor`; reuse `thread_subject_for`)
- Modified: `src/persist.rs`, `tests/`, `README.md`, `CHANGELOG.md`
