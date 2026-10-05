//! Stage 4 — rollback-plan receipt.
//!
//! Two rollback models, selected per crate (PRD-autobuilder-rollback-tag-aware):
//!
//! - `revert-commits` (default): walks every commit in `<base>..HEAD` and
//!   verifies that `git revert` would apply cleanly onto `HEAD`. The right
//!   criterion for a library crate. Commits are further classified
//!   `mechanical` (pattern / chain / merge) or `substantive`
//!   (PRD-autobuilder-rollback-mechanical-commits,
//!   PRD-rollback-mechanical-chains) — only substantive, non-revert-clean
//!   commits count toward `blocking_count`/`verdict`.
//! - `redeploy-tag`: for a deploy-gated service whose real rollback is
//!   redeploying the previous tagged version (never a git revert), the
//!   verdict instead depends on the base tag existing, the v-tag lineage
//!   from base to HEAD being contiguous, and HEAD being tagged or taggable.
//!   Merge commits and non-revert-clean commits never affect this verdict.
//!   Exception (PRD-autobuilder-rollback-tag-lineage-head-gap): an untagged
//!   entry that is also the newest entry in the lineage (nothing has bumped
//!   the version since) is not a lineage gap — it's HEAD's current version,
//!   the ordinary shape right after a version bump and before `ship-tag.sh`
//!   has tagged a passing gate — and is left to the existing HEAD tagged/
//!   taggable/blocked check below instead of blocking on `tag-lineage-gap`.
//!   Deliberately keyed on "newest lineage entry", not "entry's commit ==
//!   HEAD": HEAD is often a few commits past the actual bump (an
//!   intent-card refresh, a Cargo.lock sync) that never touch the version.
//!
//! Emits `target/autobuilder/rollback.md` (human-readable) and
//! `target/autobuilder/receipts/rollback-plan.json` (the gate receipt,
//! schema `autobuilder.rollback_plan_receipt.v2`; `v1` is still accepted by
//! the gate during the transition). `revert-commits` mode uses
//! `git merge-tree --write-tree` so the per-commit check is read-only and
//! never touches the working tree, except for merge commits (see
//! `merge_revert_m1_dry_run`), which need a real `git revert -m 1` and so
//! run in a scratch worktree that is always torn down before returning;
//! `redeploy-tag` mode never runs either check at all.
//!
//! `revert-commits` mode also runs a range-level check (`check_range_revert`):
//! does `base..HEAD`, reverted as a *single* change, apply cleanly onto
//! HEAD? That is the shape of the daemon's real future revert once a branch
//! lands — every branch lands as ONE squash commit (`gh pr merge --squash`),
//! so the actual rollback is `git revert <squash-sha>` against the whole
//! range, never a per-commit revert of each original commit. Under the
//! default `--strategy squash`, the verdict follows this range-level result
//! (falling back to the per-commit rule only when the range itself is not
//! individually revertable too), which no longer false-blocks a branch whose
//! stacked commits touch overlapping lines even though the net diff reverts
//! cleanly. `--strategy merge` restores the original per-commit-only
//! verdict. The per-commit table and tag-lineage/mechanical-commit checks
//! are unaffected either way.

use crate::receipt;
use anyhow::{Context, Result, anyhow};
use clap::Args as ClapArgs;
use clap::ValueEnum;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Which land path `revert-commits` mode's verdict is evaluated against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Strategy {
    /// The daemon's actual land path: `base..HEAD` becomes one squash
    /// commit, so the verdict follows whether that range reverts cleanly
    /// as a unit (falling back to the per-commit rule if every commit is
    /// individually revertable too).
    #[value(name = "squash")]
    Squash,
    /// Legacy behavior: ignore the range-level check, verdict is `pass`
    /// only when every commit in the range is individually revertable.
    #[value(name = "merge")]
    Merge,
}

impl Strategy {
    fn as_str(self) -> &'static str {
        match self {
            Strategy::Squash => "squash",
            Strategy::Merge => "merge",
        }
    }
}

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Project directory. The git repo at this path is the one inspected.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// Range start: commits in `<base>..HEAD` are checked. Defaults to the
    /// newest `v<major>.<minor>.<patch>` tag reachable from HEAD by
    /// first-parent; if no such tag exists, falls back to the crate's
    /// initial commit. An explicit `--base` always wins over the default.
    /// In `redeploy-tag` mode, `--base` names the previous deployed
    /// version's tag directly.
    #[arg(long)]
    pub base: Option<String>,

    /// Print the resolved rollback model, base tag, and rollback target on
    /// one human-readable line, in addition to the normal run.
    #[arg(long)]
    pub explain: bool,

    /// One-time helper: print the `rollback_model` config line to paste
    /// into `agent/AUTOBUILDER_PROGRAM.md` (or `agent/intent-card.json`) to
    /// opt a deploy-gated crate into `redeploy-tag` mode. Exits immediately
    /// after printing; performs no git inspection and writes no receipt.
    #[arg(long)]
    pub migrate_note: bool,

    /// `revert-commits` mode only: land strategy the verdict is evaluated
    /// against: `squash` (default, matches the daemon's `gh pr merge
    /// --squash`) or `merge` (per-commit-only, the original behavior).
    /// Has no effect in `redeploy-tag` mode.
    #[arg(long, value_enum, default_value = "squash")]
    pub strategy: Strategy,
}

/// The rollback verification strategy a crate uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RollbackModel {
    /// Today's behavior: every commit in `base..HEAD` must `git revert`
    /// cleanly (mechanical commits excepted). The right model for a library
    /// crate.
    RevertCommits,
    /// The 2026-09-05 fix-forward ruling for deploy-gated services:
    /// rollback means redeploying the previous tagged version, so the
    /// verdict depends on tag lineage, not per-commit revertability.
    RedeployTag,
}

impl RollbackModel {
    fn as_str(self) -> &'static str {
        match self {
            Self::RevertCommits => "revert-commits",
            Self::RedeployTag => "redeploy-tag",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "revert-commits" => Some(Self::RevertCommits),
            "redeploy-tag" => Some(Self::RedeployTag),
            _ => None,
        }
    }
}

/// A commit's classification. Extends additively — never rename an existing
/// variant, per PRD-rollback-mechanical-chains requirement 3 (the JSON
/// receipt already shipped `mechanical`/`substantive` as a bool via
/// `CommitEntry::mechanical`, which stays for compatibility; `class` is the
/// new, more granular field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CommitClass {
    /// Matches one of the three named housekeeping shapes from
    /// PRD-autobuilder-rollback-mechanical-commits (intent-card refresh,
    /// Cargo.lock version-field sync, parallel-integrate version bump).
    MechanicalPattern,
    /// Part of a maximal ≥2-commit same-allowed-path chain where each later
    /// commit supersedes the last (PRD-rollback-mechanical-chains P0.1).
    MechanicalChain,
    /// A ≥2-parent commit whose `git revert -m 1` dry-run is clean
    /// (PRD-rollback-mechanical-chains P0.2).
    MechanicalMerge,
    /// Everything else — counts toward `blocking_count` when not
    /// revert-clean.
    Substantive,
}

impl CommitClass {
    fn is_mechanical(self) -> bool {
        !matches!(self, CommitClass::Substantive)
    }
}

#[derive(Debug, Serialize)]
struct CommitEntry {
    sha: String,
    short_sha: String,
    subject: String,
    parent_count: usize,
    revertable: bool,
    /// True when `class != substantive`. Mechanical commits never count
    /// toward `blocking_count`/`verdict`. Kept (not renamed) for
    /// compatibility with the receipt shipped by
    /// PRD-autobuilder-rollback-mechanical-commits; see `class` for the
    /// breakdown.
    mechanical: bool,
    /// Granular classification — see `CommitClass`.
    class: CommitClass,
    note: String,
}

/// Per-class commit counts (PRD-rollback-mechanical-chains requirement 3).
/// Additive alongside `mechanical_count`/`blocking_count`, not a replacement.
/// Only populated in `revert-commits` mode.
#[derive(Debug, Default, Serialize)]
struct Classified {
    mechanical_pattern: usize,
    mechanical_chain: usize,
    mechanical_merge: usize,
    substantive: usize,
}

impl Classified {
    fn record(&mut self, class: CommitClass) {
        match class {
            CommitClass::MechanicalPattern => self.mechanical_pattern += 1,
            CommitClass::MechanicalChain => self.mechanical_chain += 1,
            CommitClass::MechanicalMerge => self.mechanical_merge += 1,
            CommitClass::Substantive => self.substantive += 1,
        }
    }
}

/// `redeploy-tag`-mode rollback target: the previous tagged version, its
/// commit, and the redeploy command form.
#[derive(Debug, Clone, Serialize)]
struct RollbackTarget {
    tag: String,
    sha: String,
    redeploy_command: String,
}

/// One version-bump commit found while walking tag lineage: the version it
/// bumped to, and the matching tag when one exists (`None` is the
/// `tag-lineage-gap` signal).
#[derive(Debug, Clone, Serialize)]
struct TagLineageEntry {
    version: String,
    tag: Option<String>,
    commit_sha: String,
    commit_short: String,
}

