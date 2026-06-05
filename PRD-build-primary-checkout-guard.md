# PRD-build-primary-checkout-guard

Status: Draft v0.1
build_target: shell
build_priority: high
build_into: /home/jsy/.claude/skills/build/scripts

## TL;DR

A /build branch agent left a shared `build_into` repo's **primary checkout**
switched onto a build branch (`autobuilder/homeward-schema`) with uncommitted
files. Because the primary tree was dirty *and* off its default branch, every
sibling PRD that shares that repo (`homeward-ingest/match/report/embed/connectors`)
failed its `integrate`/`land` step with `target-dirty` (exit 3/4) on every
subsequent tick — silently burning ticks until a human-shaped recovery restored
the checkout to a clean `main`. This PRD adds a deterministic pre-tick guard that
detects and auto-restores the invariant: **a shared build_into repo's primary
checkout is always on its default branch and clean between ticks.**

## Problem

The worktree-isolation design (`wm-buildtree`, `worktree-extend.sh`) assumes the
primary checkout of every `build_into` repo stays parked on its default branch
(`main`/`master`) and all branch work happens in *separate* worktrees. Nothing
enforces this. A branch agent that runs `git checkout <build-branch>` in the
primary worktree (instead of `git worktree add`) silently violates it. The
symptoms are indirect — sibling integrates fail with `target-dirty`, not with a
"primary checkout on wrong branch" message — so the root cause is expensive to
diagnose (observed 2026-06-05: the homeward cluster wedged across several ticks;
the brain `Cargo.lock`-dirty deferrals are the same family).

## Proposal

Ship `scripts/checkout-guard.sh` invoked by the parent tick in Phase 0 (after
acquiring `tick.lock`, before Select) for every distinct `build_into` repo
referenced by a non-shipped manifest entry:

1. **detect** — for each repo, read its default branch (`git symbolic-ref
   refs/remotes/origin/HEAD`, falling back to `main`/`master` via `git
   worktree list`). If the PRIMARY checkout's `HEAD` is NOT the default branch,
   emit `primary-off-default <repo> <current-branch>`.
2. **classify-dirty** — partition the dirty entries into (a) untracked files that
   are NOT declared/reachable from the tree (orphans) and (b) tracked
   modifications. Only (a) is auto-handled.
3. **auto-restore (safe subset)** — when the primary is off-default AND the only
   dirtiness is orphaned untracked files: move those files to a timestamped
   backup under `~/.claude/scratch/checkout-guard/<repo>-<ts>/` (reversible,
   never `rm`), then `git checkout <default>`. Emit `restored <repo>`.
4. **escalate** — if the primary is off-default with *tracked* modifications (real
   in-flight work), do NOT touch it: emit `wedged-needs-human <repo> <branch>`
   and let the parent mark affected PRDs `needs_classification`.
5. The guard NEVER deletes, NEVER force-anything, NEVER commits. It only moves
   orphans to backup and switches branches.

Also add a **dispatch-time assertion** to the branch agent prompt template: branch
agents MUST use `wm-buildtree ensure` / `worktree-extend.sh add` and operate only
inside the returned worktree path; a branch that runs `git checkout` in the
primary `build_into` checkout is a bug (mirror Hard Safety Rule 8 wording).

## Acceptance

1. `checkout-guard.sh detect <repo>` exits 0 and prints `on-default` when the
   primary checkout is on its default branch; prints `primary-off-default
   <repo> <branch>` (exit 0, advisory) when it is not.
2. Given a repo whose primary checkout is on a build branch with ONLY orphaned
   untracked files, `checkout-guard.sh restore <repo>` moves the orphans to a
   timestamped backup dir, switches to the default branch, leaves the tree clean
   (`git status --porcelain` empty), and prints `restored <repo>`. No file is
   deleted (backup dir contains every moved file).
3. Given a repo whose primary checkout is on a build branch with TRACKED
   modifications, `restore` makes NO mutation and prints `wedged-needs-human`
   (exit non-zero). Verified by asserting `git status` is byte-identical before
   and after.
4. The guard is idempotent: a second `restore` on an already-clean default-branch
   checkout is a no-op printing `on-default`.
5. Default-branch detection works for both `main` and `master` repos and when
   `refs/remotes/origin/HEAD` is unset (falls back via `git worktree list`).
6. A smoke test under `sbx --no-net` exercises ACs 1–4 against a throwaway git
   repo fixture and exits 0.

## Why now

The homeward wedge cost multiple no-op ticks and a manual recovery this session.
The guard turns that recovery into a deterministic, reversible, auto-applied
Phase-0 step, and the dispatch assertion stops the violation at its source.
