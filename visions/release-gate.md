# Vision: release-gate

> /build can compile, commit, and version-bump fully autonomously,
> but it cannot publish. The auto-mode classifier interrupts every
> attempt to push code or create a public repo. Release-gate is the
> discipline of replacing those classifier interrupts with narrow,
> auditable wrappers that preserve the safety property without
> halting the loop.

Created: 2026-05-26
Seed: reflection — second firing of the `git push origin main` gate
  against recall-daemon (iter-8 at 23:36Z, iter-15 at 05:40Z, same
  PRD, same shape, same human action required) plus three prior
  firings of the `gh repo create` gate (wintermute-bootstrap iter-9/
  10/11) that motivated PRD-build-publish-allowlist.md.
Pace: opt-in (default — `build_auto: false`)

## TL;DR

`/build` Phase 4 hits two distinct classifier walls:

1. **First publish** — `gh repo create j0yen/<slug> --public --source=.
   --remote=origin --push`. Hit by wintermute-bootstrap three times.
   Already drafted: `PRD-build-publish-allowlist.md` (queued,
   `build_priority: high`, `build_auto: false`).
2. **Subsequent push** — `git push origin main` to a repo that
   already exists on j0yen and already has `origin` set. Hit by
   recall-daemon twice (iter-8 + iter-15), neither resolved by the
   publish-allowlist PRD (its line 164 explicitly defers this case).

The two walls have the same root cause (no plumbing converts skill-
level authorization into a classifier-honored rule) but different
surfaces (repo creation vs. main-branch push). They want symmetric
solutions: narrow wrapper script + tight `Bash(wm-*:*)` allow rule +
patched `/build` Phase 4 step.

## End-state

When release-gate is fully built:

- /build tick on a rust-extend PRD reaches Phase 4 push, calls
  `wm-push --slug recall`, the push lands, the PRD archives. No
  classifier prompt, no interactive retry, no manifest "blocked"
  state.
- /build tick on a rust-cli PRD reaches Phase 4 publish, calls
  `wm-publish --slug <new>`, the repo is created and seeded. Same
  no-prompt path.
- Both wrappers fail closed: slug regex + hard-coded allow-list +
  remote-URL / branch / fast-forward guards reject anything outside
  the j0yen build envelope without ever invoking `gh` or `git push`.
- /self-review checks the wrapper allow-list against `REPOS.md` and
  flags drift.

## Components

**Fleet 1 — two PRDs (one already queued):**

1. **build-publish-allowlist** (`self-mod`, ALREADY DRAFTED —
   queued at `~/wintermute/autobuilder/PRD-build-publish-allowlist.md`,
   authored by /build Phase 6 on 2026-05-26, `build_priority: high`).
   Adds `~/.local/bin/wm-publish` + `Bash(wm-publish:*)` allow rule
   + /build Phase 4 patch to use it. Covers `gh repo create`.
2. **build-push-allowlist** (`self-mod`, NEW — this /dream pass).
   Adds `~/.local/bin/wm-push` + `Bash(wm-push:*)` allow rule +
   /build Phase 4 patch to use it. Covers `git push origin main`
   to existing j0yen repos. Sibling to publish-allowlist;
   independent surface, symmetric structure.

## Order

```
publish-allowlist ⟂ push-allowlist  (mutually independent)
```

Either can ship first. Both can be authorized in the same user
review pass.

## Fleet 2 (not drafted)

After both Fleet 1 PRDs ship and at least one /build tick has used
each wrapper end-to-end:

- **release-gate-repos-md-sync** — `/self-review` playbook that
  diffs the two wrappers' allow-lists against `~/wintermute/REPOS.md`
  and emits an actionable line on drift. Lifts the existing risk note
  from both PRDs into automation.
- **release-gate-prerelease** — extend the wrappers with
  `--draft` (gh release create) and `--tag <v>` flows for crates that
  want git tags published alongside the version bump.