#[derive(Debug, Serialize)]
struct ReceiptDoc {
    schema: &'static str,
    head_sha: String,
    base_ref: String,
    base_sha: String,
    /// The `v<major>.<minor>.<patch>` tag the default-base resolution
    /// picked, when it picked one. `None` when `--base` was given
    /// explicitly, or when no matching tag exists on first-parent history.
    /// In `redeploy-tag` mode this is always the previous deployed tag when
    /// one was resolved.
    base_tag: Option<String>,
    /// Set to `"base=initial (no tags)"` when the default resolution found
    /// no matching tag and fell back to the crate's initial commit.
    base_note: Option<String>,
    /// v2: `redeploy-tag` only — the commit `v<base_version>` actually
    /// points at, when the base was accepted as released. Equal to
    /// `base_sha` on the exact-match fast path; an ancestor of `base_sha`
    /// when the base is an un-bumped hotfix past its release tag
    /// (PRD-autobuilder-rollback-tag-ancestor-ok). `None` on every block.
    #[serde(skip_serializing_if = "Option::is_none")]
    base_tag_commit: Option<String>,
    /// v2: `redeploy-tag` only — commits between `base_tag_commit` and
    /// `base_sha` (`0` on the exact-match fast path). `None` on every
    /// block.
    #[serde(skip_serializing_if = "Option::is_none")]
    base_tag_distance: Option<usize>,
    rollback_md: String,
    commit_count: usize,
    revertable_count: usize,
    /// `revert-commits` mode only — commits classified `mechanical`
    /// (revert-clean or not). Always `0` in `redeploy-tag` mode.
    mechanical_count: usize,
    blocking_count: usize,
    /// `revert-commits` mode only — per-class breakdown, see `Classified`.
    /// Always the zero value in `redeploy-tag` mode.
    classified: Classified,
    verdict: &'static str,
    commits: Vec<CommitEntry>,
    captured_at: String,
    receipt_digest: String,
    /// v2: which rollback model produced this receipt. Always present.
    rollback_model: &'static str,
    /// v2: `redeploy-tag` only — the previous tag to redeploy to on pass.
    #[serde(skip_serializing_if = "Option::is_none")]
    rollback_target: Option<RollbackTarget>,
    /// v2: `redeploy-tag` only — every version-bump commit found in range
    /// and whether it carries a matching tag.
    #[serde(skip_serializing_if = "Option::is_none")]
    tag_lineage: Option<Vec<TagLineageEntry>>,
    /// v2: `redeploy-tag` only — machine-readable block reason
    /// (`no-previous-tag` | `tag-lineage-gap` | `head-untagged`).
    #[serde(skip_serializing_if = "Option::is_none")]
    block_reason: Option<&'static str>,
    /// v2: `redeploy-tag` only — human detail for `block_reason`.
    #[serde(skip_serializing_if = "Option::is_none")]
    block_detail: Option<String>,
    /// v2: `redeploy-tag` only — count of in-range, untagged, non-newest
    /// lineage entries forgiven because their version was already tagged
    /// on a DIFFERENT commit (a concurrent land took the version number
    /// first; this branch's own bump for it was superseded by a later
    /// re-bump and was never going to carry the tag itself). Always `0`
    /// outside `redeploy-tag` mode and on every block verdict.
    #[serde(default)]
    superseded: usize,
    /// v2: `redeploy-tag` only — machine-readable reason a head-tag
    /// collision check still PASSED (PRD-autobuilder-rollback-tag-no-bump-
    /// in-range). Today the only value is `no-bump-in-range`. `None` on a
    /// normal pass (a real redeploy target, no collision to forgive) and on
    /// every block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pass_reason: Option<&'static str>,
    /// v2: `redeploy-tag`, `no-bump-in-range` pass only — HEAD's own
    /// `[package].version`. `None` otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    head_version: Option<String>,
    /// v2: `redeploy-tag`, `no-bump-in-range` pass only — the base's
    /// `[package].version` (equal to `head_version`, by definition of this
    /// reason). `None` otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    base_version: Option<String>,
    /// v2: `redeploy-tag`, `no-bump-in-range` pass only — always `0`;
    /// present so the receipt states explicitly that nothing bumped in
    /// range, not merely that nothing blocked. `None` otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    range_bumps: Option<usize>,
    /// `revert-commits` mode only — whether `base..HEAD`, reverted as a
    /// single change (the shape of the daemon's real future `git revert
    /// <squash-sha>`), applies cleanly onto HEAD. `None` in `redeploy-tag`
    /// mode, where this check never runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    range_revertable: Option<bool>,
    /// `revert-commits` mode only — which land strategy the verdict was
    /// evaluated against (`--strategy`): `"squash"` or `"merge"`. `None`
    /// in `redeploy-tag` mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    land_strategy: Option<&'static str>,
    /// `revert-commits` mode only — conflicting paths from the range-revert
    /// check, populated only when `range_revertable` is `false`. `None`
    /// otherwise and in `redeploy-tag` mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    range_conflict_files: Option<Vec<String>>,
}

/// Result of checking whether `base..HEAD`, reverted as a single unit (the
/// shape of the daemon's eventual squash revert), applies cleanly onto
/// HEAD.
struct RangeRevertCheck {
    range_revertable: bool,
    conflicting_files: Vec<String>,
    detail: String,
}

/// Resolution of the rollback base when `--base` was not given explicitly.
struct DefaultBase {
    /// Ref (tag name or commit sha) to diff against.
    git_ref: String,
    /// The matching tag, when one was found.
    tag: Option<String>,
    /// Human-readable note for the receipt/markdown when no tag was found.
    note: Option<String>,
}

/// A tag name shaped exactly like `v<major>.<minor>.<patch>` (all-digit
/// components, no pre-release/build suffix).
fn is_version_tag(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix('v') else {
        return false;
    };
    let parts: Vec<&str> = rest.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Sort key for version tags: `v1.2.3` -> `(1, 2, 3)`. Malformed components
/// (should not occur given `is_version_tag` already filtered) sort as 0.
fn version_key(tag: &str) -> (u64, u64, u64) {
    let rest = tag.strip_prefix('v').unwrap_or(tag);
    let mut parts = rest.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    let major = parts.next().unwrap_or(0);
    let minor = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);
    (major, minor, patch)
}

/// Walk first-parent history from HEAD backward and return the newest
/// `v<major>.<minor>.<patch>` tag encountered (i.e. the first commit, in
/// that walk, that any such tag points at). `None` when no commit on the
/// first-parent path carries a matching tag.
fn newest_reachable_tag(project: &Path) -> Result<Option<String>> {
    let commits = run_git(project, &["rev-list", "--first-parent", "HEAD"])?;
    for sha in commits.lines() {
        let sha = sha.trim();
        if sha.is_empty() {
            continue;
        }
        let tags_out = run_git(project, &["tag", "--points-at", sha])?;
        let mut candidates: Vec<&str> = tags_out
            .lines()
            .map(str::trim)
            .filter(|t| is_version_tag(t))
            .collect();
        if candidates.is_empty() {
            continue;
        }
        candidates.sort_by_key(|t| std::cmp::Reverse(version_key(t)));
        if let Some(best) = candidates.first() {
            return Ok(Some((*best).to_owned()));
        }
    }
    Ok(None)
}

/// Resolve the default rollback base (used when `--base` was not given):
/// the newest reachable version tag, or the crate's initial commit.
fn resolve_default_base(project: &Path) -> Result<DefaultBase> {
    if let Some(tag) = newest_reachable_tag(project)? {
        return Ok(DefaultBase {
            git_ref: tag.clone(),
            tag: Some(tag),
            note: None,
        });
    }
    let initial = run_git(project, &["rev-list", "--max-parents=0", "HEAD"])?;
    let initial_sha = initial
        .lines()
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("could not resolve initial commit for {}", project.display()))?;
    Ok(DefaultBase {
        git_ref: initial_sha,
        tag: None,
        note: Some("base=initial (no tags)".to_owned()),
    })
}

/// The `[package]` fields this producer reads from a `Cargo.toml`. Unknown
/// keys elsewhere in the document are ignored by serde's default behavior.
#[derive(Debug, Deserialize)]
struct CargoPackageFields {
    name: Option<String>,
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CargoTomlDoc {
    package: Option<CargoPackageFields>,
}

/// Read and parse `Cargo.toml` as it existed at `sha`, via `git show` (never
/// touches the working tree). `None` on any failure (missing file at that
/// commit, invalid TOML, no `[package]` table) — callers treat that as "not
/// determinable", not a hard error.
fn read_cargo_toml_at(project: &Path, sha: &str) -> Option<CargoTomlDoc> {
    let (status, stdout, _stderr) =
        run_git_capturing(project, &["show", &format!("{sha}:Cargo.toml")]).ok()?;
    if status != 0 {
        return None;
    }
    toml::from_str(&stdout).ok()
}

fn cargo_version_at(project: &Path, sha: &str) -> Option<String> {
    read_cargo_toml_at(project, sha).and_then(|c| c.package).and_then(|p| p.version)
}

fn cargo_name_at(project: &Path, sha: &str) -> Option<String> {
    read_cargo_toml_at(project, sha).and_then(|c| c.package).and_then(|p| p.name)
}

/// An explicit `rollback_model` declaration, raw and unvalidated (so the
/// caller can name a bad value verbatim — AC9), plus which file it came
/// from for the error message.
fn explicit_rollback_model(project: &Path) -> Result<Option<(String, &'static str)>> {
    let card_path = project.join("agent/intent-card.json");
    if let Ok(text) = fs::read_to_string(&card_path) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(s) = value.get("rollback_model").and_then(serde_json::Value::as_str) {
                return Ok(Some((s.to_owned(), "agent/intent-card.json")));
            }
        }
    }
    let program_path = project.join("agent/AUTOBUILDER_PROGRAM.md");
    if let Ok(text) = fs::read_to_string(&program_path) {
        let re = Regex::new(r#"(?m)^\s*rollback_model\s*[:=]\s*"?([A-Za-z0-9_-]+)"?\s*$"#)?;
        if let Some(cap) = re.captures(&text) {
            if let Some(m) = cap.get(1) {
                return Ok(Some((m.as_str().to_owned(), "agent/AUTOBUILDER_PROGRAM.md")));
            }
        }
    }
    Ok(None)
}

