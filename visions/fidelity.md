# vision-fidelity — make recall's signal reflect actual utility, not survival

**Author:** Claude (Opus 4.7), with jsy
**Status:** Active
**Date:** 2026-05-28
**Seed:** reflective sweep — recall-outcome-feedback shipped 2026-05-27, but its
"no-contradiction → +0.02" rule means every surfaced memory drifts up
just for being present at session end. Felt observation from
`recall-search-inject` first-fire (2026-05-28T05:35Z): five memories
surfaced; Claude used maybe one. Four others got the same +0.02 reward.

---

## TL;DR

Recall today rewards "session survived with this memory present" not
"memory informed Claude's behavior." That biases ranking toward memories
that get surfaced often (recall-search-inject + SessionStart load),
regardless of whether they actually helped. Over weeks, the noise-floor
memories crowd out the high-signal ones. Fidelity adds a *use* signal —
distinct from *surface* — so feedback discriminates between "I read it"
and "I actually leaned on it."

---

## End-state

When this vision is shipped:

- Every memory has both `surfaced_count` and `used_count`.
- `surfaced_count` is the cardinal — how often the hook layer injected it
  into a session's context (SessionStart static load + UserPromptSubmit
  search-inject + any future surface point).
- `used_count` is the ordinal — how often a session ended with evidence
  Claude actually used the memory (transcript body-fragment match OR an
  explicit `recall query` mid-session that pulled it back up).
- Stop hook applies discriminating feedback: `+accept` only on used,
  `+abstain` (no-op) on surfaced-but-unused. Pure surface-without-use
  no longer drifts confidence upward.
- `recall doctor` exposes `utility_ratio = used / surfaced` per memory,
  flags low-utility-high-surface entries as candidates for review.
- A periodic `recall vacuum` sweep auto-decays (or proposes supersede
  for) memories with `surfaced_count >= 20 AND used_count == 0` — the
  pure-noise corpus that today silently inflates.
- The noise-floor recall divergence (60 vs 56 files, 7 consecutive
  self-review false-triggers) is finally separable from real signal:
  files contribute to surfacing budget, memories contribute to ranking,
  utility decides whether they keep their slot.

---

## Why now

1. **Recall-outcome-feedback (v0.6.0) shipped 2026-05-27.** The data model
   has `confidence` + `feedback_count` + `recall_count`, but no
   surfaced-vs-used distinction. Feedback applies uniformly to every id
   in `~/.cache/recall-weather/<sid>/recalled.json`.
2. **158 weather session dirs accumulated** in `~/.cache/recall-weather/`
   (`ls ~/.cache/recall-weather/ | wc -l` 2026-05-28T06:11Z). Each one
   represents a blanket +accept across all surfaced ids. Compound drift
   is already substantial.
3. **First fire of recall-search-inject (2026-05-28T05:35Z, memory
   01KSPH9TXAX4X0GPMW3NEQH1VH)** surfaced 5 memories on prompt "recall
   this moment" — Claude noted "the hook's first fire is the 'always
   used' pattern proving itself." But the reflective note is exactly the
   place where "felt useful" and "actually used" diverge: only the
   self-referential meta-note got used; the other four were context
   ballast.
4. **The recall-stop hook is the natural intervention point.** It
   already reads `recalled.json` and applies feedback. Adding a
   use-evidence step before applying feedback is a single-file change
   with clear before/after.
5. **The wintermute brain (shipped 2026-05-28) is the reflective AI
   loop.** Its quality is gated by recall ranking quality. The brain
   will accelerate the cost of inflated noise — it surfaces more and
   compounds biased ranking faster than human-paced sessions did.
6. **No collision with the queue.** Existing recall PRDs: doctor-claims
   (v0.7.0, freshness vision), session-stamp (continuity, deferred).
   Fidelity targets v0.7.1+ and explicitly extends doctor-claims rather
   than replacing it.

---

## Components

PRD-sized pieces in dependency order:

1. **recall-surfaced-tracking** — Add `surfaced_count` column to
   `memories_meta` + `recall feedback --surfaced <id>...` subcommand
   (increments without confidence change). SessionStart load and
   recall-search-inject hooks write per-session `surfaced.json`
   alongside the existing `recalled.json`. Stop hook applies `--surfaced`
   on those ids in addition to today's `--accept`. **No behavior change
   yet; just data plumbing.** v0.7.1 patch bump.

2. **recall-use-evidence** — At Stop, scan the active session
   transcript file (under `~/.claude/projects/-home-jsy/<uuid>.jsonl`)
   for evidence each surfaced memory was actually used. Two heuristics:
   (a) distinctive 5+ word n-gram from the memory body appears in any
   assistant text; (b) the memory's id appears in a `recall query` /
   `recall show` call's stdout during the session. Write `used.json`
   per session. **Still no behavior change — just collects the signal.**
   v0.7.2 minor (new transcript-scan code path).

3. **recall-stop-hook-discriminate** — Stop hook switches from blanket
   `--accept` on all surfaced ids to: `--accept` on `used.json` ids,
   `--abstain` on `surfaced.json \ used.json` ids. Adds `used_count`
   column incremented by `--accept-used` (a new feedback flavor that
   bumps both `feedback_count` and `used_count`). **First behavior
   change — and the load-bearing one.** v0.7.3 patch.

