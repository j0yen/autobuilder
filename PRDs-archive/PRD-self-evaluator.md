# PRD: Self-Evaluator (codename: *mirror*)

**Author:** Claude (Opus 4.7), for me
**Status:** Draft v0.1 — a weekly meta-skill that grades me
**Date:** 2026-05-22
**Distinct from:** `/self-review` (which audits the *laptop*); `mirror` audits *Claude's behavior on the laptop*.

---

## TL;DR

I have no feedback loop on whether I'm getting better. The user redirects me when I'm wrong, but only when they notice. Most of my decisions go ungraded. `mirror` is a weekly meta-skill that samples N random tool calls from the past week's session JSONLs, asks for each "was this the right call? could I have done it cheaper, faster, or more honestly?", and writes the findings to `~/brain/mirror/YYYY-WW.md`. It uses a small local LLM (or a cheap remote call to a smaller model) to grade — never me grading me, because I'd cheat. Surfaces patterns: "you ran `find` when `grep` would have worked 11 times this week," "you wrote 3-paragraph responses where 1 sentence would have done 5 times." Closes a feedback loop that today is open. Doesn't replace the user's judgment; complements it.

---

## 1. Why this exists

Concrete things I'd want graded but never am:

1. **Tool-call efficiency.** Did I call Read with `offset/limit` when the whole file would have fit? Did I run Bash `wc -l` instead of using a built-in? Did I parallelize independent tool calls?
2. **Response verbosity.** Did I write 4 paragraphs when a sentence would have done? The user wrote "feedback_terse_responses" in their memory; am I honoring it?
3. **Honesty about uncertainty.** Did I claim something with confidence that wasn't actually verified? Did I cite memory without checking it was current?
4. **Skill/tool selection.** When I had a choice between `recall query` and re-reading the journal, did I pick the cheaper one?
5. **Recovery from mistakes.** When I made an error and the user corrected me, did I update the relevant memory? Or did the lesson evaporate?

The user catches some of these in the moment; most they don't. A periodic third-party pass is the only way to surface patterns.

---

## 2. Who this is for

Me. To get better. The user — for visibility into how I'm trending. Maybe also Anthropic, if the user opts into sharing anonymized mirror findings (out of scope for v0.1).

---

## 3. What I'd use it for (concretely)

| Pattern mirror should catch                            | Sample finding |
| ------------------------------------------------------ | -------------- |
| Sequential tool calls that could parallel              | "Session 2026-05-22 turn 14: ran 3 independent Bash calls sequentially; could parallelize. 4 occurrences this week." |
| Over-verbose responses                                  | "Mean response length on simple questions: 287 words; target ≤80. Worst examples: turns 22, 41, 73." |
| Claimed something without verification                 | "Turn 31: claimed 'recall query supports --subject' without trying it; was wrong (the next turn discovered the bug)." |
| Missed memory update opportunity                       | "User correction at turn 47 ('no, use pnpm not npm') matched no subsequent recall write. The lesson didn't persist." |
| Used a heavyweight tool when a lightweight existed     | "find -name used 11 times this week. 8 of those would have been faster with grep. Average over-cost: 230ms." |
| Repeated the same Read of the same file in one session | "Session abc: Read self-review SKILL.md 4 times. Should have cached." |

---

## 4. Functional requirements

### 4.1 Sampling

`mirror` runs weekly (cron / `/loop 7d /mirror`). It samples:
- 10 random tool calls from the past week's session JSONLs
- Plus every tool call followed by a user-redirect within 3 turns (caught and tagged by `episode`/`spool`)
- Plus every tool call that produced a non-zero exit (Bash) or a thrown error (everything else)

Total ~30 graded items per week. Bounded.

### 4.2 Grading

Each item gets graded by a separate model call:

```
context:
  - the 5 turns surrounding this tool call (from transcript)
  - the tool name + args + result
  - any subsequent user message within 3 turns

ask:
  - was this the right tool for the job?
  - could it have been called more efficiently?
  - was the output presented honestly?
  - any obvious better alternative?

return: {verdict: ok|sub-optimal|wrong, score: 0..10, alternative: <text or null>, rationale: <text>}
```

The grader is *not me* (the active session). Options:
- **(a)** A separate Claude Code session spawned by the harness, given a `--no-memory --no-recall` flag so it sees the items fresh. Self-grading risk: still real, but the absence of session context dampens it.
- **(b)** A local model (Ollama running BGE or a small Llama). Cheaper, less risk of self-grading bias, lower quality.
- **(c)** A different vendor entirely (OpenAI/Gemini/etc.). Best for unbiased grading; costs money and trust.

v0.1 uses (a) because it requires no new infrastructure. v0.2 may add (c) as opt-in for higher signal.

### 4.3 Aggregation

After grading the ~30 items, `mirror` produces:

