# PRD: ctrace-session-end-resilient — summaries that survive ungraceful exit

Status: Draft v0.1
build_target: shell
Vision: visions/scribe.md

## TL;DR

ctrace session summaries are rendered only by the SessionEnd hook, which
never runs when a session is SIGKILLed by cgroup teardown — the routine
fate of headless build/dream/self-review sessions. The result: heavy
sessions' logs sit forever un-summarized. This PRD makes the render
survive ungraceful exit by adding a **SessionStart backfill sweep** (catch
the *previous* session's orphaned log on the next boundary) and hardening
the existing hooks, using `scribe backfill` (PRD-ctrace-scribe) with the
shell summarizer as the fallback.

## Why this exists

Measured live during this vision's Phase 1 research (2026-05-28 ~22:00 PDT):

- `~/.cache/ctrace/sessions/` has **18** `*.ndjson` with no `*.summary.md`.
  `~/.cache/ctrace/claude-stop.err` is **empty** — the summarizer didn't
  error, it never ran. The owning SessionEnd hook
  (`~/.claude/scripts/ctrace-session-end.sh`) did not fire.
- Memory `self_build_detached_cgroup_teardown`: headless service sessions
  (the /build, /dream, /self-review timers) are SIGKILLed on cgroup
  teardown. SIGKILL delivers no SessionEnd → `ctrace-session-end.sh` never
  runs → the log is never summarized and the tracer may be orphaned.
- The un-summarized logs are exactly the long heavy sessions (12 MB,
  10 MB build/kernel traces) — consistent with the SIGKILL-of-headless
  hypothesis, not with a slow summarizer (which renders 12 MB in 1.7 s).
- `ctrace-session-end.sh` renders only the single log in
  `claude-owns.json` and swallows all errors (`… || true`); there is no
  mechanism that ever revisits a log the hook skipped.

## What this builds

Edits to the ctrace hook scripts under `~/.claude/scripts/` plus a
SessionStart hook entry. No new binary — this is the wiring that makes the
render resilient. A `.draft.sh` is produced first (user-gated swap into
the live hook path, per the agorabus-boot-handshake precedent in
PRDs-archive).

### SessionStart backfill sweep

A new `ctrace-session-start-backfill` step (or an addition to the existing
SessionStart ctrace setup) that, **before** starting the new tracer, runs
`scribe backfill ~/.cache/ctrace/sessions` (fall back to a bounded loop
over the shell summarizer if `scribe` is absent). This guarantees that a
log orphaned by the previous session's SIGKILL is summarized at the next
session boundary — at most one boundary of latency, no human in the loop.

### SessionEnd hardening

- Keep the synchronous single-log render on graceful exit, but call
  `scribe render` when available (faster, robust to truncated final
  lines) and fall back to `summarize-ctrace-session.sh`.
- Stop swallowing failures silently: on render failure, append a
  one-line diagnostic to `claude-stop.err` with the log path and exit
  code, so a genuine error is visible (today it is indistinguishable from
  "never ran").
- Preserve the always-exit-0 contract — the hook must never block
  shutdown.

### Bounded cost

The SessionStart sweep must be cheap on the common path (no orphans):
`scribe backfill` over a directory whose summaries are all current is an
mtime stat-walk, sub-second. It must not delay session start materially;
if the backfill would touch more than a threshold of logs, it runs the
render in the background (detached) and logs the handoff.

## Acceptance criteria

1. A session whose SessionEnd hook is simulated as *not firing* (kill -9
   of a stand-in) leaves an un-summarized log; the next SessionStart sweep
   renders that log's `*.summary.md`. Verified with a fixture log + a
   harness that runs the SessionStart step directly.
2. The SessionStart backfill sweep on a directory whose summaries are all
   current renders 0 and adds no material startup latency (completes
   sub-second on the current 800+-file directory).
3. On graceful exit, the SessionEnd hook still renders the active session's
   summary (no regression), preferring `scribe render` when present and
   falling back to `summarize-ctrace-session.sh`.
4. A render failure in the SessionEnd hook appends a diagnostic line
   (log path + exit code) to `claude-stop.err`; a successful render leaves
   it empty — so "errored" is distinguishable from "never ran".
5. Both hooks exit 0 unconditionally; a forced render failure does not
   produce a nonzero hook exit.
6. With `scribe` absent from `PATH`, both the SessionStart sweep and the
   SessionEnd render fall back to the shell summarizer and still produce
   summaries (graceful degradation).
7. The live hooks are not modified in place: changes ship as
   `*.draft.sh` under `~/wintermute/autobuilder/proposals/` for a
   user-gated swap; `bash -n` clean on every draft.
8. The SessionStart sweep writes only `*.summary.md` under
   `~/.cache/ctrace/sessions/` (wchg scope-guard verifies no escape).
