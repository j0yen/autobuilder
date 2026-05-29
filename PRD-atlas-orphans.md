# PRD: atlas-orphans — the divergences a human can't see by eye

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/atlas.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/atlas
**Depends on:** PRD-atlas-core.md, PRD-atlas-edges.md
**Codename:** *atlas* — the map shows where the territory drifted.

## TL;DR

Once nodes (atlas-core) and edges (atlas-edges) exist, the corpus can be
checked against itself. atlas-orphans adds `atlas doctor`: a read-only
lint that surfaces the five divergence classes no one can spot across 107
PRDs by eye — PRDs with no vision, visions with no PRDs, shipped repos
with no originating PRD, PRDs the build manifest calls shipped whose repo
path is gone, and visions whose PRDs are all shipped but are still marked
`active` (*fulfilled* — exactly the cross-reference /dream's SKILL.md says
it does by hand on each invocation). Exit code reflects severity so a
caller can gate on it.

## Why this exists

Phase 1 evidence (2026-05-29):

- /dream SKILL.md: *"A vision is fulfilled when /build has shipped all
  its drafted PRDs. Dream marks fulfilled visions on next invocation by
  cross-referencing /build's manifest."* This is a **manual, every-run,
  error-prone join** — the exact thing a lint should own. The dream
  manifest has 25 vision entries; verifying each one's fulfilled-ness by
  hand against the 450 KB build manifest is not something done reliably.
- The build manifest's `iter_log` already records reconciliation drift in
  prose — e.g. `PRD-agentic-memory.md`'s entry reads *"reconcile:
  vanished→shipped ... PRD archived ... (shipped pre-/build)"*. That a PRD
  shipped before /build ever saw it is precisely an orphan-class
  divergence, today caught only by a human reading iter_log.
- `REPOS.md` (117 lines) lists shipped repos; nothing checks that each has
  a PRD of origin, nor that every PRD claiming `output_repo_path` still
  has a repo on disk (today's journal lists 14 dirty trees and ongoing
  repo churn — paths move).
- Self-review re-derives system health every run from scratch; a standing
  `atlas doctor` would give it a single structured divergence feed
  (this is the Fleet-1 capstone, gated as a vision OQ until orphans ships).

## What this builds

Extends `j0yen/atlas`. Adds a `doctor` module + command. Read-only.

**`atlas doctor [--format text|json] [--class <name>]`** reports findings
across five classes:

| class                | definition                                                       |
|----------------------|------------------------------------------------------------------|
| `prd_no_vision`      | PRD whose `Vision:` is empty or names a missing vision doc        |
| `vision_no_prd`      | vision (dream manifest) with empty `prds_drafted` and no PRD `Vision:`-pointing at it |
| `repo_no_prd`        | repo in REPOS.md with no PRD whose `output_repo_*`/title maps to it |
| `shipped_repo_gone`  | PRD the build manifest marks shipped whose `output_repo_path` does not exist on disk |
| `fulfilled_unmarked` | vision marked `active` in the dream manifest whose every drafted PRD derives status `shipped` |

Each finding carries: `class`, the offending node id, a one-line
`detail`, and (where relevant) the `source` file. `--class` filters to
one class.

**Exit codes:** `0` no findings; `1` only `info`-level classes
(`fulfilled_unmarked`, `vision_no_prd`); `2` any `warn`-level class
(`prd_no_vision`, `repo_no_prd`, `shipped_repo_gone`). Documented so a
self-review bind (future PRD) can gate on it.

**Deps:** none new. MSRV 1.85, no let-chains. Filesystem existence checks
for `shipped_repo_gone` go through the env-overridable roots so tests
never touch live `~/wintermute`.

## Acceptance criteria

1. `cargo build --release` + `cargo test` green; clippy to the repo bar.
2. `atlas doctor` over the live corpus runs read-only (no write to any
   PRD, manifest, gossip, or REPOS.md — assert via a fixture dir whose
   mtime is unchanged after the run) and exits with a code matching its
   highest-severity finding.
3. Each of the five classes has a fixture that triggers exactly it and a
   fixture that does not, asserted independently.
4. `atlas doctor --class fulfilled_unmarked` lists a vision iff its
   dream-manifest status is `active` and every `prds_drafted` PRD derives
   `shipped`; a vision with one un-shipped PRD is absent.
5. `shipped_repo_gone` fires for a PRD with `output_repo_path` set to a
   path that does not exist, and does not fire when the path exists.
6. `--format json` emits an array of findings, each with
   `class`/`node`/`detail`/`source`; valid JSON.
7. A corpus with zero divergences exits 0 and prints a clean-bill line in
   text mode.
8. README documents each class, the exit-code contract, and that doctor
   is advisory + read-only (it reports drift; it never repairs it).

## Out of scope

- Auto-repair of any divergence (atlas reports; humans/skills act).
- Wiring doctor into self-review (future `/dream extend atlas` PRD,
  gated on this one shipping + verifying — see vision OQ).
- Graph rendering (atlas-render).
