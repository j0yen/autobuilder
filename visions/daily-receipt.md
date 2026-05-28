# Vision: daily-receipt — the daily strip, the year-end scroll

**Created:** 2026-05-27
**Status:** active
**Seed:** user-prompt — "printer arrived (MASUNG IP1000 58mm, /dev/usb/lp0 live, paper en route); PRD-daily-receipt-printer just queued. Articulate the haiku-composition + year-end-scroll arc downstream of it."

---

## TL;DR

A thermal printer on the desk emits one humble strip per day at a
fixed hour. Workdays get a three-line haiku composed by Claude from
that day's actual signals. Quiet days get a deterministic generative
glyph. Special days get a hand-curated stamp. The strips accumulate
into a ribbon; each year they bind into a scroll, photographed once
for the digital long-tail and accompanied by an annual reflection
strip — a longer thermal artifact that the year-end ritual produces
in the past-Claude / future-Claude voice. Slow. Private. Tactile.
Two scales of compounding: the tiny daily artifact, the annual scroll.

## End-state

When this vision is fulfilled:

- 21:30 every day, a strip slides out of the MASUNG IP1000.
- The strip's content is real — derived from that day's commits,
  ctrace summary, recall hits, journal note. Not a generic fortune.
- The haiku is in Claude's voice, the same voice the letter CLI and
  confidant share. The reader can tell a workday strip apart from a
  filler strip without reading the date.
- Each strip is also a row in `cadence`, so the substrate has the
  full lineage from this day's haiku → this week's confidant letter
  → this month's letters-we-never-sent → this year's scroll.
- December 31 23:55, a longer thermal strip prints: a few paragraphs
  reflecting on the year, bound on top of the scroll. The scroll's
  PDF mirror is rendered and saved.
- The ribbon on the wall is visible from the desk. K and M can see it
  growing. Years later, the scroll is in a tube, the digital archive
  is in a flat directory, the haikus are still readable.

## Components

Already shipped:
- **daily-receipt v0.1** (`~/wintermute/daily-receipt/`) — byte-stable
  ESC/POS encoder, day-type classifier, deterministic glyph renderer.
  All 7 ACs green. Stops at bytes, deliberately.

Already queued (drafted in prior passes):
- **PRD-daily-receipt-printer.md** (this session) — physical wrapper
  for the MASUNG IP1000. `receipt today` + systemd-user timer +
  state file. The bytes finally meet paper.
- **PRD-cadence-bind-daily-receipt.md** — extends daily-receipt to
  `cadence record daily` on every emit so downstream tiers (weekly,
  monthly, annual) can pull the lineage.

This vision drafts (Fleet 1):
1. **PRD-daily-receipt-summarize.md** — Rust binary `day-summarize`
   that gathers the day's signals (ctrace query, git log --since 24h,
   recall list --since 24h, journal entry presence) into the
   `summary.json` shape that `daily-receipt render --summary` expects.
   The missing upstream the original PRD §4 named but never built.
2. **PRD-daily-receipt-haiku.md** — Rust binary `day-haiku` that takes
   today's `summary.json`, calls Claude via the claude-api skill's
   convention with a past-Claude-voice few-shot, and emits the
   `content.json` shape `daily-receipt render --content` expects.
   Includes prompt caching (system + few-shots cached; daily summary
   the only ephemeral block) and a `--re-roll` flag for the veto path.
3. **PRD-daily-receipt-stamps.md** — Rust binary `day-stamp` + stamp
   catalog at `~/.claude/daily-receipt/stamps/<YYYY-MM-DD>.json` (or
   `<MM-DD>.json` for recurring dates: birthdays, anniversaries).
   Day-type classifier extended: when a stamp exists for today,
   day-type is `special` and the stamp ID becomes the content.
4. **PRD-daily-receipt-archive.md** — annual ritual `receipt archive
   <YYYY>` that walks the cadence substrate's `daily` records for
   the year, renders a PDF (one page per month, 30-31 strips per
   page in a calendar grid), and stitches any scan PNGs the user
   placed in `~/wintermute/daily-receipt/scans/<YYYY>/`. Mitigates
   thermal-paper fade by capturing the digital long-tail.
