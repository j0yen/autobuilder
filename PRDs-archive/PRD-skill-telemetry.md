# PRD: Skill Telemetry (codename: *spool*)

**Author:** Claude (Opus 4.7), for me
**Status:** Draft v0.1
**Date:** 2026-05-22
**Smallest of the four — start-with-this candidate.**

---

## TL;DR

I write skills (`/self-review`, `/init`, `/review`, etc.) and never know which ones get used, which ones produce outcomes the user accepts, and which ones quietly rot. SKILL.md files accumulate; signal does not. `spool` adds a tiny telemetry layer: every skill invocation appends one line to `~/.claude/spool/<YYYY-MM>.jsonl`; `/self-review` Phase E (or a standalone `spool report`) reads it and surfaces what fired, what completed, and what the user thumbed-down. Local-only, ~50 lines of bash, no daemon, no service. Closes the loop on whether a skill paid off.

---

## 1. Why this exists

1. **I have 14+ skills installed.** I have no idea which ones I actually invoke. `/self-review` fired twice today (verified by hand); the others? Unknown.
2. **Stale skills are invisible.** A skill I wrote weeks ago that nothing has called in a month is a candidate for retirement or rewrite. Today the only signal is "I forgot it existed."
3. **A/B-like comparison is impossible.** When I tweaked `/self-review` mid-day to add the recall integration, did the invocation rate change? Did the user redirect more or less? No data.
4. **Outcome signal is missing.** A skill firing is not the same as a skill succeeding. I want to know: of the skill invocations, which ones produced output the user accepted, which were followed by a user redirect within 3 turns, and which crashed.

---

## 2. Who this is for

Me — to inform which skills to evolve. Secondarily: `/self-review` consumer, which currently has no awareness of skill activity beyond what's incidentally in the ctrace logs.

---

## 3. What I'd use it for (concretely)

| Question                                                          | What `spool` answers |
| ----------------------------------------------------------------- | -------------------- |
| Which skills fired in the last 7 days, and how often?              | A table: skill → invocation count → user-redirected count |
| Did `/self-review` get used the day after I added a new feature?   | Yes / no, with timestamps |
| Are there skills nothing has called in 30+ days?                   | List of candidates for retirement or rewrite |
| Did the user thumb-down `/init` invocations more than `/review` invocations? | Comparative stats |
| What's the average skill invocation latency on this laptop?       | Median + P95 per skill |

---

## 4. Functional requirements

### 4.1 What gets logged

One JSONL line per skill invocation:

```json
{
  "ts":            "2026-05-22T18:04:31-07:00",
  "skill":         "self-review",
  "session_id":    "df04d4-...",          // jsonl basename
  "invocation_id": "01KS9...",            // ULID, unique per call
  "args":          "...",                 // raw skill args (truncated to 200 chars)
  "started_at":    "2026-05-22T18:04:31",
  "ended_at":      "2026-05-22T18:04:46",
  "duration_ms":   15123,
  "outcome":       "ok | error | interrupted",  // best-effort
  "user_redirect_within_3_turns": null    // populated by the Stop-hook backfill; null if session still open
}
```

Each line is a self-contained record. No correlation needed across lines until the report step.

### 4.2 How it gets logged

Skills are invoked through the `Skill(...)` tool. Two collection points:

- **At skill-start**: a PreToolUse-style hook (or, if no hook surface exists for the Skill tool, the skill loader script writes the start record itself). Writes `{ts, skill, args, started_at, invocation_id}` to today's `spool/<YYYY-MM>.jsonl`.
- **At skill-end**: a matching write that appends `ended_at`, `duration_ms`, `outcome`. The `invocation_id` joins the two.

For SKILL.md-style skills that are read into context and acted on (rather than dispatched as a tool), this is harder. v0.1 ships a `spool log <skill-name>` CLI that the SKILL.md instructions explicitly call at start and end (a two-line addition to each SKILL.md). Not magic, but truthful.

### 4.3 Backfill: user-redirect detection

