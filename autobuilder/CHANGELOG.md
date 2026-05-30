# Changelog

## v0.2.0 — 2026-05-30

Add `autobuilder publish` subcommand that codifies the manual Stage-6 publish pipeline
(README/LICENSE generation, branch normalize to `main`, repo create via `wm-publish`,
push via `wm-push`, `REPOS.md` update) into a deterministic, idempotent, dry-run-capable
command. Shells out to safety wrappers; never calls `gh repo create` or `git push` directly.
Writes a `publish-receipt/v1` receipt to `target/autobuilder/receipts/publish-receipt.json`.
ACs 1–9 hermetic-green; AC10 (live network) deferred per PRD.
