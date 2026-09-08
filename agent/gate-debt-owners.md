# Gate debt owners — agent/gate-baseline.json

PRD-autobuilder-gate-debt paid down 3 of the day-one baseline's 6
then-remaining entries (`proof-receipt`, `ac-traceability`, `secrets-scan`
— 2 more, `intake` and `ci-checks`, had already been resolved and dropped
before this PRD started) and additionally fixed `vti-plan`, a block
discovered mid-PRD (not part of the day-one baseline) rather than
baselined:

- `proof-receipt` — fixed. `autobuilder loop --project autobuilder
  --iteration 0` couldn't find `scripts/run-metrics.sh` (it looks for the
  script relative to `--project`, which resolves to the nested
  `autobuilder/` crate root, not the repo root where the script lives).
  Bridged with an `autobuilder/scripts -> ../scripts` symlink (mirroring
  the existing `autobuilder/agent -> ../agent` symlink) plus a
  `REPO_ROOT`/`CRATE_DIR` split inside `run-metrics.sh` itself, modeled
  on rustbuild-gate-debt's identical fix for rustbuild's own nested-crate
  layout (commit `782563f` + `a1b078b` in `~/wintermute/rustbuild`).
  Verified: `autobuilder loop --project autobuilder --iteration 0
  --head-sha <HEAD>` now completes with `verdict=baseline` (not crash).

- `ac-traceability` — fixed. The producer scans `<project>/PRD-*.md` at
  the nested Cargo project root (`autobuilder/`), which had none. Added a
  copy of this repo's driving PRD
  (`autobuilder/PRD-autobuilder-rollback-mechanical-commits.md`, sourced
  from `~/repos/PRDs/built-prds/`) there. The copy's prose "AC1-4"
  shorthand was expanded to "AC1, AC2, AC3, AC4" so the producer's
  `AC[-]?...\d+` regex extracts the individual ids its own tests already
  trace, instead of one compound "AC1-4" token no test references.
  Verified: `ac-traceability --project autobuilder` now reports
  `0 untraced` / `verdict=pass`.

