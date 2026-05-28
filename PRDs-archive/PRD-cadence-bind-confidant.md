# PRD: cadence-bind-confidant — confidant reads the week's daily records as raw material

**Status:** Draft v0.1
**build_auto:** false
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/confidant
**build_version_bump:** minor
**Vision:** visions/cadence.md
**Depends on:** PRD-cadence-substrate.md
**Synergistic with:** PRD-cadence-bind-daily-receipt.md (best with daily records present)
**Created:** 2026-05-24

---

## TL;DR

`confidant` composes a weekly letter and an e-ink PNG. Today its input
is whatever the user supplies. After this PRD, `confidant` can pull
the week's seven `daily` cadence records as a primary intake source,
producing letters that are grounded in what actually happened that
week — and registers each letter it produces as a `weekly` cadence
record so the monthly tier has something to consume.

## Why this exists

Phase 1 research, 2026-05-24:

- `~/wintermute/confidant/` exists; crate name `confidant`; binary
  `confidant`. README: "Weekly letter composer + e-ink PNG renderer."
- README's "Scope of this crate" makes no mention of cadence or daily
  artifacts as input. Confirms tier-skip.
- Without this bind, `confidant`'s weekly letters are isolated from
  the daily artifacts produced one tier below, even though both tools
  are about reflective output.

## What this builds

### Extension shape

`rust-extend` into `~/wintermute/confidant/`. Add `src/cadence_intake.rs`
and `src/cadence_record.rs` (each ~50-80 LOC). Wire intake into the
existing letter-composition path; wire record into the post-emit path.
Version bump: minor (additive flag).

### CLI surface

- New flag `--cadence-intake` (default: on if substrate exists and
  contains daily records; off if substrate absent or empty). Reads
  the past 7 days of `daily` records and includes their summaries
  as part of the letter prompt context.
- New flag `--no-cadence-record` (default: record). Preserves
  side-effect-free test runs.
- New flag `--cadence-since <duration>` (default: 7d). How far back
  to pull daily records.

### Intake behavior

```
cadence list daily --since 7d --produced-by daily-receipt --json
```

For each record returned, read `summary` + (optionally) `path`-file
preview, concatenate into the prompt context block as "Daily
artifacts this week:". The letter composer's existing prompt template
gains a new section.

If `cadence list` returns empty: silently skip the section (letter
composes as before, just without the new context).

### Record behavior

After emitting the weekly letter PNG / Markdown, shell out to:

```
cadence record weekly --produced-by confidant --path <letter-md-path>
                       --summary <first-line-of-letter>
                       --sources <daily-ids-pulled-in-intake>
```

`--sources` is the comma-joined ulids of the daily records that fed
the letter; this is what gives the pyramid its lineage.

### Dependencies

No new deps. Shells out to `cadence`.

## Acceptance criteria

1. With substrate populated (PRD-cadence-bind-daily-receipt shipped
   and at least one daily record present), `confidant compose
   --week current` injects the daily summaries into the prompt
   context. Verify via dry-run mode (`--print-prompt`) which prints
   the assembled prompt to stdout.
2. The composed letter (markdown) is written to its existing output
   path AND a cadence record is created: `cadence list weekly
   --produced-by confidant --since 1h --json` includes the new
   record.
3. The cadence record's `sources` array contains the ulids of all
   daily records pulled in AC1.
4. With substrate absent (`CADENCE_HOME=/nonexistent`), `confidant
   compose --week current` runs successfully without injection and
   without recording (one warning line to stderr; no crash).
5. With substrate present but empty (no daily records), `confidant
   compose` runs successfully, omits the "Daily artifacts this
   week:" section, and records its output as a weekly record with
   empty `sources`.
6. `--no-cadence-record` suppresses the record step while preserving
   intake. `--cadence-intake=false` suppresses intake while
   preserving record.
7. `cargo test --release` green; new tests cover both intake-on /
   intake-off and record-on / record-off paths.
8. Version bumped to v0.2.0 (minor); `CHANGELOG.md` updated.

## Out of scope

- Letter quality improvements (this PRD is plumbing).
- E-ink PNG render changes (existing render path unchanged).
- `confidant` reading session JSONLs directly (deferred; cadence is
  the intended source).

## Notes for /build

- This is the first PRD that exercises both directions: it READS
  from cadence (daily) and WRITES to cadence (weekly). The pattern
  generalizes to letters / zine / reliquary in the subsequent PRDs.
- If PRD-cadence-bind-daily-receipt has not shipped, confidant still
  ships and works — it just has empty intake. The pyramid tolerates
  a half-bound state.
