# PRD: docket-escalate — recurrence becomes action; absence becomes closure

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/docket.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/docket
**build_version_bump:** minor
**Depends on:** docket-core (consumes its store + `report` path)
**Codename:** *escalate* — the third time you notice, the rule fires
itself.

## TL;DR

docket-core records that a finding has survived N runs but does nothing
when N crosses a threshold, and never closes a finding that stopped
appearing. This PRD adds the lifecycle: when a finding's
`consecutive_runs` reaches the escalation threshold (default 3) it is
marked `escalated` with a recorded reason, and a new `docket sweep`
command auto-resolves open findings that were not reported in the last K
runs as `resolved(stale)`. This turns the self-review's hand-counted
"approaching the 3-runs threshold" bookkeeping (verbatim in run-18/19
reflective memories) into a mechanical state transition.

## Why this exists

Phase 1 evidence (2026-05-29):

- `self-review/SKILL.md` line 359 (verbatim): *"A new playbook is
  justified when a signal recurs in `recall query 'self-review'` results
  across **3+ separate runs**."* — the threshold exists; the detection
  is manual.
- Run-18 reflective (recall `01KSRV7R4FERPP40HQGV5RGZNT`) and run-19
  (`01KSS21WFN5H6V42JF723Z8K2J`): the stale-binary item is described as
  *"approaching the 3-runs threshold where a more durable handling would
  be justified"* — the agent is literally counting runs in prose.
- The "agentns agent_session all-zeros" finding has been Pending ~21
  consecutive runs (run-13 `01KSK8SDM4...` → today) with no escalation
  event — proof that without a mechanical rule, escalation never fires.
- Conversely, findings that resolve (e.g. run-13: *"PID 923's
  ghost-subscriber cleared without intervention"*) are dropped from the
  carry-forward list by hand, with no record that they were closed or
  why. `sweep` makes closure explicit and automatic.

## What this builds

Extends the `docket` crate (no new binary; same `~/.local/bin/docket`).

**Status model extension.** `status` gains `escalated` (between `open`
and `resolved`). Lifecycle:

```
            report (streak ≥ threshold)
   open ───────────────────────────────► escalated
    ▲  │                                      │
    │  └──────── report (streak < threshold) ─┤ (stays escalated once tripped,
    │                                         │  until resolved/swept)
    └── report (reopen) ── resolved ◄─────────┘
                              ▲
                              │ sweep: not seen in last K runs
                            open/escalated
```

**On `report`** (extends docket-core's report path): after the streak is
updated, if `consecutive_runs >= escalation_threshold` and status is
`open`, set `status=escalated`, write `escalation_reason` and
`escalated_at`. The reason string cites the rule, e.g. `"recurred N
consecutive runs (≥3); durable handling justified per self-review
SKILL.md §359"`. Escalation is sticky: once escalated, a later report
keeps it escalated (a thing seen 5 runs then 1 more run is still a
standing problem). Only `resolve`/`sweep` leaves the escalated state.

**New command `docket sweep --run <id> [--stale-after <K>]`.** Marks
every `open`/`escalated` finding whose `last_run != <id>` *and* whose
absence now spans ≥ K runs as `resolved` with `resolve_reason="stale:
not seen in <K> runs (swept at <id>)"`. Because docket does not parse
run-id ordering, `sweep` uses a `runs` ledger: docket-core/escalate
maintains a small `runs` table recording each distinct run-id in report
order; `sweep` counts how many recorded runs have elapsed since a
finding's `last_run`. `--stale-after` defaults to 3. `sweep` also resets
`consecutive_runs` to 0 on entries that were seen in a *prior* run but
not the current one (a gap breaks the streak) — without resolving them
until the staleness window is exceeded.

**New columns** (added via idempotent migration on open): `escalated_at`
(nullable RFC3339), `escalation_reason` (nullable TEXT). **New table**
`runs (run_id TEXT PK, seq INTEGER, seen_at TEXT)` — append-only,
ordered by report arrival, so `sweep` has a canonical run sequence.

**Config:** escalation threshold (default 3) and stale-after (default 3)
overridable via flags (`docket report --escalate-threshold N`) and/or
`DOCKET_ESCALATE_THRESHOLD` / `DOCKET_STALE_AFTER` env. Document both.

**List filters:** `docket list --escalated` (added to core's filter set)
returns exactly the escalated set — this is the query the self-review
will run to find what needs a durable playbook.

## Acceptance criteria

1. Reporting key `k` across 3 distinct run-ids (`r1`,`r2`,`r3`) leaves
   `status=escalated`, non-null `escalated_at`, and an
   `escalation_reason` mentioning `3` and `SKILL.md` (assert via `docket
   show k --format json`).
2. With default threshold, 2 distinct runs leave `status=open` (not yet
   escalated); the 3rd trips it. With `--escalate-threshold 2`, the 2nd
   run trips it.
3. `docket list --escalated --format json` returns exactly the escalated
   findings and excludes `open`/`resolved` ones.
4. Once escalated, a 4th report keeps `status=escalated` (does not revert
   to `open`).
5. A `runs` table records each distinct run-id once, in arrival order,
   with a monotonic `seq` (assert reporting r1,r2,r1 yields seq 1,2 for
   r1,r2 and no duplicate r1 row).
6. `docket sweep --run r5 --stale-after 2` resolves a finding last seen
   at `r2` when ≥2 runs (`r3`,`r4`) have elapsed since, with
   `resolve_reason` starting `stale:`; it does **not** resolve a finding
   last seen at `r4`.
7. A finding seen at r1 and r3 (gap at r2) has its `consecutive_runs`
   broken (not monotonically increasing through the gap) — verify the
   streak reflects the gap rather than counting r1→r3 as consecutive.
8. Migration is idempotent: running any command against a docket-core
   (pre-escalate) DB adds the new columns/table without data loss and
   without error on second run.
9. `DOCKET_ESCALATE_THRESHOLD=2 docket report ...` trips escalation at 2
   runs (env honored; flag overrides env when both set).
10. README section documents the lifecycle diagram, `sweep` semantics,
    the `runs` table, and both threshold knobs with worked examples.

## Out of scope

- Wiring the self-review to call `report`/`sweep` → **docket-self-review-bind**.
- Typed evidence → **docket-evidence**.
