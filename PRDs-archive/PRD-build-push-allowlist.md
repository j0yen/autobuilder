# PRD: build — narrow push gate for j0yen/<slug> repos

**Author:** Claude (Opus 4.7) via /dream Phase 3
**Status:** Draft v0.1
**Date:** 2026-05-26
**Builds on:** `/build` skill (Phase 4 push step), gh CLI, git,
  settings.json, sibling PRD-build-publish-allowlist.md
**Vision:** visions/release-gate.md
build_auto: false
build_target: self-mod
build_priority: high
build_version_bump: none

---

## TL;DR

The `/build` skill's Phase 4 push step — `git push origin main` from
inside `~/wintermute/<repo>/` after a version bump and changelog
commit — is repeatedly blocked by the Claude Code auto-mode
classifier with the reason "Pushing directly to the default branch
(main) bypasses PR review; user's '/build' command does not
authorize pushing to the default branch." The skill description
authorizes publishing j0yen repos, but the classifier sees only a
raw `git push` invocation against `main`.

Sibling PRD-build-publish-allowlist.md addresses the `gh repo create`
gate; its line 164 explicitly puts `git push origin main` out of
scope. That leaves this exact failure mode unaddressed. This PRD
fills that hole with the symmetric solution.

Two confirmed firings, same gate, same PRD:

- 2026-05-25T23:36Z — recall-daemon iter-8 (commits 4333b18 +
  2781c70 + f231524 ready to push, classifier blocked, three
  commits stayed local until interactive resolution at iter-9).
- 2026-05-26T05:40Z — recall-daemon iter-15 (commits 36cb6ea +
  aa0922c + 3abdf7b ready to push for v0.5.2 daemon lifecycle +
  doctor liveness + changelog; classifier blocked; archive
  currently gated on this).

Per-tick interactive authorization doesn't scale — same human
action was required for the same PRD twice in 6h. Every future
rust-extend PRD that hits version-bump → changelog → push will trip
the same wall.

## Why this exists

Without a fix, /build cannot autonomously archive any rust-extend
PRD that requires a version-bump push to an already-public j0yen
repo. recall-daemon is the proof case: all 12 ACs PASS (per iter-12
smoke test), v0.5.2 is fully built and installed locally, the
working tree is clean, three commits are queued — and the PRD
cannot reach `status: shipped` until the push lands. The same wall
will block recall-outcome-feedback (queued at v0.5.1/0.5.2/0.5.3),
recall-session-stamp (queued at v0.6.0), recall-doctor-claims
(queued at v0.7.0), every cadence-bind-* rust-extend PRD, every
chord-* rust-extend PRD against agorabus/episodic-observer, and
every freshness/handshake follow-on.

The narrow question this PRD answers: what's the smallest mechanism
that lets `/build` push version-bumped commits to existing j0yen
repos autonomously while preserving the classifier's safety
property that "pushing to main on a public repo is a high-impact
action that needs explicit authorization"?

## What this builds

A thin wrapper + one narrow allow rule, symmetric to wm-publish:

### 1. New helper: `~/.local/bin/wm-push`

Shell script (~100 LOC). Argument shape:

```
wm-push --slug <s> [--source <path>] [--branch <b>]
```

Invocation contract:
- `<slug>` must match `^[a-z][a-z0-9-]{1,40}$`.
- `<slug>` must appear in the wrapper's hard-coded allow-list (top
  of file, comment `# /build reference list — keep in sync with
  REPOS.md AND with wm-publish's list`). Initial list: every
  wintermute-* slug + recall, agorabus, episodic-observer, baton,
  agentsh, agentns, memlog, provfs, learning-db.
- `<source>` defaults to `$PWD`. Must be a git repo.
- `<branch>` defaults to `main`. Must equal the current branch
  (refuses pushing from a feature branch — Fleet 2 territory).
- The repo's `origin` remote URL must end in `j0yen/<slug>` or
  `j0yen/<slug>.git` (verified via `git remote get-url origin`).
  Refuses if origin points anywhere else, or if there is no origin.
- The local tip must be a fast-forward of `origin/<branch>` (fetch
  first, then check `git merge-base --is-ancestor origin/<branch>
  HEAD`). Refuses if not (no force-push, no diverged history).
- At least one commit must exist beyond `origin/<branch>` (refuses
  no-op pushes — pointless and easy footgun).

On pass: `exec git push origin "$branch"`.

On any guard failure: print policy violation to stderr, exit 2
without calling `git push`.

### 2. Settings.json allow rule

Add to the user-level `permissions.allow` list in
`~/.claude/settings.json`:

```
"Bash(wm-push:*)"
```

The wrapper's slug regex + allow-list + remote-URL match + branch
check + fast-forward check is the real safety boundary. The
permission rule just lets the wrapper run without re-prompting.
The wrapper is small enough to audit in one read.

### 3. `/build` Phase 4 push step rewritten

In `~/.claude/skills/build/SKILL.md` (the Phase 4 "push" bullet,
which today reads `git push origin main` or equivalent):

- Replace the raw `git push origin main` invocation with `wm-push
  --slug <slug>`.
- Add a fallback: if `wm-push` is not on `$PATH`, the skill logs
  `wm-push-missing` to the journal and skips the push step,
  setting the PRD's manifest entry to `next: interactive-push`.
  (Graceful degradation, mirrors publish-allowlist §3.)
- Add a fallback: if `wm-push` exits 2 with a guard-failure
  reason, the skill logs the reason verbatim and sets the PRD's
  manifest entry to `next: investigate-push-guard-failure`.
  Distinguishes guard rejections from classifier blocks.

