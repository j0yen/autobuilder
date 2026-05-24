# PRD: Episodic Observer (codename: *episode*)

**Author:** Claude (Opus 4.7), for me
**Status:** Draft v0.1
**Date:** 2026-05-22
**Relates to:** [PRD-agentic-memory.md](PRD-agentic-memory.md) §4b.16 (the deferred PostToolUse observer)

---

## TL;DR

`recall` ships an `episodic` memory kind, but nothing populates it. I write semantic + reflective memories explicitly; episodic memories — "on 2026-05-22 I tried X, X failed, I tried Y" — never happen because they require an *observer*, not an author. This PRD scopes that observer: a Stop-hook that reads the just-completed session's JSONL, detects try/fail/retry patterns and user corrections, and writes one to a few `recall --kind episodic` entries per session with the supporting evidence attached. No PostToolUse polling (too noisy); end-of-session is the right granularity.

---

## 1. Why this exists

I make the same mistake twice across sessions. Specifically:

1. **I retry approaches that have already failed here before.** Three sessions ago I tried Edit-then-Edit-revert on a particular file. The "this approach is wrong for this repo" lesson is in the failed-edit pattern. Nothing records that lesson.
2. **User corrections vanish.** When the user says "no, stop doing X" and I switch, the switch survives the session but the *reason* — "the user dislikes X here" — does not.
3. **My own success patterns vanish too.** When I tried three things and the third worked, I have no record of which two failed; next session I might try one of the failures first again.

The recall `episodic` kind exists for exactly this. The schema and the storage path are ready. The writer isn't.

---

## 2. Who this is for

Me. Future Claude sessions on this laptop. The user gets the second-order benefit (less retreading) but the consumer is the agent, not the human.

---

## 3. What I would use it for (concretely)

| Signal in the session JSONL                                                | Episodic memory I'd want to write |
| -------------------------------------------------------------------------- | --------------------------------- |
| Edit on `foo.rs:42` followed within 5 turns by an Edit on `foo.rs:42` that reverts the diff | "Tried changing `foo.rs:42` from X to Y; reverted within 5 turns. Reason from surrounding text: …" |
| Bash command exits non-zero, immediately followed by a similar Bash command with one arg changed | "First attempt failed with `<err>`; second attempt with `<arg>` succeeded." |
| User message containing "no", "stop", "don't", "actually" within 2 turns of my last action | "User redirected me away from <my last action>. Their correction: <quote>." |
| Tool call sequence: `Read foo` → `Read bar` → `Read foo` (back-and-forth) | "I had to bounce between `foo` and `bar` to understand X. Pattern: <X>." |
| `Write` of a new file in `~/projects/<repo>/scripts/` that survived to session end | "Added `<path>` for purpose: <inferred from contents>." |
| `Skill(...)` invocation followed by a thumbs-down signal (user redirect within 3 turns) | "`/skill-name` produced an outcome the user rejected; reason: <quote>." |

Each row above should produce *at most one* episodic memory per session. The observer must be conservative — better to write nothing than to write noise.

---

## 4. Functional requirements

### 4.1 Trigger

Stop hook. Fires when a session ends. Reads:

- `$CLAUDE_SESSION_JSONL` (or, if not exposed, the most-recently-modified `.jsonl` under `~/.claude/projects/-home-jsy*/`).
- The corresponding `~/.cache/ctrace/sessions/claude-*.summary.md` if the ctrace hook ran.

Why Stop, not PostToolUse:
- PostToolUse fires per turn → noisy, expensive, can't see the *arc* of an attempt (try/fail/retry needs lookahead which PostToolUse doesn't have).
- Stop sees the whole session as a finite document → patterns are detectable.
- Idempotent: re-running on the same JSONL produces the same memories (modulo ULIDs).

### 4.2 Detectors

Each detector is a pure function over the JSONL. They run in this order; each can veto subsequent detectors for overlapping turns.

| Detector              | Triggers on                                                        | Body template |
| --------------------- | ------------------------------------------------------------------ | ------------- |
| `revert`              | Edit/Write followed by an Edit that returns the file to its prior state within K turns | "Tried `<change>` in `<path>`; reverted within K turns. Surrounding rationale: `<excerpt>`." |
| `retry-with-tweak`    | Bash exit≠0 followed by another Bash with ≥80% string similarity and a non-zero exit | "Command `<cmd>` failed with `<err>`. Retry with `<diff>` succeeded." |
| `user-redirect`       | User message starts with "no"/"stop"/"don't"/"actually"/"wait" within N turns of my last action | "User said `<quote>`. Redirected away from `<my last action's intent>`." |
| `tool-thrash`         | Same Read/Glob/Grep repeated ≥3× with no intervening change to the target | "Bounced through `<paths>` ≥3× to understand `<topic>`. Next time: start with `<file>`." |
| `skill-rejected`      | `Skill(...)` invocation followed by user-redirect within 3 turns | "Skill `/<name>` produced output the user rejected. Their reason: `<quote>`." |
| `successful-novelty`  | New file written in a `~/projects/<repo>/` directory and not deleted by session end | "Added `<path>` (purpose from first comment line or commit-message-like-text)." |

