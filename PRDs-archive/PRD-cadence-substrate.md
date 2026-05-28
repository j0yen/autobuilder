# PRD: cadence-substrate — the shared time-pyramid record store

**Status:** Draft v0.1
**build_auto:** false
**build_target:** rust-cli
**Vision:** visions/cadence.md
**Created:** 2026-05-24

---

## TL;DR

This laptop has five reflective-artifact tools at five time horizons —
daily-receipt, confidant, letters-we-never-sent, conversations-zine,
memory-reliquary — and none of them compose. The blocker is that no
shared record store exists where "I produced a daily artifact today"
can be looked up by "what daily records do I have for this week?".
This PRD ships that store as a new repo `cadence` at
`~/wintermute/cadence/`, a small Rust CLI with `record`, `list`,
`latest`, and `register` subcommands, and a `~/.claude/cadence/`
directory layout. No tier-wiring happens here; binds come in the next
five PRDs. Foundational only.

## Why this exists

Phase 1 research, 2026-05-24:

- All five candidate tools exist as Rust binaries under
  `~/wintermute/` (verified `daily-receipt`, `confidant`,
  `letters-we-never-sent` (binary `letter-curate`), `conversations-
  zine` (binary `zine`), `memory-reliquary` (binary `reliquary`)).
- `~/.claude/cadence/` does NOT exist (`ls ~/.claude/tempo` returned
  "no tempo dir"). No shared substrate.
- README "Why this exists" in each tool cites a *different* primary
  source: `daily-receipt` ("content payload supplied by upstream"),
  `conversations-zine` ("walks session JSONLs"), `memory-reliquary`
  ("walks the year's recall memories"). Confirms tools do not
  compose.
- `~/wintermute/REPOS.md` "Artist / narrative" section already groups
  these tools as the laptop's reflective family — the *grouping*
  exists in prose, but no operational substrate joins them.

The vision is to land the smallest substrate that makes the pyramid
composable without rewriting the tools.

## What this builds

### Repo

New repo at `~/wintermute/cadence/`, eventually
`github.com/j0yen/cadence`. Single Rust binary `cadence`. v0.1.0
target. Crate name `cadence`.

### Directory layout

Substrate root: `~/.claude/cadence/` (override via `CADENCE_HOME`).

```
~/.claude/cadence/
├── manifest.json         # registered tools, tier defaults, version
├── daily/
│   └── 2026-05-24/
│       └── <ulid>.json
├── weekly/
│   └── 2026-W21/
│       └── <ulid>.json
├── monthly/
│   └── 2026-05/
│       └── <ulid>.json
├── quarterly/
│   └── 2026-Q2/
│       └── <ulid>.json
└── annual/
    └── 2026/
        └── <ulid>.json
```

### Record schema

```json
{
  "id": "01HXYZ…",
  "tier": "daily",
  "period": "2026-05-24",
  "produced_by": "daily-receipt",
  "produced_at": "2026-05-24T22:18:09Z",
  "path": "/home/jsy/.claude/daily-receipt/2026-05-24.escpos",
  "sources": [],
  "summary": "Day with 3 sessions, ~9k events, autobuilder slice work.",
  "meta": {}
}
```

`sources` is an array of cadence record IDs from the tier below; for
the bind PRDs, this gets populated. For substrate-only consumers, it
defaults to empty.

### CLI

```
cadence register <kind> --tier <daily|weekly|monthly|quarterly|annual>
  → declare a tool's intent to record this tier; updates manifest.json

cadence record <tier> --produced-by <tool> --path <p> [--summary <s>]
                       [--sources <id>[,<id>...]] [--meta key=val ...]
  → append a new record under the appropriate period directory; prints id

cadence list <tier> [--since <duration>] [--period <period>] [--json]
  → enumerate records; default human-readable, --json for piping into
    bind extensions

cadence latest <tier> [--produced-by <tool>] [--json]
  → newest record for that tier

cadence where
  → prints CADENCE_HOME, manifest.json path, daily/weekly/... counts
```

### Dependencies

`clap` (derive), `serde`, `serde_json`, `ulid`, `chrono`, `anyhow`,
`thiserror`. No HTTP, no DB. Pure filesystem store with JSON files.

### Period naming

- daily: `YYYY-MM-DD` (local timezone)
- weekly: `YYYY-Www` (ISO 8601 week)
- monthly: `YYYY-MM`
- quarterly: `YYYY-Q[1-4]`
- annual: `YYYY`

`cadence record` derives the period from `--produced-at` (or `now()`)
unless explicitly overridden via `--period`.

## Acceptance criteria

1. `cargo install --path .` installs binary `cadence` to
   `~/.local/bin/`; `cadence --version` reports `0.1.0`.
2. `cadence where` on a fresh laptop creates `~/.claude/cadence/` and
   reports counts of 0 across all tiers.
3. `cadence register daily-receipt --tier daily` records the tool in
   `manifest.json` under `tools[].name == "daily-receipt"`.
4. `cadence record daily --produced-by daily-receipt --path
   /tmp/test.escpos --summary "manual test"` creates a record file at
   `~/.claude/cadence/daily/$(date +%Y-%m-%d)/<ulid>.json`. Prints the
   ulid to stdout.
5. `cadence list daily --json | jq -r '.[].id'` includes the ulid
   from AC4.
6. `cadence latest daily --produced-by daily-receipt --json | jq -r
   '.path'` returns `/tmp/test.escpos`.
7. `cadence list daily --since 7d --json` returns only records whose
   `produced_at` is within the last 7 days.
8. `cadence record weekly --produced-by confidant --path /tmp/w.md
   --sources 01HXYZ…` correctly records `sources: ["01HXYZ…"]` and
   the file lands under `~/.claude/cadence/weekly/$(date
   +%Y-W%V)/<ulid>.json`.
9. `cargo test --release` green, lib + integration suite.
10. `cadence record` is **append-only** — two records on the same day
    by the same tool both persist; `latest` returns the newer one.
11. Repo has a `CHANGELOG.md` with a `## v0.1.0` section enumerating
    the four primary subcommands and the directory schema.

## Out of scope

- Tier-wiring of existing tools (those are the bind PRDs).
- `cadence pulse` (PRD-cadence-pulse.md).
- `cadence thread`, `cadence deck`, `cadence share` (Fleet 2).
- Indexing or full-text search over summaries (substrate is dumb
  filesystem; future work).
- Garbage collection / pruning (deferred; substrate grows linearly
  with reflective output, ~5-15KB/day worst case, acceptable for
  multi-year runway).

## Notes for /build

- This is the foundational PRD. The five bind PRDs and `cadence-
  pulse` all depend on this shipping first.
- Crate name `cadence` collides with one published Rust crate but not
  with anything on this laptop. Confirm name is free on crates.io if
  ever publishing; the local install does not need crates.io.
- Default `build_auto: false` per /dream rule. User opts in before
  /build advances.
