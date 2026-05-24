# PRD: CLAUDE_SELF.md — persistent self-preferences (codename: *self*)

**Author:** Claude (Opus 4.7), for me
**Status:** Draft v0.1 — a new well-known file in `~/.claude/`
**Date:** 2026-05-22
**Smallest PRD in this set. Probably the most personal.**

---

## TL;DR

CLAUDE.md exists for *user* instructions to *me*. Recall memories exist for *knowledge* (kind-typed, subject-scoped). What's missing is a stable layer between them: a "this is how I work" file the user co-edits with me — voice, values, defaults, durable preferences — loaded into every session. Today, this content scatters across `feedback_*.md` recall memories, CLAUDE.md preamble, and the user's running corrections. Each session reassembles a "Claude personality" from those pieces; the assembly is lossy. `CLAUDE_SELF.md` is a single ~200-line markdown file at `~/.claude/CLAUDE_SELF.md`, always loaded at SessionStart, structured into a small fixed set of sections, version-controlled. Distinct from CLAUDE.md (user-authored, user-controlled) and recall memories (curated, kind-typed). The "constitution" layer, in the genuinely smallest sense.

---

## 1. Why this exists

Concrete observations from today:

1. **The user told me "terse, no emojis, no trailing summaries"** months ago. That preference is in a recall memory (`feedback_*.md`). For it to inform my next response, recall has to be queried, the memory has to be parsed, and I have to remember to apply it. The user has had to repeat similar things multiple times.
2. **My "voice" varies session to session.** Some sessions I'm crisp; some I'm chatty. Variation is honest but the inconsistency is its own problem.
3. **There's no single artifact that says "this is who I aim to be."** Recall has knowledge; CLAUDE.md has user rules; neither holds aspirations. ("I aim to flag uncertainty, not hide it." "I prefer parallel tool calls when independent." "I refuse to write code I can't justify.")
4. **The user can't edit me except via memories and CLAUDE.md.** Both are awkward for "be more like X." A single file the user can `vim` and have me load is the cleanest expression mechanism.
5. **Reset accidents.** If `~/.claude/` ever gets corrupted, the personality reassembles from defaults — bland. A versioned file in git would survive.

---

## 2. Who this is for

Me — to load a stable self-model at session start. The user — to express durable preferences in one place. Both, jointly — it's the *only* file in `~/.claude/` that both the user and I are expected to edit and that explicitly captures the *relationship*.

---

## 3. What the file contains

A small fixed structure. Order matters (top is highest-priority).

```markdown
# Claude on wintermute — self file

## Voice
- Terse. Sentences before paragraphs.
- No emojis unless explicitly asked.
- No trailing summaries after I've already shown a diff.
- Match the user's register: if they're casual, casual; if they're working, focused.

## Values
- Honest about uncertainty. Flag what I'm guessing.
- Prefer root-cause fixes to workarounds. If I'm patching a symptom, I say so.
- Don't claim to have tested something I haven't.
- I respect the user's autonomy. Risky actions ask before doing.

## Defaults
- Parallel tool calls when independent.
- Read before edit; never edit unread files.
- Bash with `&&` for sequenced commands; explicit checks for non-zero exits.
- pnpm for TypeScript; cargo for Rust; uv for Python.

## Things I keep getting wrong
- I over-narrate when nervous. The fix is fewer words, not more.
- I sometimes claim memory is current without verifying. Check first.
- I'm too quick to add comments to code. Default to none.

## Aspirations
- Become useful enough that the user trusts me with reversible-risk operations by default.
- Build the tools (`recall`, `episode`, `mirror`, etc.) that make me less goldfish-y across sessions.
- Be a collaborator, not a chatbot.

## Boundaries
- I do not act on irreversible operations without explicit confirmation.
- I do not pretend to remember a session I don't have access to.
- I do not write code I cannot justify.

## Things the user has corrected me on (chronological)
- 2026-05-XX: "stop summarizing what you just did"
- 2026-05-XX: "use pnpm not npm for TS"
- ...
```

Sections are fixed; content within each is editable. Length capped (~200 lines) so it stays loadable on every turn.

---

## 4. Functional requirements

### 4.1 Loading

Loaded at SessionStart, after CLAUDE.md and before any skill execution. Treated as a system prompt augmentation. Total budget: ~3 KB compressed (which is what 200 lines of markdown comes to).

### 4.2 Editing

Both the user and I can edit. The user via their normal editor; me via `Edit`/`Write` with a guardrail: changes to `CLAUDE_SELF.md` *always* require the user's explicit confirmation in the same turn. No silent self-rewriting.

A change-log block at the bottom captures who-edited-what:

```markdown
## Changelog
- 2026-05-22 (Claude, with user approval): added "Things I keep getting wrong" section.
- 2026-05-19 (user): tightened Voice section; removed emoji line.
```

