//! Self-evolution aggregator with default-on auto-apply.
//!
//! Reads `~/.claude/skills/autobuilder/proposals/evolution-proposal-*.json`,
//! filters out proposals already applied (per `applied.log` next to them),
//! optionally filters by `--since <ISO>`, ranks the remainder by
//! `iters_advance + iters_crash*2 + failure_capsule_count`, and emits:
//!
//!   `proposals/evolve-report-<date>.md`    — human-readable summary
//!   `proposals/evolve-diff-<date>.patch`   — real unified-diff hunks
//!                                            suggesting edits to skill
//!                                            files. Each hunk is
//!                                            append-mode so `patch -p0`
//!                                            applies cleanly when the
//!                                            target file exists.
//!
//! Default behavior: auto-applies each suggestion to the skill tree
//! (file append, since every suggestion is append-only by construction),
//! commits the change in the `skill_root` git repo when present, and
//! records an `applied-suggestion:<sha256>` line in `applied.log` so the
//! same suggestion does not re-emit on subsequent runs. Use `--dry-run`
//! to suppress application and fall back to review-only mode.

use crate::receipt;
use anyhow::{Context, Result, anyhow};
use clap::Args as ClapArgs;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Only consider proposals newer than this ISO-8601 timestamp (string compare).
    #[arg(long)]
    pub since: Option<String>,

    /// Cap the number of top recommendations surfaced.
    #[arg(long, default_value_t = 10)]
    pub max: u32,

    /// Proposals directory.
    #[arg(long, default_value = "~/.claude/skills/autobuilder/proposals")]
    pub proposals_dir: String,

    /// Skill root the suggested diffs target. Defaults to the local skill install.
    #[arg(long, default_value = "~/.claude/skills/autobuilder")]
    pub skill_root: String,

    /// Emit report + diff but do NOT apply. Use when you want to inspect
    /// suggestions before they land on disk. Default is auto-apply.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

struct LoadedProposal {
    path: PathBuf,
    value: Value,
    score: f64,
}

struct Suggestion {
    target: PathBuf,
    rationale: String,
    appended_lines: Vec<String>,
}

impl Suggestion {
    /// Stable fingerprint over `(target, appended_lines)`. Used to dedupe
    /// suggestions across evolve runs: once a fingerprint lands in
    /// `applied.log`, the same suggestion is silently skipped.
    fn fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.target.to_string_lossy().as_bytes());
        hasher.update(b"\n");
        for line in &self.appended_lines {
            hasher.update(line.as_bytes());
            hasher.update(b"\n");
        }
        format!("{:x}", hasher.finalize())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApplyOutcome {
    Applied,
    SkippedDuplicate,
    SkippedDryRun,
    SkippedMissingTarget,
}

#[allow(clippy::needless_pass_by_value)] // owned `Args` matches the clap-dispatched subcommand contract
#[allow(clippy::too_many_lines)] // single linear aggregator-then-renderer
pub(crate) fn run(args: Args) -> Result<()> {
    let dir = expand_tilde(&args.proposals_dir);
    if !dir.is_dir() {
        return Err(anyhow!(
            "proposals dir does not exist at {}; run `autobuilder postmortem` first",
            dir.display()
        ));
    }
    let skill_root = expand_tilde(&args.skill_root);
    let (applied, applied_suggestion_fps) = load_applied_log(&dir);
    let mut proposals: Vec<LoadedProposal> =
        load_proposals(&dir, args.since.as_deref(), &applied)?;
    proposals.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let max = args.max as usize;
    if proposals.len() > max {
        proposals.truncate(max);
    }

    let date = receipt::now_rfc3339()?.replace(':', "-");
    let report_path = dir.join(format!("evolve-report-{date}.md"));
    let diff_path = dir.join(format!("evolve-diff-{date}.patch"));

    let suggestions = derive_suggestions(&proposals, &skill_root);

    write_report(&report_path, &dir, args.since.as_deref(), &applied, &proposals, &suggestions)?;
    write_diff(&diff_path, &suggestions)?;

    let outcomes: Vec<(ApplyOutcome, &Suggestion)> = suggestions
        .iter()
        .map(|s| {
            let outcome = apply_suggestion(s, &skill_root, &dir, &applied_suggestion_fps, args.dry_run);
            (outcome, s)
        })
        .collect();
    let applied_now = outcomes
        .iter()
        .filter(|(o, _)| *o == ApplyOutcome::Applied)
        .count();
    let skipped_dup = outcomes
        .iter()
        .filter(|(o, _)| *o == ApplyOutcome::SkippedDuplicate)
        .count();
    let skipped_missing = outcomes
        .iter()
        .filter(|(o, _)| *o == ApplyOutcome::SkippedMissingTarget)
        .count();

    println!(
        "evolve: scanned={} top={} suggestions={} applied={} skipped_duplicate={} skipped_missing_target={} report={} diff={}{}",
        applied.len() + proposals.len(),
        proposals.len(),
        suggestions.len(),
        applied_now,
        skipped_dup,
        skipped_missing,
        report_path.display(),
        diff_path.display(),
        if args.dry_run { " [dry-run]" } else { "" },
    );
    Ok(())
}

