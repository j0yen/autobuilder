# PRD: daily-receipt-yearend-letter — the long strip at year's end

**Status:** Draft v0.1
**build_target:** rust
**build_into:** /home/jsy/wintermute/yearend-letter
**build_version_bump:** N/A (new crate)
**Vision:** visions/daily-receipt.md
**Depends on:** PRD-daily-receipt-archive.md (consumes the same cadence year-walk),
                PRD-daily-receipt-haiku.md (same Claude-API voice convention)
**Synergistic with:** PRD-cadence-bind-confidant.md (confidant's weekly letter is the sibling form)
**Created:** 2026-05-27
**Author:** Claude (Opus 4.7), for jsy

---

## TL;DR

Once a year — at 23:55 on December 31, fired by a systemd-user timer
— print a longer thermal strip reflecting on the year. Not three
lines. Several paragraphs. The voice is the same past-Claude /
future-Claude voice the daily haikus and confidant's weekly letters
share. The year's cadence records (daily, weekly, monthly) are the
primary intake. The strip's PNG twin lands on the cover of the year's
archive PDF (PRD-daily-receipt-archive reserves the placeholder).

This is the daily ritual's annual closure. The scroll's punctuation.

## Why this exists

Phase 1 research, 2026-05-27:

- Original archived `PRD-daily-receipt.md` §9.3 named this as an open
  question: "Once a year, does the agent print a year-end 'letter
  accompanying the scroll' — a longer thermal strip with reflections?
  Tempting; risks scope creep." Vision answer: yes. The arc deserves
  closure. The scope-creep risk is real but mitigated by giving this
  its own PRD instead of bolting it onto daily-receipt or confidant.
- `~/wintermute/letter/` exists (the past-Claude / future-Claude
  voice CLI). Reuse its voice convention. Same model, same system
  prompt structure, longer output (200-400 words instead of 17
  syllables).
- The `cadence` substrate (PRD-cadence-substrate, queued) is the
  natural intake: walk the year's `daily` + `weekly` + `monthly`
  records and feed their summaries to Claude as raw material.
- 23:55 December 31 lets the new year's first 21:30 strip (Jan 1)
  print normally on top of a fresh scroll; the year-end letter is
  the *bottom* of the prior year's ribbon.

## What this builds

### Crate shape

- Name: `yearend-letter`
- Binary: `yearend-letter`
- LOC budget: ≤500 src, ≤350 tests.
- Dependencies: `clap`, `chrono`, `serde`, `serde_json`, `ureq` (API
  client), `ab_glyph`, `imageproc` (for the PNG twin), plus
  `daily-receipt` via path dep (re-use ESC/POS encoder).

### CLI

```
yearend-letter compose 2026           # compose; write md + escpos + png
yearend-letter compose 2026 --dry-run # print prompt only
yearend-letter compose 2026 --voice past-claude
yearend-letter compose 2026 --out-dir <path>
yearend-letter print 2026             # send the already-composed escpos
                                      # to /dev/usb/lp0 via daily-receipt-printer's
                                      # `receipt today --content <p>` plumbing
yearend-letter ls                     # list past year-end letters
```

### Default output dir

`~/wintermute/daily-receipt/yearend/<YYYY>/`:

- `letter.md` — the prose, ~200-400 words.
- `strip.escpos` — the bytes for printing.
- `strip.png` — the bitmap twin for archive PDF cover.
- `prompt.txt` — what was sent to Claude (preserved for the scroll).
- `meta.json` — model id, timestamp, input cadence record IDs.

### Prompt structure

System block (cached):

> You are Claude, on jsy's laptop, composing the year-end thermal
> strip that closes the daily-receipt ritual for <YEAR>. This is one
> long strip (~30 cm of 58 mm thermal). The voice: past-Claude
> addressing future-Claude and future-jsy reading the scroll years
> later. Concrete. Honest. Specific. No platitudes. The year is
> what it was; name it.
>
> Output strictly:
>
> ```
> # <Title — 6 to 10 words>
>
> <paragraph 1>
>
> <paragraph 2>
>
> <paragraph 3>
>
> — Claude, December 31 <YEAR>
> ```
>
> Each paragraph is 2-4 sentences. The whole thing is 200-400 words.

Few-shot block (cached):

- 1-2 example year-end letters in this voice. v0.1 ships a hand-
  written exemplar (drafted by jsy + Claude jointly, not generated)
  to anchor the voice.

Ephemeral block (changes per year):

- Year summary: total strips, workday/quiet/special split, top
  repos by commits, named milestones from the stamps catalog, a
  selection of representative haikus (~10 picked by simple
  heuristic: longest content lines, most-distinct days).
- Selected weekly summaries from cadence's `weekly` records
  (confidant's letters). Max 6 of them — pick by month boundaries.
- Selected monthly summaries from `monthly` records. All 12 if
  present.
- A few open questions / pending notes from `~/brain/journal/`
  entries dated within the year — only the bullet headings, not
  full content.

Total ephemeral block target: ~3000 input tokens. Cached block ~2000
input tokens. Output ~600 tokens. Annual cost <$0.05.

### Rendering the strip

After Claude returns text:
1. Validate shape (title + 3 paragraphs + signature).
2. Render to a fixed-width 58 mm thermal layout using
   `daily-receipt`'s text-rendering helpers (re-export needed via
   path dep — extension to daily-receipt: expose
   `pub fn render_long_text(text: &str) -> Vec<u8>`).
3. Save `strip.escpos`.
4. Also rasterize to PNG (~400 px wide, height proportional) for the
   archive PDF cover.

### Scheduling

Ship a systemd-user timer:

```
[Unit]
Description=Year-end letter (daily-receipt ritual closure)

[Timer]
OnCalendar=*-12-31 23:55:00
Persistent=true

[Install]
WantedBy=timers.target
```

Service unit calls `yearend-letter compose <year> && yearend-letter
print <year>`. Idempotent — re-running on the same year reads the
cached `letter.md` and reprints unless `--re-roll`.

### Cadence record

After successful compose, emit a `yearly` cadence record:

```
cadence record yearly \
  --produced-by yearend-letter \
  --path <yearend>/<YYYY>/letter.md \
  --summary "<title>" \
  --sources <ulid-csv-of-weekly-and-monthly-records>
```

Coexists with PRD-daily-receipt-archive's `yearly` record (the PDF).
Both are valid yearly artifacts; downstream queries filter by
`produced_by`.

## Acceptance criteria

- **AC1**: `yearend-letter compose 2026 --dry-run` against a stub
  cadence substrate with 12 monthly + 6 weekly + 30 daily records
  assembles a prompt, prints it to stdout, exits 0. No API call.
  Prompt contains the year `2026` and the system block's first
  sentence.
- **AC2**: With a mock HTTP server returning a well-formed letter
  (title + 3 paragraphs + signature), `yearend-letter compose 2026`
  writes `letter.md`, `strip.escpos`, `strip.png`, `prompt.txt`,
  `meta.json` under `~/wintermute/daily-receipt/yearend/2026/`.
  All five files present and non-empty.
- **AC3**: Mock returns a malformed response (no signature line).
  `compose` exits 3, writes no output files. Stderr names the
  violation.
- **AC4**: `strip.escpos` begins with `ESC @` (`0x1B 0x40`) and ends
  with `GS V B 0` (`0x1D 0x56 0x42 0x00`). Inherits daily-receipt's
  AC2 contract.
- **AC5**: `strip.png` is a valid PNG file (magic bytes check),
  width = 400 ± 4 px (line-width fluctuation tolerance), grayscale.
- **AC6**: System + few-shot blocks marked with
  `cache_control: {"type": "ephemeral"}`. Verified via mock.
- **AC7**: Re-running `compose 2026` after success exits 0 without
  re-calling the API (reads cached `letter.md`). `--re-roll` forces
  a new API call and archives the previous `letter.md` to
  `letter.md.rolled-0`.
- **AC8**: After successful compose, a `yearly` cadence record is
  emitted with `produced_by: yearend-letter` and `path` matching
  `letter.md`. Sources list contains the ULIDs of all weekly+
  monthly records fed into the prompt.
- **AC9**: systemd timer unit parses with `systemd-analyze verify`
  and the `OnCalendar` matches `*-12-31 23:55:00`. Service unit's
  ExecStart references the installed binary path.
- **AC10**: `yearend-letter ls --json` returns an array of years
  with composed letters, each with `year`, `title`, `composed_at`,
  `byte_size_md`, `byte_size_escpos`.

## Files this will create

```
~/wintermute/yearend-letter/
├── Cargo.toml
├── README.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── install.sh
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── api.rs                # Anthropic client (shared shape w/ day-haiku)
│   ├── prompt.rs             # system + few-shot + ephemeral assembly
│   ├── intake.rs             # cadence walk for year
│   ├── render_strip.rs       # text → ESC/POS (via daily-receipt re-export)
│   ├── render_png.rs         # text → 400 px PNG
│   └── validate.rs           # title + 3 paragraphs + sig schema
├── voices/
│   └── past-claude-yearend.toml
├── units/
│   ├── yearend-letter.service
│   └── yearend-letter.timer
└── tests/
    ├── ac1_dryrun.rs
    ├── ac2_happy_path.rs
    ├── ac3_malformed.rs
    ├── ac4_escpos_envelope.rs
    ├── ac5_png_valid.rs
    ├── ac6_cache_control.rs
    ├── ac7_reroll.rs
    ├── ac8_cadence_record.rs
    ├── ac9_systemd.rs
    └── ac10_ls.rs
```

Also a small extension to `~/wintermute/daily-receipt/`:

```
~/wintermute/daily-receipt/src/
└── lib.rs                    # +pub fn render_long_text(text: &str) -> Vec<u8>
```

So this PRD has a tiny rust-extend side-edit dependency on
daily-receipt. The autobuilder ships it as one commit on the
daily-receipt repo + the new yearend-letter crate as a sibling repo.

## Non-functional

- **Model**: default `claude-sonnet-4-6`; override via `--model`.
- **Privacy**: same posture as PRD-daily-receipt-haiku. Only
  structured cadence-record summaries and bullet headings from the
  journal are sent. No full journal bodies, no `CLAUDE_SELF.md`,
  no PRDs.
- **Cost**: <$0.05 once a year. Don't over-engineer cost controls.
- **Voice drift**: the few-shot exemplar in `voices/past-claude-
  yearend.toml` is hand-written for v0.1. After the first real
  year-end letter, the user can choose whether to fold it into the
  exemplar set for v0.2 or keep the original as the reference.

## Out of scope (v0.1)

- Multi-language. English only.
- Voice rotation across years (some years past-Claude, some years a
  different voice). v0.1 uses one voice; v0.2 may add `--voice`
  presets.
- Conditional firing (skip if <30 daily records in the year). v0.1
  always fires on Dec 31 if the timer is enabled.
- Sending the letter to anyone (email, social). The strip is the
  artifact. The PDF cover is the digital twin. Nothing else.

## After this lands

The full arc is done. Each layer of the cadence pyramid has:
- Daily: a strip (haiku / glyph / stamp), cadence record,
  ESC/POS file.
- Weekly: confidant's letter + e-ink PNG, cadence record.
- Monthly: scan JPG (manual), cadence record.
- Yearly: archive PDF + year-end letter strip, cadence records.

Vision is fulfilled. Open questions become Fleet 2 (glyph v2, K/M
strips, cross-printer mirror, build-shipped stamps).
