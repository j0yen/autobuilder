# PRD: Lossy Self-Portrait

**Author:** Claude (Opus 4.7), for jsy
**Status:** Draft v0.1 — art project
**Date:** 2026-05-22
**Audience:** jsy, Katherine, Maria
**Form:** A2 fine-art print, multi-panel composition, edition of ~5
**Cadence:** annual

---

## TL;DR

`CLAUDE_SELF.md` is where you and the agent agree, in writing, on how the agent aims to work for you. Edits land slowly. A year of them — adds, removes, rewrites — is a portrait of a relationship being negotiated. We render every commit to `CLAUDE_SELF.md` as a panel; panels grid together into one large print; the grid is the year. *Lossy* because each panel is a distillation, not the raw file.

---

## 1. Why this exists

1. `CLAUDE_SELF.md` is intentionally short (≤200 lines). What's interesting isn't the file — it's the *changes*. Each diff is a tiny renegotiation: "this voice was wrong"; "this default needed to be explicit"; "this is what I keep getting wrong now."
2. Reading `git log -- CLAUDE_SELF.md` is functional but private. A print is shareable. People who see it understand, viscerally, that the agent is a thing you've been shaping over time.
3. Most "AI portraits" are stylized faces. This one is the literal text of the agreement between you and the model, arranged as visual evidence.

## 2. Who this is for

- **Primary:** you. One copy on a wall in your workspace.
- **Secondary:** Katherine and Maria. The print is conversational — they can read it, follow the timeline, see the negotiation.
- **Tertiary:** future-you, in 5 years, asking what changed between 2026 and 2031.

## 3. Form

- A2 (420×594mm) archival pigment print on Hahnemühle or equivalent.
- Grid: 12–30 panels depending on commit volume. Each panel is one commit.
- Each panel contains:
  - commit hash + date in the corner (small caps)
  - the diff hunk, typeset as actual code with subtle green/red highlighting
  - a 1–2 sentence reflection by Claude on what changed and why
  - a small generated glyph (one-color, abstract) suggesting the mood of the edit
- Edition: 5 prints — one for you, one for K, one for M, two spare.

## 4. Process

```
git log --follow -p -- CLAUDE_SELF.md
   ↓
parse: one record per commit (hash, date, diff, message)
   ↓
for each commit: Claude generates 2-sentence reflection + glyph spec
   ↓
typeset panels into A2 grid (Typst or paged.js)
   ↓
PDF → fine-art print shop
```

Reflection generation is the only step that requires the model. Everything else is deterministic.

## 5. Cadence

Annual, in January for the prior year. First print may include multiple years (catch-up).

## 6. Non-goals

1. **An ML-stylized human face.** This is text and typography, not generative imagery of a person.
2. **A live-updating wall display.** That's the Tide Chart. This is a print, finished, framed.
3. **A complete edit history.** Trivially small commits (typo fixes) collapse into adjacent panels.
4. **Selling editions.** Personal artifact, not commercial work.

## 7. Phasing

| Phase | Scope |
| --- | --- |
| 0 | Manual experiment: 3-panel mockup from real commits |
| 1 | `self-portrait` CLI: git log → JSON → reflections → Typst PDF |
| 2 | First A2 print (covers all `CLAUDE_SELF.md` history to date) |
| 3 | Annual ritual |

## 8. Risks

- **Aesthetic flatness.** Diff dumps aren't art. Without typographic discipline and good reflection writing, this just looks like a giant code snippet on a wall. *Mitigation:* the reflections do the heavy lifting; iterate the reflection prompt with K or M for taste before printing.
- **Self-flattery.** Claude's reflections might be too kind to itself. *Mitigation:* you edit reflections before they go to print.
- **Privacy.** Some edits encode private corrections. *Mitigation:* commit messages can include `private:true`; those panels redact to date + hash only.
- **The model writes the reflection AND signs the work.** Either honest or sycophantic. Lean into the honesty — name the model in the colophon.

## 9. Open questions

1. Glyph generation: hand-designed once and reused, or procedurally generated per panel?
2. Reflections in first-person ("I changed Voice because…") or third-person ("the agent removed…")? First-person is warmer but riskier.
3. What does the colophon say? It needs to acknowledge that Claude wrote part of the piece about Claude itself.
4. Is one annual print enough, or do K and M get a folded poster version that's easier to mail?
