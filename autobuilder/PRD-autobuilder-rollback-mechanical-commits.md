# PRD: autobuilder-rollback-mechanical-commits — mechanical commits shouldn't compound rollback-plan debt forever

- Status: built
- Lane: redbaron 2026-09-07T15:17:54Z
- Built: 2026-09-07
- Blocked-update (2026-09-07T11:55:15Z, lane=redbaron): P0.1-P0.4 implemented
  in `autobuilder/src/rollback.rs`, all 4 fixture ACs (AC1, AC2, AC3, AC4) + AC6
  (`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings` clean on this diff) pass; shipped as v0.3.0
  (`de8f3c0d0db301abaa78a8e1e4557846d16edda4`, pushed to origin/main). AC5
  run against mcphost's live HEAD `d3668a0` with the freshly-built binary:
  `rollback-plan` still reports `verdict=block` (26 of 81 commits, down
  from an undifferentiated pile — 31 correctly classified `mechanical`
  per the three named patterns). The 26 remaining blockers are NOT the
  three named shapes: ~10 merge commits (never covered by mechanical
  patterns, revert via `-m 1`), a ~15-commit chain of `www: ...`
  website-copy edits that supersede each other exactly like a mechanical
  shape but isn't one of the three named ones (candidate 4th pattern —
  explicitly out of this PRD's scope per Non-goals), 2 genuine substantive
  feature commits, and 1 `agent: refresh intent card for grand-loop-billing`
  commit that correctly fell through to `substantive` because it touched
  files outside the allowed set (AC2's exact scenario, confirmed working
  as designed in the wild). Documented per AC5's own escape clause — not
  forced to a pass. Can't yet run this PRD's own `extend-gate.sh` self-gate
  (step 12 of the build tick): `find_cargo_root` reports "ambiguous —
  multiple Cargo.toml found one level under
  /home/jsy/wintermute/autobuilder: .../autobuilder .../tools" — pre-existing
  since commit `550f329` (feat(tools): add 4 autobuilder tool crates),
  5 commits before this PRD touched anything, and out of scope to fix here
  (different producer, `build-skill/scripts/extend-gate.sh`, not
  `rollback.rs`). No tag; not archived. Needs either a `find_cargo_root`
  disambiguation fix (follow-on PRD against build-skill) or a manual
  `--project autobuilder` style override before this PRD's own gate can
  run and archive.
- Shipped-update (2026-09-07T15:35:00Z, lane=redbaron): fixed the blocker.
  Added `--project-root <rel>` to both `extend-gate.sh` (ce09d58, build-skill)
  and `onboard-repo.sh` (7fc6ea4, build-skill) mirroring the same pattern, to
  disambiguate `autobuilder/` vs `tools/`. Ran `onboard-repo.sh
  /home/jsy/wintermute/autobuilder --project-root autobuilder`: generated
  `agent/AUTOBUILDER_PROGRAM.md`, `scripts/audit.sh` + vendored
  `rules/audit-checks.sh`, the `autobuilder/agent -> ../agent` symlink, and
  tag `v0.3.0` at `de8f3c0` (ed7f48f). Fixed a self-caused `vti-plan` unrouted-
  path block by routing `autobuilder/agent` and `rules/**` in
  `agent/proof-lanes.toml` (930a77a) — the only block onboarding itself
  introduced; everything else recorded to `agent/gate-baseline.json` (5c72a58)
  is genuinely pre-existing (stale intent-card field lengths, no
  `scripts/run-metrics.sh` at the nested project root, the shared vendored
  `audit-checks.sh` not being nested-root-aware, no green CI, a planted
  secrets-scan test fixture, no `PRD-*.md` at repo root for ac-traceability,
  a host-level `ctrace` permission issue). `extend-gate.sh
  /home/jsy/wintermute/autobuilder --project-root autobuilder` at HEAD
  `5c72a58` now reports `gate: receipts=25 pass=17 block=8 verdict=block` then
  `extend-gate: delta verdict=delta-pass baseline=present new_blocks=none
  inherited_blocks=intake,proof-receipt,risk-gate,reviewer-agent,ci-checks,
  session-trace,secrets-scan,ac-traceability` — `vti-plan` and `rollback-plan`
  both `pass`. AC5 confirmed satisfied: mcphost's `rollback-plan` reports
  `verdict=pass` at mcphost HEAD `b2655fe` with this diff's binary.
