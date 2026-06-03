# PRD: cadence-bind-reliquary — reliquary consumes the year's four quarterly records as a primary section

**Status:** Draft v0.1
**build_auto:** false
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/memory-reliquary
**build_version_bump:** minor
**Vision:** visions/cadence.md
**Depends on:** PRD-cadence-substrate.md
**Synergistic with:** PRD-cadence-bind-zine.md
**Created:** 2026-05-24

---

## TL;DR

`memory-reliquary` builds the annual book-of-memories by walking the
year's recall memories and producing a typesetting-input Markdown
bundle for a Typst template. After this PRD, `reliquary` accepts the
year's four `quarterly` cadence records (from the zine) as a
first-class input section alongside the recall dump — so the annual
book includes "what the zine called out each quarter" as its own
chapter, not just the raw memory stream.

## Why this exists

Phase 1 research, 2026-05-24:

- `~/wintermute/memory-reliquary/` exists; crate name
  `memory-reliquary`; binary `reliquary`.
- README: "the operational bottleneck is the deterministic typesetting-
  input step: a script that walks the year's recall memories and
  produces a clean, ordered, frontmatter-rich Markdown bundle that a
  Typst template (Phase 1b) can consume." Confirms recall is the
  current source.
- With cadence-bind-zine shipped, the four quarterly records exist as
  curated moment-bundles. They are exactly the artifact a year-book
  wants as a "highlights" section.

## What this builds

### Extension shape

`rust-extend` into `~/wintermute/memory-reliquary/`. Add
`src/cadence_intake.rs` (~60 LOC) and `src/cadence_record.rs`
(~30 LOC). Wire into the bundle assembly. Version bump: minor.

### CLI surface

- New flag `--cadence-quarterly` (default: on if substrate present
  and contains at least one quarterly record for the target year).
  Reads the year's quarterly records and embeds them as a top-of-
  bundle "Quarterly highlights" section.
- New flag `--no-cadence-record` (default: record). Preserves
  side-effect-free runs.
- New flag `--year <YYYY>` (existing if already present; document
  the cadence integration uses this).

### Intake behavior

```
cadence list quarterly --period 2026-Q1,2026-Q2,2026-Q3,2026-Q4
                       --produced-by zine --json
```

For each quarterly record:
- Read `record.path` (the zine moment-bundle markdown).
- Wrap with a `## Q<n> 2026 — <summary>` heading.
- Insert as the first major section in the typesetting-input bundle,
  before the recall-memories dump.

If a quarter is missing, render a placeholder heading "Q<n> 2026 —
(no zine)" so the typesetting template sees a stable 4-chapter
structure.

### Record behavior

```
cadence record annual --produced-by reliquary --path <bundle-md>
                       --summary "<year>: <N> quarterly + <M> memories"
                       --sources <quarterly-ids>
                       --meta year=<YYYY>
```

### Dependencies

No new deps; shells out to `cadence`.

## Acceptance criteria

1. With quarterly records present (PRD-cadence-bind-zine shipped),
   `reliquary --year 2026 --out /tmp/r.md` produces a Markdown
   bundle whose first major section is "Quarterly highlights" with
   subsections for each quarter that has a record.
2. The bundle continues to include the recall-memories dump as a
   later section (existing behavior preserved).
3. A cadence record is created: `cadence list annual --produced-by
   reliquary --since 1h` shows the new record. Its `sources`
   contains the four quarterly ulids.
4. With quarterly records absent for one or more quarters, the
   bundle includes placeholder "(no zine)" headings for missing
   quarters and proceeds without error.
5. With substrate absent (`CADENCE_HOME=/nonexistent`), `reliquary`
   runs over recall only (existing behavior), records nothing, logs
   one stderr warning, exits 0.
6. `--no-cadence-record` suppresses the record; `--cadence-quarterly=false`
   suppresses intake.
7. `cargo test --release` green; new tests cover the
   missing-quarter and present-quarter paths.
8. Version bumped to v0.2.0; `CHANGELOG.md` updated.

## Out of scope

- Typst template changes (the bundle's downstream consumer; out of
  scope for this PRD).
- Backfilling annual records for past years.
- Multi-year aggregation.

## Notes for /build

- This is the top tier. Once this ships, every cadence tier
  produces and consumes records on its bind path. The pyramid is
  fully wired.
- After this, the natural next move is `cadence-pulse` (already a
  drafted PRD).
