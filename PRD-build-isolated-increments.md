# PRD: build-isolated-increments — every /build increment lands on a per-PRD branch, never as uncommitted edits in a shared tree

**Status:** Draft v0.1
**build_target:** shell
**build_into:** /home/jsy/.claude/skills/build
**build_version_bump:** N/A (build-skill doctrine + helper script)
**Created:** 2026-05-28
**Author:** Claude (Opus 4.8, headless /build tick), for jsy

---

## TL;DR

When `/build` advances a `rust-extend` (or `kernel-extend`) PRD across
multiple ticks, each tick's partial work currently lands as
**uncommitted modifications in the target's main checkout**. That has
two failure modes, both observed live:

1. **Hard-Safety-Rule-5 deadlock.** PRD A leaves half-finished
   uncommitted work in a shared tree; PRD B (a sibling extend of the
   same repo) then *cannot* make a version-bump commit without either
   committing A's unfinished work or reverting it — both user-gated.
   So B blocks indefinitely. This is **happening now** in
   `~/wintermute/recall`: `recall-surfaced-tracking` left 4 modified +
   4 untracked files (schema-v4, ~21h stale), which blocks
   `recall-session-stamp` (iter-3) and every other queued `recall-*`
   PRD (corpus-vacuum, doctor-claims, doctor-utility,
   stop-hook-discriminate, use-evidence — 6 PRDs wedged behind one
   uncommitted tree).

2. **Timeout-corrupted shared tree.** The headless tick runs under
   `claude-build.service` with `TimeoutStartSec=600`. A Rust build with
   fresh heavy deps (e.g. `imageproc`, `pdf-writer`) can exceed the
   remaining budget; a mid-compile SIGTERM leaves new module files +
   `Cargo.toml` dep edits uncommitted in a **shipped** repo. The next
   tick (or an interactive session) then opens a dirty tree it didn't
   create.

The fix is one primitive: **per-PRD work isolation**. Every increment a
tick makes against a shared/existing tree happens on a dedicated
`build/<slug>` git branch (via `git worktree` so the user's main
checkout is never touched), and is **committed at the end of every
tick**. The main checkout is therefore *always clean*. Merge to the
default branch happens exactly once, at verified-complete, as the
existing Phase-4 `bump-version & commit` / archive step — now a
fast-forward/merge of an already-clean branch instead of a commit of
loose edits.

Result: dirty-tree deadlocks become structurally impossible, and a
killed tick costs at most the current uncommitted *worktree* (isolated,
disposable), never the user's checkout.

## Why this exists

Phase-6 reflect, headless /build tick 2026-05-28T01:48Z:

- The tick selected `daily-receipt-archive` (a clean, well-formed
  rust-extend into the shipped `daily-receipt` repo). Correct
  selection. But with ~5 min left under the 600s ceiling, starting the
  scaffold would mean a first compile of new heavy deps that could not
  reliably finish — and a kill would have dirtied `daily-receipt`
  exactly as recall is dirtied today. The safe move was to *not* start,
  and to fix the mechanism instead.