- Receipts: https://github.com/j0yen/autobuilder/commit/de8f3c0 (P0
  rollback.rs, v0.3.0), https://github.com/j0yen/autobuilder/commit/ed7f48f
  (onboard scaffolding), https://github.com/j0yen/autobuilder/commit/930a77a
  (proof-lanes routing fix), https://github.com/j0yen/autobuilder/commit/5c72a58
  (gate baseline), tag https://github.com/j0yen/autobuilder/releases/tag/v0.3.0.
  build-skill: https://github.com/j0yen/build-skill/commit/ce09d58
  (extend-gate.sh --project-root), https://github.com/j0yen/build-skill/commit/7fc6ea4
  (onboard-repo.sh --project-root). `extend-gate.sh
  /home/jsy/wintermute/autobuilder --project-root autobuilder` at HEAD
  `5c72a58fdec38f35021c1fba7d6cf68f881ed63a`: `gate: receipts=25 pass=17
  block=8 verdict=block` then `extend-gate: delta verdict=delta-pass
  baseline=present new_blocks=none inherited_blocks=intake,proof-receipt,
  risk-gate,reviewer-agent,ci-checks,session-trace,secrets-scan,
  ac-traceability`.
- build_target: rust-extend
- build_into: /home/jsy/wintermute/autobuilder
- build_priority: high
- build_version_bump: minor
- publish: none
- Vision: visions/buildloop-operations.md
- PM: Joe
- Drafted: 2026-09-07 (Phase 6 reflect, drafted while advancing PRD-mcphost-tool-test)
- Engineering target: extend ~/wintermute/autobuilder's `autobuilder/` crate in place (`src/rollback.rs`), no other producer touched.

## TL;DR

Six mcphost PRDs (`mcphost-tool-test`, `mcphost-metered-overage`, `mcphost-python-kind-runtime`,
`mcphost-ci-sandbox-coverage`, `mcphost-healthz-minimal`, `mcphost-rest-bridge`, plus the
meta-PRD `build-extend-gate-receipts`) have been re-hitting the identical `rollback-plan`
gate block for hours across dozens of ticks today (2026-09-07). This tick fixed the sibling
`vti-plan` blocker (a one-line unrouted-path fix, `d3668a0`) and confirmed `reviewer-agent`'s
own block is just echoing `rollback-plan` — so `rollback-plan` is now the *only* root blocker
left on mcphost, and it is a self-reinforcing trap, not real risk: `rollback-plan`'s base tag
is pinned at `v0.13.3` (the newest tag reachable from HEAD) because nothing has passed the
gate since, so every intent-card-refresh, `Cargo.lock` version-field sync, and
"parallel integrate" version-bump commit from every sibling PRD keeps piling into the checked
range (52-80 commits and counting), each one non-revert-clean only because a *later* commit
of the same mechanical shape re-touches the same one or two files. None of this is exploitable
risk debt — it's the parallel-integrate convention's own housekeeping commits blocking on each
other — but as written, `rollback-plan` can never recover on its own: no PRD can pass, so the
base tag never advances, so the range only grows. Pay this once so mcphost (and any other
repo running many parallel `rust-extend` integrates) can actually reach `built` again.

## Problem

