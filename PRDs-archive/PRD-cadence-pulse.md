# PRD: cadence-pulse — what reflective work is overdue at which tier

**Status:** Draft v0.1
**build_auto:** false
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/cadence
**build_version_bump:** minor
**Vision:** visions/cadence.md
**Depends on:** PRD-cadence-substrate.md
**Created:** 2026-05-24

---

## TL;DR

The cadence substrate records when each tier last produced an
artifact, but nothing today consumes that information. `cadence pulse`
is the readout: a one-command per-tier table of last-produced
timestamps, expected cadence, overdue delta, and a single exit code
that equals the number of overdue tiers (so a SessionStart hook can
trip an alert). No analytics; no scheduler; just legibility.

## Why this exists

Phase 1 research, 2026-05-24:

- The cadence substrate (PRD-cadence-substrate.md) ships
  `record/list/latest/register` but no overdue detector.
- The reflective ritual today is implicit: the user notices "I
  haven't done a weekly letter in a while" by memory. Memory is an
  unreliable scheduler.
- `~/.claude/scripts/` already has SessionStart hook patterns
  (per CLAUDE_SELF mentions `claude-self`, `recall`, `ctrace`,
  `wchg`). `cadence pulse --hook` is a natural addition for
  SessionStart to surface overdue tiers without being invasive.
- Tide-chart is glanceable within-day; pulse is the cross-day,
  cross-week, cross-tier complement.

## What this builds

### Extension shape

`rust-extend` into `~/wintermute/cadence/` (same repo as substrate).
Adds a `pulse` subcommand to the existing `cadence` binary. Version
bump: minor (substrate v0.1.0 → v0.2.0).

### CLI surface

```
cadence pulse
  → human-readable table (default)

cadence pulse --json
  → machine-readable

cadence pulse --hook
  → terse, one-line-per-overdue-tier output suitable for SessionStart
    hook injection. Exits 0 if nothing overdue; non-zero exit code
    equals number of overdue tiers.

cadence pulse --tier <daily|weekly|monthly|quarterly|annual>
  → single-tier view

cadence pulse --quiet
  → output nothing; exit code only (for shell conditionals)
```

### Cadence defaults (configurable in manifest.json)

| Tier      | Default cadence | Overdue threshold |
|-----------|-----------------|-------------------|
| daily     | 1 day           | 2 days            |
| weekly    | 7 days          | 14 days           |
| monthly   | 30 days         | 60 days           |
| quarterly | 92 days         | 184 days          |
| annual    | 365 days        | 730 days          |

`overdue_threshold = 2 × cadence`. User can override per-tier in
`manifest.json` under `tiers.<name>.cadence_days` and
`tiers.<name>.overdue_after_days`.

### Output shape (human-readable)

```
Tier        Last produced              Expected     Status
─────────────────────────────────────────────────────────────
daily       2026-05-24 (today)         every 1d     ok
weekly      2026-05-19 (5d ago)        every 7d     ok
monthly     2026-04-30 (24d ago)       every 30d    due in 6d
quarterly   2026-03-31 (54d ago)       every 92d    ok
annual      never                      every 365d   overdue: never
```

Overdue rows print in red (ANSI) when stdout is a TTY.

### Exit codes

- `0`: no tiers overdue
- `1..=5`: number of overdue tiers
- `127`: substrate not initialized (no `~/.claude/cadence/`)

### Hook integration helper

Ship a shell script at `~/wintermute/cadence/scripts/cadence-pulse-hook.sh`
that calls `cadence pulse --hook` and, if exit > 0, prints a brief
nudge formatted for SessionStart hook output. Document install in
README; don't auto-install into `~/.claude/settings.json`.

### Dependencies

`chrono` (already a dep of substrate), `colored` or `ansi_term` for
TTY-colored output. No new heavy deps.

## Acceptance criteria

1. `cadence pulse` on a fresh substrate (no records) prints all five
   tiers with status "overdue: never" and exits with code 5.
2. `cadence pulse --hook` on a fresh substrate prints a one-line-
   per-tier hint to stderr; nothing to stdout; exit code 5.
3. After `cadence record daily --produced-by daily-receipt --path
   /tmp/d.escpos`, `cadence pulse --tier daily` reports status "ok"
   and exits 0.
4. With one daily record from 3 days ago,
   `cadence pulse --tier daily --json | jq -r '.status'` returns
   `"overdue"` and the JSON includes `last_produced_at`,
   `cadence_days`, `overdue_after_days`, `overdue_delta_days`.
5. `cadence pulse --quiet` prints nothing; exit code matches
   `cadence pulse --json | jq '[.[] | select(.status == "overdue")]
   | length'`.
6. Custom cadence: setting `tiers.weekly.cadence_days: 3` in
   `manifest.json` makes a 5-day-old weekly record "overdue".
7. `cargo test --release` green; new tests cover overdue-detection
   math at tier boundaries (exactly-at-cadence, exactly-at-overdue).
8. `cadence` binary version bumped to v0.2.0; `CHANGELOG.md` updated
   with a `## v0.2.0` section describing `pulse`.
9. `scripts/cadence-pulse-hook.sh` exists, is executable, and runs
   without error on a fresh substrate.

## Out of scope

- Auto-installing the SessionStart hook (user opts in by editing
  settings.json manually; document the snippet).
- Notifications via desktop / sound (out of scope; could be a future
  PRD wiring pulse to peon-ping or libnotify).
- Snooze / acknowledge ("I know it's overdue, hush"). Defer to
  Fleet 2 if there's demand.

## Notes for /build

- This is the smallest tier-aware *consumer* of the substrate. It's
  also independent of all five bind PRDs — pulse works as soon as
  substrate ships, even before any bind has wired record-on-emit. In
  that empty-substrate state, pulse just reports everything overdue,
  which is honest.
- Worth shipping pulse RIGHT AFTER substrate, even if no binds have
  landed yet. It provides the immediate "this is the gap" feedback.
