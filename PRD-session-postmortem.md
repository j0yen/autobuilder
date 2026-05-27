# PRD: session-postmortem — one-command session forensics

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-25
**Vision:** [visions/continuity.md](visions/continuity.md)
build_auto: false
build_target: rust-cli
**Depends on:** PRD-agentns-claude (#1), PRD-provq (#2),
PRD-memlog-witness (#3), PRD-recall-session-stamp (#4). Can scaffold
ahead of all four against mock inputs; live ACs gate on the
upstream PRDs shipping.

---

## TL;DR

A session writes memlog snapshots, provfs-attributed files, recall
memories, and ctrace events. Today, surfacing a usable view of any
one of them requires four commands; surfacing all of them for *one
session* is a multi-step join. `session-postmortem <id>` does the
join. Output is a single markdown brief: who the session was, what
it intended, where it spent time, what it wrote, what it learned,
and (when applicable) why it died. Stdout by default; `--out` writes
to disk; `--brief` collapses to a 10-line summary.

This is the closing PRD of the `continuity` Fleet 1. It's the tool
the rest of the fleet exists to feed.

---

## 1. Why this exists

1. **The four signals are siloed today.** Recall lives in markdown;
   memlog in `~/.claude/memlog/<id>/` after PRD #3; provfs in
   xattrs surfaced by `provq` (PRD #2); ctrace in
   `~/.claude/ctrace/*.ndjson`. The natural question — "what
   happened in *that* session" — needs all four joined on
   `session_id`.

2. **Self-review already does ad-hoc joins.** From self-review
   playbooks (per the self-review skill description): "ctrace
   cross-session aggregate," "active ndjson is included in the
   aggregate," "MEMORY.md indexes synced check." Each requires
   custom logic per signal. `session-postmortem` factors that join
   out so the playbook becomes `session-postmortem --brief <id>`.

3. **Letters and zines need this as a primitive.** Per the
   `continuity` vision Fleet 2: `letter-from-snapshot` seeds a
   letter from a session's last memlog snapshot;
   `conversations-zine` finds moments worth printing. Both want
   "give me everything about session X." That's
   `session-postmortem --format json <id>`.

---

## 2. What this builds

### 2.1 Binary: `session-postmortem`

```
session-postmortem <session-id> [--format markdown|json|brief]
                                [--out <path>]
                                [--ctrace-dir <dir>]
                                [--memlog-dir <dir>]
                                [--include <signal,signal,...>]
                                [--exclude <signal,signal,...>]
```

- `<session-id>` accepts the same forms as `recall query --session`:
  full 128-bit hex, ≥8-char prefix, `current`, `latest`,
  `last-died`.
- `--format markdown` (default) writes a structured markdown brief.
- `--format json` writes a single JSON object for downstream tools.
- `--format brief` writes ~10 lines — useful in scripts and skills.
- `--out <path>` writes to disk; default stdout.
- `--include` / `--exclude` to drop signals (e.g.,
  `--exclude ctrace` if ctrace isn't running).

### 2.2 Markdown brief shape

```markdown
# Session 6a4f9d2e — /build — 2026-05-25 03:57 → 04:42

**Intent:** /build
**Duration:** 44m 18s
**Exit:** clean (SessionEnd hook fired)
**Counters:** 4823 syscalls · 6.4 MB write_bytes · 0 unlink · 12 fork

## Pre-compaction snapshots (3)
- snap-00000 (03:57:14) — 1842 tokens, "tick recall-daemon iter-2"
- snap-00001 (04:18:02) — 1923 tokens, "fix UDS framing test"
- snap-00002 (04:39:51) — 1751 tokens, "commit + push to recall"

## Files written (provfs, top 10)
- 6 in ~/wintermute/recall/src/
- 1 ~/wintermute/recall/Cargo.toml
- 1 ~/wintermute/recall/CHANGELOG.md
- 1 ~/.claude/skills/build/state/manifest.json
- 1 ~/wintermute/autobuilder/notes/gossip.md

## Memories written (recall, 2)
- self_recall_daemon_iter2 (reflective)
- project_recall_daemon_v050_uds (semantic)

## Execve top (ctrace, 5)
- cargo (87) · rustc (412) · git (8) · gh (3) · jq (12)

## Outbound (ctrace, 2 distinct hosts)
- api.github.com (3 connects)
- crates.io (1 connect)

## Notable
- 0 sensitive-path writes
- 1 budget warning at 0:38 (write_bytes 80% of soft limit)
```

### 2.3 Source resolution

| Signal | Source |
|---|---|
| Identity | session_id, intent_tag → `agentns-claude --verbose` records (if available), OR memlog snapshots, OR recall memories with this session_id |
| Duration | first → last timestamp across all signals |
| Pre-compaction snapshots | `~/.claude/memlog/<id>/snap-*.json` (PRD #3) |
| Files written | `provq scan ~ --session <id> --format paths` (PRD #2) |
| Memories written | `recall list --session <id> --format json` (PRD #4) |
| Execve, connects, unlinks | shell out to `ctrace summary --session-id <id>` if ctrace supports it (currently keys by tracer id; matching may require a small ctrace fork or session_id→tracer_id mapping read from `/proc/<pid>/agent_session` of the tracer's child) |
| Counters | `/proc/<pid>/agent_counters` of any live process in the session; if dead, persisted snapshot (best-effort) |

When a signal is absent (e.g., session predates memlog-witness),
the section header is included with `(no data)` rather than dropped
— so missing signals are visible.

### 2.4 Composition over invention

`session-postmortem` shells out. It does not re-implement ctrace
parsing, recall queries, or xattr reads. The PRDs it depends on do
that work; this one orchestrates. The implementation is
~300–500 LOC, mostly format-and-join.

---

## 3. Non-goals (v0.1)

- Multi-session aggregation. (`session-postmortem` is per-session.
  A future `session-rollup --since 1d` could roll up; not in
  scope.)
- Interactive UI. (Markdown to stdout; downstream tooling can pipe
  to a pager.)
- E-ink rendering, PNG output, HTML. (Markdown is the artifact.)
- Replay or re-execute the session. Pure read.

---

## 4. Acceptance criteria

1. **AC1 — Builds and installs.** `cargo build --release` →
   `target/release/session-postmortem`. `--version` matches crate
   version.
2. **AC2 — `--format markdown` against fixtures.** Repo ships a
   `tests/fixtures/session-deadbeef/` directory containing a
   handcrafted memlog dir, a fake `recall list` output, a fake
   `provq scan` output, and a fake `ctrace summary` output. With
   env overrides pointing the tool at the fixtures, output matches
   a golden markdown file.
3. **AC3 — `--format json`.** JSON validates against a committed
   schema; round-trips through `jq` cleanly.
4. **AC4 — `--format brief`.** Output is ≤10 lines, fits typical
   terminal width, includes id, intent, duration, exit, top-line
   counts.
5. **AC5 — Missing signals graceful.** With ctrace fixtures
   removed, output includes `## Execve top\n(no data)` rather than
   panicking or omitting the section.
6. **AC6 — `latest` and `last-died`.** With fixtures for three
   sessions where one died unclean, `session-postmortem latest`
   resolves to the most-recent by start time; `last-died` resolves
   to the unclean one.
7. **AC7 — `--out` writes file.** `--out /tmp/p.md` writes the
   brief there with 0644 perms; stdout is empty on success.
8. **AC8 — `--include` / `--exclude`.** `--exclude ctrace,memlog`
   produces a brief with only recall + provfs sections.
9. **AC9 [boot+upstream] — Real session end-to-end.** With PRDs
   #1–#4 shipped: run a tiny end-to-end test (a wrapped subshell
   that touches a file under provfs, writes a recall memory,
   triggers a synthetic memlog event), confirm
   `session-postmortem latest` includes all four signal sections
   with non-empty content.
10. **AC10 — README + CHANGELOG.** Repo README documents subcommands
    with examples; CHANGELOG v0.1.0 entry.

---

## 5. Shape

```
~/wintermute/session-postmortem/         new repo
├── Cargo.toml
├── README.md
├── CHANGELOG.md
├── src/
│   ├── main.rs           argv + dispatch
│   ├── resolve.rs        session-id alias → canonical id
│   ├── sources/
│   │   ├── mod.rs        Source trait
│   │   ├── memlog.rs     reads ~/.claude/memlog/<id>/
│   │   ├── provfs.rs     shells to provq scan
│   │   ├── recall.rs     shells to recall list --session
│   │   └── ctrace.rs     shells to ctrace summary; best-effort id match
│   ├── join.rs           assemble Brief struct
│   └── render/
│       ├── markdown.rs
│       ├── json.rs
│       └── brief.rs
└── tests/
    ├── fixtures/         pre-canned source outputs
    └── render.rs         golden markdown / json
```

Dependencies: `clap`, `serde_json`, `chrono`, `which` (to detect
sibling tools). No tokio.

---

## 6. Open questions

- Should `session-postmortem` *write* a recall memory of itself
  (reflective: "self_postmortem_<short-id>")? Tempting — closes the
  loop — but it's stateful and may surprise. Leaning no for v0.1.
  Re-evaluate after operating it.
- ctrace currently keys by tracer id, not agentns session_id. For
  AC9 to land cleanly, ctrace either needs a session-id index or
  this tool needs a tracer-id↔session-id mapping (read at session
  start from the tracer-spawning hook). Decide during iter-1:
  either path is small.
- `--out` directory convention: `~/brain/postmortems/<id>.md`?
  Leaving that to a thin downstream skill (`/postmortem`) that
  orchestrates path-and-journal-link. This tool just writes where
  told.

---

## 7. Provenance

- Vision: visions/continuity.md, Fleet 1 PRD #5 (closing).
- Composition target — depends on #1, #2, #3, #4. Can scaffold
  with mock inputs ahead of upstream PRDs.
- Pain motivating the join: 2026-05-24 self-review run-2 journal —
  "Cross-session ctrace aggregate over all 4 today's ndjsons shows
  7911 writes into ~/wintermute, 0 sensitive-path writes." That's a
  cross-session view; the analogous per-session view is what this
  tool provides.