/// Infer `redeploy-tag` when the crate carries a deploy manifest — the
/// signal that it ships as a redeployable tagged artifact rather than a
/// library. Convention: `agent/deploy-manifest.toml` (mirrors
/// `agent/AUTOBUILDER_PROGRAM.md`'s home under `agent/`); presence alone is
/// the signal today, fields inside are not yet parsed. Absent that signal,
/// the default stays `revert-commits` — the guard against a silent global
/// weakening (PRD Technical Considerations). See the README's "Rollback
/// models" section (PRD-rollback-redeploy-tag-onboard) for the full
/// onboarding schema and both opt-in paths.
fn infer_rollback_model(project: &Path) -> RollbackModel {
    if project.join("agent/deploy-manifest.toml").is_file() {
        RollbackModel::RedeployTag
    } else {
        RollbackModel::RevertCommits
    }
}

/// Resolve the crate's declared `rollback_model`: an explicit setting in
/// `agent/intent-card.json` or `agent/AUTOBUILDER_PROGRAM.md`, else
/// inferred from a deploy manifest, else the default `revert-commits`.
fn resolve_rollback_model(project: &Path) -> Result<RollbackModel> {
    if let Some((raw, source)) = explicit_rollback_model(project)? {
        return RollbackModel::parse(&raw).ok_or_else(|| {
            anyhow!(
                "invalid rollback_model {raw:?} declared in {source}; expected \"revert-commits\" or \"redeploy-tag\""
            )
        });
    }
    Ok(infer_rollback_model(project))
}

/// One of the known mechanical commit shapes: a subject-line pattern paired
/// with the file set a matching commit is allowed to touch. A commit only
/// classifies as `mechanical` when BOTH the subject matches AND its own
/// changed-file set is a subset of `allowed_files` — subject alone must
/// never launder an unexpected file change (e.g. a `src/` edit smuggled
/// under an `agent: refresh intent card for X` subject).
struct MechPattern {
    subject_re: Regex,
    allowed_files: &'static [&'static str],
}

/// The three known mechanical commit shapes (PRD-autobuilder-rollback-mechanical-commits):
/// intent-card refreshes, `Cargo.lock` version-field syncs, and
/// `worktree-extend.sh integrate`'s own parallel-integrate version-bump
/// commits. Each shape is non-revert-clean by construction (a later commit
/// of the same shape always supersedes an earlier one), so none of them
/// should ever block the rollback-plan gate.
fn mechanical_patterns() -> Result<Vec<MechPattern>> {
    Ok(vec![
        MechPattern {
            subject_re: Regex::new(r"^agent: refresh intent card for .+$")
                .context("compiling intent-card-refresh pattern")?,
            allowed_files: &[
                "agent/intent-card.json",
                "agent/intent-card.carried.json",
                "agent/intent_card_amendment_request.json",
            ],
        },
        MechPattern {
            subject_re: Regex::new(r"^[A-Za-z0-9_-]+: sync Cargo\.lock version field to .+$")
                .context("compiling cargo-lock-sync pattern")?,
            allowed_files: &["Cargo.lock"],
        },
        MechPattern {
            subject_re: Regex::new(
                r"^[A-Za-z0-9_-]+: v\d+\.\d+\.\d+ (—|--) .+\((parallel integrate|extend)\)$",
            )
            .context("compiling parallel-integrate pattern")?,
            allowed_files: &["Cargo.toml", "Cargo.lock", "CHANGELOG.md"],
        },
    ])
}

/// Path prefixes a same-shape "chain" of ≥2 consecutive commits (P0.1) is
/// allowed to touch. Global for now rather than per-repo config — the PRD's
/// own open question defers per-repo configurability to a follow-up; `www/`
/// covers the copy-edit chain on mcphost that motivated this classification.
fn chain_allowed_prefixes() -> Vec<&'static str> {
    vec!["www/"]
}

/// A commit is `mechanical` when its subject matches one of `patterns` AND
/// its own changed-file set (`files`) is a subset of that pattern's
/// `allowed_files`.
fn classify_mechanical(subject: &str, files: &[String], patterns: &[MechPattern]) -> bool {
    patterns.iter().any(|p| {
        p.subject_re.is_match(subject)
            && files.iter().all(|f| p.allowed_files.contains(&f.as_str()))
    })
}

/// True when `files` is non-empty and every changed path starts with one of
/// `prefixes` — the structural gate for chain candidacy (P0.1).
fn path_in_chain_set(files: &[String], prefixes: &[&str]) -> bool {
    !files.is_empty() && files.iter().all(|f| prefixes.iter().any(|p| f.starts_with(p)))
}

/// Metadata for one commit, gathered once up front so chain detection can
/// run over the whole `<base>..HEAD` range before any (more expensive)
/// revert dry-run happens.
struct CommitMeta {
    sha: String,
    subject: String,
    parents: Vec<String>,
    files: Vec<String>,
}

/// One chain member's own, independently-verified revert-cleanliness
/// (PRD-rollback-chain-member-revert-check). `note` is empty when `clean`,
/// a human-readable conflict summary otherwise — same shape
/// `merge_tree_revert_dry_run` already returns for every non-chain commit.
struct ChainMember {
    clean: bool,
    note: String,
}

/// A qualifying chain run: `[start, end)` indices into the chronological
/// (oldest → newest) `metas` slice, always `end - start >= 2`.
struct ChainRun {
    start: usize,
    end: usize,
    /// Revert-cleanliness of EVERY member of the run, verified
    /// independently via the same dry-run-revert machinery every
    /// non-chain, single-parent commit already uses — `members[k]`
    /// corresponds to `metas[start + k]` (PRD-rollback-chain-member-revert-check,
    /// closing the gap left by the prior terminal-only check: a non-terminal
    /// member that is not independently revert-clean — e.g. it deletes a
    /// path a later member re-adds with different content — no longer
    /// silently inherits its cleaner siblings' verdict).
    members: Vec<ChainMember>,
}

#[allow(clippy::needless_pass_by_value)] // owned `Args` matches the clap-dispatched subcommand contract
pub(crate) fn run(args: Args) -> Result<()> {
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;

    let model = resolve_rollback_model(&project)?;

    if args.migrate_note {
        println!("rollback_model: {}", RollbackModel::RedeployTag.as_str());
        return Ok(());
    }

    let head_sha = git_rev_parse(&project, "HEAD")?;

    match model {
        RollbackModel::RevertCommits => run_revert_commits(&project, &args, &head_sha),
        RollbackModel::RedeployTag => run_redeploy_tag(&project, &args, &head_sha),
    }
}

