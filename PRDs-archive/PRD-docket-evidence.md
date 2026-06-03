# PRD: docket-evidence — every occurrence leaves a trail

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Done (v0.4.0, 2026-05-30)
**Date:** 2026-05-29
**Vision:** visions/docket.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/docket
**build_version_bump:** minor
**Depends on:** docket-core (extends its `--evidence` flag + store)
**Codename:** *evidence* — point at the runs that saw it.

## TL;DR

docket-core accepts `--evidence` as an opaque string and stores the
latest. But a finding's value is its *trail* — which runs observed it,
with what proof (a recall memory id, a journal line, a pid, a binary
timestamp, a commit). This PRD makes evidence typed and accumulated:
each report appends a parsed evidence ref to the finding, and `docket
show` renders the full chronological trail. So the "agorabus stale
binary" finding can point at run-18's reflective ULID, run-19's, the
provfs `user.prov.ts` on the on-disk binary, and the pid that was
running the deleted inode — all in one place.

## Why this exists

Phase 1 evidence (2026-05-29):

- The same finding is currently re-evidenced from scratch every run in
  prose. Run-18 reflective (`01KSRV7R4FERPP40HQGV5RGZNT`) cites pid
  `2138939` and build time `14:55`; run-19 (`01KSS21WFN5H6V42JF723Z8K2J`)
  cites the same pid and a rebuild at `~20:51`; today's journal cites
  `/proc/2138939/exe → (deleted)`. These are three evidence points for
  **one** finding, scattered across three documents with nothing linking
  them.
- The laptop already emits crisp, typed evidence the prose throws away:
  provfs stamps `user.prov.ts` (homestead vision observed `1780026726`),
  the kernel marks deleted-inode exes with a `(deleted)` suffix, recall
  hands back ULIDs, journals have stable `date#line` coordinates. docket
  should *capture* these as structured refs, not re-narrate them.
- Hard rule (vision): docket links to recall, it does not duplicate it.
  Typed `recall:<ulid>` refs are the join key between the lifecycle store
  and the memory store.

## What this builds

Extends the `docket` crate.

**New table** `evidence (id INTEGER PK, key TEXT, run_id TEXT, kind
TEXT, ref TEXT, note TEXT, seen_at TEXT)`, FK `key → findings.key`.
Append-only.

**Typed evidence parsing.** `--evidence <ref>` (repeatable) is parsed by
`<kind>:<ref>` prefix into known kinds; unknown prefixes are stored as
kind `raw`. Known kinds:

| prefix      | meaning                          | example                          |
|-------------|----------------------------------|----------------------------------|
| `recall:`   | recall memory ULID               | `recall:01KSRV7R4FERPP40HQGV5RGZNT` |
| `journal:`  | journal date + optional `#line`  | `journal:2026-05-28#7`           |
| `pid:`      | process id observed              | `pid:2138939`                    |
| `provfs:`   | `user.prov.ts` epoch / xattr     | `provfs:1780026726`              |
| `commit:`   | git sha                          | `commit:02350fb`                 |
| `path:`     | filesystem path                  | `path:/home/jsy/.local/bin/agorabus` |
| (other)     | stored as kind `raw`             | `anything else`                  |

`docket report ... --evidence recall:01K... --evidence pid:2138939`
appends one row per ref, tagged with the report's `run_id`.

**Rendering.** `docket show <key>` (text) gains an *Evidence* section:
chronological list grouped by run, each line `[<run_id>] <kind>: <ref>`.
`docket show <key> --format json` includes an `evidence` array of the
typed rows. `docket list --format json` may include an `evidence_count`
per finding (cheap aggregate) but not the full trail.

**Validation (lenient).** Malformed refs never fail the report — a bad
`recall:` ulid is still stored (kind `recall`, ref as-given) so reporting
is robust under a flaky producer. Optional `docket evidence verify <key>`
(stretch, in-scope if cheap) flags refs whose target is unreachable
(`recall:` ulid not in `recall show`, `journal:` date file absent) —
read-only, advisory.

## Acceptance criteria

1. `docket report --run r1 --key k --evidence recall:01KSRV7R4FERPP40HQGV5RGZNT --evidence pid:2138939`
   creates two evidence rows for `k` with kinds `recall` and `pid` and
   the correct refs (assert via `docket show k --format json`).
2. Reporting `k` again at `r2` with `--evidence provfs:1780026726`
   appends a third row tagged `run_id=r2`; the r1 rows are unchanged
   (append-only, no overwrite).
3. `--evidence somethingweird` (no known prefix) stores a row with kind
   `raw` and `ref=somethingweird`.
4. `docket show k` (text) renders an *Evidence* section grouping rows by
   run in chronological order, each line showing kind and ref.
5. `docket show k --format json` includes an `evidence` array whose
   length equals the number of reported refs; `docket list --format
   json` includes `evidence_count` matching that length.
6. Repeatable `--evidence` (passed N times in one report) yields N rows.
7. A malformed `recall:` ref (not a valid ULID) is still stored (kind
   `recall`) and never causes a nonzero exit on `report`.
8. Migration onto a docket-core/escalate DB adds the `evidence` table
   idempotently with no data loss.
9. JSON outputs remain valid (jq-parseable) with the new fields.
10. README documents every evidence kind, the repeatable flag, and a
    worked multi-run trail example (the agorabus-stale-binary case).

## Out of scope

- self-review producing these refs → **docket-self-review-bind**
  (this PRD only makes docket *accept and render* them).
