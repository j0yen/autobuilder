# PRD: ctrace-scribe — single-pass ctrace summary renderer + backfill

Status: Draft v0.1
build_target: rust-cli
Vision: visions/scribe.md

## TL;DR

ctrace traces every Claude session to an NDJSON log and a SessionEnd hook
renders a one-page Markdown summary — but only when the session exits
gracefully. Heavy headless sessions are SIGKILLed by cgroup teardown
before the hook runs, leaving the log forever un-summarized, and nothing
backfills. `ctrace-scribe` is the reusable engine that fixes this: a
single-pass NDJSON→summary renderer (faithful to the current shell
output) plus `scribe backfill <dir>` that renders every `*.ndjson`
lacking a `*.summary.md`, idempotently and without per-file `jq` storms.

## Why this exists

Measured live during this vision's Phase 1 research (2026-05-28 ~22:00 PDT):

- `~/.cache/ctrace/sessions/` holds **828** `*.ndjson` and **810**
  `*.summary.md` — **18** logs with no summary. The oldest gaps are heavy
  build/kernel sessions: `claude-20260528T162617.ndjson` (12 MB /
  124 154 events), `T163729` (10 MB), `T164732` (10 MB).
- The existing renderer is **not** the bottleneck:
  `~/.claude/scripts/summarize-ctrace-session.sh` rendered that 12 MB log
  in **1.7 s** when run by hand this session. `~/.cache/ctrace/claude-stop.err`
  is **empty** — the summarizer never ran on the missing logs because the
  SessionEnd hook never fired (ungraceful exit; see memory
  `self_build_detached_cgroup_teardown` — headless service sessions are
  SIGKILLed on cgroup teardown).
- `summarize-ctrace-session.sh` makes **6 full passes** over each file
  (1 awk + 5 `jq`). Acceptable for one file; the wrong shape for
  backfilling 18 logs or feeding a 300-file rollup. Recall `01KSK8SDM4J0…`
  (self-review run 13): *"Variable-expansion ARG_MAX hit on 69-file
  aggregation — needs xargs."* Shell-side aggregation already scaled out.
- Self-review (`~/brain/journal/2026-05-28.md`, runs 16/17/18) hand-counts
  the missing summaries (1→4→5) every tick and never renders them. The
  gap persists because no command closes it.

`session-postmortem` (visions/continuity.md) *consumes* ctrace as one of
its four substrates; a complete summary record makes that join honest.

## What this builds

New repo `~/wintermute/ctrace-scribe/`, published as `j0yen/ctrace-scribe`.
Single Rust binary, no async runtime.

### Subcommands

- `scribe render <log.ndjson> [--out PATH]` — render one log to its
  `<log>.summary.md` (next to the input by default). Stdout with
  `--out -`. This is the engine the SessionEnd path can call instead of
  the shell script.
- `scribe backfill <dir> [--dry-run] [--force]` — scan `<dir>` for every
  `*.ndjson` whose `*.summary.md` is missing (or older than the ndjson
  with `--force`) and render it. Prints one line per rendered/ skipped
  file and a final count. Idempotent: a second run with no new logs
  renders nothing. `--dry-run` lists what *would* render and exits 0.

### Parsing

One streaming pass over the NDJSON per file (read line, parse JSON,
fold into accumulators) — never 6 passes. Accumulate in a single walk:
event count, duration (min/max `ts`), `execve` file histogram, `openat`
path set (in-scope vs out-of-scope vs flagged), `unlinkat` set,
`connect` comm histogram, unique exec'd PID count.

### Output parity

The rendered Markdown reproduces the sections the shell version emits:
title, `Log:` line, the `Duration … · N events · N PIDs · N writes`
line, **Top binaries executed**, **Writes outside expected scope**,
optional **⚠ Flagged sensitive-path writes**, **Deletions**, **Outbound
connect() by process**. Scope/flag prefix sets match
`summarize-ctrace-session.sh` exactly (the `in_scope` and `flag_paths`
regexes are copied into the binary as the v0.1 defaults).

### Robustness

Malformed/truncated JSON lines (an ungraceful exit can leave a partial
final line) are counted and skipped, never fatal — the summary still
renders with a `(N malformed lines skipped)` note. This is the property
the shell version lacks and the whole vision needs.

## Acceptance criteria

1. `scribe render <log>` writes `<log>.summary.md` containing all of:
   the `Log:` line, the duration/event/PID/write count line, and the five
   section headers (Top binaries, Writes outside expected scope,
   Deletions, Outbound connect, plus the Flagged section when any flagged
   write exists).
2. For a fixture log with no flagged writes, the Flagged section is
   omitted (matching shell behavior); for one with a `/home/jsy/.ssh/…`
   write, the Flagged section is present and lists it.
3. `scribe render` on the 12 MB / 124k-event fixture completes in **≤ 2 s**
   and makes a single pass (assert via an internal pass counter exposed
   under `--stats`, or by structural test — no second file open).
4. A truncated final line (valid prefix, no closing brace) does not fail
   the render; the summary is produced and reports `1 malformed line
   skipped`.
5. `scribe backfill <dir>` renders exactly the `*.ndjson` files missing a
   `*.summary.md` and leaves existing summaries untouched; it prints a
   final `rendered N, skipped M` count.
6. `scribe backfill <dir>` run twice in a row renders 0 on the second run
   (idempotent).
7. `scribe backfill <dir> --dry-run` writes no files and exits 0, listing
   the would-render set.
8. `scribe backfill --force <dir>` re-renders a log whose summary is older
   than the ndjson (mtime comparison).
9. `--help` documents `render` and `backfill` with their flags; exit 0.
10. Rendering a non-existent log exits non-zero with a usage error; an
    empty dir backfills 0 and exits 0.