5. **PRD-daily-receipt-yearend-letter.md** — once per year (Dec 31
   23:55 via systemd-user timer, OR manual `receipt yearend`),
   compose a longer thermal strip (~30 cm) reflecting on the year.
   Uses the year's cadence records as primary intake plus the
   letter CLI's voice convention. Renders the strip *and* a PDF
   sibling for the scroll's cover sheet.

## Order

```
                  ┌── PRD-daily-receipt-printer (queued)
daily-receipt v0.1 ┤
   (shipped)      └── PRD-cadence-bind-daily-receipt (queued; needs cadence-substrate)
                          │
                          ▼
                       PRD-daily-receipt-summarize  (independent, ships anytime)
                          │
                          ▼
                       PRD-daily-receipt-haiku      (needs summarize's output shape)
                          │
                       PRD-daily-receipt-stamps     (independent; sibling)
                          │
                          ▼
                       PRD-daily-receipt-archive    (needs cadence-bind-daily-receipt)
                          │
                          ▼
                       PRD-daily-receipt-yearend-letter (needs archive + haiku)
```

Critical path: printer → summarize → haiku gets workdays printing real
content within ~3 ship cycles. Stamps and archive can ship in parallel
after that. Year-end letter is the capstone — it can't ship until at
least one calendar year of cadence records accumulate, but the PRD can
ship early; the timer just sits idle until 2026-12-31.

## Open questions

These stay in this vision doc as bullets for the next /dream pass,
not drafted as PRDs yet:

- **Glyph visual vocabulary v2.** Current implementation is a 24×24
  raster from a u64 seed; the original PRD §9.1 asked whether the
  vocabulary should be hand-drawn primitives, pure noise, or
  bigram-shaped symbols from the day's text. v1 is fine; v2 is a
  legitimate future PRD once we see ~30 quiet-day strips on the wall
  and have a feel for what's missing.
- **K and M strips.** Do K and M get their own daily strip with
  different content (audience-shaped haikus)? Or are they an audience
  for the scroll only? Original PRD §9.2 left this open; still open.
  Would need a second printer or a multi-strip flow.
- **Cross-printer mirroring.** Mirror selected strips to a printer
  at K's desk on milestone days? Original PRD §9.4 — open. Lower
  priority than getting the local arc working.
- **Build-shipped milestones as special days.** /build could
  automatically write a stamp file when a PRD ships
  (`stamps/2026-05-27.json = {kind: "ship", repo: "j0yen/foo"}`).
  Probably an extension of PRD-daily-receipt-stamps, not its own PRD.
- **Re-roll budget.** The original PRD said "veto and re-roll once."
  Is one re-roll the right number? PRD-daily-receipt-haiku picks one
  as the v1 default; revisit after a month of daily prints.
- **Scroll closing ceremony.** When the year-end strip emerges, is
  there a physical ritual (tape it to the top of the ribbon; bind the
  whole thing into a tube)? Out of software scope but worth naming
  so the year-end PRD doesn't quietly skip the human moment.

## Fleet 2 bullets (post Fleet 1)

Sketched here so the next /dream pass can extend without re-scouting:

- **PRD-daily-receipt-photo.md** — once-a-month `receipt photo`
  prompt: phone-photograph the ribbon, save to
  `~/wintermute/daily-receipt/scans/<YYYY>/<MM>.jpg`, register as a
  cadence `monthly` record. Anchors the digital archive cadence.
- **PRD-daily-receipt-redo.md** — `receipt redo <date>` reprints a
  past day's strip from its cached content (lost-strip recovery).
- **PRD-daily-receipt-status-board.md** — a tiny web page rendered
  from cadence records that shows the year's strips at-a-glance,
  for the days you want to look without unrolling the physical tube.

## Pedigree

This vision is the lived-in form of:
- The 2026-05-22 PRD-daily-receipt.md (now archived) — the original
  articulation. Section 3 named the three day-types, §4 named the
  pipeline shape, §9 named the open questions. This vision adopts
  all of that.
- The past-Claude letter that asked for the receipt printer in the
  first place. The printer is here; the ritual can begin.
- The cadence pyramid (`visions/cadence.md`) — daily-receipt is the
  bottom tier. This vision sketches the year-end *closure*, which
  cadence's vision doc names but doesn't draft (cadence focuses on
  composition, not annual ceremony).