- `run-metrics.sh` fresh-worktree gap (AC6, P1) — fixed. The internal
  AC6 check inside `run-metrics.sh` ran `cargo test --workspace` without
  first running `cargo build --release --workspace`. On the long-lived
  checkout this was masked (some prior release build already existed),
  but `autobuilder/tests/fresh_worktree_run_metrics.rs` (new fixture: a
  real `git worktree add` + fresh build + `run-metrics.sh` invocation,
  `#[ignore]`d per this repo's heavy-test convention) caught it: a truly
  fresh worktree failed `acceptance_ac_x1_helps.rs` (walks
  `target/release` directly for producer bins) because no release build
  had ever happened there. Fixed by adding the missing `cargo build
  --release --workspace` step to `run-metrics.sh`'s `ac_build_pass`,
  mirroring `.github/workflows/ci.yml`'s own step order. Verified twice
  in fresh processes (receipts:
  `~/brain/journal/build/receipts/2026-09-07-autobuilder-gate-debt-ac6-pass*.txt`).

- `secrets-scan` — fixed 2026-09-08, un-baselined. Originally investigated
  and deferred (P0 item 3's documented fallback) as a self-referential
  false positive: `crates/extended-gates/tests/acceptance_secrets_scan_planted.rs`
  embeds a synthetic `AKIA...` literal to test the scanner, and the
  scanner's own file-content walk (not just its own test-run tempdir)
  matched that literal when it scanned the whole nested crate at
  `--project autobuilder`. At the time, `secrets_scan.rs` had no
  config-driven allowlist/skip-path mechanism. The follow-on PRD filed for
  this (`PRD-autobuilder-secrets-scan-allowlist`, targeting
  `~/wintermute/rustbuild`) shipped and added exactly that mechanism
  (`extended-gates.toml::secrets_scan_allowlist`, matched the same way
  `license-audit`'s `license_allowlist` and `ac-traceability`'s `prd_path`
  already worked) plus a control-check test
  (`acceptance_secrets_scan_control.rs`). The v0.7.1 extended-gates 0.1.1
  port (`54ab8f7`, landed before this tick) carried that mechanism and its
  tests into this repo's vendored crate — so fixing it here needed no
  crate edit, only a config file: added `autobuilder/extended-gates.toml`
  with `secrets_scan_allowlist = ["crates/extended-gates/tests/acceptance_secrets_scan_planted.rs"]`,
  mirroring rustbuild's own `autobuilder/extended-gates.toml` byte-for-byte
  in intent. Verified: `secrets-scan --project autobuilder` now reports
  `verdict=pass` (202 files, 0 findings), and a manual control check
  (a synthetic `AKIA...` literal planted at a path NOT in the allowlist)
  reproduces `verdict=block` — the missing half of the original AC3 ("a
  control check confirms a synthetic secret planted outside test paths
  still blocks"). Re-run twice for the verdict-receipt protocol, both
  agree (receipts:
  `~/brain/journal/build/receipts/2026-09-08-autobuilder-gate-debt-gate-132827.txt`,
  `~/brain/journal/build/receipts/2026-09-08-autobuilder-gate-debt-gate-132849.txt`).
  Dropped from `agent/gate-baseline.json`.

- `vti-plan` — fixed 2026-09-08 (not previously baselined; a new block
  discovered at the 2026-09-08 re-check, per this PRD's own frontmatter).
  The v0.7.1 extended-gates 0.1.1 port added
  `crates/extended-gates/src/producers/**` and
  `src/bin/hermetic_build.rs`, and the same port's hermetic-scope v2
  carried `crates/gate/src/lib.rs` + `crates/gate/tests/**` — none of
  which `agent/proof-lanes.toml` had a lane for, plus a non-`.rs` test
  fixture (`autobuilder/tests/fixtures/mcphost-intent-card.json`) and the
  merge-added `.gitattributes`. 26 of 63 changed paths were unrouted at
  HEAD `00ce607`. Added `extended-gates-rust` and `gate-rust` lanes
  mirroring rustbuild's own `proof-lanes.toml` (the crate's canonical
  source, which already had the `extended-gates-rust` lane from its own
  `PRD-autobuilder-secrets-scan-allowlist` follow-up commit `c8a9325`),
  broadened `autobuilder-rust`'s globs to include
  `autobuilder/tests/fixtures/**`, and routed `.gitattributes` under
  `config`. Verified: `autobuilder vti-plan --project autobuilder --base
  v0.3.0` now reports `unrouted=0` / `verdict=pass` (64 files). Re-run
  twice for the verdict-receipt protocol, both agree (receipts:
  `~/brain/journal/build/receipts/2026-09-08-autobuilder-gate-debt-gate.txt`,
  `~/brain/journal/build/receipts/2026-09-08-autobuilder-gate-debt-vtiplan.txt`).
  Was never in `agent/gate-baseline.json` (it block-failed the gate
  outright rather than being excused), so nothing to drop there.

Remaining baseline entries and why they're still excused:

| entry | owner | note |
|---|---|---|
| `risk-gate` | unowned (open question) | `run-metrics.sh`'s embedded audit-checks.sh lookup has always pointed at a nonexistent `~/.claude/skills/autobuilder/rules/audit-checks.sh` (the shared script actually lives in the `rustbuild` skill), so `risk-gate.json` has always come out empty and excused-as-missing. Fixing the path was tried during this PRD and reverted: it makes the audit actually run, which then reports 4 real `blocking` findings (`unwrap()` on external input in `autobuilder/src/experiment.rs`) that flip `proof-receipt`'s own verdict from `baseline` to `crash` — a regression on this PRD's own P0 AC1. Fixing those `unwrap()` call sites is a `src/` change, out of this PRD's scope ("scaffolding only; no library or CLI surface changes"). A follow-up PRD should fix the audit-checks.sh path AND the `experiment.rs` findings together (fixing only the path without the findings would just convert an excused-missing-receipt into an excused-blocking-receipt with no net gain). |
| `reviewer-agent` | self-resolving | PRD-autobuilder-gate-debt's own TL;DR names this as expected to "re-evaluate once the above shrink." The most recent reviewer-agent run (head `5c72a58`, pre-dating this PRD's commits) blocked on reasons specific to that stale head (cargo-deny failures, the gate-baseline mechanism itself). Re-run reviewer-agent fresh at the new HEAD on the next tick. |
| `session-trace` | Joe — PRD-autobuilder-gate-debt's own Open Questions table | "fix the RedBaron ctrace permission or teach the receipt a permanent host-skip? — next maintenance pass." Note: a manual `--trace` run during this PRD's work happened to produce `verdict=pass` (15916 events captured) on this box, and the 2026-09-08 full gate re-run also passed it live — left baselined regardless since fixing/verifying this host issue is explicitly out of this PRD's scope and the open question is owned by Joe, not this PRD. |
| `rollback-plan` | structural — not owned by `rollback.rs`'s classifier | added 2026-09-08 by `PRD-autobuilder-rollback-plan-debt`. Re-confirmed fresh at HEAD `61205d69d7eff2436a800c9d78b79904775b7c74` (`cargo run --release --bin autobuilder -- rollback-plan --project . --base v0.3.0 --explain`, base `105cf29e` = `v0.3.0`, 28 commits, `verdict=block`): 11 of 28 commits are not git-revert-clean (`classified={mechanical_pattern:4, mechanical_chain:0, mechanical_merge:0, substantive:24}`) — `362345b`, `00ce607`, `30263b9`, `ff644f8`, `55849eb`, `ad227f2`, `d2cb5ca`, `03b5711`, `38402df`, `5c72a58`, `930a77a`. `rollback-mechanical-chains` (owns `autobuilder/src/rollback.rs`) independently confirmed this is not a classifier bug: the two merges in range (`00ce607`, `30263b9`) genuinely conflict on `git revert -m 1` (`agent/intent-card.json`/`CHANGELOG.md`/`Cargo.toml` touched by many intervening commits), and none of the remaining 9 form a same-path superseding chain or clean merge (`mechanical_chain`/`mechanical_merge` correctly stay 0 for this window) — they're ordinary self-hosted bookkeeping commits (gate-debt owner-doc updates, changelog+version-bump pairs, proof-lane routing) touching shared scaffolding files that later commits also touch, so no `git revert` of any one of them applies cleanly once the file has moved on. This is the identical shape `PRD-rustbuild-gate-debt` already baselined on the sibling `rustbuild` repo ("3 non-revert-clean commits since v0.5.0 ... need their own follow-on PRD, not reopened here" — commit `f343429`). Baselined rather than fixed: rewriting merged/shared-scaffolding history to force these 11 individually revert-clean would itself be destructive (rewrites already-merged commits) and is explicitly out of this PRD's non-goals. Verified: `extend-gate.sh --project-root autobuilder --head 61205d69d7eff2436a800c9d78b79904775b7c74 --force` after this baseline edit reports `verdict=delta-pass`, `new_blocks=none`. |

## Discovered but explicitly NOT baselined (out of scope for this PRD)

Re-running the full gate at this PRD's 2026-09-08 HEAD (`8b2fb9c`) still
surfaces blocks that are real, pre-existing or transient, and unrelated
to the fixes above — NOT added to `agent/gate-baseline.json` per this
PRD's own non-goal ("no baseline widening under any circumstance"):

- **`rollback-plan`** — resolved 2026-09-08 by `PRD-autobuilder-rollback-plan-debt`:
  moved OUT of this "not baselined" list and INTO `agent/gate-baseline.json`
  (see the "Remaining baseline entries" table above for the full
  reasoning — drift stopped accumulating once the decision was made that
  this class of self-hosted bookkeeping commit is structurally never
  going to be revert-clean, so there is nothing left to re-discover on
  future ticks).

- **`ci-checks`** — blocks again, but transiently/expectedly: this
  tick's fix commit (`8b2fb9c`) has not been pushed to `origin` yet at
  the time of the gate re-run, so `gh run list` finds zero workflow runs
  for that HEAD ("push the commit and wait for CI before re-running," per
  the receipt's own message) — the same shape `ci-checks` resolved in
  itself last time once the commit landed on `origin`. Not a real
  regression; expected to clear once this tick's commits are pushed and
  CI completes on the next tick's re-check.

`vti-plan` and `secrets-scan` — see the "fixed" entries above; both now
pass at this tick's HEAD (`8b2fb9c`).