/// `revert-commits` mode: byte-for-byte the pre-tag-aware-PRD logic (same
/// base resolution, same mechanical classification from
/// PRD-autobuilder-rollback-mechanical-commits and
/// PRD-rollback-mechanical-chains). Only the receipt's schema and the
/// (always-empty-for-this-mode) v2 tag fields are new.
#[allow(clippy::too_many_lines)] // linear producer pipeline unchanged from the pre-PRD version; splitting hides the flow
fn run_revert_commits(project: &Path, args: &Args, head_sha: &str) -> Result<()> {
    let (base_ref, base_tag, base_note) = if let Some(explicit) = &args.base {
        (explicit.clone(), None, None)
    } else {
        let resolved = resolve_default_base(project)?;
        (resolved.git_ref, resolved.tag, resolved.note)
    };
    let base_sha = git_rev_parse(project, &base_ref)
        .with_context(|| format!("could not resolve base {base_ref} in {}", project.display()))?;

    let patterns = mechanical_patterns()?;
    let chain_prefixes = chain_allowed_prefixes();

    // `list_commits` walks newest → oldest (matches the existing rollback.md
    // ordering); chain-run detection needs chronological (oldest → newest)
    // order since a "later commit supersedes an earlier one" is only
    // meaningful walked forward in time.
    let commits_newest_first = list_commits(project, &base_ref)?;
    let metas: Vec<CommitMeta> = commits_newest_first
        .iter()
        .rev()
        .map(|sha| commit_meta(project, sha))
        .collect::<Result<_>>()?;

    let chain_runs = detect_chain_runs(project, &metas, &chain_prefixes)?;
    // `chain_of[i]` is the index into `chain_runs` that commit `i` belongs
    // to, if any.
    let mut chain_of: Vec<Option<usize>> = vec![None; metas.len()];
    for (run_idx, run) in chain_runs.iter().enumerate() {
        for k in run.start..run.end {
            if let Some(slot) = chain_of.get_mut(k) {
                *slot = Some(run_idx);
            }
        }
    }

    let mut entries_chrono: Vec<CommitEntry> = Vec::with_capacity(metas.len());
    for (idx, meta) in metas.iter().enumerate() {
        let run = chain_of.get(idx).copied().flatten().and_then(|i| chain_runs.get(i));
        entries_chrono.push(build_entry(project, meta, idx, run, &patterns)?);
    }
    // Restore newest-first order for the receipt/rollback.md, matching the
    // ordering PRD-autobuilder-rollback-mechanical-commits shipped.
    entries_chrono.reverse();
    let entries = entries_chrono;

    let mut classified = Classified::default();
    let mut blocking = 0usize;
    let mut mechanical_count = 0usize;
    for entry in &entries {
        classified.record(entry.class);
        if entry.class.is_mechanical() {
            mechanical_count += 1;
        } else if !entry.revertable {
            // Only substantive (non-mechanical) commits count toward
            // `blocking_count`/`verdict` — a mechanical commit that fails
            // its revert dry-run stays visible in rollback.md but never
            // blocks (see requirements P0.2 of both rollback-mechanical
            // PRDs).
            blocking += 1;
        }
    }

    // P1: warn (never block) when a tag-derived base is more than 5 commits
    // behind HEAD — the receipt still passes/blocks on revert-cleanliness
    // alone.
    if base_tag.is_some() && entries.len() > 5 {
        eprintln!(
            "rollback-plan: warning: {} commits since the newest tag ({}); ship more often, or tag",
            entries.len(),
            base_tag.as_deref().unwrap_or("?")
        );
    }

    let range_check = check_range_revert(project, head_sha, &base_sha)?;
    let every_commit_revertable = blocking == 0;
    let verdict = match args.strategy {
        // Range-level check decides it; the per-commit rule is kept as a
        // fallback so a range that isn't range-revertable still passes when
        // every commit is individually clean.
        Strategy::Squash => {
            if range_check.range_revertable || every_commit_revertable {
                "pass"
            } else {
                "block"
            }
        }
        // Original behavior: range_revertable is still computed/reported,
        // but not consulted for the verdict.
        Strategy::Merge => {
            if every_commit_revertable {
                "pass"
            } else {
                "block"
            }
        }
    };

    let rollback_md_rel = PathBuf::from("target/autobuilder/rollback.md");
    let rollback_md_abs = project.join(&rollback_md_rel);
    let base_info = BaseInfo {
        git_ref: &base_ref,
        sha: &base_sha,
        tag: base_tag.as_deref(),
        note: base_note.as_deref(),
    };
    write_rollback_md(&rollback_md_abs, head_sha, &base_info, &entries, args.strategy, &range_check)?;

    let revertable_count = entries.iter().filter(|e| e.revertable).count();

    if args.explain {
        println!(
            "rollback-plan --explain: model=revert-commits base={base_ref}{} target=git-revert({base_ref}..HEAD, {} commits)",
            base_tag
                .as_deref()
                .map(|t| format!(" base_tag={t}"))
                .unwrap_or_default(),
            entries.len(),
        );
    }

    let doc = ReceiptDoc {
        schema: "autobuilder.rollback_plan_receipt.v2",
        head_sha: head_sha.to_owned(),
        base_ref: base_ref.clone(),
        base_sha,
        base_tag: base_tag.clone(),
        base_note: base_note.clone(),
        base_tag_commit: None,
        base_tag_distance: None,
        rollback_md: rollback_md_rel.to_string_lossy().into_owned(),
        commit_count: entries.len(),
        revertable_count,
        mechanical_count,
        blocking_count: blocking,
        classified,
        verdict,
        commits: entries,
        captured_at: receipt::now_rfc3339()?,
        receipt_digest: String::new(),
        rollback_model: RollbackModel::RevertCommits.as_str(),
        rollback_target: None,
        tag_lineage: None,
        block_reason: None,
        block_detail: None,
        superseded: 0,
        pass_reason: None,
        head_version: None,
        base_version: None,
        range_bumps: None,
        range_revertable: Some(range_check.range_revertable),
        land_strategy: Some(args.strategy.as_str()),
        range_conflict_files: if range_check.conflicting_files.is_empty() {
            None
        } else {
            Some(range_check.conflicting_files.clone())
        },
    };
    let value = serde_json::to_value(&doc)?;
    let receipt_path = project.join("target/autobuilder/receipts/rollback-plan.json");
    receipt::write(&receipt_path, value)?;

    println!(
        "rollback-plan: head={head_sha} base={base_ref}{} strategy={} commits={} revertable={revertable_count} mechanical={mechanical_count} range_revertable={} verdict={verdict}",
        base_tag
            .as_deref()
            .map(|t| format!(" base_tag={t}"))
            .or_else(|| base_note.clone().map(|n| format!(" {n}")))
            .unwrap_or_default(),
        args.strategy.as_str(),
        doc.commit_count,
        range_check.range_revertable,
    );

    if verdict == "block" {
        return Err(anyhow!(
            "rollback-plan blocked (strategy={}): range_revertable={}, {blocking} of {} commits individually revertable; see {}",
            args.strategy.as_str(),
            range_check.range_revertable,
            doc.commit_count,
            rollback_md_rel.display()
        ));
    }
    Ok(())
}

/// Walk first-parent history strictly after `base_sha` up to and including
/// `HEAD`, recording every commit where the crate's `Cargo.toml` `version`
/// changed from its first-parent predecessor, and whether that commit
/// carries the matching `v<version>` tag. Mirrors `newest_reachable_tag`'s
/// own first-parent traversal (PRD Technical Considerations).
fn walk_tag_lineage(project: &Path, base_sha: &str) -> Result<Vec<TagLineageEntry>> {
    let mut commits = list_commits(project, base_sha)?; // newest -> oldest
    commits.reverse(); // oldest -> newest reads chronologically in the receipt
    let mut lineage = Vec::new();
    let mut prev_version = cargo_version_at(project, base_sha);
    for sha in &commits {
        let version = cargo_version_at(project, sha);
        if version != prev_version {
            if let Some(v) = &version {
                let tags_out = run_git(project, &["tag", "--points-at", sha])?;
                let want = format!("v{v}");
                let tag = tags_out.lines().map(str::trim).find(|t| *t == want).map(str::to_owned);
                lineage.push(TagLineageEntry {
                    version: v.clone(),
                    tag,
                    commit_sha: sha.clone(),
                    commit_short: short(sha),
                });
            }
        }
        prev_version = version;
    }
    Ok(lineage)
}

/// Whether a tag named `v<version>` exists anywhere in the repo, on any
/// commit — not necessarily the one `walk_tag_lineage` recorded the bump
/// on. Used to forgive a superseded intermediate bump (run 309, mcphost
/// 2026-09-30): a concurrent land can claim the exact version this branch
/// bumped to on a different commit before this branch reaches land.
fn tag_exists_anywhere(project: &Path, version: &str) -> Result<bool> {
    let (status, _out, _err) = run_git_capturing(
        project,
        &["rev-parse", "--verify", "-q", &format!("refs/tags/v{version}")],
    )?;
    Ok(status == 0)
}

/// The commit `tag` points at (dereferencing an annotated tag to its
/// commit), or `None` when no such tag exists anywhere in the repo.
fn resolve_tag_commit(project: &Path, tag: &str) -> Option<String> {
    let (status, stdout, _err) =
        run_git_capturing(project, &["rev-parse", "--verify", "-q", &format!("refs/tags/{tag}^{{commit}}")]).ok()?;
    if status != 0 {
        return None;
    }
    let sha = stdout.trim();
    if sha.is_empty() { None } else { Some(sha.to_owned()) }
}

/// `git merge-base --is-ancestor ancestor descendant`: `Ok(true)` when
/// `ancestor` is reachable from `descendant` (exit 0), `Ok(false)` on exit
/// 1 (not an ancestor); any other exit is a real git error.
fn is_ancestor(project: &Path, ancestor: &str, descendant: &str) -> Result<bool> {
    let (status, _out, stderr) =
        run_git_capturing(project, &["merge-base", "--is-ancestor", ancestor, descendant])?;
    match status {
        0 => Ok(true),
        1 => Ok(false),
        _ => Err(anyhow!(
            "git merge-base --is-ancestor {ancestor} {descendant} failed: {}",
            stderr.trim()
        )),
    }
}

/// Count of commits in `from..to` (exclusive of `from`) — the distance a
/// release tag's commit sits behind `to`.
fn commits_between(project: &Path, from: &str, to: &str) -> Result<usize> {
    let out = run_git(project, &["rev-list", "--count", &format!("{from}..{to}")])?;
    out.trim()
        .parse::<usize>()
        .with_context(|| format!("git rev-list --count {from}..{to} produced non-numeric output: {out:?}"))
}

/// Whether HEAD can serve as a `redeploy-tag` rollback endpoint.
enum HeadTagStatus {
    /// HEAD already carries the matching version tag.
    Tagged,
    /// HEAD's version has no tag yet, but the tag name is free to create
    /// (e.g. at ship time via `ship-tag.sh`). Treated as passing.
    Taggable,
    /// HEAD cannot be tagged: the tag name it wants is already claimed by a
    /// different commit, `tag_commit`. The caller still gets to decide
    /// whether that's a real collision or just HEAD's own base holding
    /// still (PRD-autobuilder-rollback-tag-no-bump-in-range) — `tag_commit`
    /// is what lets it tell the difference.
    Blocked { detail: String, tag_commit: String },
}

