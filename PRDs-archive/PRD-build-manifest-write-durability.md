---
title: Build tick manifest writes must survive N-way branch contention
Status: Draft v0.1
build_target: shell
build_priority: high
build_into: /home/jsy/.claude/skills/build
---

# PRD — build-manifest-write-durability

## TL;DR

When a `/build` tick fans out to ~10 parallel branch agents, each branch
ends with a Phase-7 read-modify-write of its own slug's entry in the
single shared `state/manifest.json`, guarded by `state/manifest.lock`.
Under real contention this loses writes: in the 2026-05-30T04:5x tick,
**2 of 9 branches (`earshot-gentle-reprompt`, `agorabus-drain-notice`)
landed and pushed their work to GitHub but their manifest status stayed
stale** (`queued` / `in_progress`). The parent had to reconcile by hand
from the branch return lines. Silent loss means the next tick re-selects
already-shipped PRDs and burns a full agent re-verifying done work — a
pattern also seen across 5 other PRDs this same tick.

Root cause hypothesis: branches acquire `manifest.lock` but the
read-modify-write is not reliably serialized — either the lock fd isn't
held across the whole read→jq→rename window, a branch reads a snapshot
before a peer's rename and then clobbers it, or the `mktemp`+`mv` of a
~490 KB file interleaves with another branch's mv.

## Goal

Make a branch's Phase-7 status update durable regardless of how many
sibling branches write concurrently. No branch's committed work should
ever be left mis-recorded in the manifest.

## Proposed approaches (pick one in build)

1. **Verify-after-write + bounded retry** (smallest change): the branch
   re-reads its own slug entry after releasing the lock; if the status
   doesn't match what it wrote, retry the locked RMW up to 3× with small
   backoff. Cheap, local to the branch prompt + a helper script.
2. **Per-PRD status sidecar + parent merge**: branches write
   `state/status/<slug>.json` (no shared-file contention at all); the
   parent merges sidecars into `manifest.json` once, serially, after
   collecting all branches. Eliminates the race class entirely.
3. **Parent-side reconcile from return lines**: the parent parses each
   branch's `<slug>: <action> <outcome>` return and re-applies the
   intended status under a single serial pass. Belt-and-suspenders on
   top of (1) or (2).

Approach 2 is the most robust and is the recommended target; it removes
the shared-writer hazard rather than papering over it.

## Acceptance

1. A stress harness spawns N=12 concurrent writers each updating a
   distinct slug's `status`; after all exit, all 12 updates are present
   in `manifest.json` (0 lost writes) across 20 repeated runs.
2. The branch Phase-7 instructions in `SKILL.md` are updated to use the
   chosen mechanism, and the change is documented in the skill's
   "Locking" section.
3. `jq -e . state/manifest.json` stays valid after every concurrent run
   (no torn/partial writes).
4. Backward compatible: a single-branch tick still updates exactly one
   entry with no sidecar cruft left behind (or sidecars are cleaned up
   after merge).

## Notes

- Observed 2026-05-30 tick (parent on Opus, 9 branches on Sonnet). The
  parent already does an ad-hoc reconcile from return lines today; this
  PRD makes durability a property of the mechanism, not of parent
  vigilance.
- Related efficiency follow-on (NOT in scope here): a pre-selection
  reconciler that detects "repo already at target version + tests green"
  and marks `done` before dispatching an agent, so ticks stop
  re-verifying shipped work. Worth a separate PRD if this pattern
  persists.