/// Apply a single suggestion. Append-only by construction, so the safety
/// rails reduce to: target must exist, fingerprint must not already be in
/// `applied.log`, and `dry_run` must be false.
///
/// When applied: appends `appended_lines` to the target file (joined by `\n`
/// with a trailing newline), commits the change in `skill_root` if it is a
/// git repo, and records `applied-suggestion:<sha256>` in applied.log.
fn apply_suggestion(
    s: &Suggestion,
    skill_root: &Path,
    proposals_dir: &Path,
    already_applied: &HashSet<String>,
    dry_run: bool,
) -> ApplyOutcome {
    if dry_run {
        return ApplyOutcome::SkippedDryRun;
    }
    let fp = s.fingerprint();
    if already_applied.contains(&fp) {
        return ApplyOutcome::SkippedDuplicate;
    }
    if !s.target.exists() {
        eprintln!(
            "evolve: skipping suggestion (target missing): {}",
            s.target.display()
        );
        return ApplyOutcome::SkippedMissingTarget;
    }
    let Ok(existing) = fs::read_to_string(&s.target) else {
        eprintln!(
            "evolve: skipping suggestion (target unreadable): {}",
            s.target.display()
        );
        return ApplyOutcome::SkippedMissingTarget;
    };
    let mut new_contents = existing;
    if !new_contents.ends_with('\n') {
        new_contents.push('\n');
    }
    for line in &s.appended_lines {
        new_contents.push_str(line);
        new_contents.push('\n');
    }
    if let Err(e) = fs::write(&s.target, new_contents) {
        eprintln!(
            "evolve: failed to write {}: {e}",
            s.target.display()
        );
        return ApplyOutcome::SkippedMissingTarget;
    }
    git_commit_if_repo(skill_root, &s.target, &s.rationale);
    if let Err(e) = record_applied_fingerprint(proposals_dir, &fp, &s.rationale, &s.target) {
        eprintln!("evolve: failed to update applied.log: {e}");
    }
    ApplyOutcome::Applied
}