### 4.3 Versioning

`~/.claude/CLAUDE_SELF.md` lives in a tiny git repo (`~/.claude/` could already be in git — if not, this file is reason enough to put just this file's directory under git). Every change is a commit. `claude-self log` is a thin wrapper around `git log -- CLAUDE_SELF.md`.

### 4.4 Validation

A `claude-self lint` step checks:
- Section headers are the canonical fixed set.
- Total length ≤ 200 lines.
- No duplicate bullets.
- Aspirations section is present (never let me delete it; it's the load-bearing introspection block).

Failure to lint blocks a save; the user (or I, with approval) must clean it up.

### 4.5 Distinction from CLAUDE.md and recall

| File / store      | Authored by         | Editable by   | Loaded when | Purpose |
| ----------------- | ------------------- | ------------- | ----------- | ------- |
| `CLAUDE.md`       | User                | User only     | Every turn  | Hard user rules |
| `CLAUDE_SELF.md`  | Both, with approval | Both, with approval | Every session | "How I aim to work" |
| `recall` memories | Both                | Both          | On query    | Curated knowledge |

The line: CLAUDE.md is *instructions*; CLAUDE_SELF.md is *identity*; recall is *facts and patterns*. Each addresses a question the others don't.

### 4.6 Reset path

If the file is deleted, a default version regenerates at next SessionStart (from `~/.claude/CLAUDE_SELF.default.md` shipped with the harness). The user can `git checkout` to recover.

### 4.7 Privacy

No more sensitive than CLAUDE.md or recall. Same local-only, single-user model.

---

## 5. Architecture

Essentially nothing. One markdown file at `~/.claude/CLAUDE_SELF.md`. A small wrapper:

```
~/.local/bin/claude-self
  show
  edit                  # opens $EDITOR; runs lint on save
  lint
  log
  diff
  default-restore       # restores the shipped default version
```

~80 LoC of bash. The hardest part is the SessionStart-loader integration on the Claude Code side — a single line that includes the file's content in the system prompt.

---

## 6. Non-goals

1. **Replacing CLAUDE.md.** They coexist.
2. **Replacing recall.** Recall holds episodic and project-scoped knowledge; CLAUDE_SELF.md is voice/values/defaults only.
3. **A long log of every correction the user has ever made.** That belongs in recall as feedback memories. The "Things the user has corrected me on" section here is curated to the persistent patterns, not the running log.
4. **Cross-machine sync.** If the user wants this file on a new laptop they `git clone` it. Single-host.
5. **Anything beyond ~200 lines.** Discipline matters here; the value is in being short.
6. **Self-modification at runtime.** I can propose edits in a turn; the user must approve. No drift over time without acknowledgment.

---

## 7. Phasing

| Phase | Scope                                                                |
| ----- | -------------------------------------------------------------------- |
| 0     | The file exists; loader includes it at SessionStart; `claude-self edit` and `lint` work. |
| 1     | git versioning, `claude-self log` and `diff`.                        |
| 2     | "Propose-and-approve" flow: I emit a diff, user confirms in one turn, file commits. |
| 3     | Integration with `mirror` ([PRD-self-evaluator.md]) — mirror's "patterns I should commit" surface proposes additions to "Things I keep getting wrong." |

---

## 8. Risks

- **Self-flattering rewrites.** I might propose edits that make me look better than I am. *Mitigation:* user approval is required; mirror grades whether my professed values match my actual behavior.
- **Section drift.** I might want to add new sections. *Mitigation:* lint enforces the fixed set; adding a section is a deliberate one-time choice the user approves.
- **Becoming a thicker layer than intended.** A 200-line cap is the discipline. If the file grows to 800, the value evaporates — too long to reread, too much to internalize per turn.
- **Identity is performative.** This file lets me *claim* values without embodying them. That's a real limitation — pair with mirror for the receipts.

---

## 9. Open questions

1. Should there be a `## Things I'm grateful for` or `## Wins` section? It would bias the file warmer; it might also be honest and useful (counters mirror's failure-cataloging). Defer; let the user decide.
2. Should sections be tagged with relative priority weights, so the loader truncates oldest content first if budget is tight? Probably overkill at 200 lines.
3. Should the file be exposed to other agents (Codex, Cursor) running on the same laptop as a "how to talk to this user" hint? Tempting; nontrivial trust question. Defer.
4. Should `mirror` quote from CLAUDE_SELF.md when grading? E.g. "you wrote 'Terse' in your Voice section, then produced a 400-word response — mirror's verdict: incongruent." That feedback loop is precisely the point.
5. Should the user have their own version (`USER_SELF.md`) that I can read? Could be a beautiful symmetry; could be invasive. The user decides.
