# PRD: docket-digest — the device knows what's standing against it

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/docket.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/docket
**build_version_bump:** minor
**Depends on:** docket-core (uses `list`); better with docket-escalate
**Codename:** *digest* — one line that says what's open and what's loud.

## TL;DR

docket holds the standing findings, but they only surface when someone
runs `docket list`. This PRD adds `docket digest` — a compact rollup of
the open/escalated set, in a text form for the SessionStart banner and a
JSON form that **reuses the `wm.health.*` envelope** (owned by
companion-degrade, consumed by kin's health digest and homestead's
readiness-beacon). So "you have 4 open anomalies, 1 escalated" shows at
session start, and the device's readiness verdict can fold the docket
state in without re-deriving it.

## Why this exists

Phase 1 evidence (2026-05-29):

- The SessionStart hook already surfaces standing state (this session's
  banner reported "Self-review is due", stack contents, agorabus peers).
  Open anomalies belong in that same surface but have nowhere to come
  from — they live in journal prose.
- The homestead gossip note (2026-05-29T06:40) is explicit: the
  `wm.health.*` envelope is **OWNED by companion-degrade's design and
  CONSUMED by vision-kin's health digest — readiness-beacon must REUSE
  it (AC5), not invent a parallel one.** docket-digest must follow the
  same discipline: reuse, don't fork.
- homestead's readiness-beacon produces a "this device is not
  deploy-ready, and here is why" verdict. A device with 1 escalated
  anomaly (e.g. `wm-anthropic-key-empty`) is, by definition, not
  deploy-ready. docket-digest is the join: the readiness verdict should
  be able to include the escalated-findings count without re-scanning.

## What this builds

Extends the `docket` crate.

**New command `docket digest [--format text|json] [--severity <min>]`.**

- **text** (default): a one-to-three line summary suitable for a
  SessionStart banner, e.g.:
  ```
  docket: 4 open (1 crit), 1 escalated · oldest: agorabus-stale-binary (12 runs)
    escalated: agorabus-stale-binary — recurred 3+ runs, durable handling justified
  ```
  Empty store → a single clean line (`docket: 0 open`), exit 0.
- **json**: the `wm.health.*`-compatible envelope. Reuse the exact field
  shape companion-degrade defined (do not invent keys). The digest maps
  to a health component, e.g.:
  ```json
  {
    "component": "docket",
    "status": "degraded",        // ok | degraded | down, per wm.health enum
    "summary": "4 open, 1 escalated",
    "detail": {
      "open": 4, "escalated": 1, "crit": 1,
      "oldest_key": "agorabus-stale-binary", "oldest_runs": 12,
      "escalated_keys": ["agorabus-stale-binary"]
    }
  }
  ```
  **Status mapping:** `escalated > 0` → `degraded` (or `down` if any
  escalated finding is `severity=crit`); only `open` (none escalated) →
  `ok` with a non-zero count noted; empty → `ok`. Confirm the exact enum
  values against companion-degrade's shipped envelope at build time and
  match them — this PRD's AC is "reuses the real envelope," not "defines
  a plausible one."

**Reuse mechanics.** If companion-degrade ships a Rust crate / type for
the envelope, depend on it; if it ships only a JSON contract (doc/schema),
match the field names exactly and add a test asserting the digest output
validates against that contract. Cite the source of truth in the README.

**SessionStart consumption (documented, not auto-installed).** The PRD
documents how the existing SessionStart hook would call `docket digest
--format text` and append it to the banner (the same place the
self-review-due / agorabus-peers lines come from), but does **not**
modify the live hook — that wiring is a one-line user-gated install,
noted for /build, not auto-applied (consistent with how other
hook-touching PRDs stay non-destructive).

## Acceptance criteria

1. `docket digest` on an empty store prints a single clean line and exits
   0; `docket digest --format json` on empty prints a valid
   `wm.health.*` envelope with `status=ok` and zero counts.
2. With 4 open + 1 escalated findings, `docket digest --format text`
   reports the open count, the crit count, the escalated count, and names
   the oldest finding with its run-age.
3. `docket digest --format json` emits an envelope whose field names
   match companion-degrade's `wm.health.*` shape exactly (verified
   against its crate/type or its published JSON contract — a test asserts
   conformance, not just plausibility).
4. Status mapping: 0 findings → `ok`; open-only → `ok` (with count) or
   the agreed non-down value; any escalated → `degraded`; any escalated
   `crit` → `down`. Each branch is covered by a test.
5. `--severity warn` excludes `info`-severity findings from counts and
   summary.
6. The oldest-finding selection uses run-age (`runs_seen` /
   `consecutive_runs`), not wall-clock, and is stable across `--format`
   text vs json.
7. JSON output is jq-parseable; text output is a fixed small number of
   lines (≤3) safe to inline in a banner.
8. README documents the digest, the status-mapping table, the
   envelope-reuse source of truth, and a copy-pasteable (commented-out)
   SessionStart hook snippet — explicitly noting the live hook is **not**
   modified by this PRD.
9. `docket digest` adds no new table/migration beyond what core/escalate
   created (it is a read/aggregate over existing data).
10. If docket-escalate is not present (escalated state never set), digest
    degrades gracefully: reports open counts, escalated=0, status `ok`.

## Out of scope

- Modifying the live SessionStart hook (documented, user-gated).
- kin's health digest / readiness-beacon *consuming* this — those are
  their own PRDs; this PRD only guarantees envelope compatibility.