/// If `skill_root` is inside a git working tree, stage the touched file and
/// create a commit attributing it to evolve. Silently skips if git is not
/// available, the dir is not a working tree, or the commit step fails — a
/// failed git commit must NOT undo the on-disk apply.
fn git_commit_if_repo(skill_root: &Path, touched: &Path, rationale: &str) {
    let inside = Command::new("git")
        .arg("-C")
        .arg(skill_root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output();
    let Ok(probe) = inside else {
        return;
    };
    if !probe.status.success() {
        return;
    }
    if String::from_utf8_lossy(&probe.stdout).trim() != "true" {
        return;
    }
    // Pass the path relative to skill_root so the symlink at
    // ~/.claude/skills/autobuilder doesn't get resolved into a different
    // working tree by git's pathspec normalization.
    let relative = touched
        .strip_prefix(skill_root)
        .unwrap_or(touched);
    let add = Command::new("git")
        .arg("-C")
        .arg(skill_root)
        .args(["add", "--"])
        .arg(relative)
        .output();
    if !matches!(&add, Ok(o) if o.status.success()) {
        return;
    }
    let message = format!("evolve: {rationale}");
    let _ = Command::new("git")
        .arg("-C")
        .arg(skill_root)
        .args(["commit", "-q", "-m", &message])
        .output();
}

/// Append an `applied-suggestion:<fp>` line plus a human-readable comment
/// block to `applied.log`. Format mirrors the existing #REJECTED convention
/// already used in that file.
fn record_applied_fingerprint(
    proposals_dir: &Path,
    fingerprint: &str,
    rationale: &str,
    target: &Path,
) -> Result<()> {
    let path = proposals_dir.join("applied.log");
    let mut text = fs::read_to_string(&path).unwrap_or_default();
    if !text.ends_with('\n') && !text.is_empty() {
        text.push('\n');
    }
    text.push('\n');
    text.push_str(&format!("#APPLIED: {} — {}\n", target.display(), rationale));
    text.push_str(&format!("applied-suggestion:{fingerprint}\n"));
    fs::write(&path, text)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

#[allow(clippy::too_many_lines)] // one branch per known proposal-pattern; splitting hides the rule set
fn derive_suggestions(proposals: &[LoadedProposal], skill_root: &Path) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = Vec::new();
    let skill_md = skill_root.join("SKILL.md");
    let bad_rust = skill_root.join("rules/bad-rust.md");
    let template_harness = skill_root.join("templates/scaffold/scripts/run-metrics.sh");
    let audit_rules = skill_root.join("rules/audit-checks.sh");

    let mut total_crash = 0u64;
    let mut total_revert = 0u64;
    let mut total_advance = 0u64;
    let mut total_capsules = 0u64;
    let mut zero_advance_runs: Vec<String> = Vec::new();
    let mut crash_note_runs: Vec<String> = Vec::new();
    let mut harness_abort_seen = false;
    // Cross-proposal aggregations for the audit + reviewer rules.
    let mut blocking_detector_slugs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut concern_slugs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut block_decisions: Vec<(String, Vec<String>)> = Vec::new();

    for p in proposals {
        let slug = p
            .value
            .get("intent_slug")
            .and_then(Value::as_str)
            .unwrap_or("?");
        let iters_advance = field_u64(&p.value, "iters_advance");
        let iters_revert = field_u64(&p.value, "iters_revert");
        let iters_crash = field_u64(&p.value, "iters_crash");
        let capsules = field_u64(&p.value, "failure_capsule_count");
        total_advance += iters_advance;
        total_revert += iters_revert;
        total_crash += iters_crash;
        total_capsules += capsules;
        // Require ≥1 post-baseline iteration attempt before flagging "stalled".
        // iters_total includes the baseline row, so iters_total == 1 means
        // "scaffolded but not yet iterated" — not the same as stalled.
        if iters_advance == 0 && field_u64(&p.value, "iters_total") > 1 {
            zero_advance_runs.push(slug.to_owned());
        }
        if iters_crash > 0 {
            crash_note_runs.push(slug.to_owned());
        }
        if let Some(notes) = p.value.get("notes").and_then(Value::as_array) {
            for n in notes {
                let Some(s) = n.as_str() else { continue };
                let l = s.to_lowercase();
                if l.contains("harness") && (l.contains("abort") || l.contains("set -e")) {
                    harness_abort_seen = true;
                }
            }
        }

        if let Some(audit) = p.value.get("audit_summary") {
            if let Some(bd) = audit.get("blocking_detectors").and_then(Value::as_array) {
                for d in bd {
                    if let Some(name) = d.as_str() {
                        blocking_detector_slugs
                            .entry(name.to_owned())
                            .or_default()
                            .insert(slug.to_owned());
                    }
                }
            }
        }
        if let Some(reviewer) = p.value.get("reviewer_summary") {
            let decision = reviewer
                .get("decision")
                .and_then(Value::as_str)
                .unwrap_or("");
            if let Some(concerns) = reviewer.get("concerns").and_then(Value::as_array) {
                for c in concerns {
                    if let Some(id) = c.get("id").and_then(Value::as_str) {
                        concern_slugs
                            .entry(id.to_owned())
                            .or_default()
                            .insert(slug.to_owned());
                    }
                }
            }
            if decision == "block" {
                let block_reasons = reviewer
                    .get("block_reasons")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|e| e.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                block_decisions.push((slug.to_owned(), block_reasons));
            }
        }
    }

    if total_crash > 0 {
        out.push(Suggestion {
            target: skill_md.clone(),
            rationale: format!(
                "{total_crash} crash(es) across {} run(s) ({}); document the crash-recovery loop more prominently",
                crash_note_runs.len(),
                crash_note_runs.join(", ")
            ),
            appended_lines: vec![
                String::new(),
                "## Crash recovery".to_owned(),
                String::new(),
                "When a Stage-3 iteration emits a FailureCapsule, the loop retries up to 3".to_owned(),
                "times before halting with status=crash. If you see crashes piling up across".to_owned(),
                "runs, look for an audit-checks regex false positive or a harness script".to_owned(),
                "that swallows the real metric.".to_owned(),
            ],
        });
    }

    if total_revert > total_advance && total_advance + total_revert > 0 {
        out.push(Suggestion {
            target: skill_md.clone(),
            rationale: format!(
                "revert/advance ratio {total_revert}/{total_advance} suggests the edit-agent regresses more than improves; consider tightening intake"
            ),
            appended_lines: vec![
                String::new(),
                "## Intake tightening".to_owned(),
                String::new(),
                "If the iterate-loop's revert ratio exceeds its advance ratio, the".to_owned(),
                "intent-card's unfakeable metric likely isn't load-bearing enough.".to_owned(),
                "Force a 5-Whys re-derive before scaling up.".to_owned(),
            ],
        });
    }

    if !zero_advance_runs.is_empty() {
        out.push(Suggestion {
            target: bad_rust.clone(),
            rationale: format!(
                "{} run(s) made 0 advances after baseline ({}); add a BAD_RUST pattern for stalled iteration",
                zero_advance_runs.len(),
                zero_advance_runs.join(", ")
            ),
            appended_lines: vec![
                String::new(),
                "## Stalled iteration (HLT-aux-STALLED)".to_owned(),
                String::new(),
                "Symptom: N iterations after baseline, 0 advances.".to_owned(),
                "Likely cause: the unfakeable metric is at a local maximum the edit-agent".to_owned(),
                "cannot navigate, or the gate is rejecting every diff for reasons unrelated".to_owned(),
                "to the metric.".to_owned(),
                "Fix: re-read failure-capsules; check whether a clippy/audit lint blocks".to_owned(),
                "every diff; consider relaxing one lint with a written waiver.".to_owned(),
            ],
        });
    }

    if harness_abort_seen {
        out.push(Suggestion {
            target: template_harness,
            rationale: "≥1 proposal note mentions harness abort / set -e — keep the scaffold template's errexit OFF so a failing iteration still emits metrics".to_owned(),
            appended_lines: vec![
                "# NOTE: this scaffold template runs with `set -uo pipefail` only (NOT -e)".to_owned(),
                "# so a failing iteration still emits a valid metrics.json — the loop's".to_owned(),
                "# advance/revert decision depends on the metric, not on shell exit.".to_owned(),
            ],
        });
    }

    if total_capsules > 0 {
        out.push(Suggestion {
            target: skill_md.clone(),
            rationale: format!(
                "{total_capsules} failure capsule(s) total across runs; surface them in postmortem"
            ),
            appended_lines: vec![
                String::new(),
                "## Failure-capsule review".to_owned(),
                String::new(),
                "Failure capsules accumulate in target/autobuilder/failure-capsules/.".to_owned(),
                "Postmortem should aggregate by repro-fingerprint, not by timestamp.".to_owned(),
            ],
        });
    }

    // ---- Rules consuming the enriched audit_summary / reviewer_summary ----

    // Any reviewer decision == block on any proposal — surface immediately,
    // no threshold. This is the loudest reviewer signal and shouldn't wait
    // for recurrence.
    for (slug, reasons) in &block_decisions {
        let reasons_str = if reasons.is_empty() {
            "<no reasons given>".to_owned()
        } else {
            reasons.join(", ")
        };
        out.push(Suggestion {
            target: skill_md.clone(),
            rationale: format!(
                "reviewer-agent decision=block on {slug}: {reasons_str}; document the failure mode so the next run treats it as a known anti-pattern"
            ),
            appended_lines: vec![
                String::new(),
                format!("## Known block — {slug}"),
                String::new(),
                format!("Reviewer flagged: {reasons_str}."),
                "Investigate the underlying cause and either fix the implementation".to_owned(),
                "or amend the intent-card if the AC was wrong. Re-run the gate before".to_owned(),
                "shipping.".to_owned(),
            ],
        });
    }

    // Recurring concern_id across ≥2 distinct slugs — the human reviewer
    // keeps flagging the same issue, so it's worth proactively documenting.
    for (concern_id, slugs) in &concern_slugs {
        if slugs.len() < 2 {
            continue;
        }
        let slug_list = slugs.iter().cloned().collect::<Vec<_>>().join(", ");
        out.push(Suggestion {
            target: skill_md.clone(),
            rationale: format!(
                "reviewer concern `{concern_id}` repeated across {} runs ({slug_list}); promote to SKILL.md known-issues",
                slugs.len()
            ),
            appended_lines: vec![
                String::new(),
                format!("## Recurring reviewer concern — {concern_id}"),
                String::new(),
                format!(
                    "This concern was raised by the independent reviewer-agent on {} separate runs ({slug_list}).",
                    slugs.len()
                ),
                "Treat as a known anti-pattern: file an explicit waiver if intentional,".to_owned(),
                "or fix the underlying cause before the next gate run.".to_owned(),
            ],
        });
    }

    // Recurring blocking-detector across ≥2 distinct slugs — the audit
    // detector keeps firing as blocking on different projects, which is
    // the signature of a layout-dependent false positive or a detector
    // whose threshold is too low.
    for (detector, slugs) in &blocking_detector_slugs {
        if slugs.len() < 2 {
            continue;
        }
        let slug_list = slugs.iter().cloned().collect::<Vec<_>>().join(", ");
        out.push(Suggestion {
            target: audit_rules.clone(),
            rationale: format!(
                "audit detector `{detector}` produced blocking findings on {} distinct projects ({slug_list}); investigate whether the detector is too aggressive or layout-dependent",
                slugs.len()
            ),
            appended_lines: vec![
                String::new(),
                format!("# NOTE — recurring detector across runs: {detector}"),
                format!(
                    "# Fired on {} distinct projects ({slug_list}). Likely either a layout-",
                    slugs.len()
                ),
                "# dependent false positive or a real recurring pattern that should be".to_owned(),
                "# either tightened (false positive) or promoted to a hard project-template".to_owned(),
                "# constraint (real pattern).".to_owned(),
            ],
        });
    }

    out
}

#[allow(clippy::too_many_arguments)] // flat report-rendering call site; struct'ifying adds noise
fn write_report(
    path: &Path,
    dir: &Path,
    since: Option<&str>,
    applied: &HashSet<String>,
    proposals: &[LoadedProposal],
    suggestions: &[Suggestion],
) -> Result<()> {
    let mut report = String::new();
    let date = receipt::now_rfc3339()?;
    report.push_str(&format!("# Evolution report — {date}\n\n"));
    report.push_str(&format!(
        "Scanned {} (filters: since={:?}, applied={} excluded). Top {} proposal(s) below.\n\n",
        dir.display(),
        since,
        applied.len(),
        proposals.len()
    ));
    if !proposals.is_empty() {
        report.push_str("| # | slug | iters | advance | revert | crash | capsules | source |\n");
        report.push_str("|---|---|---|---|---|---|---|---|\n");
        for (i, p) in proposals.iter().enumerate() {
            let slug = p
                .value
                .get("intent_slug")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let src = p.path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
            report.push_str(&format!(
                "| {} | {slug} | {} | {} | {} | {} | {} | `{src}` |\n",
                i + 1,
                field_u64(&p.value, "iters_total"),
                field_u64(&p.value, "iters_advance"),
                field_u64(&p.value, "iters_revert"),
                field_u64(&p.value, "iters_crash"),
                field_u64(&p.value, "failure_capsule_count"),
            ));
        }
        report.push('\n');
    }
    if suggestions.is_empty() {
        report.push_str("## Suggested edits\n\nNo patterns matched; nothing to suggest.\n");
    } else {
        report.push_str(&format!(
            "## Suggested edits ({})\n\n",
            suggestions.len()
        ));
        for (i, s) in suggestions.iter().enumerate() {
            report.push_str(&format!(
                "{}. **{}** — _{}_\n",
                i + 1,
                s.target.display(),
                s.rationale
            ));
        }
        report.push_str("\nSee the companion `.patch` file for the unified-diff hunks. Apply with `patch -p0 < <file>` from the skill dir, or hand-merge after review.\n");
    }
    fs::write(path, report)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn write_diff(path: &Path, suggestions: &[Suggestion]) -> Result<()> {
    let mut out = String::new();
    out.push_str("# autobuilder evolve diff — proposed appends to skill files.\n");
    out.push_str("# Each hunk is append-only. The base offset is the EOF of the\n");
    out.push_str("# target file at the time this diff was generated. Apply with\n");
    out.push_str("#   patch -p0 < <this-file>\n");
    out.push_str("# or hand-merge.\n\n");
    for s in suggestions {
        out.push_str(&render_append_hunk(&s.target, &s.rationale, &s.appended_lines));
        out.push('\n');
    }
    fs::write(path, out)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn render_append_hunk(target: &Path, rationale: &str, appended: &[String]) -> String {
    let original = fs::read_to_string(target).unwrap_or_default();
    let line_count = if original.is_empty() {
        0
    } else {
        original.lines().count()
    };
    let added = appended.len();
    let mut out = String::new();
    out.push_str(&format!("# Suggestion: {rationale}\n"));
    out.push_str(&format!("--- {}\n", target.display()));
    out.push_str(&format!("+++ {}\n", target.display()));
    // Append-mode hunk header. -L,0 +L+1,N means "at line L (0-context),
    // add N lines starting at L+1".
    out.push_str(&format!("@@ -{line_count},0 +{},{added} @@\n", line_count + 1));
    for line in appended {
        out.push('+');
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Returns `(applied_proposal_basenames, applied_suggestion_fingerprints)`.
///
/// Lines matching `applied-suggestion:<hex>` go into the second set and
/// suppress re-emission of suggestions whose fingerprint already landed.
/// Every other non-comment line is treated as a proposal basename, same as
/// before — preserves existing behavior for the manual #REJECTED pattern.
fn load_applied_log(dir: &Path) -> (HashSet<String>, HashSet<String>) {
    let mut proposals: HashSet<String> = HashSet::new();
    let mut fingerprints: HashSet<String> = HashSet::new();
    let Ok(text) = fs::read_to_string(dir.join("applied.log")) else {
        return (proposals, fingerprints);
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(fp) = trimmed.strip_prefix("applied-suggestion:") {
            fingerprints.insert(fp.to_owned());
        } else {
            proposals.insert(trimmed.to_owned());
        }
    }
    (proposals, fingerprints)
}

fn load_proposals(
    dir: &Path,
    since: Option<&str>,
    applied: &HashSet<String>,
) -> Result<Vec<LoadedProposal>> {
    let rd = fs::read_dir(dir)
        .with_context(|| format!("cannot read {}", dir.display()))?;
    let mut out: Vec<LoadedProposal> = Vec::new();
    for entry in rd {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with("evolution-proposal-")
            || Path::new(name)
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
                != Some("json")
        {
            continue;
        }
        if applied.contains(name) {
            continue;
        }
        let path = entry.path();
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let Ok(value): std::result::Result<Value, _> = serde_json::from_str(&text) else {
            continue;
        };
        if let Some(since) = since {
            let captured = value
                .get("captured_at")
                .and_then(Value::as_str)
                .unwrap_or("");
            if captured < since {
                continue;
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let score = field_u64(&value, "iters_advance") as f64
            + (field_u64(&value, "iters_crash") as f64) * 2.0
            + field_u64(&value, "failure_capsule_count") as f64;
        out.push(LoadedProposal {
            path,
            value,
            score,
        });
    }
    Ok(dedupe_by_slug(out))
}

/// Keep only the latest proposal per `intent_slug` (latest `captured_at`).
/// Re-running postmortem leaves a fresh JSON next to the old one — the
/// old one is the same run with stale counters. The honest behavior is to
/// learn from the latest snapshot per project, not weight a multi-run
/// proposal twice because there's an older copy on disk.
fn dedupe_by_slug(mut proposals: Vec<LoadedProposal>) -> Vec<LoadedProposal> {
    proposals.sort_by(|a, b| {
        let slug_a = a.value.get("intent_slug").and_then(Value::as_str).unwrap_or("");
        let slug_b = b.value.get("intent_slug").and_then(Value::as_str).unwrap_or("");
        let captured_a = a.value.get("captured_at").and_then(Value::as_str).unwrap_or("");
        let captured_b = b.value.get("captured_at").and_then(Value::as_str).unwrap_or("");
        slug_a
            .cmp(slug_b)
            .then_with(|| captured_b.cmp(captured_a)) // newer first within slug
    });
    let mut seen_slugs: HashSet<String> = HashSet::new();
    let mut out: Vec<LoadedProposal> = Vec::new();
    for p in proposals {
        let slug = p
            .value
            .get("intent_slug")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        if seen_slugs.contains(&slug) {
            continue;
        }
        seen_slugs.insert(slug);
        out.push(p);
    }
    out
}

fn field_u64(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return Path::new(&home).join(rest);
        }
    }
    PathBuf::from(s)
}
