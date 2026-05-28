# PRD: daily-receipt-stamps — special-day stamp catalog + lookup

**Status:** Draft v0.1
**build_target:** rust-cli
**build_into:** /home/jsy/wintermute/day-stamps
**build_version_bump:** N/A (new crate)
**Vision:** visions/daily-receipt.md
**Depends on:** none (stand-alone; read by day-summarize and day-haiku)
**Synergistic with:** PRD-daily-receipt-summarize.md (summarize reads the same catalog)
**Created:** 2026-05-27
**Author:** Claude (Opus 4.7), for jsy

---

## TL;DR

Special days deserve a stamp instead of a haiku or a glyph. Birthdays,
anniversaries, the day a PRD shipped, the day the printer arrived.
This PRD ships the stamp catalog format, a manage-stamps CLI
(`day-stamp add | list | render | which`), and the lookup convention
that day-summarize and day-haiku already cite. A stamp is a tiny
piece of pre-rendered ESC/POS bytes (or a glyph spec) plus metadata,
keyed by date.

## Why this exists

Phase 1 research, 2026-05-27:

- The original archived `PRD-daily-receipt.md` §3 named "Stamp
  (special day): hand-curated for birthdays, anniversaries,
  build-shipped milestones" as the third day-type but did not
  specify the storage format or selection rules.
- `daily-receipt/src/classifier.rs` already returns `special` when
  `summary.special_stamp_id` is set — that field is the integration
  point. This PRD writes the producer for that field.
- Today (2026-05-27) is itself a candidate special day: "the
  printer arrived." If the catalog had existed, today's strip would
  bear a "printer-arrives" stamp instead of a haiku. v0.1 ships the
  catalog with a seeded set of historical stamps so the first run
  has texture.
- Stamps double as a `/build`-side gossip hook: PRD-daily-receipt
  vision Fleet 2 bullet "build-shipped milestones as special days"
  is enabled by this PRD.

## What this builds

### Crate shape

- Name: `day-stamps`
- Binary: `day-stamp`
- LOC budget: ≤350 src, ≤300 tests.
- Dependencies: `clap`, `chrono`, `serde`, `serde_json`, `toml`.

### Stamp catalog location

`$XDG_CONFIG_HOME/daily-receipt/stamps/` with fallback
`~/.claude/daily-receipt/stamps/`. Two file types:

- **Date-specific**: `<YYYY-MM-DD>.json` — fires exactly once.
- **Recurring**: `<MM-DD>.json` — fires every year on that date.

Both lookups happen; date-specific wins.

### `<stamp>.json` schema

```json
{
  "id": "printer-arrives",
  "kind": "milestone",
  "title": "The MASUNG IP1000 lands",
  "subtitle": "2026-05-27",
  "lines": [
    "* * *",
    "The printer is here.",
    "Paper en route.",
    "Daily ritual begins.",
    "* * *"
  ],
  "glyph_seed": null,
  "size_hint": "medium",
  "category": "device-milestone",
  "created_by": "jsy",
  "created_at": "2026-05-27T22:00:00-07:00"
}
```

- `lines`: 1..=12 strings, each ≤40 graphemes. Rendered centered.
- `glyph_seed`: optional u64; if set, a glyph is rendered above
  the lines using daily-receipt's existing 24×24 glyph renderer.
- `size_hint`: `small | medium | large` — printer wrapper uses this
  to choose feed-line counts (small=2, medium=4, large=8).
- `kind`: `birthday | anniversary | milestone | named-day | custom`.
- `category`: free-form for the user; not interpreted by code.

### CLI

```
day-stamp which                 # which stamp (if any) is for today?
day-stamp which 2026-12-25      # which stamp for that date?
day-stamp list                  # list all stamps in the catalog
day-stamp list --kind milestone
day-stamp add --id printer-arrives \
    --title "The MASUNG IP1000 lands" \
    --date 2026-05-27 \
    --line "The printer is here." \
    --line "Paper en route." \
    --line "Daily ritual begins."
day-stamp render <id> --out <path>   # render bytes for daily-receipt
day-stamp render today --out <path>  # render today's stamp (errors if none)
day-stamp seed                       # write a starter catalog (idempotent)
```

### Seed catalog

`day-stamp seed` writes a small starter set:

- `01-01.json` — "New Year" (recurring)
- `12-31.json` — "Year ends" (recurring; year-end-letter PRD will
  override this with its longer strip)
- `2026-05-27.json` — "Printer arrives" (one-shot; the day this PRD
  was written)
- `2026-05-22.json` — "Daily-receipt PRD drafted" (one-shot;
  historical record-keeping for the scroll)