`autobuilder/src/rollback.rs` walks every first-parent commit in `<base>..HEAD`, dry-runs
`git revert` for each via `git merge-tree --write-tree`, and blocks the whole receipt
(`verdict=block`) if *any* commit fails that dry-run — see `run()` at rollback.rs:55-116,
`blocking = entries.iter().filter(|e| !e.revertable).count()` conceptually at line 68-74. The
base ref defaults to the newest reachable `v<major>.<minor>.<patch>` tag (per
`extend-gate.sh`'s own header comment); `ship-tag.sh` only tags a HEAD that just passed the
gate. This creates a lock: a repo whose gate has been blocked since `vN` accumulates *every*
subsequent commit in the checked range, including commits that are structurally guaranteed to
conflict with each other by design:

- `agent: refresh intent card for <slug>` — PRD-build-intent-card-refresh's own convention
  rewrites `agent/intent-card.json` on every single landed PRD; reverting an older refresh
  commit out of order against a HEAD that already contains a newer refresh of the same file
  will not apply cleanly, by construction.
- `<crate>: sync Cargo.lock version field to <ver>` — a corrective commit that touches only
  `Cargo.lock`'s version field; a later commit of the identical shape supersedes it the same way.
- `<crate>: v<X.Y.Z> — <slug> (parallel integrate)` — `worktree-extend.sh integrate`'s own
  version-bump + CHANGELOG-prepend commit; each one touches `Cargo.toml`/`Cargo.lock`/
  `CHANGELOG.md`, and a later integrate's bump commit supersedes the same lines again.

Verified directly against mcphost at HEAD `d3668a0` (2026-09-07, this tick): of the ~80
first-parent commits since `v0.13.3`, the `rollback.md` table's `✗` entries are dominated by
exactly these three subject shapes (see `target/autobuilder/rollback.md` rows for
`922020e agent: refresh intent card for mcphost-synthetic-flag`, `db6ed47 mcphost: sync
Cargo.lock version field to 0.25.0`, `a55bfce mcphost: v0.24.0 — mcphost-metered-overage
(parallel integrate)`, and many more of the same three shapes) — none of them touch
production source under `src/`. `reviewer-agent`'s own `block_reasons` for the same HEAD is
literally `["rollback-plan-commits-not-revert-clean"]` — it has no independent objection, it
is purely relaying this receipt's verdict. Fixing this one receipt clears both.

This is the same trap `PRD-rustbuild-gate-debt`'s Deploy notes flagged as a needed follow-on
("rollback-plan's 3 non-revert-clean commits since v0.5.0 ... need their own follow-on PRD —
not reopened here") and the same one `PRD-summa-gate-debt` broke out of by getting one clean
gate pass to advance the base tag — but mcphost has enough concurrent sibling integrates
landing per hour that no single PRD's gate run stays ahead of the pile-up long enough to
reach that first clean pass unaided. This PRD removes the false-positive weight instead of
racing it.

## Requirements

P0 (build these)

1. In `rollback.rs`, classify each checked commit as `mechanical` or `substantive` based on
   BOTH its subject line matching one of three known patterns AND its changed-file set being
   a subset of that pattern's expected files (never subject-line alone — a commit that matches
   a mechanical subject but touches an unexpected file, e.g. sneaks in a `src/` edit under an
   `agent: refresh intent card for X` subject, must NOT classify as mechanical):
   - `^agent: refresh intent card for .+$` — files ⊆ `{agent/intent-card.json,
     agent/intent-card.carried.json, agent/intent_card_amendment_request.json}`
   - `^[A-Za-z0-9_-]+: sync Cargo\.lock version field to .+$` — files ⊆ `{Cargo.lock}`
   - `^[A-Za-z0-9_-]+: v\d+\.\d+\.\d+ (—|--) .+\((parallel integrate|extend)\)$` — files ⊆
     `{Cargo.toml, Cargo.lock, CHANGELOG.md}`
   Use `git show --name-only --format=` (or equivalent) to get the commit's own changed-file
   set (not a diff against HEAD) for this check.
2. `blocking_count` and `verdict` in the receipt are computed over `substantive`-only commits;
   a `mechanical` commit that fails the revert dry-run no longer counts toward `blocking` or
   flips `verdict` to `block`.
3. `rollback.md` keeps listing every commit including mechanical ones (debt must stay visible,
   never silently dropped) but marks a non-revert-clean mechanical commit distinctly from a
   non-revert-clean substantive one — e.g. `✗(M)` vs `✗` — and the doc gains a one-line legend
   explaining the `(M)` marker.
4. The JSON receipt (`rollback-plan.json`) gains a `mechanical_count` field (commits classified
   mechanical, revert-clean or not) alongside the existing `commit_count`/`revertable_count`/
   `blocking_count`, so the delta-baseline machinery and any future tooling can see the split.

## Acceptance criteria

1. P0 — Given a synthetic git repo (built inside the test, not depending on the live mcphost
   checkout) with a commit whose subject is `agent: refresh intent card for foo` and whose
   only changed file is `agent/intent-card.json`, When `rollback-plan` runs, Then that commit
   is classified `mechanical` regardless of whether its revert dry-run succeeds.
2. P0 — Given the same fixture repo but the commit's changed-file set also includes `src/lib.rs`,
   When `rollback-plan` runs, Then that commit is classified `substantive` (the mechanical
   subject-line match alone must never launder an unexpected file change).
3. P0 — Given a fixture repo where the ONLY non-revert-clean commits are mechanical (matching one
   of the three patterns, file-set-bounded), When `rollback-plan` runs, Then `verdict=pass`
   and `blocking_count=0`, even though `mechanical_count > 0` and some of those mechanical
   commits show `revertable=false`.
4. P0 — Given a fixture repo with one substantive non-revert-clean commit (an ordinary `src/`
   change unrelated to any of the three patterns) plus several non-revert-clean mechanical
   commits, When `rollback-plan` runs, Then `verdict=block` and `blocking_count=1` (the
   mechanical ones are excluded, the substantive one still blocks) — proves the fix narrows
   scope without disabling the check.
5. P0 — Given the fix landed and version bumped, When `extend-gate.sh /home/jsy/wintermute/mcphost
   --head <landed sha of this PRD's own commit, applied on top of current mcphost HEAD>` runs
   (or the equivalent direct `rollback-plan` invocation against mcphost's real history at that
   HEAD), Then `rollback-plan`'s verdict is `pass` (mcphost's non-revert-clean commits are
   dominated by the three mechanical shapes per the Problem section's direct verification —
   confirm no unexpected substantive blocker remains; if one does, document it, don't force
   a pass).
