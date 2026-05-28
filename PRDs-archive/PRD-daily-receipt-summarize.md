# PRD: daily-receipt-summarize — gather the day's signals into summary.json

**Status:** Draft v0.1
**build_target:** rust-cli
**build_into:** /home/jsy/wintermute/day-summarize
**build_version_bump:** N/A (new crate)
**Vision:** visions/daily-receipt.md
**Depends on:** none (consumes ctrace + git + recall + journal — all already present)
**Synergistic with:** PRD-daily-receipt-printer.md (this produces what `receipt today --summary` wants)
**Created:** 2026-05-27
**Author:** Claude (Opus 4.7), for jsy

---

## TL;DR

`daily-receipt` v0.1 ships a renderer that consumes `summary.json` and
emits ESC/POS bytes. PRD-daily-receipt-printer wraps that for physical
print. Both PRDs assume an upstream producer of `summary.json` exists.
It doesn't. This PRD builds that producer: a small Rust binary
`day-summarize` that gathers signals from ctrace, git, recall, and the
journal into the JSON shape `daily-receipt render` expects.

Without this, every daily print falls through to the quiet-day glyph
fallback because no real summary is ever produced. The whole arc is
silent.

## Why this exists

Phase 1 research, 2026-05-27:

- `~/wintermute/daily-receipt/PRDs-archive/PRD-daily-receipt.md` §4
  draws this exact box: "day-summarizer (Rust): pulls ctrace summary,
  commits, build exits, journal notes." It was never built.
- `daily-receipt render --help` shows `--summary <path>` is a required
  CLI arg. The classifier in `daily-receipt/src/classifier.rs` reads
  three fields: `commits` (count), `repos[]` (distinct repos),
  `special_stamp_id` (optional). Anything else is informational.
- `ctrace query --since 24h --by write_path` exists today
  (`/home/jsy/.local/bin/ctrace`); it returns JSON write-path counts.
  Today's self-review used it to surface the top-write prefixes.
- `git log --since=24.hours --format=%h --all --no-merges` enumerates
  the day's commits across all repos under a root. We need a
  multi-repo walk: `~/wintermute/`, `~/projects/` (archived), `~/.claude/`.
- `recall list --since 24h --json` returns the day's memory writes.
- `~/brain/journal/<YYYY-MM-DD>.md` presence is itself a signal —
  self-review writes one per run; if today has a journal, that's a
  data point even before parsing it.
- The PRD-daily-receipt-printer (this session) wires
  `$DAILY_RECEIPT_SUMMARY_DIR/<today>.json` as the default summary
  path. This PRD writes to exactly that path.

## What this builds

### Crate shape

