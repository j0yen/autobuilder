# PRD: docket-core — a ledger for standing findings

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/docket.md
**build_target:** rust-cli
**Depends on:** none
**Codename:** *docket* — a finding reported twice is the same finding.

## TL;DR

The self-review rediscovers the same findings every run and parks them
as free text. There is no structured place to record that a finding
exists, when it was first seen, and how many runs it has survived.
docket-core is that place: a small SQLite-backed CLI where a producer
*reports* a finding under a stable key, and the ledger dedupes by key,
tracks first/last-seen and a consecutive-run streak, and lists what is
currently open. This PRD builds the store and the report/list/show/
resolve contract; lifecycle automation (escalation, auto-close) and
evidence richness are follow-on PRDs that extend this crate.

## Why this exists

Phase 1 evidence (2026-05-29):

- `self-review/SKILL.md` lines 452-465: each run persists exactly **one**
  reflective recall memory whose free-text *"Pending"* line is the whole
  carry-forward state. No per-finding entity exists.
- `~/.claude/skills/self-review/state/` **does not exist** — the skill
  has no structured state; verified live this session (`ls` returns
  nothing).
- `grep -l "Carried forward" ~/brain/journal/*.md` → **6 consecutive
  days** of the same hand-maintained prose section.
- The "agorabus daemon stale binary" finding appears **7× in the
  2026-05-28 journal**; it is one finding, recorded seven times, with no
  identity tying those mentions together.

A finding needs a primary key. docket-core gives it one.

## What this builds

A standalone Rust CLI published as `j0yen/docket`, installed to
`~/.local/bin/docket`. Mirrors the shape of the existing local toolkit
(`recall`, `ctrace`, etc.).

**Store:** SQLite at `${XDG_DATA_HOME:-~/.local/share}/docket/docket.db`,
created on first use. WAL mode. Single table `findings`:

| column             | type    | notes                                        |
|--------------------|---------|----------------------------------------------|
| `key`              | TEXT PK | stable slug, e.g. `agorabus-stale-binary`    |
| `title`            | TEXT    | human one-liner (latest report wins)         |
| `severity`         | TEXT    | `info`/`warn`/`crit` (default `warn`)        |
| `status`           | TEXT    | `open`/`resolved` (escalation in next PRD)   |
| `first_seen`       | TEXT    | RFC3339, set on create                       |
| `last_seen`        | TEXT    | RFC3339, bumped each report                  |
| `first_run`        | TEXT    | run-id of first report                       |
| `last_run`         | TEXT    | run-id of most recent report                 |
| `runs_seen`        | INTEGER | count of **distinct** run-ids reported       |
| `consecutive_runs` | INTEGER | current streak (see Run model)               |
| `report_count`     | INTEGER | raw report calls (diagnostic)                |
| `resolved_at`      | TEXT    | nullable                                     |
| `resolve_reason`   | TEXT    | nullable                                     |

**Run model.** A "run" is a caller-supplied opaque string passed via
`--run <id>` (e.g. `2026-05-29.1`). Reporting the same key twice within
the same run-id is idempotent for streak purposes: `runs_seen` and
`consecutive_runs` count distinct run-ids, `report_count` counts raw
calls. `consecutive_runs` increments when a key is reported in a run-id
lexically/temporally after `last_run` and `last_run` differs; this PRD
treats *any new run-id* as advancing the streak (the "did the previous
run also see it" gap-detection lives in docket-escalate's `sweep`). Keep
the run-id comparison string-based and caller-ordered; docket does not
parse run-id semantics.

**Commands:**

- `docket report --run <id> --key <slug> --title <t> [--severity info|warn|crit] [--evidence <ref>]`
  — upsert. Creates the entry if new (status `open`, streak 1) or bumps
  an existing one (last_seen/last_run/report_count always; runs_seen +
  consecutive_runs only when run-id is new). A resolved entry reported
  again **reopens** (status→open, streak reset to 1, resolved_at/reason
  cleared). `--evidence` is accepted and stored as a raw string in v1
  (typed parsing arrives in docket-evidence); accept the flag now so the
  binding PRD's call sites are stable.
- `docket list [--open|--resolved|--all] [--format text|json] [--severity <min>]`
  — default `--open --format text`. JSON is an array of full rows.
- `docket show <key> [--format text|json]` — one finding's full record.
  Nonzero exit if key unknown.
- `docket resolve <key> [--reason <r>]` — status→resolved,
  resolved_at=now. Idempotent. Nonzero exit if key unknown.
- `docket --version`, `docket --help`.

**Deps:** `rusqlite` (bundled SQLite), `clap` (derive), `serde` +
`serde_json` for `--format json`, a minimal RFC3339 timestamp (chrono or
time). No network. MSRV 1.85, edition 2021, no let-chains (matches the
recall baseline constraint).

**Atomicity:** every mutating command is one SQLite transaction. Concurrent
`report` from overlapping self-review loop iterations must not corrupt
counts (WAL + `BEGIN IMMEDIATE`).

## Acceptance criteria

1. `cargo build --release` produces `target/release/docket`; `--version`
   prints `docket <semver>` and `--help` lists `report|list|show|resolve`.
2. `docket report --run r1 --key k --title "T"` on an empty store creates
   a row with `status=open`, `first_seen==last_seen`, `runs_seen=1`,
   `consecutive_runs=1`, `report_count=1` (assert via `docket show k
   --format json`).
3. Reporting key `k` again with the **same** `--run r1` leaves
   `runs_seen=1` and `consecutive_runs=1` but sets `report_count=2` and
   bumps `last_seen`.
4. Reporting key `k` with a **new** `--run r2` sets `runs_seen=2`,
   `consecutive_runs=2`, `last_run=r2`.
5. `docket list --open --format json` returns a JSON array containing `k`;
   after `docket resolve k --reason done`, `docket list --open` omits
   `k`, `docket list --resolved` includes it with `resolve_reason=done`
   and a non-null `resolved_at`.
6. Reporting a resolved key reopens it: `status=open`,
   `consecutive_runs=1`, `resolved_at` null again.
7. `docket show <unknown-key>` exits nonzero with a clear message;
   `docket list` on an empty store exits 0 with empty output/`[]`.
8. The DB is created at the XDG path on first use without manual `mkdir`;
   a second invocation reuses it (no data loss across process restarts —
   verify by reporting in one process, listing in another).
9. `--format json` output for `list` and `show` is valid JSON parseable
   by `python3 -json.tool` / `jq .` (the autobuilder JSON-shape check).
10. README documents the schema, the run model, and every subcommand
    with a worked example (`docket report` → `docket list` → `docket
    show` → `docket resolve`).

## Out of scope (later PRDs)

- Escalation at the 3-run threshold and `sweep` auto-resolve →
  **docket-escalate**.
- Typed evidence refs and trail rendering → **docket-evidence**.
- self-review wiring → **docket-self-review-bind**.
- `docket digest` / health envelope → **docket-digest**.
