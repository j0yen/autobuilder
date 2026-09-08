# Changelog

## v0.7.0 — 2026-09-07

Unifies this crate with `~/wintermute/rustbuild/autobuilder` (PRD-autobuilder-source-unify):
ports the v0.4.0→v0.6.1 rollback deltas from that repo — the `RedeployTag`
rollback model, `agent/intent-card.json` / `agent/AUTOBUILDER_PROGRAM.md` /
`agent/deploy-manifest.toml` model resolution, and the tag-lineage verdict
(`redeploy-tag` mode: pass iff the base tag exists, the v-tag lineage from
base to HEAD is contiguous, and HEAD is tagged or taggable) — into this
crate's `rollback.rs`, alongside the mechanical-commit classification
already here. `revert-commits` mode (the default) is unchanged behaviorally
and keeps `mechanical(pattern)`/`mechanical(chain)`/`mechanical(merge)`
classification; `redeploy-tag` mode is opt-in per crate and never runs the
mechanical classifier. The receipt schema bumps to
`autobuilder.rollback_plan_receipt.v2` (both parents' fields are disjoint
and both now ship in one document); `v1` receipts are still accepted by
the gate during the transition. This version (≥0.7.0) is deliberately
higher than either parent crate's, so "the canonical install" is checkable
by version number alone. `~/wintermute/rustbuild/autobuilder` is now
frozen/ported-from (see its `PORTED.md`) — new autobuilder feature PRDs
target this crate (`~/wintermute/autobuilder`, `--project-root autobuilder`)
only.

## v0.4.1 — 2026-09-07

`tests/publish.rs`'s `ac2_dry_run_no_writes_no_network` test snapshots every file under a test project tree before and after a dry-run publish, then asserts the two snapshots are identical. Twice today (autobuilder CI run 34148731828, rustbuild CI run 34151347561 exercising the same test via a shared code path) the assertion failed only because `.git/objects/maintenance.lock` transiently appeared or disappeared between snapshots — a file git's own background maintenance creates and removes, unrelated to anything the publish dry-run code does. Both failures reproduced as clean passes on immediate local re-run. This PRD makes the snapshot comparator ignore git-internal lock files so the test stops flaking.

## v0.4.0 — 2026-09-07

`rollback-plan` adds two more commit classifications on top of v0.3.0's
three named housekeeping shapes: `mechanical(chain)` — a maximal ≥2-commit
sequence on an allowed same-path set (e.g. `www/`) where each later commit
supersedes the last, with revert-cleanliness checked once against the
chain's terminal commit, not per-link — and `mechanical(merge)` — a
≥2-parent commit verified via a real `git revert --no-commit -m 1` dry-run
in a scratch worktree, clean → excluded from blocking, conflict → stays
`substantive` with the conflicting paths recorded in the receipt. The JSON
receipt gains a `classified` breakdown (`mechanical_pattern`,
`mechanical_chain`, `mechanical_merge`, `substantive` counts) alongside the
existing `mechanical_count`/`blocking_count` fields (additive; no existing
field renamed). On mcphost's live HEAD (v0.13.3..HEAD, 84 commits),
`blocking_count` drops from 26 to 13: the 15-commit `www:` copy-edit chain
(split into a 5- and an 11-commit run by an interleaved Cargo.lock sync)
reclassifies `mechanical(chain)`, and 9 of 17 merge commits reclassify
`mechanical(merge)`; the remaining 13 (8 merges whose `-m 1` revert
genuinely conflicts, plus 5 non-chain non-merge commits) are substantive by
inspection, not a classifier gap — see PRD-rollback-mechanical-chains'
ship note for the itemized list.

## v0.3.0 — 2026-09-07

`rollback-plan` now classifies each checked commit as `mechanical` (subject
matches one of three known housekeeping shapes — intent-card refresh,
Cargo.lock version-field sync, or parallel-integrate version bump — AND its
own changed-file set is a subset of that pattern's expected files) or
`substantive`. `blocking_count`/`verdict` are computed over substantive
commits only, so mechanical commits that are non-revert-clean by
construction (a later commit of the same shape always supersedes an
earlier one) never block the gate. Mechanical commits stay visible in
`rollback.md` (marked `✗(M)`, with a legend) and the JSON receipt gains a
`mechanical_count` field. Fixes the self-reinforcing trap where nothing
could pass the gate, so the rollback base tag never advanced and every
subsequent housekeeping commit piled into an ever-growing false-positive
block (observed on mcphost: 50-80 commits since v0.13.3).

## v0.2.0 — 2026-05-30

Add `autobuilder publish` subcommand that codifies the manual Stage-6 publish pipeline
(README/LICENSE generation, branch normalize to `main`, repo create via `wm-publish`,
push via `wm-push`, `REPOS.md` update) into a deterministic, idempotent, dry-run-capable
command. Shells out to safety wrappers; never calls `gh repo create` or `git push` directly.
Writes a `publish-receipt/v1` receipt to `target/autobuilder/receipts/publish-receipt.json`.
ACs 1–9 hermetic-green; AC10 (live network) deferred per PRD.