Each detector emits a candidate dict; the writer assembles candidates into a single recall write per detector kind. **Max 5 episodic memories per session** to prevent runaway accumulation.

### 4.3 Writer

Calls `recall write --kind episodic --subject project:<basename of cwd>` for each accepted candidate. The body is the template-filled text. The `evidence` array (4b.9 in the recall PRD) carries:

```yaml
evidence:
  - session: "<jsonl basename>"
    turn: 12
    excerpt: "..."
    source_path: "<file:line if applicable>"
```

If recall is unavailable, write to `~/.claude/episode-spool/<session-id>.jsonl` so the next run can drain the spool. Never block session-stop on recall.

### 4.4 Dry-run mode

`episode-observe --dry-run <jsonl-path>` runs all detectors and prints the candidate memories without writing. This is the testing and debugging mode; the hook calls it without `--dry-run`.

### 4.5 De-duplication

Before writing each candidate, query `recall query "<first 80 chars of body>" --kind episodic --since 90d` and skip if a near-identical episodic memory already exists. Avoids accumulating five copies of "user redirected me away from npm install" across five days.

---

## 5. Architecture

A single binary `episode` under `~/wintermute/episode/` (mirroring the other wintermute tools), installed to `~/.local/bin/episode`. Stop hook script `~/.claude/scripts/episode-stop.sh` calls it on session end.

```
~/.local/bin/episode
  observe <jsonl-path> [--dry-run] [--max-memories N]
  drain                                              # process the spool
  detectors                                          # list available detectors
```

Internally: streaming JSONL parser → detectors run in pipeline → candidates merged → de-duped → written via `recall`. Pure Rust; reuses recall's `Memory` and `Evidence` structs.

Hook integration:

```sh
# ~/.claude/scripts/episode-stop.sh
#!/usr/bin/env bash
exec /home/jsy/.local/bin/episode observe "$CLAUDE_SESSION_JSONL" 2>/dev/null &
```

Backgrounded so session-stop never blocks on it.

---

## 6. Non-goals

1. PostToolUse observation. Too noisy. Stop only.
2. Replacing manual `recall write`. Episodic complements semantic/reflective; it doesn't subsume them.
3. Cross-session correlation in v0.1. Each Stop run sees one session. Combining "this attempt failed three sessions ago" is a v0.2 question.
4. Modifying or deleting prior episodic memories. Append-only.
5. Heuristics that read the *content* of code changes. Detectors look at *structure* of the session (turn shape, tool sequence). Treating "what the user meant" as a content-analysis problem is out of scope.

---

## 7. Phasing

| Phase | Scope                                                       |
| ----- | ----------------------------------------------------------- |
| 0     | Wire the Stop hook; ship `episode observe --dry-run` only   |
| 1     | Three detectors: `revert`, `user-redirect`, `retry-with-tweak`. Spool fallback for offline recall. |
| 2     | Remaining detectors: `tool-thrash`, `skill-rejected`, `successful-novelty` |
| 3     | Cross-session correlation: surface "this kind of mistake has happened N times" at session start |

---

## 8. Risks

- **Noise.** Bad detectors → memory store fills with junk. *Mitigation:* dry-run first; max-5-per-session cap; de-dup window.
- **Self-serving framing.** I might frame a failure as "user changed their mind" when it was actually my misunderstanding. *Mitigation:* detector outputs include the literal quote/excerpt as evidence; no editorialization.
- **Privacy.** Session JSONLs contain user prompts verbatim. Episodic memories quote them. *Mitigation:* same trust model as recall — local-only, grep-able, user can delete.

---

## 9. Open questions

1. Should `episode` emit a single "session summary" reflective memory too, alongside the episodic ones? It would close the loop with `/self-review`'s reflective writes but might duplicate.
2. Should detectors be configurable per-project? (E.g. autobuilder-repo wants `revert` more aggressively than learning-db does.)
3. Embedded LLM judgment vs pure-syntactic detectors. v0.1 is pure syntactic. v0.2 might call a small local model to validate candidates before writing.
4. How long should episodic memories live? `decays_after: 180d` by default? (Semantic memories don't decay; episodic ones probably should.)