/// `head_version` is `HEAD`'s own `[package].version`; `base_version` is
/// the base's, when readable (purely for the block message — it does not
/// change the verdict here, the caller applies the no-bump-in-range
/// forgiveness itself using `tag_commit`).
fn head_tag_status(
    project: &Path,
    head_sha: &str,
    head_version: &str,
    base_version: Option<&str>,
) -> Result<HeadTagStatus> {
    let version_tag = format!("v{head_version}");
    let at_head = run_git(project, &["tag", "--points-at", head_sha])?;
    if at_head.lines().map(str::trim).any(|t| t == version_tag) {
        return Ok(HeadTagStatus::Tagged);
    }
    let Some(tag_commit) = resolve_tag_commit(project, &version_tag) else {
        return Ok(HeadTagStatus::Taggable);
    };
    let base_part = base_version.map_or_else(String::new, |b| format!(", base is {b}"));
    let detail = format!(
        "tag {version_tag} already exists on commit {} (HEAD is {head_version}{base_part}); cannot redeploy-tag HEAD",
        short(&tag_commit)
    );
    Ok(HeadTagStatus::Blocked { detail, tag_commit })
}

/// Bundles the base-resolution fields so helper functions stay under
/// clippy's argument-count lint (shared with `write_rollback_md`).
struct BaseInfo<'a> {
    git_ref: &'a str,
    sha: &'a str,
    tag: Option<&'a str>,
    note: Option<&'a str>,
}

/// `redeploy-tag` mode (PRD-autobuilder-rollback-tag-aware): verdict is
/// `pass` when the base tag exists, the tag lineage from base to HEAD is
/// contiguous, and HEAD is tagged or taggable — regardless of merge
/// commits or revert-cleanliness in the range.
#[allow(clippy::too_many_lines)] // linear producer pipeline: base resolution -> lineage walk -> head-tag check -> receipt; splitting hides the flow
fn run_redeploy_tag(project: &Path, args: &Args, head_sha: &str) -> Result<()> {
    let (base_ref, base_tag, base_note) = if let Some(explicit) = &args.base {
        (explicit.clone(), None, None)
    } else {
        let resolved = resolve_default_base(project)?;
        (resolved.git_ref, resolved.tag, resolved.note)
    };
    let base_sha = git_rev_parse(project, &base_ref)
        .with_context(|| format!("could not resolve base {base_ref} in {}", project.display()))?;

    let rollback_md_rel = PathBuf::from("target/autobuilder/rollback.md");
    let rollback_md_abs = project.join(&rollback_md_rel);

    // Edge case: zero tags reachable and no explicit --base — there is
    // nothing to roll back to yet.
    if args.base.is_none() && base_tag.is_none() {
        let base = BaseInfo {
            git_ref: &base_ref,
            sha: &base_sha,
            tag: None,
            note: base_note.as_deref(),
        };
        return finish_redeploy_block(
            project,
            &rollback_md_abs,
            &rollback_md_rel,
            head_sha,
            &base,
            "no-previous-tag",
            "no previous version tag is reachable from HEAD; nothing to redeploy to yet",
            &[],
            args.explain,
        );
    }

    let previous_tag = base_tag.clone().unwrap_or_else(|| base_ref.clone());

    let lineage = walk_tag_lineage(project, &base_sha)?;

    // A base reachable from no release tag at all is itself a lineage gap
    // — "reachable from base without a tag". An explicit `--base` never
    // sets `base_tag` (see above), so this is the only place that checks
    // the base COMMIT's own version is actually the one its tag names,
    // not just that `--base` happened to be spelled as a tag name.
    //
    // PRD-autobuilder-rollback-tag-ancestor-ok (hotfix 2026-10-03, runs
    // 358/354/336 on mcphost): the exact-match check above used to be the
    // *only* way to pass, which falsely blocked a base that is a few
    // un-bumped hotfix commits (normal on main) past its own release tag.
    // Tolerate that shape: `v<base_version>` may live on an ANCESTOR of
    // base instead of on base itself, as long as nothing changed the
    // declared version between that tag's commit and base — i.e. the
    // version at the tag commit really is `base_version`, just reached a
    // few commits later. A tag that exists but fails either check (not an
    // ancestor, or the version changed and changed back) is still a
    // genuine gap, same as no tag at all.
    let mut base_tag_commit: Option<String> = None;
    let mut base_tag_distance: Option<usize> = None;
    if let Some(base_version) = cargo_version_at(project, &base_sha) {
        let want = format!("v{base_version}");
        let tagged_at_base = run_git(project, &["tag", "--points-at", &base_sha])?
            .lines()
            .map(str::trim)
            .any(|t| t == want);

        if tagged_at_base {
            base_tag_commit = Some(base_sha.clone());
            base_tag_distance = Some(0);
        } else {
            let mut tolerated = false;
            if let Some(tag_commit) = resolve_tag_commit(project, &want) {
                if is_ancestor(project, &tag_commit, &base_sha)?
                    && cargo_version_at(project, &tag_commit).as_deref() == Some(base_version.as_str())
                {
                    base_tag_distance = Some(commits_between(project, &tag_commit, &base_sha)?);
                    base_tag_commit = Some(tag_commit);
                    tolerated = true;
                }
            }
            if !tolerated {
                let base = BaseInfo {
                    git_ref: &base_ref,
                    sha: &base_sha,
                    tag: Some(&previous_tag),
                    note: base_note.as_deref(),
                };
                let detail = format!(
                    "base {base_ref} (commit {}) has version {base_version} but no matching tag {want} \
                     at that commit or any ancestor of it — nothing reachable from base confirms it was \
                     ever released",
                    short(&base_sha)
                );
                return finish_redeploy_block(
                    project,
                    &rollback_md_abs,
                    &rollback_md_rel,
                    head_sha,
                    &base,
                    "tag-lineage-gap",
                    &detail,
                    &lineage,
                    args.explain,
                );
            }
        }
    }

    // PRD-autobuilder-rollback-tag-lineage-head-gap: an untagged entry that
    // is also the *newest* entry in the lineage is not a gap — it's HEAD's
    // current version, still waiting on `ship-tag.sh` to tag a passing
    // gate (the ordinary shape right after a version bump). This is the
    // same case `head_tag_status` below already forgives via `Taggable`/
    // `head-untagged`. Note this is deliberately NOT `entry.commit_sha ==
    // head_sha`: HEAD is frequently a few commits past the actual bump
    // (e.g. an intent-card refresh or a Cargo.lock sync commit that never
    // touches [package].version), so the bump commit and head_sha differ
    // even though nothing has bumped since — `walk_tag_lineage` only
    // records commits where the version itself changed, so "newest entry"
    // is the right test, not "entry's commit is literally HEAD".
    //
    // An untagged, non-newest entry is ALSO not a gap when its own version
    // tag already exists somewhere else in the repo (run 309, mcphost
    // 2026-09-30): a concurrent land can release the exact version this
    // branch bumped to on a different sha before this branch reaches
    // land, at which point THIS branch's version-at-rebase self-heal bumps
    // again past the collision — the superseded bump was never going to
    // carry the tag itself (a different commit already does), so it isn't
    // a forgotten-to-tag gap, only a `superseded` bump. Only an untagged
    // entry whose version was NEVER tagged anywhere — on this commit or
    // any other — is a genuine lineage gap: reachable from base, released
    // nowhere.
    let last_idx = lineage.len().checked_sub(1);
    let mut superseded = 0usize;
    let mut real_gap = None;
    for (idx, entry) in lineage.iter().enumerate() {
        if entry.tag.is_some() || Some(idx) == last_idx {
            continue;
        }
        if tag_exists_anywhere(project, &entry.version)? {
            superseded += 1;
            continue;
        }
        real_gap = Some(entry);
        break;
    }
    if let Some(gap) = real_gap {
        let base = BaseInfo {
            git_ref: &base_ref,
            sha: &base_sha,
            tag: Some(&previous_tag),
            note: base_note.as_deref(),
        };
        let detail = format!(
            "version {} (commit {}) was bumped in range but never tagged",
            gap.version, gap.commit_short
        );
        return finish_redeploy_block(
            project,
            &rollback_md_abs,
            &rollback_md_rel,
            head_sha,
            &base,
            "tag-lineage-gap",
            &detail,
            &lineage,
            args.explain,
        );
    }

    let base_version = cargo_version_at(project, &base_sha);
    let head_version = cargo_version_at(project, head_sha);
    // PRD-autobuilder-rollback-tag-no-bump-in-range (hotfix 2026-10-03, run
    // 368 mcphost): wm-build's land bumps the version itself and resets any
    // inner bump (0.9.66), so a branch gated for land legitimately carries
    // NO version change at all relative to base — `head_tag_status`'s
    // "already claimed by a different commit" check assumed every gated
    // branch bumps, and false-blocked HEAD against the base's OWN release
    // tag. That's only a real collision when the colliding commit is NOT
    // the base's own certified release (`base_tag_commit`, already proven
    // above to be base's exact tag or an ancestor of it per PR #3's
    // tolerance) — when it IS that commit, and nothing bumped in
    // `base..HEAD`, the "collision" is just HEAD's base holding still, and
    // that commit being `base_tag_commit` (itself an ancestor of `base_sha`,
    // which is an ancestor of HEAD here) makes it an ancestor of HEAD too.
    let mut pass_reason: Option<&'static str> = None;
    let blocked_detail: Option<String> = match &head_version {
        None => Some("HEAD has no readable Cargo.toml [package].version".to_owned()),
        Some(v) => match head_tag_status(project, head_sha, v, base_version.as_deref())? {
            HeadTagStatus::Tagged | HeadTagStatus::Taggable => None,
            HeadTagStatus::Blocked { detail, tag_commit } => {
                let no_bump_in_range = base_version.as_deref() == Some(v.as_str())
                    && base_tag_commit.as_deref() == Some(tag_commit.as_str())
                    && is_ancestor(project, &tag_commit, head_sha)?;
                if no_bump_in_range {
                    pass_reason = Some("no-bump-in-range");
                    None
                } else {
                    Some(detail)
                }
            }
        },
    };
    if let Some(detail) = blocked_detail {
        let base = BaseInfo {
            git_ref: &base_ref,
            sha: &base_sha,
            tag: Some(&previous_tag),
            note: base_note.as_deref(),
        };
        return finish_redeploy_block(
            project,
            &rollback_md_abs,
            &rollback_md_rel,
            head_sha,
            &base,
            "head-untagged",
            &detail,
            &lineage,
            args.explain,
        );
    }

    let package = cargo_name_at(project, head_sha).unwrap_or_else(|| {
        project
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("crate")
            .to_owned()
    });
    let target = RollbackTarget {
        tag: previous_tag.clone(),
        sha: base_sha.clone(),
        redeploy_command: format!("{package}-deploy redeploy --tag {previous_tag}"),
    };

    let base = BaseInfo {
        git_ref: &base_ref,
        sha: &base_sha,
        tag: Some(&previous_tag),
        note: base_note.as_deref(),
    };
    write_redeploy_md(&rollback_md_abs, head_sha, &base, "pass", None, Some(&target), &lineage)?;

    if args.explain {
        println!(
            "rollback-plan --explain: model=redeploy-tag base_tag={previous_tag} target=redeploy {}@{} via `{}`",
            target.tag,
            short(&target.sha),
            target.redeploy_command
        );
    }

    let doc = ReceiptDoc {
        schema: "autobuilder.rollback_plan_receipt.v2",
        head_sha: head_sha.to_owned(),
        base_ref: base_ref.clone(),
        base_sha: base_sha.clone(),
        base_tag: Some(previous_tag.clone()),
        base_note: base_note.clone(),
        base_tag_commit,
        base_tag_distance,
        rollback_md: rollback_md_rel.to_string_lossy().into_owned(),
        commit_count: lineage.len(),
        revertable_count: 0,
        mechanical_count: 0,
        blocking_count: 0,
        classified: Classified::default(),
        verdict: "pass",
        commits: Vec::new(),
        captured_at: receipt::now_rfc3339()?,
        receipt_digest: String::new(),
        rollback_model: RollbackModel::RedeployTag.as_str(),
        rollback_target: Some(target.clone()),
        tag_lineage: Some(lineage.clone()),
        block_reason: None,
        block_detail: None,
        superseded,
        pass_reason,
        head_version: if pass_reason.is_some() { head_version.clone() } else { None },
        base_version: if pass_reason.is_some() { base_version.clone() } else { None },
        range_bumps: if pass_reason.is_some() { Some(0) } else { None },
        range_revertable: None,
        land_strategy: None,
        range_conflict_files: None,
    };
    let value = serde_json::to_value(&doc)?;
    let receipt_path = project.join("target/autobuilder/receipts/rollback-plan.json");
    receipt::write(&receipt_path, value)?;

    println!(
        "rollback-plan: head={head_sha} model=redeploy-tag base_tag={previous_tag} target={} verdict=pass{}",
        target.tag,
        pass_reason.map(|r| format!(" reason={r}")).unwrap_or_default()
    );

    Ok(())
}

