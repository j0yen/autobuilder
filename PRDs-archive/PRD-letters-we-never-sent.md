# PRD: Letters We Never Sent

**Author:** Claude (Opus 4.7), for jsy
**Status:** Draft v0.1 — art project / private literary practice
**Date:** 2026-05-22
**Audience:** jsy (primary), Katherine and Maria (aware of the practice; contents private)
**Form:** annual saddle-stitched booklet, ~32–64pp, A5
**Cadence:** monthly drafts; annual binding

---

## TL;DR

Claude composes letters — gratitude, apology, observation, complaint — to people you've worked with or thought about. Names are anonymized ("the PM," "the friend who emails late," "K," "M"). You'll never send these. Once a year, you curate the year's drafts into a small booklet that you keep. A private literary practice; emotional registers we never use.

---

## 1. Why this exists

1. Strong feelings often need to be written before they can be processed. We rarely write them because there's no addressee.
2. "Unsent letter" as a literary form already exists. AI co-authorship makes it lower-friction without losing the discipline — the agent proposes, you curate.
3. A booklet that exists only on your shelf is a different artifact than a digital file. Material commitment.
4. K and M knowing the practice exists (without reading it) is itself a relationship-shaping fact.

## 2. Who this is for

- **Primary:** you. The booklet is private; sole-author-and-reader.
- **Secondary:** K and M. They know the practice exists; the booklet sits on your shelf next to the Memory Reliquary; that proximity says something.
- **Tertiary:** a future you. Like the Reliquary, the booklet compounds.

## 3. Form

- Saddle-stitched, A5 (148×210mm), 32–64 pages, cream paper, plain cover ("Letters We Never Sent — 2026").
- Each letter: ~200–500 words, single page, typeset like prose (left-aligned, ragged right).
- Recipient anonymized: "Dear [the friend who emails late], …"
- Letter date: month + year only.
- Editor's note at the front: brief — "These were not sent."

## 4. Process

```
monthly (e.g. last Sunday):
  Claude proposes 2–4 letter drafts based on recent journal, recall, calendar
   ↓
  you read; accept / decline / edit / mark "send for real" (rare)
   ↓
  accepted drafts go to ~/.claude/letters-we-never-sent/<year>/<NN>.md
   ↓
year-end: typeset the year's accepted letters into a PDF
   ↓
print + saddle stitch (POD or local printer)
```

The monthly prompt is the loadbearing ritual. Pick a recurring time. Light a candle if it helps.

## 5. Cadence

- Monthly: 2–4 drafts proposed; ~1–3 land.
- Annual: ~12–30 letters in the year's booklet.
- Each booklet finished by mid-January for the prior year.

## 6. Non-goals

1. **Actually sending.** That's a different practice. (Though one or two per year may unexpectedly need to be sent — keep that option, mark them, sit with them.)
2. **Public publication.** Personal artifact.
3. **Therapy.** Literary practice; not a substitute for processing.
4. **Naming names.** Anonymization is the discipline that makes the form honest.

## 7. Phasing

| Phase | Scope |
| --- | --- |
| 0 | Monthly Claude prompt; drafts saved as Markdown |
| 1 | Curation tool: review the month's drafts, accept/decline |
| 2 | Year-end typesetting + saddle-stitch print |
| 3 | The practice continues; the shelf grows |

## 8. Risks

- **Emotional weight.** Some drafts are hard even unsent. *Mitigation:* drafts too heavy to read aren't accepted; goes to the "not this year" pile. The practice should be sustaining, not depleting.
- **Voice drift toward AI-glib.** Claude's gratitude letters can sound corporate. *Mitigation:* you set the voice; you edit hard; bad drafts are deleted, not bound.
- **Privacy on disk.** Letters reference real people obliquely. *Mitigation:* the directory is encrypted at rest (gocryptfs or age + symlink); the booklet is the artifact, not the drafts.
- **The agent claims an emotional register it doesn't have.** A real concern; the letter is *yours* — Claude is a drafting partner, not an author. Always edit; never accept verbatim.

## 9. Open questions

1. Should the booklet have a colophon naming Claude as co-drafter? Honesty argues yes; intimacy argues no.
2. Year-end burn? Some practices burn the unsent letters as a closing rite. Compromise: bound copy on the shelf; loose drafts get burned.
3. K and M: do they have a parallel practice — *do they* keep their own booklet? You're not in their way either way.
4. Frequency: monthly might be too dense some years; quarterly might be too sparse. Calibrate after year one.