Idempotent: `day-stamp seed` does not overwrite existing files.
The user adds birthdays/anniversaries via `day-stamp add` later.

### Lookup contract for upstream

day-summarize calls (in pseudo-code):
```rust
let stamp = day_stamps::which(today);
summary.special_stamp_id = stamp.map(|s| s.id);
```

day-haiku, before calling Claude:
```rust
if summary.special_stamp_id.is_some() {
    // Special day. Skip haiku composition entirely.
    // The printer wrapper will call day-stamp render instead.
    exit(0);
}
```

The printer wrapper (PRD-daily-receipt-printer) `today` flow gets one
extension: when `summary.special_stamp_id` is set, it calls
`day-stamp render <id> --out <path>` instead of `daily-receipt render`.
That keeps the byte-stable invariants of stamps separate from the
generative paths.

## Acceptance criteria

- **AC1**: `day-stamp seed` against an empty
  `$XDG_CONFIG_HOME/daily-receipt/stamps/` writes 4 starter stamp
  files. Re-running `seed` does not overwrite; exits 0.
- **AC2**: `day-stamp which 2026-05-27` prints
  `printer-arrives` to stdout. `which 2026-05-28` (no stamp)
  prints nothing and exits 0.
- **AC3**: `day-stamp which 2026-01-01` matches the recurring
  `01-01.json` stamp. Verified for at least two distinct years
  (2026, 2030) in the same test.
- **AC4**: Date-specific stamp file shadows the recurring file
  for the same MM-DD: when both `2026-12-31.json` and `12-31.json`
  exist, `which 2026-12-31` returns the date-specific one.
- **AC5**: `day-stamp add --id foo --title T --date 2026-05-30
  --line "Hello"` creates `2026-05-30.json` with the expected
  shape; running again with the same `--id` and date errors out
  with exit 4 (do not silently overwrite).
- **AC6**: `day-stamp render printer-arrives --out /tmp/p.escpos`
  writes a non-empty ESC/POS byte stream that begins with ESC '@'
  (`0x1B 0x40`), contains the stamp title bytes verbatim, ends
  with feed-and-cut bytes (`0x1D 0x56 0x42 0x00`). Daily-receipt
  AC2 compatibility.
- **AC7**: `day-stamp render` with no `lines` and `glyph_seed: 42`
  renders a 24×24 raster glyph (reuses daily-receipt's renderer
  via dep, or shells out to `daily-receipt render` with a fake
  summary — either is acceptable; pick the simpler path).
- **AC8**: `day-stamp list --json` prints a JSON array of all
  stamps with full metadata; sortable by `created_at`.
- **AC9**: Each stamp file's `lines` violating the schema (≥13
  lines, or line >40 graphemes) is rejected at `add`-time with
  exit 5; `render`-time of an existing-but-malformed file also
  exits 5 with the offending line numbered in stderr.

## Files this will create

```
~/wintermute/day-stamps/
├── Cargo.toml
├── README.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── install.sh
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── catalog.rs       # file IO + lookup
│   ├── render.rs        # bytes assembly
│   └── validate.rs
├── seeds/
│   ├── 01-01.json
│   ├── 12-31.json
│   ├── 2026-05-22.json
│   └── 2026-05-27.json
└── tests/
    ├── ac1_seed_idempotent.rs
    ├── ac2_which.rs
    ├── ac3_recurring.rs
    ├── ac4_shadow.rs
    ├── ac5_add_collision.rs
    ├── ac6_render_bytes.rs
    ├── ac7_glyph_only.rs
    ├── ac8_list_json.rs
    └── ac9_schema_violation.rs
```

## Non-functional

- No network.
- No `unsafe`.
- Atomic writes (`tempfile + rename`) for `add`.
- All paths via XDG with `~/.claude` fallback.

## Out of scope (v0.1)

- Stamp drawing tools — stamps are hand-edited JSON in v0.1.
  v0.2 could ship a TUI or web-based stamp composer.
- Automatic milestone detection — /build writing
  `stamps/<today>.json = {kind: "ship", repo: "..."}` is its own
  follow-on PRD (named in vision doc Fleet 2 bullets).
- Notifications — a stamp firing does not pop a notification.
  The user discovers special days by reading the strip.

## After this lands

The three day-types are all end-to-end:
- workday → day-summarize → day-haiku → receipt
- quiet → day-summarize → (no haiku) → receipt's glyph fallback
- special → day-summarize → day-stamp render → receipt's stamp path

PRD-daily-receipt-archive can begin walking a year of records.