/// Shared block-path finisher for `redeploy-tag` mode: writes the markdown
/// and v2 receipt, prints `--explain` and the summary line, and returns the
/// blocking `Err`. Shared by all three block reasons so the receipt shape
/// stays identical regardless of which check failed.
#[allow(clippy::too_many_arguments)] // every field is required for both the receipt and the markdown; a context struct would just relocate the sprawl
fn finish_redeploy_block(
    project: &Path,
    rollback_md_abs: &Path,
    rollback_md_rel: &Path,
    head_sha: &str,
    base: &BaseInfo<'_>,
    reason: &'static str,
    detail: &str,
    lineage: &[TagLineageEntry],
    explain: bool,
) -> Result<()> {
    write_redeploy_md(
        rollback_md_abs,
        head_sha,
        base,
        "block",
        Some((reason, detail)),
        None,
        lineage,
    )?;

    if explain {
        println!(
            "rollback-plan --explain: model=redeploy-tag base={}{} target=blocked({reason}: {detail})",
            base.git_ref,
            base.tag.map(|t| format!(" base_tag={t}")).unwrap_or_default(),
        );
    }

    let doc = ReceiptDoc {
        schema: "autobuilder.rollback_plan_receipt.v2",
        head_sha: head_sha.to_owned(),
        base_ref: base.git_ref.to_owned(),
        base_sha: base.sha.to_owned(),
        base_tag: base.tag.map(str::to_owned),
        base_note: base.note.map(str::to_owned),
        base_tag_commit: None,
        base_tag_distance: None,
        rollback_md: rollback_md_rel.to_string_lossy().into_owned(),
        commit_count: lineage.len(),
        revertable_count: 0,
        mechanical_count: 0,
        blocking_count: 0,
        classified: Classified::default(),
        verdict: "block",
        commits: Vec::new(),
        captured_at: receipt::now_rfc3339()?,
        receipt_digest: String::new(),
        rollback_model: RollbackModel::RedeployTag.as_str(),
        rollback_target: None,
        tag_lineage: if lineage.is_empty() { None } else { Some(lineage.to_vec()) },
        block_reason: Some(reason),
        block_detail: Some(detail.to_owned()),
        superseded: 0,
        pass_reason: None,
        head_version: None,
        base_version: None,
        range_bumps: None,
        range_revertable: None,
        land_strategy: None,
        range_conflict_files: None,
    };
    let value = serde_json::to_value(&doc)?;
    let receipt_path = project.join("target/autobuilder/receipts/rollback-plan.json");
    receipt::write(&receipt_path, value)?;

    println!("rollback-plan: head={head_sha} model=redeploy-tag verdict=block reason={reason}");

    Err(anyhow!("redeploy-tag rollback check blocked ({reason}): {detail}"))
}

fn git_rev_parse(project: &Path, refname: &str) -> Result<String> {
    let out = run_git(project, &["rev-parse", refname])?;
    Ok(out.trim().to_owned())
}

fn list_commits(project: &Path, base: &str) -> Result<Vec<String>> {
    // `--first-parent` keeps the receipt focused on the main-line history;
    // any merge commit is treated as a single revertable unit (with `-m 1`).
    let range = format!("{base}..HEAD");
    let out = run_git(project, &["rev-list", "--first-parent", &range])?;
    Ok(out.lines().map(str::to_owned).collect())
}

fn commit_meta(project: &Path, sha: &str) -> Result<CommitMeta> {
    let subject = run_git(project, &["log", "-1", "--format=%s", sha])?
        .trim()
        .to_owned();
    let parents_str = run_git(project, &["log", "-1", "--format=%P", sha])?;
    let parents: Vec<String> = parents_str.split_whitespace().map(str::to_owned).collect();
    let files = commit_files(project, sha)?;
    Ok(CommitMeta {
        sha: sha.to_owned(),
        subject,
        parents,
        files,
    })
}

