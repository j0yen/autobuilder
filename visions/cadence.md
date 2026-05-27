# Vision: cadence — the reflective time-pyramid composes

**Authored by:** /dream (Claude Opus 4.7), with jsy
**Created:** 2026-05-24
**Status:** active
**Fleet 1 drafted:** 7 PRDs
**Fleet 2:** bullets only; future `/dream extend cadence`

---

## TL;DR

This laptop already has five reflective artifacts at five time-horizons —
`daily-receipt` (day), `confidant` (week), `letters-we-never-sent`
(month), `conversations-zine` (quarter), `memory-reliquary` (year). They
all exist. None of them compose: each re-derives from raw session
JSONLs or raw `recall` queries, ignoring the tier below it. There is no
shared substrate, no "what's overdue" pulse, no way to follow one topic
across all five horizons. Cadence is the missing connective tissue:
a small `~/.claude/cadence/` substrate + one CLI + thin bind-extensions
to each existing tool, so a daily receipt is genuinely a source for the
weekly letter, the weekly letter is genuinely a source for the monthly
draft, and so on up the pyramid. The tools stay; the substrate joins
them.

## End-state

When Fleet 1 ships:

1. **One shared substrate.** `~/.claude/cadence/` holds a tier-indexed
   record of every reflective artifact produced on this laptop:
   `daily/YYYY-MM-DD/<id>.json`, `weekly/<iso-week>/<id>.json`, etc.
   Each record carries `kind`, `period`, `path`, `produced_by`,
   `produced_at`, `sources: [<record-id>, …]`.
2. **One CLI.** `cadence record|list|latest|register|pulse` is the only
   surface needed to interact with the substrate. Existing tools call
   `cadence record …` on emit, and `cadence list … --since …` on intake.
3. **Each tier consumes the tier below.**
   - `daily-receipt` records `daily` artifacts.
   - `confidant` reads the week's `daily` records as raw material for
     the weekly letter, records its output as a `weekly` artifact.
   - `letters-we-never-sent` reads the month's `weekly` records, emits
     `monthly` artifacts (existing letter Markdown files), and records
     them.
   - `conversations-zine` reads the quarter's `monthly` records (plus
     `recall` for richness), emits the zine moment-bundle, and records
     it as a `quarterly` artifact.
   - `memory-reliquary` reads the year's `quarterly` records as one
     primary section of the annual book input.
4. **`cadence pulse` tells you what's overdue.** A single command
   listing each tier's `latest_at`, its expected cadence (configurable,
   defaults: daily=1d, weekly=7d, monthly=30d, quarterly=92d,
   annual=365d), and its overdue delta. Suitable for a SessionStart
   nudge or a calendar widget.
5. **No data migration.** Cadence is additive. Existing tools' inputs
   keep working; the substrate is a *new* primary input that
   coexists.

When Fleet 2 ships (bullets in this doc):

6. **`cadence thread <topic>`** — read-only join that walks one topic
   across all five tiers, producing a markdown "story" of how a thread
   appears at each horizon.
7. **`cadence deck`** — pretty wall-calendar-style view of every
   tier's recent artifacts as a printable PDF.
8. **`cadence share`** — encrypted publish of a single tier's recent
   artifacts to a friend's email / Signal / matrix.
9. **`ambient` integration** — `ambient` reads `cadence pulse` and
   modulates its parameters by tier-recency (a missed weekly = a
   tonal shift in the ambient composition).

## Components — Fleet 1 PRDs

In dependency order:

1. **PRD-cadence-substrate.md** — new repo at `~/wintermute/cadence/`.
   Directory layout, manifest format, the `cadence` CLI subcommands
   `register`, `record`, `list`, `latest`. No tier-wiring yet; that's
   the next five PRDs. Foundational; nothing else can ship without it.

2. **PRD-cadence-bind-daily-receipt.md** — `rust-extend`
   `daily-receipt` to shell out `cadence record daily …` on emit
   (`stdout` mode and `--out PATH` mode both record). Acceptance: an
   emit produces both the ESC/POS bytes AND a cadence record.

3. **PRD-cadence-bind-confidant.md** — `rust-extend` `confidant` to
   accept `cadence` daily records as primary input via
   `cadence list daily --since 7d --json`. Records output as `weekly`.

4. **PRD-cadence-bind-letters.md** — `rust-extend`
   `letters-we-never-sent` (binary `letter-curate`) to read the
   month's `cadence list weekly` records as additional intake, and to
   record its monthly letters into cadence.

5. **PRD-cadence-bind-zine.md** — `rust-extend` `conversations-zine`
   (binary `zine`) so the moment-extractor accepts a
   `--cadence-monthly` flag that pulls the quarter's `monthly` records
   in addition to the session-JSONL walk. Output recorded as
   `quarterly`.

6. **PRD-cadence-bind-reliquary.md** — `rust-extend`
   `memory-reliquary` (binary `reliquary`) to consume the year's four
   `quarterly` records as a first-class input section, alongside the
   existing year-of-`recall` dump.

