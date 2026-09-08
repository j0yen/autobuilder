# Changelog

## v0.7.2 — 2026-09-08

Commit `30263b9` (v0.7.0 merge of PRD-autobuilder-source-unify) added `icu_collections`, `icu_locale_core`, `icu_normalizer`, `icu_properties`, `icu_provider` @2.3.0 and `idna_adapter` @1.2.2 to `autobuilder/autobuilder/Cargo.lock` — all of which require rustc >=1.86–1.88 — while `autobuilder/autobuilder/rust-toolchain.toml` still pins `1.85.0`. `cargo check --workspace` now fails with exit 101 on a clean checkout, confirmed independently by PRD-autobuilder-dryrun-snapshot-git-lock-flake's re-verification (two fresh local reproductions, receipts under `~/brain/journal/build/receipts/2026-09-08-autobuilder-dryrun-snapshot-git-lock-flake-msrv-break*.txt`) and by GH Actions run `34186177914` failing at the same HEAD. This cascades into `ci-checks`/`msrv-verify`/`vti-plan` gate blocks and most of the 17 extended-gates producers failing to emit receipts under parallel contention. Bump the toolchain pin (fleet precedent already exists: `agorabus`/`agorabus-nats-bridge` and 3 other repos pin `1.88.0`) to unbreak the build.

The actual fix already landed at commit `618ead6` ("autobuilder: pin icu_*/idna_adapter transitive deps to MSRV-1.85-compatible versions"), which took the PRD's documented fallback path — pinning `icu_collections`/`icu_locale_core`/`icu_normalizer`/`icu_properties`/`icu_provider` to 2.1.1/2.1.2 and `idna_adapter` to 1.2.1 via `cargo update -p <pkg> --precise <ver>` — rather than bumping the toolchain channel, since the pinned versions were the narrower fix that restored a green build under the existing 1.85.0 pin. This v0.7.2 bump retroactively versions that already-merged fix: no source changed here beyond `Cargo.toml`'s version field and this CHANGELOG entry. Re-verified fresh at HEAD `7d87baf`: `cargo check --workspace` and `cargo test --release --workspace` both exit 0 on RedBaron.

## v0.7.1 — 2026-09-08

Closes the two gaps the operator found in v0.7.0's port (PRD-autobuilder-source-unify,
both operator notes dated 2026-09-08): the gate/producer pair was still on the
hermetic-build v1 schema, and the extended-gates producers were still on the
0.1.0 lineage rather than the proven-passing 0.1.1 lineage.

- `crates/gate/src/lib.rs`: `hermetic-build`'s `expected_schema` is now
  `autobuilder.hermetic_build_receipt.v2` (PRD-rustbuild-hermetic-scope),
  with `autobuilder.hermetic_build_receipt.v1` accepted as a transition-window
  `alt_schema` — same pattern already in place for `rollback-plan`'s v1→v2.
  Ported the transition test `crates/gate/tests/hermetic_scope_ac6_v1_transition.rs`
  from rustbuild's autobuilder unchanged.
- `crates/extended-gates` bumps `0.1.0` → `0.1.1`, porting every producer
  delta from `~/wintermute/rustbuild/autobuilder`'s 0.1.1 lineage (proven
  passing mcphost's gate at b2655fe 2026-09-08, where this crate's 0.1.0
  producers blocked at cf62c92 on determinism/hermetic-build/msrv-verify/
  ac-traceability/flake-audit):
  - `determinism` and `cold-build-time`: both `cargo clean` invocations now
    redirect `CARGO_TARGET_DIR` to an isolated tempdir instead of the
    project's own `target/`. Previously `cargo clean` on the project's real
    target dir wiped `target/autobuilder/receipts/` — every other
    producer's already-written receipt — mid-run, which is what made
    unrelated receipts (msrv-verify, ac-traceability, flake-audit) read as
    missing downstream of determinism in producer order.
  - `hermetic-build`: rewritten producer + `--strict` CLI flag
    (PRD-rustbuild-hermetic-scope) — per-socket pid/comm/remote attribution
    instead of bare-string `new_sockets` entries, writes schema v2.
  - `ac-traceability`: `locate_prd` now falls back to the project's parent
    directory for a nested-crate layout (`--project-root autobuilder`);
    AC-id extraction now also parses numbered `## Acceptance criteria`
    lines (`1. P0 — Given …`), not just `AC<N>`-token form, and accepts
    ac-judge's `tests/ac<N>_*.rs` / `tests/ac<0N>_*.rs` file-naming as
    coverage pairing.
  - `secrets-scan`: optional `extended-gates.toml::secrets_scan_allowlist`
    (path-glob array) to skip planted-fixture files during self-hosted runs.
  - `mutation-kill`: comment/string-aware candidate-site scanner (skips
    mutation sites inside comments and string/char literals, which would
    misreport as surviving mutants) trying up to 5 sites per operator.
  Ported test files: `acceptance_ac_traceability.rs`,
  `acceptance_ac_x1_helps.rs`, `acceptance_heavy_ignored.rs`,
  `acceptance_secrets_scan_control.rs` (new), `fixtures.rs`, and the eight
  `hermetic_scope_ac{1,2,3,4,5,7,8}_*.rs` fixture tests (AC6 lives in the
  gate crate, above).
- Both parents' rollback lineages plus every ported hermetic/producer test
  pass together in one `cargo test --workspace` at the nested root.

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
inspection, not a classifier gap, satisfying AC5's escape clause (counts
by class, in place of a since-superseded per-commit list: mcphost has
since moved to `redeploy-tag` rollback mode, so the exact commit set this
breakdown was computed against is no longer the live check for that repo).

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