- This is not a one-off. The accumulation-of-uncommitted-increments
  pattern is the build skill's *default* for multi-tick rust-extend
  work (see `recall-session-stamp` iter-1/iter-2 notes: "No commit yet
  — recall working tree has dirty surfaced_count work … that needs
  commit-or-revert before any version-bump commit"). The skill already
  *knows* it's stuck; it has no mechanism to avoid the stuck state.
- Hard Safety Rule 5 ("defer to the user on conflict") is correct and
  must stay. This PRD removes the *conditions* that trigger it, rather
  than weakening the rule.

## What this builds

### 1. `wm-buildtree` helper (`~/.local/bin/wm-buildtree`)

A small POSIX-sh wrapper around `git worktree` + branch bookkeeping,
sibling in spirit to `wm-push` / `wm-publish` (slug-gated, idempotent,
structured stdout). Subcommands:

```
wm-buildtree ensure  <slug> <repo-path>   # create/attach worktree+branch; print its path (JSON)
wm-buildtree path    <slug> <repo-path>   # print the worktree path if it exists, else exit 3
wm-buildtree commit  <slug> <repo-path> <msg-file>   # stage+commit ALL changes in the worktree (Joe Yen identity)
wm-buildtree status  <slug> <repo-path>   # JSON: {branch, worktree, commits_ahead, dirty:bool}
wm-buildtree land    <slug> <repo-path> [--ff-only]  # merge build/<slug> into default branch, then prune worktree
wm-buildtree abort   <slug> <repo-path>   # remove the worktree + delete branch (disposable kill recovery)
```

- Worktrees live at `<repo-path>/../.build-worktrees/<slug>/` (outside
  the main checkout; one per slug).
- Branch name: `build/<slug>`. Created from the repo's current default
  branch tip at `ensure` time.
- `commit` uses `git -c user.email=jyen.tech@gmail.com -c
  user.name="Joe Yen"` per wintermute identity rule. Never `--force`,
  never touches `origin`.
- `land` is the only step that mutates the default branch; it is
  `--ff-only` by default and refuses (exit 4) if the main checkout is
  dirty, surfacing the conflict to the user (Rule 5 preserved, but now
  only at the *land* boundary, not every increment).
- Slug allow-list + path-must-be-a-git-repo guard, mirroring
  `wm-push`'s ALLOW array convention. Keep in sync with `wm-push`.

### 2. Build-skill Phase-4 doctrine change (`SKILL.md`)

For `rust-extend` and `kernel-extend` targets whose `build_into` is an
**existing git repo**:

- iter-1 calls `wm-buildtree ensure <slug> <build_into>` and records the
  returned worktree path as `manifest.<slug>.work_tree`. **All
  subsequent file writes for that PRD target the worktree path, not
  `build_into`.**
- Every tick that mutates files ends with `wm-buildtree commit <slug>
  <build_into> <msg-file>` so the increment is durable and the main
  checkout stays clean. The commit subject: `wip(<slug>): iter-N — <one
  line>`; squashed at land time.
- The existing `bump-version & commit` Phase-4 action becomes: bump on
  the worktree, commit, then `wm-buildtree land <slug> <build_into>
  --ff-only`. `push` (via `wm-push`) runs after a successful land,
  against the now-updated main checkout.
- `archive` / verified-complete is unchanged except it asserts
  `wm-buildtree status` reports `dirty:false` and `commits_ahead:0`
  (everything landed).

### 3. Recovery doctrine for the live recall deadlock

This PRD does **not** auto-resolve the existing recall dirty tree (that
remains user-gated — committing or reverting another PRD's in-flight
work is Rule 5). But it adds a documented one-time migration in
`SKILL.md` §Recovery: once `wm-buildtree` exists, the user (or a gated
tick) can `git stash` the loose recall edits onto a `build/recall-
surfaced-tracking` branch via `wm-buildtree adopt <slug> <repo-path>`
(a thin `git stash` → worktree-branch helper), unblocking the other
recall PRDs without losing the schema-v4 work.

## Acceptance criteria

- **AC1**: `wm-buildtree ensure <slug> <repo>` on a clean repo creates
  `<repo>/../.build-worktrees/<slug>` on branch `build/<slug>`, prints
  JSON `{worktree, branch, base_commit}`, exits 0. The main checkout's
  `git status --porcelain` is **empty** afterward.
- **AC2**: Writing files into the worktree and running `wm-buildtree
  commit <slug> <repo> <msg>` produces a commit on `build/<slug>` with
  the Joe Yen identity (`git log -1 --format='%an <%ae>'` ==
  `Joe Yen <jyen.tech@gmail.com>`). The main checkout stays clean.
- **AC3**: `wm-buildtree status` returns `commits_ahead` equal to the
  number of `commit` calls and `dirty:false` after each commit.
- **AC4**: `wm-buildtree land <slug> <repo> --ff-only` fast-forwards
  the default branch to the worktree tip, removes the worktree, deletes
  the branch, and leaves the main checkout clean with the new commits
  reachable from `HEAD`. Exits 4 (no mutation) if the main checkout is
  dirty.
- **AC5**: `wm-buildtree abort <slug> <repo>` removes the worktree and
  deletes `build/<slug>` with zero changes to the default branch and a
  clean main checkout — simulating kill-recovery.
- **AC6**: Slug not in the ALLOW array → exit 2, no filesystem change.
  Repo path not a git repo → exit 5, no change.
- **AC7**: Idempotent `ensure`: a second `ensure` for an existing slug
  attaches to the existing worktree (does not error, does not reset the
  branch), printing the same `worktree` path.
- **AC8**: `bash -n wm-buildtree` parses clean; `shellcheck` (if
  present) reports no errors at default severity.
- **AC9**: SKILL.md Phase-4 sections for `rust-extend` and
  `kernel-extend` reference `wm-buildtree ensure/commit/land` and state
  the invariant "the main checkout is never left dirty between ticks."

## Files this will create / modify

```
~/.local/bin/wm-buildtree                 # new helper (POSIX sh)
~/.claude/skills/build/SKILL.md           # Phase-4 doctrine + §Recovery
~/wintermute/REPOS.md                     # (if wm-buildtree is published as a repo) — else untouched
```

`wm-buildtree` is a build-skill-local primitive; v0.1 ships it to
`~/.local/bin/` only (no new public repo) — same posture as `wm-push`.

## Non-functional

- No network. `land` never touches `origin`; pushing stays the explicit
  `wm-push` step.
- Never `--force`, never `rm -rf` outside the dedicated
  `.build-worktrees/<slug>/` dir (Hard Safety Rule 2 preserved).
- Worktree dirs are disposable; `git worktree prune` is safe to run any
  time.

## Out of scope (v0.1)

- Auto-resolving the existing recall dirty tree (user-gated; AC-covered
  only as a documented `adopt` path).
- Concurrent multi-tick writers to the *same* slug (the tick.lock +
  one-action-per-tick already serialize this).
- New-repo (non-extend) PRDs — those already build in a fresh isolated
  dir; the dirty-shared-tree failure mode doesn't apply.

## After this lands

Every `rust-extend` / `kernel-extend` PRD accumulates its multi-tick
work on an isolated, always-committed branch. The 600s ceiling stops
being a corruption risk (a kill loses only the disposable worktree
delta). Hard-Safety-Rule-5 deadlocks across sibling extends of one repo
become structurally impossible. The recall logjam gets a documented,
non-destructive exit. The build loop can then safely resume advancing
`daily-receipt-archive` and the rest of the queued extend backlog.
