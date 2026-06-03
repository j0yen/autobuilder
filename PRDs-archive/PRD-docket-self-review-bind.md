# PRD: docket-self-review-bind — the carry-forward list becomes a query

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/docket.md
**build_target:** mixed
**build_into:** /home/jsy/.claude/skills/self-review
**Depends on:** docket-core, docket-escalate
**Codename:** *bind* — stop grepping prose; report to the ledger.

## TL;DR

docket exists but nothing reports to it. This PRD wires the self-review
skill to docket: at Phase 0 it reads `docket list --open` instead of
grepping journals/recall prose for carried-forward findings; in the
"Carried forward" / "Pending your call" handling it `docket report`s each
finding under a stable key; at Phase E it runs `docket sweep` to close
findings that stopped appearing; and the "3+ separate runs ⇒ justify a
playbook" rule (SKILL.md line 359) becomes `docket list --escalated`.
This is the integration that makes the whole vision pay off — it turns a
hand-maintained prose section, re-typed every run for 6 days, into a
ledger that maintains itself.

## Why this exists

Phase 1 evidence (2026-05-29):

- `self-review/SKILL.md` line 414 (`## Carried forward from prior
  reflections`) is a prose section the agent re-writes every run.
- Lines 452-465: the run persists **one** reflective recall memory whose
  free-text *"Pending"* line is the carry-forward state; future runs hit
  it with `recall query 'self-review'`.
- Line 359: the playbook-justification rule counts "3+ separate runs" by
  reading those query results — manual.
- `grep -l "Carried forward" ~/brain/journal/*.md` → 6 consecutive days;
  the stale-binary finding recurs 7× in one day's journal. This is the
  re-typing the binding eliminates.

The store and lifecycle (docket-core + docket-escalate) are inert
without a producer. The self-review is producer #1.

## What this builds

Two artifacts, both under `~/.claude/skills/self-review/`:

**1. A helper script** `scripts/docket-runid.sh` (and thin wrappers if
useful) that computes a stable **run-id** for the current self-review
invocation. Proposed run-id: `YYYY-MM-DD.<n>` where `<n>` is the run
number of the day (the skill already prints "Run N of the day"). The
script derives `<n>` by counting today's self-review reflective memories
+ 1, or accepts an override. Output: a single run-id string on stdout.
This keeps run-id generation deterministic and out of the skill prose.

**2. SKILL.md edits** (additive, anchor-based — do **not** restructure
the skill):

- **Phase 0 (Listen):** add a step — `docket list --open --format json`
  (and `docket list --escalated`) to load structured carry-forward state.
  Keep the existing `recall query` step (recall remains the prose
  narrative); docket becomes the authoritative *list of open items*. Add
  a guard: if `docket` is absent on PATH, fall back to the current
  prose-grep behavior (the skill must not hard-fail on a box without
  docket installed).
- **"Carried forward from prior reflections" (line 414 area):** instruct
  the agent to source the carried list from `docket list --open` and, for
  each item still observed this run, `docket report --run <runid> --key
  <stable-slug> --title "<one-liner>"` with evidence refs
  (`recall:<this-run-ulid>` once known, `journal:<date>`, `pid:`, etc.
  per docket-evidence). Define a **stable-key convention**: kebab-case,
  durable across runs (e.g. `agorabus-stale-binary`,
  `agentns-session-zeros`, `ctrace-sessionend-flake`,
  `wm-anthropic-key-empty`). Seed the initial keys for the known standing
  findings in an appendix so producers agree.
- **Playbook-justification (line 359):** replace "recurs across 3+
  separate runs (eyeballed)" with "appears in `docket list --escalated`"
  — the escalation is now mechanical, and the skill consumes it.
- **Phase E (Persist):** after writing the reflective recall memory, run
  `docket sweep --run <runid>` so findings not reported this run age
  toward auto-close, and back-link the new reflective ULID into each
  reported finding via `docket report ... --evidence recall:<ulid>` (or a
  follow-up `report` pass once the ULID exists).

**Idempotency & safety:** all docket calls are reporting/listing — no
destructive action, no user-gated step. Reporting the same finding twice
in one run is idempotent (docket-core AC3). The binding adds zero new
user-gated blockers.

## Acceptance criteria

1. `scripts/docket-runid.sh` prints a run-id of the form `YYYY-MM-DD.<n>`
   for today; running it twice in the same logical run yields the same
   id (deterministic given the same memory count / override).
2. SKILL.md Phase 0 includes a `docket list --open --format json` step
   and a `docket list --escalated` step, with an explicit
   docket-absent fallback to the existing prose behavior.
3. SKILL.md defines a stable-key convention (kebab-case, run-durable) and
   an appendix seeding keys for ≥4 known standing findings
   (`agorabus-stale-binary`, `agentns-session-zeros`,
   `ctrace-sessionend-flake`, `wm-anthropic-key-empty`).
4. The line-359 playbook-justification rule is rewritten to consume
   `docket list --escalated` rather than eyeballed recall recurrence.
5. Phase E includes a `docket sweep --run <runid>` step.
6. A dry-run walkthrough (documented in the PRD's test notes / a
   `scripts/` self-test) shows: reporting the 4 seeded findings under
   run `r1` then `r2` then `r3` leaves `agorabus-stale-binary` (assumed
   reported each run) `escalated`, and a finding reported only at `r1`
   becomes `resolved(stale)` after `sweep` at `r3` (with `--stale-after
   2`). This exercises the bind contract end-to-end against the real
   `docket` binary.
7. `bash -n scripts/docket-runid.sh` passes; the script has no
   `set -e` foot-guns that would abort the hook on a missing memory dir.
8. The edited SKILL.md still parses as the skill (front-matter intact,
   phase structure unchanged) and stays within its existing length
   discipline (no wholesale rewrite — additive anchors only).
9. The binding adds **no** new user-gated blocker: every docket call is
   non-destructive (`list`/`report`/`sweep`), verified by inspection.
10. README / SKILL note documents the docket integration so a future
    reader understands the carry-forward list is now ledger-backed.

## Out of scope

- The `docket` binary itself (docket-core/escalate/evidence).
- SessionStart banner surfacing → **docket-digest**.
- Other producers (vigil, readiness-beacon) reporting to docket — vision
  boundary note, not this PRD.