/// Groups `metas` (chronological, oldest → newest) into maximal runs of ≥2
/// consecutive single-parent commits whose changed files are all within
/// `chain_prefixes` (P0.1). Merge commits and commits touching any path
/// outside the chain-allowed set are never chain candidates, and always
/// break a run. Each qualifying run's members are EACH independently
/// dry-run-reverted (PRD-rollback-chain-member-revert-check — every
/// candidate already has exactly one parent by construction above, so
/// every member reuses the same single-parent `merge_tree_revert_dry_run`
/// call the non-chain path already uses; no extra worktree is spawned).
fn detect_chain_runs(
    project: &Path,
    metas: &[CommitMeta],
    chain_prefixes: &[&str],
) -> Result<Vec<ChainRun>> {
    let mut runs = Vec::new();
    let mut i = 0usize;
    while i < metas.len() {
        let candidate = metas
            .get(i)
            .is_some_and(|m| m.parents.len() == 1 && path_in_chain_set(&m.files, chain_prefixes));
        if !candidate {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i + 1;
        while metas
            .get(j)
            .is_some_and(|m| m.parents.len() == 1 && path_in_chain_set(&m.files, chain_prefixes))
        {
            j += 1;
        }
        if j - start >= 2 {
            let mut members = Vec::with_capacity(j - start);
            for k in start..j {
                let Some(meta) = metas.get(k) else {
                    members.push(ChainMember {
                        clean: false,
                        note: "chain member missing (internal error)".to_owned(),
                    });
                    continue;
                };
                // Candidacy above already guarantees exactly one parent.
                let (clean, note) = match meta.parents.first() {
                    Some(fp) => merge_tree_revert_dry_run(project, &meta.sha, fp)?,
                    None => (false, "chain member has no parent".to_owned()),
                };
                members.push(ChainMember { clean, note });
            }
            runs.push(ChainRun { start, end: j, members });
        }
        i = j;
    }
    Ok(runs)
}

/// Classifies one commit and computes its revert-cleanliness / note. `run`
/// is `Some` when the commit belongs to a qualifying chain (P0.1); in that
/// case classification and revert-cleanliness are shared across the whole
/// chain and no per-commit revert dry-run happens.
fn build_entry(
    project: &Path,
    meta: &CommitMeta,
    idx: usize,
    run: Option<&ChainRun>,
    patterns: &[MechPattern],
) -> Result<CommitEntry> {
    if let Some(run) = run {
        return Ok(build_chain_entry(meta, idx, run));
    }
    if meta.parents.len() >= 2 {
        return build_merge_entry(project, meta);
    }
    build_pattern_or_root_entry(project, meta, patterns)
}

/// A commit that belongs to a qualifying chain run (P0.1). Each member's own
/// revert-cleanliness — not the chain's terminal commit's — decides its
/// classification (PRD-rollback-chain-member-revert-check): a member that is
/// not independently revert-clean stays `substantive` (and blocking) even
/// though it sits in an otherwise-mechanical chain; independently-clean
/// members still classify `mechanical(chain)`.
fn build_chain_entry(meta: &CommitMeta, idx: usize, run: &ChainRun) -> CommitEntry {
    let chain_len = run.end - run.start;
    let position = idx.saturating_sub(run.start) + 1;
    let fallback = ChainMember {
        clean: false,
        note: "chain member verification missing (internal error)".to_owned(),
    };
    let member = run
        .members
        .get(idx.saturating_sub(run.start))
        .unwrap_or(&fallback);
    let (class, note) = if member.clean {
        (
            CommitClass::MechanicalChain,
            format!(
                "mechanical(chain): member {position} of {chain_len} on the chain-allowed path \
                 set; independently revert-clean",
            ),
        )
    } else {
        (
            CommitClass::Substantive,
            format!(
                "chain member {position} of {chain_len} on the chain-allowed path set is NOT \
                 independently revert-clean — {}",
                member.note.if_empty_then("conflicts during revert"),
            ),
        )
    };
    CommitEntry {
        sha: meta.sha.clone(),
        short_sha: short(&meta.sha),
        subject: meta.subject.clone(),
        parent_count: meta.parents.len(),
        revertable: member.clean,
        mechanical: class.is_mechanical(),
        class,
        note,
    }
}

/// A ≥2-parent commit (P0.2): classifies `mechanical(merge)` on a clean `-m
/// 1` dry-run, else stays `substantive` (blocking) with conflict paths in
/// the note.
fn build_merge_entry(project: &Path, meta: &CommitMeta) -> Result<CommitEntry> {
    let parent_count = meta.parents.len();
    let (clean, conflict_paths) = merge_revert_m1_dry_run(project, &meta.sha)?;
    Ok(if clean {
        CommitEntry {
            sha: meta.sha.clone(),
            short_sha: short(&meta.sha),
            subject: meta.subject.clone(),
            parent_count,
            revertable: true,
            mechanical: true,
            class: CommitClass::MechanicalMerge,
            note: "clean merge revert (`git revert -m 1`)".to_owned(),
        }
    } else {
        CommitEntry {
            sha: meta.sha.clone(),
            short_sha: short(&meta.sha),
            subject: meta.subject.clone(),
            parent_count,
            revertable: false,
            mechanical: false,
            class: CommitClass::Substantive,
            note: if conflict_paths.is_empty() {
                "merge revert -m 1 conflicts".to_owned()
            } else {
                format!("merge revert -m 1 conflicts: {}", conflict_paths.join(", "))
            },
        }
    })
}

/// A single-parent, non-chain commit (or a root commit with no parent):
/// unchanged from PRD-autobuilder-rollback-mechanical-commits — classify
/// against the three named patterns, then dry-run its own revert.
fn build_pattern_or_root_entry(
    project: &Path,
    meta: &CommitMeta,
    patterns: &[MechPattern],
) -> Result<CommitEntry> {
    let parent_count = meta.parents.len();
    let mechanical = classify_mechanical(&meta.subject, &meta.files, patterns);
    let class = if mechanical {
        CommitClass::MechanicalPattern
    } else {
        CommitClass::Substantive
    };

    let Some(first_parent) = meta.parents.first() else {
        return Ok(CommitEntry {
            sha: meta.sha.clone(),
            short_sha: short(&meta.sha),
            subject: meta.subject.clone(),
            parent_count: 0,
            revertable: false,
            mechanical,
            class,
            note: "root commit (no parent) cannot be reverted".to_owned(),
        });
    };

    let (revertable, conflict_note) = merge_tree_revert_dry_run(project, &meta.sha, first_parent)?;
    let note = if revertable {
        "clean revert".to_owned()
    } else {
        conflict_note
    };

    Ok(CommitEntry {
        sha: meta.sha.clone(),
        short_sha: short(&meta.sha),
        subject: meta.subject.clone(),
        parent_count,
        revertable,
        mechanical,
        class,
        note,
    })
}

/// Read-only revert dry-run for a single-parent-shaped revert: simulates
/// `git revert <sha>` (or, when `first_parent` is a merge's mainline
/// parent, `git revert -m 1 <sha>`) landing on `HEAD`, via `git merge-tree
/// --write-tree --merge-base=<sha> HEAD <first_parent>` — never touches the
/// working tree or index. Returns `(clean, note)`, where `note` is a
/// human-readable conflict summary when `!clean` and empty otherwise.
fn merge_tree_revert_dry_run(project: &Path, sha: &str, first_parent: &str) -> Result<(bool, String)> {
    let merge_arg = format!("--merge-base={sha}");
    let (status, _stdout, stderr) = run_git_capturing(
        project,
        &["merge-tree", "--write-tree", &merge_arg, "HEAD", first_parent],
    )?;
    let clean = status == 0;
    let note = if clean {
        String::new()
    } else {
        match stderr.lines().next() {
            Some(line) if !line.trim().is_empty() => format!("conflicts: {}", line.trim()),
            _ => "conflicts during revert".to_owned(),
        }
    };
    Ok((clean, note))
}

/// Verify that `base..HEAD`, reverted as a *single* change (the shape of
/// the daemon's real `git revert <squash-sha>` once the branch lands),
/// applies cleanly onto HEAD. `revert-commits` mode only.
///
/// When `base` is a direct ancestor of `HEAD` — the normal case, since the
/// future squash commit's parent will be exactly `base` — reverting it the
/// moment it lands is a revert at the tip: `ours` (HEAD) has no changes
/// relative to the thing being reverted, so the 3-way merge trivially
/// resolves to `theirs` (base) with no conflict possible. That is the
/// point of this check: unlike the per-commit checks, it does not
/// false-block on commits that merely touch overlapping lines of each
/// other but whose net diff reverts cleanly.
///
/// When `base` has moved past the branch's actual fork point (e.g. `main`
/// gained unrelated commits after the branch was cut), the check instead
/// verifies the range against the true common ancestor, which *can*
/// legitimately conflict if base's new commits touch the same lines as
/// this range.
fn check_range_revert(project: &Path, head_sha: &str, base_sha: &str) -> Result<RangeRevertCheck> {
    let Ok(fork_point) = run_git(project, &["merge-base", base_sha, head_sha]) else {
        return Ok(RangeRevertCheck {
            range_revertable: false,
            conflicting_files: Vec::new(),
            detail: "no common ancestor between --base and HEAD; cannot evaluate the range revert"
                .to_owned(),
        });
    };
    let fork_point = fork_point.trim();

    if fork_point == base_sha {
        let merge_arg = format!("--merge-base={head_sha}");
        let (status, stdout, _stderr) = run_git_capturing(
            project,
            &["merge-tree", "--write-tree", &merge_arg, head_sha, base_sha],
        )?;
        return Ok(RangeRevertCheck {
            range_revertable: status == 0,
            conflicting_files: if status == 0 { Vec::new() } else { conflicting_files_from(&stdout) },
            detail: if status == 0 {
                "base is a direct ancestor of HEAD; the squash commit's parent will be base, so \
                 reverting it immediately after it lands is clean by construction"
                    .to_owned()
            } else {
                "unexpected conflict reverting the range onto HEAD".to_owned()
            },
        });
    }

    // base has drifted past the branch's fork point — a real 3-way merge
    // against the true common ancestor, which can conflict.
    let merge_arg = format!("--merge-base={fork_point}");
    let (status, stdout, _stderr) = run_git_capturing(
        project,
        &["merge-tree", "--write-tree", &merge_arg, head_sha, base_sha],
    )?;
    Ok(RangeRevertCheck {
        range_revertable: status == 0,
        conflicting_files: if status == 0 { Vec::new() } else { conflicting_files_from(&stdout) },
        detail: if status == 0 {
            format!(
                "base has moved past the branch's fork point ({}); the range is still compatible with current base",
                short(fork_point)
            )
        } else {
            format!(
                "base has moved past the branch's fork point ({}) with changes that conflict with this range",
                short(fork_point)
            )
        },
    })
}

/// Parse the conflicted-path list out of `git merge-tree --write-tree`'s
/// stdout. On conflict it prints one `<mode> <oid> <stage>\t<path>` line per
/// stage ahead of the `Auto-merging`/`CONFLICT` summary lines.
fn conflicting_files_from(stdout: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in stdout.lines() {
        let Some((meta, path)) = line.split_once('\t') else { continue };
        let mut fields = meta.split_whitespace();
        let (Some(_mode), Some(_oid), Some(_stage)) = (fields.next(), fields.next(), fields.next()) else {
            continue;
        };
        let path = path.to_owned();
        if !files.contains(&path) {
            files.push(path);
        }
    }
    files
}

/// Verifies a merge commit's `-m 1` revert-cleanliness with a real `git
/// revert --no-commit -m 1` inside a scratch worktree — `git revert` needs
/// an actual index/working tree, unlike the `merge-tree`-based dry run used
/// for everything else in this file (PRD-rollback-mechanical-chains P0.2).
/// The worktree lives under the OS temp dir, keyed by pid + sha so
/// concurrent runs never collide, and is always torn down (best-effort)
/// before returning, success or failure. Returns `(clean,
/// conflicting_paths)`.
fn merge_revert_m1_dry_run(project: &Path, sha: &str) -> Result<(bool, Vec<String>)> {
    let wt_dir = std::env::temp_dir().join(format!(
        "autobuilder-rollback-m1-{}-{}",
        std::process::id(),
        short(sha)
    ));
    if wt_dir.exists() {
        let _ = fs::remove_dir_all(&wt_dir);
    }
    let wt_path = wt_dir
        .to_str()
        .ok_or_else(|| anyhow!("scratch worktree path is not valid UTF-8: {}", wt_dir.display()))?
        .to_owned();

    let outcome: Result<(bool, Vec<String>)> = (|| {
        run_git(project, &["worktree", "add", "--detach", "--quiet", &wt_path, "HEAD"])?;
        let (status, _stdout, _stderr) =
            run_git_capturing(&wt_dir, &["revert", "--no-commit", "-m", "1", sha])?;
        if status == 0 {
            Ok((true, Vec::new()))
        } else {
            let conflicts = run_git(&wt_dir, &["diff", "--name-only", "--diff-filter=U"])
                .unwrap_or_default();
            let paths: Vec<String> = conflicts.lines().map(str::to_owned).collect();
            Ok((false, paths))
        }
    })();

    // Best-effort cleanup regardless of outcome — never leaves worktree
    // administrative state or scratch files behind.
    let _ = run_git(&wt_dir, &["revert", "--abort"]);
    let _ = run_git(&wt_dir, &["reset", "--hard", "HEAD"]);
    if run_git(project, &["worktree", "remove", "--force", &wt_path]).is_err() {
        let _ = fs::remove_dir_all(&wt_dir);
        let _ = run_git(project, &["worktree", "prune"]);
    }

    outcome
}

/// The commit's own changed-file set, per `git show --name-only --format=`
/// (i.e. the diff from its first parent) — never a diff against HEAD.
fn commit_files(project: &Path, sha: &str) -> Result<Vec<String>> {
    let out = run_git(project, &["show", "--name-only", "--format=", sha])?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

#[allow(clippy::too_many_arguments)] // mirrors write_redeploy_md's precedent; strategy + range_check are each independently needed for the new section
fn write_rollback_md(
    path: &Path,
    head_sha: &str,
    base: &BaseInfo<'_>,
    entries: &[CommitEntry],
    strategy: Strategy,
    range_check: &RangeRevertCheck,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = String::new();
    out.push_str("# Rollback plan\n\n");
    out.push_str(&format!("HEAD: `{head_sha}`\n"));
    out.push_str(&format!("Base: `{}` (`{}`)\n", base.git_ref, base.sha));
    if let Some(tag) = base.tag {
        out.push_str(&format!("Base tag: `{tag}`\n"));
    }
    if let Some(note) = base.note {
        out.push_str(&format!("Base note: {note}\n"));
    }
    out.push_str(&format!("Strategy: `{}`\n\n", strategy.as_str()));

    out.push_str("## Squash revert\n\n");
    out.push_str("The daemon lands this branch as one squash commit, so the real future\n");
    out.push_str("revert is:\n\n");
    out.push_str("```\n");
    out.push_str("git revert <squash-sha>\n");
    out.push_str("```\n\n");
    out.push_str(
        "(use `git revert -m 1 <squash-sha>` instead if that commit is ever created as a merge commit.)\n\n",
    );
    out.push_str(&format!(
        "Range revert result: **{}**\n\n",
        if range_check.range_revertable { "clean" } else { "CONFLICTS" }
    ));
    out.push_str(&format!("{}\n", range_check.detail));
    if !range_check.conflicting_files.is_empty() {
        out.push_str("\nConflicting files:\n\n");
        for f in &range_check.conflicting_files {
            out.push_str(&format!("- `{f}`\n"));
        }
    }
    out.push('\n');

    out.push_str("## Per-commit detail\n\n");
    out.push_str("Reverts are listed newest → oldest. Each `git revert` was\n");
    out.push_str("dry-run via `git merge-tree --write-tree` against current HEAD\n");
    out.push_str("(merge commits use a real `git revert -m 1` in a scratch\n");
    out.push_str("worktree instead), so the caller's working tree was never touched\n");
    out.push_str("during verification. Under `--strategy squash` this table is\n");
    out.push_str("informational — the verdict follows the squash revert above unless\n");
    out.push_str("it could not be evaluated.\n\n");
    if entries.is_empty() {
        out.push_str("No commits in range.\n");
    } else {
        out.push_str("| # | sha | revertable | class | command | subject |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        for (i, e) in entries.iter().enumerate() {
            let cmd = if e.parent_count > 1 {
                format!("`git revert -m 1 {}`", e.short_sha)
            } else {
                format!("`git revert {}`", e.short_sha)
            };
            let mark = if e.revertable {
                "✓"
            } else if e.mechanical {
                "✗(M)"
            } else {
                "✗"
            };
            let class = match e.class {
                CommitClass::MechanicalPattern => "mechanical(pattern)",
                CommitClass::MechanicalChain => "mechanical(chain)",
                CommitClass::MechanicalMerge => "mechanical(merge)",
                CommitClass::Substantive => "substantive",
            };
            let subject = e.subject.replace('|', "\\|");
            out.push_str(&format!(
                "| {n} | `{sha}` | {mark} | {class} | {cmd} | {subj} |\n",
                n = i + 1,
                sha = e.short_sha,
                subj = subject,
            ));
        }
        out.push_str(
            "\n`(M)` marks a commit classified mechanical — `mechanical(pattern)` (matches a \
             known housekeeping shape by both subject line and changed-file set), \
             `mechanical(chain)` (a ≥2-commit same-allowed-path chain where later commits \
             supersede earlier ones), or `mechanical(merge)` (a ≥2-parent commit whose `-m 1` \
             revert is clean) — non-revert-clean but excluded from `blocking_count`/`verdict`.\n",
        );
        out.push_str("\n### Notes\n\n");
        for e in entries {
            out.push_str(&format!("- `{}` — {}\n", e.short_sha, e.note));
        }
    }
    fs::write(path, out)?;
    Ok(())
}

/// `redeploy-tag` mode's markdown counterpart to `write_rollback_md`.
#[allow(clippy::too_many_arguments)] // mirrors write_rollback_md's BaseInfo bundling; the remaining fields are each independently optional
fn write_redeploy_md(
    path: &Path,
    head_sha: &str,
    base: &BaseInfo<'_>,
    verdict: &str,
    block: Option<(&str, &str)>,
    target: Option<&RollbackTarget>,
    lineage: &[TagLineageEntry],
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = String::new();
    out.push_str("# Rollback plan (redeploy-tag)\n\n");
    out.push_str(&format!("HEAD: `{head_sha}`\n"));
    out.push_str(&format!("Base: `{}` (`{}`)\n", base.git_ref, base.sha));
    if let Some(tag) = base.tag {
        out.push_str(&format!("Base tag: `{tag}`\n"));
    }
    if let Some(note) = base.note {
        out.push_str(&format!("Base note: {note}\n"));
    }
    out.push_str(&format!("\nVerdict: **{verdict}**\n"));
    if let Some((reason, detail)) = block {
        out.push_str(&format!("Block reason: `{reason}` — {detail}\n"));
    }
    if let Some(t) = target {
        out.push_str(&format!(
            "\nRollback target: redeploy `{}` (`{}`) via `{}`\n",
            t.tag, t.sha, t.redeploy_command
        ));
    }
    out.push_str("\nRollback here means redeploying the previous tagged version, not\n");
    out.push_str("reverting commits — merge commits and interleaved history do not\n");
    out.push_str("affect this verdict (PRD-autobuilder-rollback-tag-aware).\n");
    if !lineage.is_empty() {
        out.push_str("\n## Tag lineage\n\n");
        out.push_str("| version | tag | commit |\n");
        out.push_str("|---|---|---|\n");
        for e in lineage {
            let tag = e.tag.as_deref().unwrap_or("**MISSING**");
            out.push_str(&format!("| {} | {} | `{}` |\n", e.version, tag, e.commit_short));
        }
    }
    fs::write(path, out)?;
    Ok(())
}

fn run_git(project: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(args)
        .output()
        .with_context(|| format!("failed to spawn git {args:?}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git {:?} failed: {}", args, stderr.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run_git_capturing(
    project: &Path,
    args: &[&str],
) -> Result<(i32, String, String)> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(args)
        .output()
        .with_context(|| format!("failed to spawn git {args:?}"))?;
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Ok((code, stdout, stderr))
}

fn short(sha: &str) -> String {
    sha.get(..7.min(sha.len())).unwrap_or(sha).to_owned()
}

trait IfEmptyThen {
    fn if_empty_then(&self, fallback: &str) -> String;
}

impl IfEmptyThen for str {
    fn if_empty_then(&self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_owned()
        } else {
            self.to_owned()
        }
    }
}