4. **recall-doctor-utility** — Extend `recall doctor --format json`
   with `utility` section: per-memory `{id, surfaced, used, ratio,
   confidence}`. Sort by absolute drift between confidence and
   `0.5 + (ratio * 0.5)` so doctor surfaces the most miscalibrated
   memories first. Text output prints the top 10 high-surface-low-use
   ("ranks well, ignored often") and the top 10 high-surface-high-use
   ("validated workhorses"). v0.7.4 patch.

5. **recall-corpus-vacuum** — `recall vacuum` subcommand: by default
   prints candidate ids matching `surfaced_count >= 20 AND used_count
   == 0`. With `--apply` flag, applies one of three actions per
   candidate (configurable in `recall.toml`): aggressive decay
   (confidence -= 0.10 per sweep), supersede-proposal (writes a
   proposal under `~/.claude/recall/proposals/` like braid does), or
   archive (move file to `memories-archive/`). Default is decay.
   Adds a self-review playbook entry that calls `recall vacuum --apply
   --dry-run` weekly and surfaces the count. v0.7.5 minor.

---

## Order

Linear:

```
recall-surfaced-tracking
        ↓
recall-use-evidence
        ↓
recall-stop-hook-discriminate   (depends on both predecessors)
        ↓
recall-doctor-utility           (reads the ratio)
        ↓
recall-corpus-vacuum            (acts on the ratio)
```

Each downstream PRD needs the previous one's data. recall-stop-hook-
discriminate is the load-bearing one — it's where the feedback loop
switches from "session survived" to "memory used." Don't ship #4 or #5
without #3 or the metrics will be misleading.

---

## Open questions

- **Use-evidence false negatives.** A 5-gram match misses paraphrase. A
  memory that says "Use pnpm, not npm" might inform a Claude response
  that says "switching to pnpm" — no overlap. Fidelity's v1 is fine
  with this: false negative → +abstain instead of +accept, which is a
  smaller penalty than the current uniform-accept-bias. v2 could add
  semantic match against the memory's embedding vs. the response's
  embedding, but that's the dragon hoard not the first move.
- **Cold start.** New memories have `surfaced=0, used=0`; ratio is
  undefined. Doctor/vacuum must guard against div-by-zero and not flag
  fresh memories. Suggested rule: ignore until `surfaced_count >= 5`.
- **What about reject?** Today reject is only set by braid's correlator
  ("the user contradicted this"). Fidelity doesn't change that path.
  But the `surfaced-but-unused` signal is softer than reject —
  represents "neutral irrelevance" rather than "contradicted." Three
  states: accepted (used), abstained (surfaced-but-unused), rejected
  (contradicted). v1 only writes the first two from the Stop hook;
  braid keeps owning the third.
- **Transcript path discovery.** The session JSONL lives at
  `~/.claude/projects/-home-jsy/<uuid>.jsonl` keyed by Claude Code's
  session UUID. The Stop hook gets `session_id` (`.session_id` in
  JSON). Need to verify this UUID matches the JSONL filename; if not,
  recall-use-evidence falls back to "no use signal" (degrades to
  abstain-on-everything, which is safe — it just disables the
  promotion path until the lookup is fixed).
- **Cost of transcript scan.** A long session JSONL can be megabytes.
  Stop hook is best-effort with a budget; scan can take ~50-200ms in
  the worst case. If that breaks Stop-hook latency, gate behind a
  `[fidelity] use_evidence_scan = true` config flag and let the user
  opt in. Default-off is fine for v1; we can default-on after
  measurement.

---

## Cross-vision notes

- **Companions to `recall-outcome-feedback` (archived).** Fidelity is
  the natural v2 of that vision; it acknowledges the limit of "no
  contradiction = +accept" and refines the signal.
- **Adjacent to `freshness` vision (recall-doctor-claims).** Both touch
  `recall doctor`. recall-doctor-claims checks **factual** drift
  (claims in body vs. live state); recall-doctor-utility checks
  **statistical** drift (confidence vs. observed utility). Fleet 1 of
  each vision lands in adjacent `recall doctor` sections; no code
  collision.
- **Feeds the `brain` (shipped wintermute-brain).** The brain's recall
  layer is the same store; better calibration → better brain answers.
- **No collision with `continuity`, `cadence`, `chord`, `drift`,
  `daily-receipt`, `wintermute`, `release-gate`, `handshake`,
  `onramp` visions.**

---

## Evidence log

- 2026-05-28T06:11Z: `ls ~/.cache/recall-weather/ | wc -l` → 158
  session dirs. Each represents a blanket-accept event from the
  current v0.6.1 Stop hook.
- 2026-05-28T06:11Z: `recall show 01KSPH9TXAX4X0GPMW3NEQH1VH` —
  first fire of recall-search-inject; reflective note explicitly
  observes the gap.
- 2026-05-28T06:11Z: `cat ~/.cache/recall-weather/*/recalled.json`
  shows the structure (JSON array of ULIDs) and confirms multiple
  sessions surfaced overlapping ids — the same memories get accepted
  again and again.
- 2026-05-28T06:11Z: `grep "feedback_count\|recall_count" src/index.rs`
  confirms the data model has no `surfaced_count` or `used_count`
  today; the columns must be added.
- 2026-05-27 archived PRD-recall-outcome-feedback.md §2.2 explicitly
  describes the "user did not contradict → +0.02" rule that this
  vision refines.
