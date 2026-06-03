# PRD: ctrace-scribe-rollup — cross-session daily trace digest

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/ctrace-scribe
Vision: visions/scribe.md

## TL;DR

Every self-review run rebuilds, by hand, a "Cross-session aggregate" of
the day's ctrace activity — top write-path prefixes, top binaries,
outbound connects, deletions, flagged sensitive writes, session count.
It's slow, sampled (run 17 used a 40-file sample because the full set was
"too large to stream"), and shell-side aggregation has already hit
ARG_MAX. `scribe rollup` extends ctrace-scribe with one streaming command
that emits that digest deterministically across all of a window's
sessions.

## Why this exists

From this vision's Phase 1 research (2026-05-28):

- `~/brain/journal/2026-05-28.md` run 18 contains a hand-built
  "Cross-session aggregate (308-file full-day aggregate)" listing top
  write-path prefixes (`/dev/null` 92 664, `/home/jsy/wintermute` 88 511,
  …), top binaries (`sed` 646 363, …), outbound (HTTP Client 3 719, …),
  and deletions. Run 17 had to fall back to a **40-file sample** —
  *"full 268-file aggregate too large to stream."* This digest is
  reconstructed by hand every single review.
- Recall `01KSK8SDM4J0…` (run 13): *"Variable-expansion ARG_MAX hit on
  69-file aggregation — needs xargs."* The shell approach to
  cross-session aggregation is already at its scaling limit; today there
  are **828** session logs.
- ctrace-scribe (this fleet's root PRD) gives a single-pass NDJSON parser;
  rollup reuses it across N files so the per-file work is done once and
  folded, never re-`jq`'d.

## What this builds

Extends `~/wintermute/ctrace-scribe/` (rust-extend) with:

- `scribe rollup [--dir DIR] [--since WHEN] [--top N] [--format md|json]`
  — walk every `*.ndjson` in `--dir` (default `~/.cache/ctrace/sessions`)
  whose mtime is within `--since` (e.g. `24h`, `today`, `7d`; default
  `today`), fold each via the shared single-pass parser, and emit one
  digest.

### Digest contents (parity with the hand-built section)

- Session count and total events in window.
- **Top write-path prefixes** — `openat` paths bucketed to a configurable
  prefix depth, counted, top `N`.
- **Top binaries executed** — `execve` file histogram, top `N`.
- **Outbound connect() by process** — `connect` comm histogram, top `N`.
- **Deletions** — `unlinkat` paths bucketed by prefix, top `N`.
- **⚠ Flagged sensitive-path writes** — any write under the flag prefixes
  (`/etc/`, `~/.ssh/`, `~/.aws/`, `~/.gnupg/`, …), with the owning session
  log named, or an explicit "none" line.

### Streaming

Files are processed one at a time with bounded memory (histograms only,
never the full path list in RAM); the command must handle the full
800+-file directory without ARG_MAX or OOM. No shell `for`-loop over an
expanded glob — the binary does its own directory walk.

## Acceptance criteria

1. `scribe rollup --dir <fixture-dir>` emits a Markdown digest with all of:
   session count, Top write-path prefixes, Top binaries executed, Outbound
   connect by process, Deletions, and a Flagged section (lines or "none").
2. `--since today` includes only logs with mtime on the current local day;
   a fixture log dated yesterday is excluded and a verifying test proves it.
3. `--format json` emits a single valid JSON object with keys for each
   section (parseable by `jq`); `--format md` is the default.
4. `--top N` caps each histogram section to N entries; the default is
   documented in `--help`.
5. A flagged write (e.g. `/home/jsy/.ssh/known_hosts`) in any session log
   appears under the Flagged section naming the source log; with no flagged
   writes the section reads "none".
6. `scribe rollup` over a directory of **≥ 300** fixture logs completes
   without error and in bounded memory (no per-file subprocess, no
   ARG_MAX) — verified by a test that generates ≥ 300 small logs.
7. An empty/`--since`-excludes-everything window emits a well-formed
   "0 sessions" digest and exits 0.
8. `--help` documents `rollup` and its flags; exit 0.
