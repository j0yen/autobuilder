# PRD: cadence-bind-letters — letter-curate reads the month's weekly records

**Status:** Draft v0.1
**build_auto:** false
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/letters-we-never-sent
**build_version_bump:** minor
**Vision:** visions/cadence.md
**Depends on:** PRD-cadence-substrate.md
**Synergistic with:** PRD-cadence-bind-confidant.md
**Created:** 2026-05-24

---

## TL;DR

`letters-we-never-sent` curates a monthly draft-ritual aggregate from
`~/.claude/letters/`. Today it reads its own letter directory; it has
no awareness of the weekly `confidant` letters produced one tier below.
After this PRD, `letter-curate` reads the past month's `weekly` cadence
records as additional intake, includes them in the curation pass, and
registers each monthly aggregate it produces as a `monthly` cadence
record.

## Why this exists

Phase 1 research, 2026-05-24:

- `~/wintermute/letters-we-never-sent/` exists; crate name
  `letters-we-never-sent`; binary `letter-curate`.
- README: "monthly draft ritual produces 2-4 letter Markdown files
  per month in ~/.claude/letters-we-never-sent/<year>/." Confirms it
  reads its own directory and produces Markdown.
- No mention of consuming weekly artifacts. With cadence-bind-confidant
  shipped, those weekly artifacts exist; this PRD wires letter-curate
  to use them.

## What this builds

### Extension shape

`rust-extend` into `~/wintermute/letters-we-never-sent/`. Add
`src/cadence_intake.rs` (~70 LOC) and `src/cadence_record.rs`
(~40 LOC). Wire into the existing curation loop. Version bump: minor.

### CLI surface

- New flag `--cadence-intake` (default: on if substrate present).
  Reads the past 30 days of `weekly` records from confidant as part
  of the monthly aggregation source set.
- New flag `--no-cadence-record` (default: record). Preserves
  side-effect-free runs.
- New flag `--cadence-since <duration>` (default: 30d).

### Intake behavior

```
cadence list weekly --since 30d --produced-by confidant --json
```

For each weekly record returned, read the letter Markdown at
`record.path`, parse the first ~500 chars as the letter's lede, and
include in the monthly curator's source pool alongside the existing
`~/.claude/letters/` Markdown files.

The curator's existing scoring/ranking (whatever it uses today)
applies to the merged pool. No quality knobs change in this PRD; the
intake just grows.

### Record behavior

After emitting the monthly aggregate, record it:

```
cadence record monthly --produced-by letter-curate --path <month-md>
                        --summary "<month>: <N> letters"
                        --sources <weekly-ids>
```

`--sources` is the ulids of all weekly records pulled into intake
(not just those that ended up in the final aggregate, since the
curator might cite or omit at its discretion).

### Dependencies

No new deps; shells out to `cadence`.

## Acceptance criteria

1. With weekly records present (PRD-cadence-bind-confidant shipped),
   `letter-curate --month current` includes the weekly letter ledes
   in its source pool. Verify via `--print-sources` flag (new),
   which lists all candidate sources before curation.
2. The monthly aggregate Markdown is written to its existing output
   path AND a cadence record is created: `cadence list monthly
   --produced-by letter-curate --since 1h` shows the new record.
3. The cadence record's `sources` contains the ulids of all weekly
   records pulled in AC1 (whether or not they ended up cited).
4. With weekly records absent (substrate empty), `letter-curate
   --month current` still runs over `~/.claude/letters/` as today.
5. With substrate absent entirely (`CADENCE_HOME=/nonexistent`), no
   crash, one stderr warning, intake skipped.
6. `--no-cadence-record` suppresses record while preserving intake;
   `--cadence-intake=false` suppresses intake while preserving
   record.
7. `cargo test --release` green; new tests cover intake-on / intake-
   off paths.
8. Version bumped to v0.2.0; `CHANGELOG.md` updated.

## Out of scope

- Changes to the curator's scoring/ranking logic.
- Migrating existing `~/.claude/letters/` to cadence records.
- Backfilling monthly records for past months.

## Notes for /build

- Same shape as PRD-cadence-bind-confidant. Once you've shipped one
  bind-PRD, the next is mechanical.
- If `cadence-bind-confidant` has not shipped, this still ships;
  intake will be empty until confidant starts producing weekly
  records.