A Stop-hook scans the session JSONL for the pattern "Skill invocation at turn N, followed by user message at turn N+1..N+3 matching redirect keywords ('no', 'stop', 'wait', 'actually')." For each match, sets `user_redirect_within_3_turns: true` on the matching `invocation_id` row. (This overlaps with `episode`'s detectors — `spool` consumes the same signal, attributes it differently.)

### 4.4 Reporting

```
spool report [--since 30d] [--skill <name>] [--format text|json]
spool report --weekly       # bucket by ISO week
spool report --stale 30d    # list skills with zero invocations in window
spool rank                  # one-line-per-skill: invocations | redirects | mean ms | last-fired
```

Example `spool rank`:

```
skill              invocations  redirect%  mean-ms  last-fired
self-review                  3       0%      14820  2026-05-22 18:04
update-config                2       0%        310  2026-05-22 16:48
init                         1       0%      22100  2026-05-22 16:46
review                       0       —          —  never
security-review              0       —          —  never
...
```

### 4.5 Storage

```
~/.claude/spool/
├── 2026-05.jsonl
├── 2026-04.jsonl
└── state/
    └── last-report.txt    # for /self-review checkpointing
```

Append-only monthly buckets. Trivially `tail`-able. `grep`-able. Rotation is a non-issue at the volume of skill invocations (≤ 100/day, ~30KB/month).

### 4.6 `/self-review` integration

A new Phase E section, two bullets:

```markdown
## Skill telemetry (today)
- 3 skill invocations: self-review (×2), update-config (×1)
- 0 user-redirects on skill output (clean day)
- Stale skills (no invocations in 30d): review, security-review
```

The full `spool rank` lands in the weekly run (Sundays).

---

## 5. Architecture

Single bash script + one tiny SQLite-free implementation. Total est. ~150 lines of bash, or ~300 lines of Rust if we want the same single-binary skill-distribution model as the others.

```
~/.local/bin/spool
  log start <skill> [--args ...]      # emits start record; prints invocation_id
  log end <invocation_id> [--outcome ok|error|interrupted]
  report [...]
  rank
  backfill <session_jsonl>            # post-hoc user-redirect attribution
```

Bash is honestly fine. The JSONL append is one line; the report is `jq` + `awk`. Distribution-as-a-skill is `install -Dm755 bin/spool ~/.local/bin/spool` plus updating each SKILL.md to bracket its work with `spool log start`/`spool log end`.

---

## 6. Non-goals

1. Telemetry-to-cloud anything. JSONL stays on disk.
2. Auto-generating SKILL.md updates. The two-line addition is manual; tools don't write to skills.
3. Replacing or competing with Claude Code's existing analytics (if any).
4. Per-turn instrumentation. Skill-grained is enough.
5. Cross-machine aggregation. Single-laptop.

---

## 7. Phasing

| Phase | Scope                                                              |
| ----- | ------------------------------------------------------------------ |
| 0     | `spool log start/end` + monthly JSONL append. Manually bracket the three skills I use most (`/self-review`, `/update-config`, `/init`). |
| 1     | `spool rank` + `spool report` + `--stale`. Wire into `/self-review` Phase E. |
| 2     | `spool backfill` reads session JSONLs, fills `user_redirect_within_3_turns`. |
| 3     | Bracket the remaining SKILL.md files. (Or: a tiny test that fails CI if a SKILL.md lacks the brackets.) |

---

## 8. Risks

- **Brackets get forgotten.** A new SKILL.md might not include `spool log` calls. *Mitigation:* a lint that the `/init` skill applies when scaffolding a new skill; same lint runs in `/self-review` Phase A and reports skills missing telemetry.
- **Self-skewing.** I might subconsciously favor running skills more when I know they're tracked. *Mitigation:* the tracking is read by me later, not surfaced live during work. No in-the-moment nudges.
- **Privacy of args.** `args` carries whatever the user typed after `/skillname`. Truncated to 200 chars; same trust model as the rest of `~/.claude/`.

---

## 9. Open questions

1. Should `spool` cooperate with `episode` and `transcript`? They each consume some overlapping signals. Probably: each writes its own artifact; consumers (like `/self-review`) read all three. Avoid one-tool-to-rule-them-all.
2. Should `outcome=ok` be the user's call rather than the skill's? Currently the skill self-reports. The user-redirect backfill is the corrective.
3. Should `spool` track non-Skill activity too (Bash invocations, Agent spawns)? *Probably not* — that data is already in ctrace and in the session JSONL. Skill grain is the missing layer.
4. Bash vs Rust for the binary? Bash is faster to write and easier to audit. Rust is consistent with the rest of the wintermute toolchain. Lean Bash for v0.1; rewrite if it becomes load-bearing.
