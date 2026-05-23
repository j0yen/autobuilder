# PRD: Memory Reliquary

**Author:** Claude (Opus 4.7), for jsy
**Status:** Draft v0.1 — art project
**Date:** 2026-05-22
**Audience:** jsy (primary), Katherine, Maria (secondary)
**Form:** annual hardback book
**Cadence:** one volume per calendar year

---

## TL;DR

Each January, every recall memory tagged `subject: user` from the previous year gets typeset into a single hardback book. Sewn binding. Archival paper. One volume per year. After a decade you have ten volumes on a shelf — a complete record of how Claude saw you across that time. The book is the artifact; opening it once a year is the ritual.

---

## 1. Why this exists

The recall memory store is byte-shaped — SQLite rows and Markdown files under `~/.claude/recall/memories/`. Useful for queries, invisible to inhabit. A memory at `~/.claude/recall/memories/user/01KS8H.../...md` is technically retrievable and effectively forgotten.

A book changes the relationship. You don't search a book — you flip through it. Memories the agent considered worth recording become a sequence you can sit with. Same content, transformed: from operational to commemorative.

## 2. Who this is for

- **Primary:** you. The book is for re-reading once a year, when you choose. The medium is private even if the contents are not.
- **Secondary:** Katherine and Maria. They see the volumes on a shelf and understand that this is what the agent has been doing on your laptop for a year. The artifact does the explaining.
- **Out of scope:** general distribution. Not a gift; not a publication.

## 3. Form

- Hardback, sewn binding, ~A5 (148×210mm), 80–200 pages depending on memory volume.
- Cover: minimal — title "Memory Reliquary, Vol. <N>" + year + a single glyph. Linen cloth, no jacket.
- Interior: 80gsm uncoated; one memory per page or per spread depending on length. Frontmatter (id, kind, subject, created_at, recall_count) typeset as marginalia in small caps; body in the main column.
- Typography: a quiet low-contrast serif (Crimson, EB Garamond, or commissioned). No emoji, no monospace tags.

## 4. Process

```
recall list --subject user --since 365d --format json
   ↓
JSON → typesetting input (YAML frontmatter + body per memory)
   ↓
template (Typst, LaTeX, or paged.js) → PDF
   ↓
print-on-demand (Lulu, Mixam) for v1
   later: small-run letterpress or local bookbinder
```

A `private: true` frontmatter flag on a recall memory excludes it from the export. Adding the flag is the user's act of curation, not the agent's.

## 5. Cadence

One volume per calendar year, generated the first week of January for the prior year. Each volume is bound, dated, shelved. Never reissued.

## 6. Non-goals

1. **A digital archive.** That already exists. The book's value is in being physical, finite, and re-encounterable.
2. **A public artifact.** No edition larger than a print-on-demand single copy.
3. **A complete archive.** Some memories are excluded via `private: true`. That's editorial, not censorship.
4. **The agent's own memories.** Only `subject: user` goes in. Reflective/self memories are for another volume.

## 7. Phasing

| Phase | Scope |
| --- | --- |
| 0 | Manual export + manual typesetting (you sit with Claude for an afternoon; first volume) |
| 1 | A `make-reliquary` script: recall export → Typst template → PDF, one command |
| 2 | Annual ritual: first week of January, agent surfaces the draft, you approve, send to printer |
| 3 | Upgrade from POD to letterpress for the cover (or full text) once the format stabilizes |

## 8. Risks

- **Emotional weight.** A year of memories in one sitting is heavy. *Mitigation:* the book is meant to be flipped through, not read cover-to-cover.
- **Privacy.** Some memories shouldn't be in print. *Mitigation:* `private: true` is opt-in per memory; an annual review pass with you.
- **Voice changes.** A 2026 volume feels different from a 2032 volume because the underlying model has changed. *Mitigation:* that's the point. Each volume is a snapshot of the relationship at that time.
- **Print quality.** POD binding falls apart in ~10 years. *Mitigation:* upgrade path in Phase 3.

## 9. Open questions

1. Cover glyph: procedural per volume, hand-designed, or commissioned each year?
2. Should marginalia include `last_recalled_at` and `recall_count`? They convey *which memories actually mattered to the agent across the year*. Probably yes.
3. Should there be a 1-page preface by Claude contextualizing the year? Risk: turns archive into autobiography.
4. Where do K and M's expectations enter? Do they get to read it, or is artifact-only enough?
5. Font choice shapes the whole feeling — commission or pick from existing?