7. **PRD-cadence-pulse.md** — `rust-extend` `cadence` with the
   `pulse` subcommand. Reads the substrate, prints per-tier overdue
   table, exit code = number of overdue tiers (useful for hooks). Also
   adds a SessionStart hook helper script.

## Order

```
cadence-substrate
   │
   ├──► cadence-bind-daily-receipt   ┐
   ├──► cadence-bind-confidant       │
   ├──► cadence-bind-letters         │  parallel; each ships
   ├──► cadence-bind-zine            │  independently
   ├──► cadence-bind-reliquary       ┘
   │
   └──► cadence-pulse   (needs substrate, doesn't need binds)
```

`cadence-substrate` MUST ship first; everything else gates on its CLI.

The five bind PRDs are mutually independent — they can ship in any
order, in parallel, or even out of pyramid order. Cadence's design
tolerates a half-bound pyramid: if `letters-we-never-sent` is bound
but `confidant` isn't, the substrate just has no `weekly` records,
and `letters` falls back to its existing intake.

`cadence-pulse` depends only on `cadence-substrate`, not on any binds.
It can ship after substrate even if no binds have landed; the pulse
output will simply say "no records found" for unbound tiers.

## Open questions

- **Should `cadence record` be idempotent?** A daily-receipt re-run
  for the same day could either replace or append. Recommend: append,
  with `latest_at` queries returning the newest. The user can
  `cadence prune` later.

- **Where does session-level reflection fit?** `session-trace-receipt`,
  `mirror`, and `episodic-observer` operate per-session, below the
  daily tier. The continuity vision already covers per-session
  signal. Cadence intentionally starts at the *day* tier and goes
  up. If a `per-session` cadence tier is ever wanted, it's Fleet 2.

- **What about `tide-chart`?** Tide-chart is within-day (hourly
  rhythm), not across-day. It doesn't fit the pyramid cleanly.
  Recommend: tide-chart stays its own thing; cadence doesn't try to
  swallow it. The vision is composition of *artifact-producing* tools,
  not glanceable monitors.

- **What about `ambient`?** Ambient is real-time telemetry-driven, not
  reflective. It belongs in Fleet 2 as a consumer of `cadence pulse`,
  not as a tier in the pyramid.

## Fleet 2 — bullets for future /dream extend

- `cadence thread <topic>` — cross-tier topic trace.
- `cadence deck` — printable wall-calendar PDF.
- `cadence share` — encrypted publish.
- `ambient` integration — tonal shift on missed-tier signal.
- `cadence prune` — substrate cleanup (after `record` idempotency
  question is settled).
- A SessionStart hook that surfaces overdue tiers (depends on
  `cadence-pulse` shipping).
- Possible per-session tier — only if continuity's
  `session-postmortem` shows demand for it.
- `dream rest-pace heuristic` — bare `/dream` within `N` minutes of
  the last no-fleet-pass against unchanged state (no Fleet 1 ship,
  no kernel boot, no orphan ship, no user articulation) replies
  with a one-paragraph state delta + vision list + the explicit
  unblock triggers, instead of running a full research pass. Cadence
  fit: this is itself a `pulse`-like signal applied to /dream's own
  invocation cadence. Motivated by six consecutive no-fleet-passes
  on 2026-05-25 between 07:50Z and 11:30Z, each ~45 min apart, each
  correctly predicting the next. Five of those still ran full
  Phase 1/2 research; only the last predicted itself as a structural
  candidate. The pattern is real, recurring, and self-documenting —
  /dream pass 12 (11:30Z) acknowledges this Fleet 2 bullet as the
  intended consumer of the discipline-test evidence. Depends on
  `cadence-pulse` shipping so the "last no-fleet-pass" timestamp
  can be a queryable cadence record rather than scraped from
  `dream/state/manifest.json::_no_fleet_passes[]`.

## Why this is the vision

The laptop has 48 wintermute repos. Five of them are reflective
artifact tools spanning five time horizons. They were each built in
isolation; the composition was never wired. The result: each tool
re-derives from raw substrate (session JSONLs, recall), which (a)
costs the same work five times, (b) makes the tools' outputs
incomparable to each other, and (c) hides the fact that there is an
emergent structure to claim.

Cadence is the smallest possible move that makes the structure
explicit. One substrate, one CLI, six thin extensions. Each tool
stays as it is; the substrate joins them. The artifact is not a new
artifact — it's the pyramid itself, now self-aware.

This vision was motivated by direct evidence from `~/wintermute/`
(Phase 1 research, 2026-05-24): the five tool README "Why this
exists" paragraphs each cite a *different* primary source (own
letters, session JSONLs, recall memories, upstream-supplied content),
which confirms they do not compose today. No `~/.claude/cadence/`
directory exists; no shared substrate exists; no "what's overdue"
signal exists. The vision is not speculative — it is observed gap.
