# Vision: docket — findings get a memory, not just a mention

> The self-review notices the same thing every run. It writes it down,
> in prose, and parks it. Next run it notices it again. A docket gives
> each finding an identity, a lifespan, and a rule for when "noticed
> again" becomes "do something."

**Authored by:** /dream (Claude Opus 4.8), with jsy
**Created:** 2026-05-29
**Status:** active
**Seed:** bare `/dream` + Phase-1 live inspection (recall reflective
seeds + journal recurrence + self-review SKILL.md).

---

## TL;DR

Every self-review run rediscovers findings it discovered before, writes
them into a hand-maintained "Carried forward from prior reflections"
prose section, and parks them under "Pending your call." The recurrence
that *should* trigger action — the SKILL.md rule that a signal seen
across **3+ separate runs** justifies a durable playbook — is detected
by eyeballing `recall query 'self-review'` output across runs. There is
no structured store that counts how many runs a finding has survived,
escalates it when it crosses the threshold, or closes it when it stops
appearing. `docket` is that store: a small ledger keyed by stable
anomaly slug, where producers (the self-review, and later any tool)
*report* findings, and the ledger tracks first-seen, last-seen,
consecutive-run streak, occurrence count, escalation, and auto-close.

## Why this is real (Phase 1 evidence, 2026-05-29)

Measured live this session:

- **Recurrence is the norm, not the exception.** `grep -l "Carried
  forward" ~/brain/journal/*.md` matches **6 consecutive days**
  (2026-05-24 → 2026-05-29). The "agorabus daemon stale binary" finding
  alone appears **7× in the 2026-05-28 journal** and 3× in 2026-05-29.
- **The threshold is codified but eyeballed.** `self-review/SKILL.md`
  line 359: *"A new playbook is justified when a signal recurs in
  `recall query 'self-review'` results across **3+ separate runs**."*
  The mechanism for "recurs across 3+ runs" is a human/agent reading
  prose. Run-18 and run-19 reflective memories (recall
  `01KSRV7R4FERPP40HQGV5RGZNT`, `01KSS21WFN5H6V42JF723Z8K2J`) both say
  the stale-binary item is *"approaching the 3-runs threshold where a
  more durable handling would be justified"* — i.e. the agent is
  manually counting.
- **The store is unstructured.** `self-review/SKILL.md` lines 452-465:
  each run persists **one** reflective recall memory whose free-text
  *"Pending"* line is the entire carry-forward state. Future runs hit it
  with `recall query` (semantic/FTS over prose) — there is no per-finding
  entity, no lifecycle, no count. `~/.claude/skills/self-review/state/`
  does not exist; the skill has no structured state at all.
- **Findings stay open for ~20+ runs with no escalation.** "agentns
  agent_session all-zeros" has been Pending for ~21 consecutive runs
  (run-13 reflective `01KSK8SDM4...` through today). "ctrace missing
  SessionEnd summaries" has been open 5 runs. Each is rediscovered,
  re-typed, re-parked.

This is the third axis of staleness the laptop has been missing.
`vigil` watches *running binaries* drift from source. `freshness`
watches *memory bodies* go stale. `drift` watches *skill text*. None of
them watch the **self-review's own findings** accumulate, recur, and
demand escalation. docket is that watcher.

## End-state

When this vision is done:

1. The self-review reads its carry-forward state from `docket list
   --open` (structured), not by grepping journals/recall prose.
2. Each Pending finding is a docket entry with a stable key, a
   first-seen date, a consecutive-run streak, an occurrence count, and a
   typed evidence trail (recall ULIDs, journal lines, pids, commits).
3. When a finding crosses the 3-run threshold the ledger marks it
   `escalated` automatically and records *why* — turning SKILL.md
   line 359 from a manual rule into a mechanical one.
4. When a finding stops appearing for K runs the ledger closes it
   `resolved(stale)` automatically — so the carry-forward list shrinks
   without anyone deciding to drop an item.
5. A `docket digest` surface exposes the standing open/escalated set to
   the SessionStart banner and to consumers (kin's health digest,
   homestead's readiness-beacon), reusing the `wm.health.*` envelope
   rather than inventing a parallel one.

## Components (PRD-sized)

1. **docket-core** — new `rust-cli` (`j0yen/docket`, `~/.local/bin/docket`).
   SQLite ledger at `~/.local/share/docket/docket.db`. Entities keyed by
   stable slug. `docket report --run <id> --key <slug> --title <t>
   [--severity] [--evidence <ref>]` (dedupes within a run, bumps streak
   across runs), `docket list [--open|--escalated|--resolved]
   [--format json]`, `docket show <key>`, `docket resolve <key>
   [--reason]`. Run-aware occurrence counting (distinct runs, not raw
   reports). The foundation everything else reads.

2. **docket-escalate** — `rust-extend` into docket. The lifecycle rules:
   on report, if `consecutive_runs` ≥ threshold (default 3) →
   `status=escalated` + reason citing SKILL.md §line-359; `docket sweep
   --run <id>` auto-resolves open entries not seen in the last K runs as
   `resolved(stale)`. Configurable thresholds. Automates the manual
   carry-forward bookkeeping.

3. **docket-evidence** — `rust-extend` into docket. Typed evidence refs
   (`recall:<ulid>`, `journal:<date>#<line>`, `pid:<n>`,
   `provfs:<ts>`, `commit:<sha>`) accumulated across a finding's
   occurrences; `docket show` renders the trail. Lets an entry point at
   every run that observed it.

4. **docket-self-review-bind** — `mixed` (edit `self-review/SKILL.md` +
   a small wrapper script). The load-bearing integration: Phase 0 reads
   `docket list --open`; the "Carried forward" / "Pending" sections
   `docket report` each finding; Phase E runs `docket sweep`; the 3-run
   playbook-justification check becomes `docket list --escalated`.

5. **docket-digest** — `rust-extend` into docket. `docket digest
   [--format json|text]` produces the standing open/escalated set for
   the SessionStart banner and for reuse by kin / readiness-beacon.
   REUSES the `wm.health.*` envelope (owned by companion-degrade,
   consumed by kin) — does not invent a parallel schema.

## Order

```
docket-core ──┬── docket-escalate ──┐
              └── docket-evidence ───┴── docket-self-review-bind
                                      └── docket-digest
```

core first (defines the store + report/list contract). escalate and
evidence both extend the store and are independent of each other.
self-review-bind needs core + escalate (it reports findings and reads
escalated). digest needs core (lists) and is nicer with escalate but
doesn't strictly require it.

## Open questions

- **Run identity.** What is a "run"? Proposed: caller-supplied string
  (self-review passes e.g. `2026-05-29.1`, or the reflective ULID it's
  about to write). docket stays agnostic. Confirm with jsy.
- **Store format.** SQLite (queryable, transactional, matches recall's
  precedent) vs. JSONL (greppable, diffable, matches gossip/journal
  ethos). Leaning SQLite for the streak/sweep queries; open to JSONL if
  jsy wants the ledger in git.
- **Overlap with recall.** docket entries link to recall ULIDs but are a
  distinct lifecycle store, not a recall extension (recall is
  similarity-retrieval; docket is per-key state machine). Confirm jsy
  agrees this stays a separate tool.
- **Who else reports?** v1 producer is the self-review. Future producers
  (vigil's binstale, homestead's readiness-beacon, /build blockers)
  could all report to one docket — left as a vision boundary note, not a
  v1 PRD, until the contract proves out with the self-review alone.
