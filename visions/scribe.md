# Vision: scribe — the session trace record must never have holes

## TL;DR

ctrace traces every Claude session to an NDJSON log, and a SessionEnd
hook renders a one-page Markdown summary when the session ends. But the
render is *coupled to a graceful goodbye that heavy sessions never give*:
when a headless build/kernel/dream session is SIGKILLed by cgroup
teardown, the SessionEnd hook never runs, the tracer is orphaned, and the
log is left forever un-summarized. Nothing notices, nothing retries,
nothing backfills — the only "detector" is the human-run self-review,
which hand-counts the gap every tick (1→4→5 missing across runs 16–18)
and never closes it. **scribe** makes the trace record self-completing:
a single fast renderer that backfills any orphaned log, a cross-session
rollup that replaces the digest self-review rebuilds by hand, and the
hook/startup wiring that renders summaries independent of how a session
dies.

## End-state

When this is done:

- Every `*.ndjson` under `~/.cache/ctrace/sessions/` has a matching
  `*.summary.md`, regardless of whether its session exited gracefully.
  A missing summary is an *anomaly that self-heals within one session
  boundary*, not a number a human re-counts every review.
- The cross-session daily digest (top write-path prefixes, top binaries,
  outbound connects, deletions, flagged sensitive writes, session count)
  is produced by one deterministic command, not reconstructed by hand
  from 40–300 files every self-review run.
- An orphaned ctrace tracer (owner died ungracefully) is detected and
  reconciled, not left to a transient `running:false` that "recovers on
  its own" or an orphan count that drifts.
- `session-postmortem` (visions/continuity.md), which *consumes* ctrace
  as one of its four substrates, gets a complete record to join against.

## Why this is real (Phase 1 evidence, 2026-05-28 ~22:00 PDT)

Measured live this session:

- `~/.cache/ctrace/sessions/`: **828** `*.ndjson`, **810** `*.summary.md`
  → 18 un-summarized logs. The 5 oldest gaps are the heavy build/kernel
  sessions self-review flagged: `T162617` (12 MB / 124 154 events),
  `T163729` (10 MB), `T164732` (10 MB), `T181900` (389 KB), `T220013`
  (116 KB, the live session).
- The summarizer is **not** slow: `summarize-ctrace-session.sh` renders
  the 12 MB / 124k-event log in **1.7 s** and the 10 MB one in **1.4 s**
  (timed on a `/tmp` copy this session). So a hook timeout is not the
  cause.
- `~/.cache/ctrace/claude-stop.err` is **empty**. The summarizer never
  ran and never errored — the SessionEnd hook (`ctrace-session-end.sh`)
  simply did not fire. That is the signature of an ungraceful exit.
- Memory `self_build_detached_cgroup_teardown`: headless service sessions
  (the /build, /dream, /self-review timers) share a service cgroup and
  are SIGKILLed on tick teardown. SIGKILL → no SessionEnd → no summary.
  The un-summarized logs are exactly the long headless sessions.
- `ctrace-session-end.sh` renders only the single log named in
  `claude-owns.json` and swallows every error (`… || true`,
  `2>>"$err"`). Failures are silent by construction.
- `summarize-ctrace-session.sh` makes **6 separate full passes** over the
  file (1 awk for duration + 5 `jq` scans for execve/openat/unlinkat/
  connect/pid). Fine for one file; the wrong shape for batch backfill or
  a 300-file rollup.
- Self-review journal `~/brain/journal/2026-05-28.md` runs 16/17/18:
  missing-summary count hand-counted 1→4→5, never repaired; the
  "Cross-session aggregate" section is rebuilt by hand each run (40-file
  sample at run 17 because the full set was "too large to stream").
- Recall `01KSK8SDM4J0…` (run 13): *"Variable-expansion ARG_MAX hit on
  69-file aggregation — needs xargs."* Cross-session aggregation in shell
  already hit a scaling wall.
- Recall `01KSRV7R4FE…` (run 18) + run-12/13: ctrace orphans 7→1; a
  transient `running:false` "recovered on its own" — orphan-tracer state
  drifts with no reconciler.

## Components (each one PRD)

1. **ctrace-scribe** — new rust-cli. Single-pass NDJSON→summary renderer
   (faithful port of the shell logic) plus `scribe backfill <dir>` that
   renders every `*.ndjson` lacking a `*.summary.md`. The reusable engine
   the rest of the fleet calls. *Root.*
2. **ctrace-scribe-rollup** — rust-extend ctrace-scribe. `scribe rollup
   --since <when>` emits the cross-session digest self-review hand-builds,
   streaming so it never hits ARG_MAX. Depends on (1)'s parser.
3. **ctrace-scribe-selfreview** — shell. Wire `scribe backfill` + `scribe
   rollup` into self-review Phase B.5, replacing the hand-count and the
   hand-built aggregate. Depends on (1) and (2); degrades safely if absent.
4. **ctrace-session-end-resilient** — shell/config. A SessionStart sweep
   that backfills any prior un-summarized logs before starting the new
   tracer, so a summary lands on the *next* boundary even when the owning
   session was SIGKILLed; plus hardening of the existing hooks. Uses (1);
   degrades to the shell summarizer if scribe isn't installed yet.
5. **ctrace-orphan-reap** — rust-cli (or fold into scribe). Reconcile
   `tracer.pid` + `claude-owns.json` against live PIDs: detect a tracer
   whose owner died, render its log, clear the stale marker. Pairs with
   (4).

## Order

```
ctrace-scribe
   ├──► ctrace-scribe-rollup
   ├──► ctrace-scribe-selfreview   (needs backfill + rollup)
   └──► ctrace-session-end-resilient (needs backfill)
ctrace-orphan-reap                  (independent; pairs with #4)
```

## Open questions

- **scribe vs the shell scripts**: should scribe *replace*
  `summarize-ctrace-session.sh` outright, or sit beside it as the batch
  engine while the single-file SessionEnd path keeps calling the shell
  version? Leaning replace, with the shell script kept as a fallback the
  resilient hook falls back to. Discuss before #4 rewires the live hook.
- **ctrace has no source repo**: `ctrace` is a standalone Python script
  in `~/.local/bin` plus a `.bt` and two shell scripts in
  `~/.claude/scripts/`. scribe is a new repo, not an extend — confirm
  that's the right home rather than first wrapping ctrace into a repo.
- **Relationship to vigil**: orphan-reap's "owner died, thing still
  running" shape rhymes with vigil's running-process staleness axis but
  is distinct (a leaked *tracer*, not a stale *binary*). Keep separate;
  cross-reference in gossip.
- **Backfill cadence**: SessionStart sweep (fast, bounded) vs a periodic
  timer vs self-review-only. v0.1 does SessionStart + self-review; a
  dedicated timer is probably overkill given the volume.