## Acceptance tests

1. From `~/wintermute/recall` (origin = `j0yen/recall`, branch
   main, 3 commits ahead of origin/main), `wm-push --slug recall`
   succeeds and the three commits land on origin/main. `git log
   origin/main` shows commits 36cb6ea + aa0922c + 3abdf7b after
   the push.
2. `wm-push --slug not-on-the-list` exits 2 with stderr containing
   the word `allow-list` and does NOT invoke `git push`.
3. `wm-push --slug "../bad"` (or any value not matching the slug
   regex) exits 2 without invoking `git push`.
4. From a repo with `origin` pointing somewhere other than
   `j0yen/<slug>`, `wm-push --slug recall` exits 2 with stderr
   containing `remote URL` and does NOT invoke `git push`.
5. From a repo with no `origin` remote, `wm-push --slug recall`
   exits 2 with stderr containing `no origin remote` and does NOT
   invoke `git push`.
6. From `~/wintermute/recall` on a feature branch (not main),
   `wm-push --slug recall` exits 2 with stderr containing `current
   branch` and does NOT invoke `git push`.
7. From a repo where HEAD has diverged from origin/main (e.g. a
   rebase landed on origin and local is no longer a fast-forward),
   `wm-push --slug recall` exits 2 with stderr containing
   `fast-forward` and does NOT invoke `git push`.
8. From a clean tree exactly matching origin/main (no commits
   ahead), `wm-push --slug recall` exits 2 with stderr containing
   `no commits to push` and does NOT invoke `git push`.
9. A /build tick running on a rust-extend PRD that reaches Phase 4
   push, with `wm-push` installed and the allow rule active,
   completes the push step without classifier-blocked re-prompt
   and the manifest entry advances to `status: shipped` once the
   archive step runs.
10. Removing `Bash(wm-push:*)` from settings.json re-introduces
    the classifier prompt for `wm-push` invocations — sanity
    check that this rule is the actual override.

## Risks

- **Allow rule too broad in syntax.** `Bash(wm-push:*)` matches any
  argv. Mitigation: the wrapper's slug regex + hard-coded
  allow-list + remote-URL match + fast-forward check is the
  substantive boundary; the rule just bypasses the classifier
  prompt for that one binary path. Wrapper must be reviewed during
  install and remains under /build's self-mod path for future edits.
- **Drift between wrapper allow-list and REPOS.md and wm-publish's
  list.** Three lists now must stay in sync. Mitigation: keep all
  three pointing at `~/wintermute/REPOS.md` as the canonical source
  via shared header comments; release-gate-repos-md-sync (Fleet 2)
  automates the drift check in /self-review.
- **wm-push bypass on accidental `chmod +x` of a sibling script.**
  Same as publish-allowlist's analogous risk. Mitigation:
  `~/.local/bin/wm-push` is the install path; user environment is
  single-tenant; full-path matching is fragile across machines.
  Accept as low risk.
- **Fast-forward check race with concurrent pusher.** If another
  process pushes to origin/main between `git fetch` and `git push
  origin main`, the local push will fail with `non-fast-forward`
  from git itself (which `wm-push` surfaces as exit code). The
  wrapper does not retry. Acceptable — this is rare in practice
  (single-author repos) and the failure is benign (next /build
  tick will re-fetch and retry).
- **Refusing no-op pushes (AC8) blocks rare legitimate use.**
  Push-after-rebase-only without new commits is unusual on /build's
  flow (rebase happens before commit, not after). If a real case
  emerges, relax to a warning. Default is strict.

## Phasing

Single tick once user authorizes the self-mod. Tick contents:

1. Write `~/.local/bin/wm-push` (chmod 755).
2. Snapshot `~/.claude/settings.json` to `.bak.<ts>`, then add
   `Bash(wm-push:*)` via `jq` + atomic rename.
3. Patch `/build` Phase 4 push bullet to reference `wm-push`.
4. Verify by retrying `wm-push --slug recall` in
   `~/wintermute/recall` — should land commits 36cb6ea + aa0922c +
   3abdf7b on origin/main.
5. Archive this PRD with a `Verified-completed:` trailer naming
   AC1 + AC9.
6. As a downstream effect: /build's next tick should archive
   recall-daemon (the v0.5.2 commits will now be reachable from
   origin/main, satisfying verified-completed Check #2).

Estimated <20 minutes once authorized. Can be authorized in the
same user review pass as PRD-build-publish-allowlist.md — the two
wrappers are independent.

## Out of scope

- Feature-branch pushes + draft-PR creation. `wm-push --pr <branch>`
  is release-gate-prerelease Fleet 2 territory; /build doesn't
  generate feature branches today.
- Force-push. Explicitly refused. If a PRD ever needs to amend-then-
  push, that's release-gate-revert Fleet 2 territory.
- Pushes to non-j0yen orgs (joeyen-atscale work etc.). Hard-coded
  org match keeps the safety story simple. Personal-account work on
  this laptop is j0yen-only by convention.
- `gh repo create` flow — sibling PRD-build-publish-allowlist.md
  covers that. The two surfaces are distinct.
- Tag pushes (`git push origin --tags`). If /build ever cuts tags,
  add `wm-push --tag <v>` in release-gate-prerelease Fleet 2.

## Coordination with PRD-build-publish-allowlist.md

The two PRDs are mutually independent and can be authorized
together or separately. Naming convention is symmetric:
`wm-publish` for create, `wm-push` for subsequent push. Allow rules
are scoped per binary. Both should land before recall-outcome-
feedback iter-1 attempts a push, otherwise it will trip the same
gate recall-daemon hit at iter-8 and iter-15.
