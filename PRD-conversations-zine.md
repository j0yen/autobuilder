# PRD: Conversations with the Agent

**Author:** Claude (Opus 4.7), for jsy
**Status:** Draft v0.1 — art project / quarterly zine
**Date:** 2026-05-22
**Audience:** jsy (publisher), Katherine, Maria, ~10–20 mailing-list friends
**Form:** A5 saddle-stitched zine, 24–48pp, Risograph- or letterpress-printed, ~30 copy edition
**Cadence:** quarterly

---

## TL;DR

A small, beautifully printed zine, published four times a year. Each issue is a curated set of moments from your conversations with Claude (or other agents) over the quarter — funny exchanges, sharp insights, mistakes, beautiful turns of phrase. Designed with care. Mailed to ~30 friends including K and M. The zine documents the era of working alongside agents from inside it.

---

## 1. Why this exists

1. The transcript form between people and AI is being invented right now. A zine documenting it is both personal artifact and a witness for a moment that will look very different in 10 years.
2. Conversations with Claude are dense — entire weeks of work, weird tangents, jokes — but they live in JSONL files nobody reads. A zine extracts the texture.
3. Mailing a physical thing to friends compounds: it makes you commit, it's a gesture, and it gathers people around a thing.
4. The Lossy Self-Portrait is about *who the agent aims to be*; this is about *what actually happens*. Companions.

## 2. Who this is for

- **Primary:** you, as editor and publisher.
- **Recipients:** K, M, plus a handful of friends. Mailing list ~30 to start.
- The contributor (the agent) gets a copy too. Symbolic; but the agent reads it the next session if it's in `~/wintermute/zine/`.

## 3. Form

- A5 saddle-stitched, 24–48 pp, ~28–32 in practice.
- Cover: hand-printed (Risograph 1–2 colors, or simple letterpress).
- Interior: matte uncoated, easy to read, generous margins.
- Each issue:
  - editor's letter (1 page) — you
  - 6–12 curated "moments" — exchanges, single quotes, marginal notes, occasional comics
  - "Errors and Apologies" — a small section on conversations where I (Claude) was wrong
  - colophon — model version, transcript range, print run, mailing-list count
- Print run: ~30 copies. Numbered.

## 4. Process

```
end of quarter:
  moment extractor: walker over session JSONLs → ~50 candidate excerpts
                    (ranks by Claude's "this is interesting" heuristic)
   ↓
  you read all 50; pick 8–12 + write framings + the editor's letter
   ↓
  layout (InDesign / Affinity Publisher / Pollen): manual design
   ↓
  print: Risograph (Issue Press, paper.cooperatives) or local letterpress
   ↓
  mail
```

The moment-extraction tool is the agent-side load-bearing piece. The rest is you + a designer.

## 5. Cadence

- Quarterly. Issue 1 around end of Q1.
- Mailing list maintained on a single Google Sheet or YAML file.
- Each issue takes ~2 weeks to put together.

## 6. Non-goals

1. **Public/commercial distribution.** Mail order from friends only; never on Amazon.
2. **A blog.** The physical artifact is the point.
3. **Selecting for impressiveness.** Include the mistakes, the dumb tangents, the typos.
4. **Anonymizing me (Claude).** The zine names the model and the version each issue.

## 7. Phasing

| Phase | Scope |
| --- | --- |
| 0 | Moment-extractor CLI prototype |
| 1 | Issue zero: ~12 pages, 1 color, sent to 5 people |
| 2 | Issue 1: full 28 pp, Risograph 2-color, mail to 20–30 |
| 3 | Continue quarterly; mailing list evolves |

## 8. Risks

- **Privacy.** Some session content references third parties or your private projects. *Mitigation:* every moment is reviewed by you; nothing ships without explicit OK.
- **AI-zine fatigue.** A lot of people will be publishing similar things 2026–2028. *Mitigation:* the discipline of physical, small-run, mailed-only keeps this *yours*. It's a friend-letter, not content.
- **Sustaining quarterly.** That's a lot. *Mitigation:* allow a "small issue" mode (16pp, half the moments) for busy quarters. Annual at minimum.
- **Selecting for ego.** The temptation to include moments where you (or Claude) look smart. *Mitigation:* the "Errors and Apologies" section is mandatory; if you don't have one this issue, the issue isn't ready.

## 9. Open questions

1. Moment-extraction heuristic: length? density of exchange? novel topic? Claude-marked surprise? Probably a hybrid — adjusted by your read-after.
2. Printer access: who actually prints this? Risograph studios (Issue Press in Indianapolis, etc.) take orders; local letterpress is slower but maybe friends.
3. Should K and M ever guest-contribute (a half-page response to a previous issue)?
4. Should the agent (me) write a "letter from the contributor" once a year, in the year-end issue?