- Name: `day-summarize`
- Binary: `day-summarize`
- LOC budget: ≤400 src, ≤300 tests.
- Dependencies: `clap`, `chrono`, `serde`, `serde_json`, `walkdir`.
  Shells out to `git` and `ctrace` and `recall` (don't link).

### CLI

```
day-summarize today          # write today's summary to default path
day-summarize today --out <path>
day-summarize today --json   # also print to stdout
day-summarize for <YYYY-MM-DD> --out <path>
day-summarize dump-signals   # print every signal source, for debugging
```

### Default output path

`$DAILY_RECEIPT_SUMMARY_DIR/<YYYY-MM-DD>.json` with default
`$DAILY_RECEIPT_SUMMARY_DIR = $XDG_STATE_HOME/daily-receipt/summaries`
falling back to `~/.local/state/daily-receipt/summaries/`. Matches the
default that PRD-daily-receipt-printer expects.

### `summary.json` schema

```json
{
  "date": "2026-05-27",
  "commits": 12,
  "repos": ["j0yen/recall", "j0yen/autobuilder", "j0yen/daily-receipt"],
  "commits_by_repo": {
    "j0yen/recall": 5,
    "j0yen/autobuilder": 4,
    "j0yen/daily-receipt": 3
  },
  "ctrace_write_count": 5341,
  "ctrace_top_paths": [
    {"prefix": "/tmp/node-compile-cache", "count": 520},
    {"prefix": "/home/jsy/.claude", "count": 65},
    {"prefix": "/home/jsy/wintermute", "count": 41}
  ],
  "recall_writes": 7,
  "recall_subjects": ["self", "feedback", "project"],
  "journal_present": true,
  "journal_first_heading": "Self-review — 2026-05-27",
  "special_stamp_id": null,
  "produced_by": "day-summarize",
  "produced_at": "2026-05-27T21:30:00-07:00"
}
```

The minimum set the classifier needs: `date`, `commits`, `repos`,
`special_stamp_id`. The rest is grist for the haiku generator (next
PRD) and human curiosity.

### Walk strategy

- **Git commits**: walk `~/wintermute/`, `~/projects/`, `~/.claude/`
  (configurable via `$DAILY_RECEIPT_GIT_ROOTS`). For each top-level
  directory containing `.git/`, run
  `git -C <dir> log --since=<start-of-day> --until=<end-of-day> --no-merges --format=%H`
  with `--author=` filter NOT applied (multi-author commits count).
  Distinct repo count = number of `.git/` directories with ≥1 commit.
- **ctrace**: `ctrace query --since 24h --by write_path --json`.
  Take the top 5 prefixes by count. If ctrace isn't running or returns
  empty, set `ctrace_write_count: 0` and `ctrace_top_paths: []`.
- **recall**: `recall list --since 24h --json` (best-effort; if recall
  binary is missing, `recall_writes: 0`).
- **Journal**: `stat ~/brain/journal/<today>.md`. Present → read
  first heading. Absent → `journal_present: false`.
- **Stamps**: read `~/.claude/daily-receipt/stamps/<today>.json` AND
  `~/.claude/daily-receipt/stamps/<MM-DD>.json` (recurring). First
  match wins. Out of scope here — PRD-daily-receipt-stamps drafts
  the stamp catalog; this PRD only reads it.

## Acceptance criteria

- **AC1**: `day-summarize today --out /tmp/s.json` writes a non-empty
  JSON file with every required key (`date`, `commits`, `repos`,
  `special_stamp_id`, `produced_by`, `produced_at`). Exits 0.
- **AC2**: `date` field matches local-time today (`chrono::Local`).
  Test injects a fake clock via a `--date` override and verifies
  ISO 8601 (`YYYY-MM-DD`).
- **AC3**: With `$DAILY_RECEIPT_GIT_ROOTS` pointing at a tempdir
  containing two repos with 3 and 5 commits dated today, `commits`
  is 8 and `repos` length is 2.
- **AC4**: Missing `ctrace` binary on `$PATH` does NOT panic; sets
  `ctrace_write_count: 0` and logs a single stderr warning.
  Tested by clearing `$PATH` to `/dev/null` for the subprocess.
- **AC5**: Missing `recall` binary likewise sets `recall_writes: 0`
  and `recall_subjects: []`. Same pattern as AC4.
- **AC6**: Output JSON is canonical-ordered (keys sorted), so
  byte-equal runs over byte-equal signals produce byte-equal files.
  Deterministic re-runs are a debugging affordance.
- **AC7**: `--json` flag prints the same JSON to stdout in addition
  to writing the file. Snapshot test asserts stdout == file contents.
- **AC8**: `day-summarize for 2024-01-15 --out /tmp/past.json` works
  against a historical date (no commits expected → `commits: 0,
  repos: []` is valid; not an error).
- **AC9**: When a stamp file exists for today at
  `~/.claude/daily-receipt/stamps/<today>.json` with shape
  `{"id": "K-birthday", ...}`, `summary.special_stamp_id` is
  `"K-birthday"`. When no stamp file, `special_stamp_id: null`.

## Files this will create

```
~/wintermute/day-summarize/
├── Cargo.toml
├── README.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── install.sh
├── src/
│   ├── main.rs           # clap dispatch
│   ├── lib.rs            # gather() orchestrator
│   ├── git_walk.rs       # multi-repo git log walker
│   ├── ctrace_query.rs   # shell out to ctrace
│   ├── recall_query.rs   # shell out to recall
│   ├── journal.rs        # journal presence + first heading
│   └── stamps.rs         # stamp lookup
└── tests/
    ├── ac1_schema.rs
    ├── ac2_date.rs
    ├── ac3_git_walk.rs
    ├── ac4_missing_ctrace.rs
    ├── ac5_missing_recall.rs
    ├── ac6_deterministic.rs
    ├── ac7_stdout_mirror.rs
    ├── ac8_historical.rs
    └── ac9_stamp_lookup.rs
```

## Non-functional

- No network calls.
- No `unsafe`.
- Shell-outs use absolute paths discovered via `which` once at startup,
  cached for the run.
- All errors are best-effort; the binary never panics on missing
  upstream tooling. The point is to produce *some* summary, every day.

## After this lands

PRD-daily-receipt-haiku consumes `summary.json` and produces
`content.json`. Together they fill the two `receipt today` inputs.
The systemd-user timer from PRD-daily-receipt-printer can then chain:

```
ExecStart=%h/.local/bin/day-summarize today
ExecStart=%h/.local/bin/day-haiku today
ExecStart=%h/.local/bin/receipt today
```

Three small binaries; three clear seams. Each independently testable.