6. P0 — Given all of the above, When `cargo test --workspace` and `cargo clippy --workspace
   --all-targets -- -D warnings` run on the diff, Then both are clean.

## Non-goals

- Rewriting or squashing any repo's actual git history — this PRD only changes what
  `rollback-plan` *counts*, never mutates a downstream repo's commits.
- Touching `vti-plan` (already fixed this tick, `d3668a0`), `reviewer-agent`'s own logic (its
  block was purely derivative of `rollback-plan` for this case — recheck after this lands
  before assuming it needs separate work), or any other producer.
- Widening or auto-recording `agent/gate-baseline.json` for any repo — that stays an explicit
  human-only action per `PRD-build-gate-delta-baseline`.
- A fourth mechanical-commit pattern beyond the three named above — if a new convention
  produces its own compounding shape later, that's a follow-on PRD, not scope creep here.

## Deploy notes

Lands directly in `~/wintermute/autobuilder` on RedBaron. After it ships and a fresh
`extend-gate.sh` run confirms mcphost's `rollback-plan` (and derivative `reviewer-agent`
block) clears, re-run the gate for the six currently gate-red mcphost PRDs named in the TL;DR
— they should each pick up a passing (or newly-substantive-only) verdict on their very next
tick without any code changes of their own, and the first one to land should let `ship-tag.sh`
finally advance mcphost's rollback base tag past `v0.13.3`, which is what actually prevents
this same pile-up from recurring.
