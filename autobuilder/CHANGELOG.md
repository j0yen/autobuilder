# Changelog

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