- **release-gate-revert** — narrow `wm-revert --slug <s> --commit <h>`
  that force-pushes a single revert commit (creates the revert
  locally, sanity-checks it's a true revert, then pushes). Justified
  if /build ever needs to roll back a botched ship; not motivated
  yet.

Draft Fleet 2 after Fleet 1 ships AND at least one /build tick has
used each wrapper end-to-end.

## Why this is a small vision

Per dream rule 6: I observed two confirmed firings of the push gate
on the same PRD across six hours (iter-8 + iter-15), with the
recall-daemon archive currently blocked, and the publish-allowlist
PRD's own author flagged push as a "separate gate" they didn't
cover. That motivates the sibling PRD. It does NOT motivate a 5-7
PRD release-management fleet — Fleet 2 bullets above are
hypotheses, not observed gaps.

## Evidence log (post-creation)

- 2026-05-26T05:40Z (recall-daemon iter-15) — Second firing of the
  `git push origin main` gate against j0yen/recall. v0.5.2 fully
  built + installed locally (commits 36cb6ea + aa0922c + 3abdf7b);
  archive blocked because verified-completed Check #2 requires the
  version-bump commit reachable from origin/main. Cited in /build's
  iter-15 manifest entry as "Resolution path:
  PRD-build-publish-allowlist.md" — but publish-allowlist line 164
  explicitly puts this case out of scope. The mis-citation is the
  freshness-on-prds signal this vision's draft was triggered by.
- 2026-05-25T23:36Z (recall-daemon iter-8) — First firing of the
  same gate. Resolved by interactive authorization at iter-9 01:05Z
  (push landed). Per-tick interactive authorization doesn't scale
  across iter cycles — iter-15 needed the same human action again,
  ~5h later, on the same PRD.
- 2026-05-26T01:01Z / 02:02Z / 03:14Z (wintermute-bootstrap iter-9/
  10/11) — Three firings of the `gh repo create` gate that motivated
  publish-allowlist. Each one a separate /build tick that reached
  Phase 4 and bounced. Documented in PRD-build-publish-allowlist.md
  §"Why this exists."
- 2026-05-26T06:33Z / 06:51Z / 18:03Z (recall-daemon iter-16/17/18)
  — Third, fourth, fifth firings of the push gate on the same PRD.
  iter-18 ended unexpectedly: a concurrent session (likely
  claude-2308-jsy per agorabus peer list at tick start) succeeded
  on the same `git push origin main` "moments later" while my own
  session was still classifier-blocked, landing all 5 stranded
  commits including v0.5.2 (3abdf7b). **This both reinforces and
  nuances Fleet 1 priority:** reinforces it because 5 blocks in
  one session across 18h is the strongest case yet that
  per-tick interactive authorization doesn't scale; nuances it
  because the classifier is also non-deterministic *across
  sessions* — sibling concurrence sometimes substitutes for a
  deterministic wrapper. push-allowlist makes the substitution
  unnecessary by giving every session the same deterministic
  surface, eliminating both the within-session retry waste and
  the cross-session race. The cross-session-substitution pattern
  also surfaces a chord-vision Fleet 2 candidate (`chord-fulfill`
  — publish-bounty), which release-gate Fleet 1 makes moot for
  the publish/push case but may matter for non-publish bounties
  later. Logged on chord.md too.
- 2026-05-26T18:24Z (wintermute-audio iter-5) — Fourth distinct
  PRD blocked by the same classifier surface: wintermute-audio
  iter-2 carryover notes "publish still classifier-blocked per
  wintermute-bootstrap iter-10 precedent," recurring across
  iters 2/3/4/5/7/8/9/11. Confirms release-gate Fleet 1 unblocks
  >=4 PRDs in flight (recall-daemon, wintermute-bootstrap,
  wintermute-platform, wintermute-audio).

## Open questions

- **Wrapper composition**: should `wm-publish` and `wm-push` share a
  common slug-validation library, or duplicate the regex + allow-list?
  Fleet 1 duplicates (simpler audit story per wrapper; drift between
  the two is caught by Fleet 2's `release-gate-repos-md-sync`).
  Reconsider if a third gate emerges and the pattern truly is
  load-bearing.
- **Branch policy**: `wm-push` Fleet 1 only allows `main`. A future
  PR-flow PRD might want `wm-push --pr <branch>` that pushes a
  feature branch + opens a draft PR via `gh pr create`. Out of
  scope for now; /build doesn't generate feature branches today.
- **Force-push**: explicitly refused by `wm-push` (fast-forward only).
  If a PRD ever needs to amend-then-push, that's `wm-revert` Fleet 2
  territory.
