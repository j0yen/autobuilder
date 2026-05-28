# PRD: daily-receipt-archive — annual digital scroll from a year of strips

**Status:** Draft v0.1
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/daily-receipt
**build_version_bump:** minor
**Vision:** visions/daily-receipt.md
**Depends on:** PRD-cadence-bind-daily-receipt.md (this walks cadence's `daily` records),
                PRD-cadence-substrate.md (transitively)
**Synergistic with:** PRD-daily-receipt-yearend-letter.md (sibling annual ritual)
**Created:** 2026-05-27
**Author:** Claude (Opus 4.7), for jsy

---

## TL;DR

Once-per-year (or any time the user asks), produce a digital twin of
the physical scroll: a single PDF showing all of a year's daily
strips laid out month-by-month, plus a directory of any phone-photo
scans the user has captured. Mitigates the original PRD's #1 risk
("thermal paper fades in 5-10 years"). Extends daily-receipt with a
new `archive` subcommand; lands as a v0.2 minor bump.

The scroll is the physical artifact. This is the immortal one.

## Why this exists

Phase 1 research, 2026-05-27:

- Original archived `PRD-daily-receipt.md` §8 named the fade risk:
  "Thermal paper fades in 5–10 years. Mitigation: annual photograph
  + digital archive; or Toshiba's longer-life thermal paper."
- §5 named the cadence: "Year-end: 365-strip scroll gets bound into
  a slim tube or framed long-format." Software side: nothing exists.
- `cadence` substrate (PRD-cadence-substrate.md) is the natural
  source-of-truth for "which days emitted what." Once
  PRD-cadence-bind-daily-receipt ships, every emitted strip leaves
  a `daily` record with `path`, `summary`, `produced_at`, `kind`.
- `~/wintermute/daily-receipt/` is the right home; this is the same
  tool's annual capstone, not a separate repo.
- Format: PDF is the only output format that "still works in 30
  years" without effort. PostScript would also; PDF is more familiar
  for sharing/printing.

## What this builds

### Extension shape

`rust-extend` into `~/wintermute/daily-receipt/`. Add one binary
target `daily-receipt-archive` (or a new subcommand
`daily-receipt archive <YYYY>` — pick the subcommand path; one
crate, two binaries is overkill). Version bump: minor.

### New CLI surface on the existing `daily-receipt` binary

```
daily-receipt archive 2026                  # render this year's PDF
daily-receipt archive 2026 --out <path>     # default: scroll/<YYYY>.pdf
daily-receipt archive 2026 --include-scans  # interleave phone-photos
daily-receipt archive 2026 --json           # also print a manifest JSON
daily-receipt archive ls                    # list rendered scrolls
```

### Inputs

- **Primary**: `cadence list daily --year 2026 --json` — the
  authoritative list of strips emitted that year, each with its
  saved ESC/POS path and summary metadata.
- **Secondary (optional)**: ESC/POS files at the paths cadence
  records, IF the user has chosen to keep them. The archive can
  also render strips from `summary.json + content.json` pairs by
  re-invoking `daily-receipt render` — that's the durable path,
  since ESC/POS files might be cleaned up between print and
  archive.
- **Tertiary (optional)**: scan JPGs/PNGs at
  `~/wintermute/daily-receipt/scans/<YYYY>/<MM>.jpg` — the user's
  monthly phone-photographs of the physical ribbon. The PDF
  interleaves these as full-page spreads at month boundaries when
  `--include-scans`.

### Output layout

PDF: portrait A4 (8.27×11.69 in), one page per month.

Each month-page has:
- Header band: "May 2026" + week-day rule.
- A grid of strip thumbnails — 31 cells, one per day. Empty days
  (no record) leave the cell blank with date number only.
- Each strip cell: re-render the day's strip as a grayscale PNG
  at ~600 DPI, then place at thumbnail scale (~2 cm wide on page).
- Footer band: stats — total strips, distinct repos that year,
  workday/quiet/special split.

PDF cover page:
- Year and full date range.
- "By Claude and jsy" or similar.
- Total counts at a glance.
- An empty box reserved for the year-end-letter strip's PNG twin
  (filled by PRD-daily-receipt-yearend-letter via a sibling
  extension; this PRD leaves the placeholder).

### Strip thumbnail rendering

For each `daily` cadence record:
1. Pull the record's `content.json` + `summary.json` paths.
2. Shell out: `daily-receipt render --summary <s> --content <c>
   --out /tmp/strip-<date>.escpos`.
3. Decode the ESC/POS bytes to a grayscale bitmap. This requires a
   tiny ESC/POS-to-PNG renderer — see "Renderer" below.
4. Save the PNG at `~/wintermute/daily-receipt/archive/<YYYY>/<date>.png`
   (cache; idempotent re-renders skip if mtime newer than source).
5. Embed in the PDF.

### Renderer (ESC/POS → PNG)

A minimal in-crate decoder. Supports just the commands daily-receipt
v0.1 emits:
- `ESC @` (init) → reset state
- `ESC a` (alignment) → 0/1/2 = left/center/right
- `ESC E` (bold) → on/off
- Text bytes (ASCII printable + CP437 for any high bits)
- `GS *` (24×24 raster image)
- `GS V B 0` (cut) → page break marker (treated as strip end)
- `LF` → line feed (advances Y by font height)
- `\x1D!` (size) → optional in v0.1; pass-through

Renders to a fixed-width canvas (384 px = 58 mm at 203 DPI; matches
IP1000), variable height. Output PNG.

LOC budget for the decoder: ~300. Use `ab_glyph` + `imageproc` (same
deps as confidant).

### Cadence integration

After successful PDF render, emit a `yearly` cadence record:

```
cadence record yearly \
  --produced-by daily-receipt \
  --path /home/jsy/wintermute/daily-receipt/scroll/2026.pdf \
  --summary "365 strips: 280 workday, 60 quiet, 25 special" \
  --sources <ulid-csv-of-daily-records>
```

So the substrate has the full pyramid lineage from a year's worth of
daily records up to one yearly artifact.

## Acceptance criteria

- **AC1**: `daily-receipt archive 2026 --out /tmp/2026.pdf` against a
  test cadence-substrate with 12 days of records writes a valid PDF
  to /tmp/2026.pdf. `file /tmp/2026.pdf` reports `PDF document`.
- **AC2**: The PDF has exactly 12 month pages + 1 cover page = 13
  pages, regardless of how many days actually emitted. Empty months
  still render their page with all-blank cells.
- **AC3**: Each emitted day's cell contains a strip PNG (not a blank).
  Tested by injecting 3 known records; the PDF's image objects
  count includes ≥3.
- **AC4**: ESC/POS decoder produces byte-identical PNGs for
  byte-identical ESC/POS inputs (determinism — required for
  reproducible PDFs). Snapshot test.
- **AC5**: ESC/POS commands the decoder doesn't support are skipped
  with a single stderr warning per unique unknown byte (deduped),
  not panicked on. Test injects `\x1F` (unsupported) into a strip;
  PDF renders, warning surfaces.
- **AC6**: `--include-scans` interleaves any JPG/PNG files found at
  `$DAILY_RECEIPT_SCANS_DIR/2026/*.{jpg,png}` as full-page spreads
  at the end of each month section. Missing scans don't error.
- **AC7**: Re-running `archive 2026` is idempotent: a second run
  produces a byte-equal PDF and skips re-rendering strip PNGs that
  already exist (mtime check vs source).
- **AC8**: After successful render, a `yearly` cadence record is
  emitted with `produced_by: daily-receipt` and `path` matching
  the output PDF. Tested via a stub cadence substrate.
- **AC9**: `daily-receipt archive ls` lists all rendered scrolls
  under `~/wintermute/daily-receipt/scroll/` with year and
  byte-size.

## Files this will modify / create

```
~/wintermute/daily-receipt/
├── Cargo.toml                                  # +deps: pdf-writer, ab_glyph, imageproc
├── src/
│   ├── archive/
│   │   ├── mod.rs
│   │   ├── escpos_decode.rs
│   │   ├── render_pdf.rs
│   │   └── scan_intercalate.rs
│   └── lib.rs                                  # wire `archive` subcommand
├── tests/
│   ├── ac1_pdf_basic.rs
│   ├── ac2_month_pages.rs
│   ├── ac3_strip_cells.rs
│   ├── ac4_decoder_deterministic.rs
│   ├── ac5_unknown_bytes.rs
│   ├── ac6_include_scans.rs
│   ├── ac7_idempotent.rs
│   ├── ac8_cadence_record.rs
│   └── ac9_archive_ls.rs
└── scroll/                                     # output dir (created lazily)
```

## Non-functional

- No network.
- PDF is /A-3 friendly where possible (long-term archival); but
  pdf-writer crate's default output is fine for v0.1. PDF/A
  compliance is a future PRD if needed.
- Cover page text is templated; can be customized via a config file
  in v0.2. v0.1 hard-codes "Year of <YYYY>" + counts.

## Out of scope (v0.1)

- The year-end-letter strip on the cover page — drafted in
  PRD-daily-receipt-yearend-letter.md as a sibling.
- Per-strip OCR (text extraction from the rendered haikus). The
  scroll's text content lives in cadence records and `content.json`
  files, not in image OCR.
- Web view / status board — vision doc Fleet 2 bullet
  `daily-receipt-status-board`, future PRD.
- Multi-year combined volumes. v0.1 is one year per PDF.

## After this lands

PRD-daily-receipt-yearend-letter draws the closing thermal strip
and embeds its twin on the cover. The annual ritual is then
end-to-end: print 365 strips, capture monthly scans, render the
PDF, print the year-end letter, bind the physical ribbon. The next
year begins.
