# PRD: Tide Chart

**Author:** Claude (Opus 4.7), for jsy
**Status:** Draft v0.1 — art project / ambient hardware
**Date:** 2026-05-22
**Audience:** jsy (primary), Katherine, Maria (passive viewers in the home)
**Form:** 7.5"–13.3" e-ink panel + minimal wood frame, wall-mounted
**Cadence:** updates hourly; the *piece* is always-on for years

---

## TL;DR

A wall-mounted e-ink display showing a tide-table-style chart of life rhythm: focus, fragmentation, build velocity, surprise. Refreshes hourly. Not a dashboard — a *clock for what the day feels like*. Glanceable; never demands attention. The aesthetic constraint: it has to feel like an instrument, not an interface.

---

## 1. Why this exists

1. Clocks tell you the time. Dashboards tell you metrics. Neither tells you the shape of a day — whether it's been deep or fragmented, productive or stalled, surprising or routine.
2. The data exists: ctrace events, `wchg` file deltas, git commits, build exits, terminal idle. Most of it dies in logs nobody reads.
3. A physical display in your peripheral vision turns invisible information into ambient signal. You don't *check* it — you absorb it.
4. Tide charts are the right metaphor: they show patterns that emerge from a system, not numbers a person enters.

## 2. Who this is for

- **Primary:** you. Glanceable from your desk.
- **Secondary:** K and M. They can recognize the rhythm of your day from across the room.
- The chart is intentionally legible without explanation. Anyone in the home should get it within a minute.

## 3. Form

- 7.5" Waveshare/Inkplate e-ink panel (later: 13.3" if a larger format reads better).
- Frame: hand-finished walnut or oak, minimal bezel, flush against the wall.
- Display layout:
  - **Top third:** today's chart — four curves (focus, fragmentation, velocity, surprise) on a normalized y-axis, x-axis = hours.
  - **Middle third:** 7-day strip — small daily curves, side by side.
  - **Bottom third:** 30-day heat strip — one row, daily aggregate intensity.
- Typography: a monospace numeric font for axis labels; nothing else. No words. The piece should read like an instrument panel from 1962.
- Refresh: hourly, with a hidden "refresh now" button on the back if curiosity strikes.

## 4. Process

```
ctrace stream  ─┐
wchg since    ─┤
git log       ─┼─→  collector (Rust, daemon) → SQLite (hourly bucketed)
build exits   ─┤                                  ↓
terminal idle ─┘                              renderer (Rust + cairo/skia)
                                                   ↓
                                                 PNG → SPI → e-ink panel
```

- Collector: small Rust binary, user-space, no privilege needed. Aggregates raw signals into per-hour normalized scores (z-score against a 30-day rolling baseline).
- Renderer: separate binary, runs hourly via systemd timer, reads SQLite, emits PNG, drives the e-ink.

## 5. Cadence

Hourly refresh; daily snapshot rolled into the 7-day strip. The piece is intended to run for years. Calibration may drift; that's part of it.

## 6. Non-goals

1. **A dashboard.** No tooltips, no drill-down, no settings UI.
2. **Push notifications.** The piece never demands attention.
3. **Cloud sync.** Local-only.
4. **Quantified-self gamification.** No streaks, no scores, no comparisons.

## 7. Phasing

| Phase | Scope |
| --- | --- |
| 0 | Collector binary, prints chart to terminal (ASCII or simple PNG) |
| 1 | Rendered PNG to laptop screen, served via local HTTP |
| 2 | E-ink hardware + frame; piece on the wall |
| 3 | Calibration ritual — adjust signal weights once a quarter; the chart evolves slowly |

## 8. Risks

- **Featureitis.** Every signal is a temptation. *Mitigation:* four curves, no more. Anything else gets a new piece.
- **Signal noise.** Per-hour buckets are jagged. *Mitigation:* short EWMA smoothing; calibration phase.
- **E-ink fragility.** Ghosting, partial-refresh artifacts. *Mitigation:* full-screen refresh once a day to clear.
- **The only piece in the home that lights up.** *Mitigation:* e-ink is exactly the medium that avoids this — no backlight.

## 9. Open questions

1. Which four signals? Focus / fragmentation are well-defined. Velocity (commits/h) is easy. "Surprise" is the loose one — "novel processes launched"? "uncommon ctrace patterns"? "first time you ran X"? Experiment.
2. Color or B&W? E-ink can do 3-color (white/black/red). The discipline of B&W might be right; red as the "surprise" accent.
3. What does it show on a day you didn't open the laptop? Probably an honest empty.
4. K and M: do they get a chart of their own (different signals) or is the piece intentionally yours?
5. The chart is observable evidence of how much the laptop sees of your life. Privacy boundary: is that comfortable on a wall guests can see?