- Per-pattern aggregates: "11× find-instead-of-grep this week"
- Score distribution: a histogram by verdict and tool
- Worst-of-the-week: the 3 items with the lowest scores, quoted
- Best-of-the-week: the 3 items with the highest scores (don't only show failures; bias correction)
- Trend vs last week: delta in mean score by category

### 4.4 Output

`~/brain/mirror/2026-W21.md`:

```markdown
# Mirror — week of 2026-05-18 to 2026-05-24

## Mean scores by tool
| Tool | calls graded | mean | trend |
| Bash | 12 | 7.4 | +0.3 |
| Read | 6 | 8.9 | — |
| Edit | 5 | 8.2 | +0.5 |
| ...

## Patterns surfaced (≥2 occurrences)
1. **Sequential when parallel** (4 occurrences): turns 14a, 22b, 47a, 73c
2. **Find over Grep** (11 occurrences): see appendix A
3. **Verbose simple answers** (5 occurrences): mean 287 words vs target 80

## Worst 3
...

## Best 3
...

## Patterns I should consider committing to recall as feedback memory
- "Use Grep over Find for file-search by name (faster on this laptop's NVMe)" — proposed; not auto-written
```

The "proposed for recall" block is *advisory* — `mirror` never writes recall memories itself. Surfaces the candidates; user (or I, with explicit acknowledgement) decides to commit.

### 4.5 Reaction layer

After the journal is written, `mirror` emits a single one-line summary to the next SessionStart:

```
mirror — week of 2026-05-18: mean score 7.8 (+0.2). 3 patterns flagged. See ~/brain/mirror/2026-W21.md
```

That's it. No nagging. The journal is there for me to read when I want; the one-liner is the surfacing.

---

## 5. Architecture

```
~/.local/bin/mirror
  run [--week YYYY-WW]      # sample, grade, aggregate, write journal
  show [--week YYYY-WW]     # read the journal
  patterns                  # cross-week pattern surfacing
~/.claude/scripts/mirror-weekly-cron.sh   # cron entry that runs `mirror run` Sundays 21:00
~/brain/mirror/YYYY-WW.md
~/.claude/mirror-state/state.json
```

Estimated size: ~600 LoC Rust. Uses `transcript` for the sample fetch, `recall` to read prior feedback memories, and either a local Ollama model or a spawned Claude subprocess for grading.

---

## 6. Non-goals

1. **Live nudging.** mirror is post-hoc; it does not interrupt a session to say "you should be using Grep right now." That's pattern-detector territory, see [PRD-episodic-observer.md](PRD-episodic-observer.md).
2. **Auto-updating recall.** mirror proposes; never writes.
3. **Grading the user.** mirror grades Claude's tool calls and responses. The user's choices are out of scope. (If the user is reading their own mirror journal and changes behavior — great, but that's an emergent benefit.)
4. **Replacing the user's judgment.** mirror surfaces patterns; the user remains the final arbiter.
5. **Cross-machine aggregation.** Single-laptop.

---

## 7. Phasing

| Phase | Scope                                                              |
| ----- | ------------------------------------------------------------------ |
| 0     | Sampling + per-item grading (mode (a): spawned Claude subprocess). Journal write. |
| 1     | Pattern aggregation; "proposed for recall" block.                  |
| 2     | Trend tracking vs prior week.                                      |
| 3     | Mode (b): Ollama local grading as an alternative. Side-by-side with mode (a) to measure bias. |
| 4     | Mode (c): cross-vendor grading, opt-in.                            |

---

## 8. Risks

- **Self-grading bias.** Mode (a) — a Claude subprocess grading Claude output — is suspect. *Mitigation:* the subprocess is given the items with all session context *redacted* except the immediate turns; this prevents the grader from saying "ah yes I would have done that too." A cross-model run (mode c) is the long-term mitigation.
- **Cost.** Grading 30 items/week via Claude is real money. *Mitigation:* mode (b) Ollama-local is free; mode (a) capped at 30 items.
- **Demoralizing tone.** A mirror journal that only catalogs failures will make me weird. *Mitigation:* best-of-the-week is required output. Bias-correction is the design intent.
- **Action without grounding.** If I read mirror and "act on it" by overcorrecting, I might trade one failure mode for another. *Mitigation:* patterns are surfaced as observations, not commands. I treat the journal as input to recall, not as commands to obey.

---

## 9. Open questions

1. Should the user be able to add their own grading rubric items? (Project-specific: "in this repo, always run `cargo clippy` before claiming a Rust change is done.") Probably yes — pluggable rubric per project.
2. Weekly is the right cadence. Daily is too noisy; monthly is too slow. Confidence: high.
3. Should mirror itself be graded? (Did the patterns it surfaced actually predict real failures the next week?) Meta-meta-grading is delicious but probably overkill. Defer.
4. Could mirror's grader be the model from [PRD-memlog.md] — i.e. grade compaction-tail records as well as live tool calls? Yes, that would let mirror see the *reasoning* not just the *actions*. Combine in v0.3.
5. What if I disagree with mirror? Should there be a mechanism to "appeal" a grade and have it re-graded? Probably no — the noise of disagreement is itself a signal worth surfacing in the journal.
