# PRD: cadence-bind-zine — zine reads the quarter's monthly records as moment seeds

**Status:** Draft v0.1
**build_auto:** false
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/conversations-zine
**build_version_bump:** minor
**Vision:** visions/cadence.md
**Depends on:** PRD-cadence-substrate.md
**Synergistic with:** PRD-cadence-bind-letters.md
**Created:** 2026-05-24

---

## TL;DR

`conversations-zine` extracts ~50 ranked moment-excerpts per quarter
by walking session JSONLs raw. After this PRD, the moment-extractor
accepts a `--cadence-monthly` flag that pulls the quarter's three
`monthly` cadence records (from `letter-curate`) as an additional
source of pre-curated moments, in parallel to the existing JSONL
walk. The zine's output is registered as a `quarterly` cadence record.

## Why this exists

Phase 1 research, 2026-05-24:

- `~/wintermute/conversations-zine/` exists; crate name
  `conversations-zine`; binary `zine`.
- README: "the bottleneck is the moment-extractor step — without a
  CLI that walks session JSONLs and surfaces ~50 ranked candidate
  excerpts." Confirms JSONLs are the current source.
- Re-deriving from raw JSONLs every quarter is expensive AND
  duplicates the work already done by `confidant` (weekly) and
  `letter-curate` (monthly). The monthly aggregates are exactly the
  pre-curated moment-seeds the zine needs.

## What this builds

### Extension shape

`rust-extend` into `~/wintermute/conversations-zine/`. Add
`src/cadence_intake.rs` (~80 LOC) — slightly meatier than the lower
tiers because it parses Markdown lettre-style aggregates into
candidate-moment shape. Version bump: minor.

### CLI surface

- New flag `--cadence-monthly` (default: off — this is opt-in because
  the zine's quality is sensitive to source mixing; let the editor
  toggle). When on, pulls past 92 days of `monthly` records and
  parses each for moment-shaped excerpts (paragraph-level chunks
  ≤300 chars, scored by markdown emphasis + position-in-letter).
- New flag `--cadence-record` (default: on if `--cadence-monthly`
  is on; otherwise off — don't record from a JSONL-only run unless
  explicit). Records the zine output as quarterly.
- New flag `--cadence-since <duration>` (default: 92d).

### Intake behavior

```
cadence list monthly --since 92d --produced-by letter-curate --json
```

For each monthly record, read `record.path`, split on markdown
headings + paragraph breaks, score each chunk, emit top-K (configurable,
default 15) as candidate-moments into the existing moment-pool. The
zine's existing JSONL-walk continues in parallel; the two sources
merge in the same ranker.

### Record behavior

```
cadence record quarterly --produced-by zine --path <zine-bundle-md>
                          --summary "Q<n> <year>: <N> moments,
                                     <M> from monthly, <K> from JSONL"
                          --sources <monthly-ids>
                          --meta moment_count=<N> source_mix=<m/k>
```

### Dependencies

`pulldown-cmark` (likely already a dep; if not, add it). No HTTP.

## Acceptance criteria

1. With monthly records present and `--cadence-monthly` flag set,
   `zine extract --quarter current --print-candidates` lists
   candidate moments from BOTH the JSONL walk AND the monthly
   records (sourced excerpts tagged with origin).
2. Without `--cadence-monthly`, the existing JSONL-walk behavior is
   unchanged. No regressions to the existing default path.
3. With `--cadence-monthly` and `--cadence-record`, the zine output
   produces a `quarterly` record: `cadence list quarterly
   --produced-by zine --since 1h` returns the new record.
4. The cadence record's `sources` contains the ulids of all monthly
   records pulled.
5. With monthly records absent (substrate empty),
   `--cadence-monthly` is a graceful no-op; one stderr warning;
   zine still produces output from JSONL walk alone.
6. `cargo test --release` green; new tests cover the markdown-parse
   path with a fixture monthly record.
7. Version bumped to v0.2.0; `CHANGELOG.md` updated.

## Out of scope

- Changes to the ranker.
- Backfilling quarterly records for past quarters.
- Mixed-source quality tuning (defer to a future PRD if editor
  reports the mix is worse than JSONL-only).

## Notes for /build

- `--cadence-monthly` is intentionally off-by-default — unlike the
  daily/weekly/monthly binds where the substrate is the canonical
  source, the zine's quality reputation matters and the editor
  should explicitly opt in to the merged-source experiment.
- Once this ships, the only remaining bind is `reliquary` (annual).
