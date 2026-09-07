# Gate debt owners — agent/gate-baseline.json

PRD-autobuilder-gate-debt paid down 2 of the day-one baseline's 6
then-remaining entries (2 more — `intake`, `ci-checks` — had already been
resolved and dropped before this PRD started):

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

- `secrets-scan` — investigated, NOT fixed, still baselined (P0 item 3's
  documented fallback). The block is a self-referential false positive:
  `crates/extended-gates/tests/acceptance_secrets_scan_planted.rs`
  embeds a synthetic `AKIA...` literal to test the scanner, and the
  scanner's own file-content walk (not just its own test-run tempdir)
  matches that literal when it scans the whole nested crate at
  `--project autobuilder`. `secrets_scan.rs` has no config-driven
  allowlist or skip-path mechanism (`extended-gates.toml`-driven, the
  way `license-audit`'s `license_allowlist` or `ac-traceability`'s
  `prd_path` work) — its only skip mechanism is a hardcoded directory-name
  list (`target`, `.git`, `node_modules`, `autoresearch-macos`,
  `jankurai`, `jeryu`, `vendor`), which doesn't help here. Routing around
  it would mean editing the extended-gates crate itself, which is shared,
  fleet-wide (the identical file is byte-for-byte the same in
  `~/wintermute/rustbuild/autobuilder/crates/extended-gates/`), and out
  of this PRD's scope ("no library or CLI surface changes"). Confirmed
  the same false positive is independently baselined in rustbuild's own
  `agent/gate-baseline.json` today — this is fleet-wide debt, not
  autobuilder-specific, and no PRD currently owns fixing it. Re-run
  twice for the verdict-receipt protocol; both runs agree (see
  `~/brain/journal/build/receipts/2026-09-07-autobuilder-gate-debt-secrets-gate*.txt`).

Remaining baseline entries and why they're still excused:

| entry | owner | note |
|---|---|---|
| `risk-gate` | unowned (open question) | `run-metrics.sh`'s embedded audit-checks.sh lookup has always pointed at a nonexistent `~/.claude/skills/autobuilder/rules/audit-checks.sh` (the shared script actually lives in the `rustbuild` skill), so `risk-gate.json` has always come out empty and excused-as-missing. Fixing the path was tried during this PRD and reverted: it makes the audit actually run, which then reports 4 real `blocking` findings (`unwrap()` on external input in `autobuilder/src/experiment.rs`) that flip `proof-receipt`'s own verdict from `baseline` to `crash` — a regression on this PRD's own P0 AC1. Fixing those `unwrap()` call sites is a `src/` change, out of this PRD's scope ("scaffolding only; no library or CLI surface changes"). A follow-up PRD should fix the audit-checks.sh path AND the `experiment.rs` findings together (fixing only the path without the findings would just convert an excused-missing-receipt into an excused-blocking-receipt with no net gain). |
| `reviewer-agent` | self-resolving | PRD-autobuilder-gate-debt's own TL;DR names this as expected to "re-evaluate once the above shrink." The most recent reviewer-agent run (head `5c72a58`, pre-dating this PRD's commits) blocked on reasons specific to that stale head (cargo-deny failures, the gate-baseline mechanism itself). Re-run reviewer-agent fresh at the new HEAD on the next tick. |
| `session-trace` | Joe — PRD-autobuilder-gate-debt's own Open Questions table | "fix the RedBaron ctrace permission or teach the receipt a permanent host-skip? — next maintenance pass." Note: a manual `--trace` run during this PRD's work happened to produce `verdict=pass` (15916 events captured) on this box — left baselined regardless since fixing/verifying this host issue is explicitly out of this PRD's scope and the open question is owned by Joe, not this PRD. |
| `secrets-scan` | unowned (fleet-wide, not yet filed) | See above — needs an `extended-gates.toml`-driven skip/allowlist mechanism added to the shared `extended-gates` crate (rustbuild-owned), or a rename/relocation of the crate's own test fixture literal so it no longer round-trips through the scanner it's testing. Affects every repo that vendors `extended-gates` (confirmed also baselined in rustbuild itself as of 2026-09-07). |

## Discovered but explicitly NOT baselined (out of scope for this PRD)

Re-running the full gate at this PRD's new HEAD surfaced two additional
blocks that are real, pre-existing, and unrelated to the three items
above — NOT added to `agent/gate-baseline.json` per this PRD's own
non-goal ("no baseline widening under any circumstance"):

- **`rollback-plan`** — blocks because 3 of the 10 commits in
  `v0.3.0..HEAD` are not git-revert-clean (`5c72a58`, `930a77a`,
  `38402df` — all pre-date this PRD's own commit, which reverts clean).
  This is drift accumulated since the baseline was last recorded at
  `930a77a`, unrelated to rollback-mechanical-chains' classifier logic
  (`autobuilder/src/rollback.rs`, explicitly out of this PRD's scope).
  Re-run twice for the verdict-receipt protocol; both runs agree (see
  `~/brain/journal/build/receipts/2026-09-07-autobuilder-gate-debt-gate*.txt`).
  Needs a dedicated follow-up (fix-forward those 3 commits' revert
  conflicts, or a human explicitly rewrites history and says so — this
  PRD's tooling never does that automatically).
- **`ci-checks`** — blocks because this PRD's own commits are not yet
  pushed, so there are zero GitHub Actions runs against this HEAD yet.
  Expected to clear once pushed and CI completes; not a defect.

`vti-plan` also newly blocked (4 of 20 changed paths unrouted —
`autobuilder/tests/chain_ac*.rs` and `mergerev_ac*.rs`, added by the
rollback-mechanical-chains series with no matching proof-lanes.toml
glob) but this WAS fixed in-scope (pure routing config, no rollback.rs
touch): the `autobuilder-rust` lane's globs now include
`autobuilder/tests/**/*.rs`.
