//! Stage 5 — Postmortem aggregator. Reads
//! `target/autobuilder/{results.tsv, receipts/*.json, failure-capsules/*.json}`,
//! renders `target/autobuilder/postmortem.md`, and queues an
//! `evolution-proposal-<slug>-<ts>.json` into
//! `~/.claude/skills/autobuilder/proposals/`.
//!
//! Inputs are best-effort: missing files are noted but never fatal, so a
//! run that crashed mid-loop can still produce a postmortem.

use crate::receipt;
use anyhow::{Context, Result, anyhow};
use clap::Args as ClapArgs;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Project directory containing target/autobuilder/.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// Where to deposit the evolution-proposal JSON. Defaults to the skill dir.
    #[arg(
        long,
        default_value = "~/.claude/skills/autobuilder/proposals"
    )]
    pub proposals_dir: String,

    /// Skill root, used to locate templates/scaffold/ for the
    /// template-drift detector. When the project's scripts/*.sh has been
    /// patched away from what the template ships, the diff is captured in
    /// the proposal so evolve can detect the recurring-fix pattern across
    /// multiple projects.
    #[arg(long, default_value = "~/.claude/skills/autobuilder")]
    pub skill_root: String,
}

#[derive(Debug, Serialize)]
struct ResultsRow {
    commit: String,
    metric: String,
    iteration: String,
    ac_pass: String,
    ac_total: String,
    status: String,
    description: String,
}

#[derive(Debug, Serialize)]
struct Proposal {
    schema: &'static str,
    intent_slug: String,
    head_sha: String,
    captured_at: String,
    iters_total: usize,
    iters_advance: usize,
    iters_revert: usize,
    iters_crash: usize,
    failure_capsule_count: usize,
    final_metric: Option<f64>,
    notes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    audit_summary: Option<AuditSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reviewer_summary: Option<ReviewerSummary>,
    /// Project-local divergence from `templates/scaffold/scripts/*.sh`,
    /// captured as unified diffs. Empty when scripts match the template
    /// (the common case for fresh scaffolds). When the same diff appears
    /// across multiple projects, evolve treats it as evidence of a
    /// recurring template bug and surfaces a suggestion to patch the
    /// template — closing the "we keep fixing the same thing per project"
    /// loop.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    template_diffs: Vec<TemplateDiff>,
}

#[derive(Debug, Serialize)]
struct TemplateDiff {
    /// Path relative to the project root, e.g. "scripts/run-metrics.sh".
    /// Same relative path resolves under `templates/scaffold/<path>` in
    /// the skill tree.
    path: String,
    /// sha256 of the diff body. Used by evolve to group identical
    /// per-project fixes across slugs.
    diff_sha256: String,
    /// The unified diff itself (`diff -u template project`).
    diff_body: String,
}

#[derive(Debug, Serialize)]
struct AuditSummary {
    blocking_count: i64,
    advisory_count: i64,
    /// Per-detector occurrence counts across all findings in risk-gate.json.
    /// Distinct detector names are the load-bearing signal for evolve — a
    /// detector that fires across multiple projects' risk-gates is the
    /// "audit-checks needs tightening" pattern we want to surface.
    detector_counts: BTreeMap<String, i64>,
    /// The subset of detectors that produced ≥1 blocking finding here.
    blocking_detectors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ReviewerSummary {
    decision: String,
    block_reasons: Vec<String>,
    /// Each concern's `id` plus its `note`, preserved verbatim so evolve
    /// can recognize identical concerns across runs.
    concerns: Vec<ConcernEntry>,
}

#[derive(Debug, Serialize)]
struct ConcernEntry {
    id: String,
    note: String,
}

#[allow(clippy::needless_pass_by_value)] // owned `Args` matches the clap-dispatched subcommand contract
#[allow(clippy::too_many_lines)] // single linear aggregation pipeline; splitting hides the flow
pub(crate) fn run(args: Args) -> Result<()> {
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;
    let head_sha = git_head(&project).unwrap_or_else(|_| "unknown".to_owned());
    let intent_slug = read_intent_slug(&project).unwrap_or_else(|| "unknown".to_owned());

    let results_tsv = project.join("target/autobuilder/results.tsv");
    let rows = read_results_tsv(&results_tsv);
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for r in &rows {
        let bucket = match r.status.as_str() {
            "advance" => "advance",
            "revert" => "revert",
            "crash" => "crash",
            "baseline" => "baseline",
            _ => "other",
        };
        *counts.entry(bucket).or_insert(0) += 1;
    }

    let receipts_dir = project.join("target/autobuilder/receipts");
    let receipt_count = count_jsons(&receipts_dir);
    let capsule_dir = project.join("target/autobuilder/failure-capsules");
    let capsule_count = count_jsons(&capsule_dir);

    let final_metric = rows.last().and_then(|r| r.metric.parse::<f64>().ok());

    let mut notes: Vec<String> = Vec::new();
    let crash_count = counts.get("crash").copied().unwrap_or(0);
    if crash_count > 0 {
        notes.push(format!(
            "{crash_count} iteration(s) crashed; see target/autobuilder/failure-capsules/"
        ));
    }
    let advance = counts.get("advance").copied().unwrap_or(0);
    let revert = counts.get("revert").copied().unwrap_or(0);
    if revert > advance && revert > 0 {
        notes.push(format!(
            "revert ratio {revert}/{advance} suggests the edit-agent is regressing more than improving"
        ));
    }
    if rows.is_empty() {
        notes.push("results.tsv empty or missing; nothing to aggregate".to_owned());
    }

    let postmortem_path = project.join("target/autobuilder/postmortem.md");
    write_postmortem(
        &postmortem_path,
        &intent_slug,
        &head_sha,
        &rows,
        &counts,
        receipt_count,
        capsule_count,
        &notes,
    )?;

    let audit_summary = read_audit_summary(&receipts_dir.join("risk-gate.json"));
    let reviewer_summary = read_reviewer_summary(&receipts_dir.join("reviewer-agent.json"));
    let skill_root = expand_tilde(&args.skill_root);
    let template_diffs = compute_template_diffs(&project, &skill_root);

    let proposal = Proposal {
        schema: "autobuilder.evolution_proposal.v1",
        intent_slug: intent_slug.clone(),
        head_sha,
        captured_at: receipt::now_rfc3339()?,
        iters_total: rows.len(),
        iters_advance: advance,
        iters_revert: revert,
        iters_crash: counts.get("crash").copied().unwrap_or(0),
        failure_capsule_count: capsule_count,
        final_metric,
        notes,
        audit_summary,
        reviewer_summary,
        template_diffs,
    };
    let proposals_dir = expand_tilde(&args.proposals_dir);
    fs::create_dir_all(&proposals_dir)
        .with_context(|| format!("cannot create proposals dir {}", proposals_dir.display()))?;
    let ts = receipt::now_rfc3339()?.replace(':', "-");
    let proposal_path =
        proposals_dir.join(format!("evolution-proposal-{intent_slug}-{ts}.json"));
    let value = serde_json::to_value(&proposal)?;
    fs::write(&proposal_path, serde_json::to_vec_pretty(&value)?)
        .with_context(|| format!("cannot write {}", proposal_path.display()))?;

    println!(
        "postmortem: iters={} advance={advance} revert={revert} crash={} wrote={} proposal={}",
        rows.len(),
        proposal.iters_crash,
        postmortem_path.display(),
        proposal_path.display()
    );
    Ok(())
}

/// Parse the risk-gate receipt into a structured summary suitable for evolve's
/// pattern-matching. Returns None if the file is missing/unreadable — evolve
/// degrades to "no audit signal" rather than crashing.
fn read_audit_summary(path: &Path) -> Option<AuditSummary> {
    let text = fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let findings = v.get("findings").and_then(Value::as_array)?;
    let mut detector_counts: BTreeMap<String, i64> = BTreeMap::new();
    let mut blocking_set: BTreeMap<String, bool> = BTreeMap::new();
    for f in findings {
        let Some(detector) = f.get("detector").and_then(Value::as_str) else { continue };
        *detector_counts.entry(detector.to_owned()).or_insert(0) += 1;
        if f.get("severity").and_then(Value::as_str) == Some("blocking") {
            blocking_set.insert(detector.to_owned(), true);
        }
    }
    Some(AuditSummary {
        blocking_count: v
            .get("blocking_count")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        advisory_count: v
            .get("advisory_count")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        detector_counts,
        blocking_detectors: blocking_set.into_keys().collect(),
    })
}

/// Parse the reviewer-agent receipt into a structured summary. Returns None
/// if missing — evolve degrades to "no reviewer signal."
fn read_reviewer_summary(path: &Path) -> Option<ReviewerSummary> {
    let text = fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let decision = v.get("decision").and_then(Value::as_str)?.to_owned();
    let block_reasons: Vec<String> = v
        .get("block_reasons")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let concerns: Vec<ConcernEntry> = v
        .get("concern_reasons")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let id = c.get("id").and_then(Value::as_str)?.to_owned();
                    let note = c.get("note").and_then(Value::as_str)?.to_owned();
                    Some(ConcernEntry { id, note })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(ReviewerSummary {
        decision,
        block_reasons,
        concerns,
    })
}

fn read_intent_slug(project: &Path) -> Option<String> {
    let text = fs::read_to_string(project.join("agent/intent-card.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    v.get("intent_slug")?.as_str().map(str::to_owned)
}

fn git_head(project: &Path) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["rev-parse", "HEAD"])
        .output()
        .context("failed to spawn git")?;
    if !out.status.success() {
        return Err(anyhow!(
            "git rev-parse HEAD failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn read_results_tsv(path: &Path) -> Vec<ResultsRow> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if i == 0 && line.starts_with("commit\t") {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 6 {
            continue;
        }
        out.push(ResultsRow {
            commit: cols.first().copied().unwrap_or_default().to_owned(),
            metric: cols.get(1).copied().unwrap_or_default().to_owned(),
            iteration: cols.get(2).copied().unwrap_or_default().to_owned(),
            ac_pass: cols.get(3).copied().unwrap_or_default().to_owned(),
            ac_total: cols.get(4).copied().unwrap_or_default().to_owned(),
            status: cols.get(5).copied().unwrap_or_default().to_owned(),
            description: cols.get(6).copied().unwrap_or_default().to_owned(),
        });
    }
    out
}

fn count_jsons(dir: &Path) -> usize {
    let Ok(rd) = fs::read_dir(dir) else {
        return 0;
    };
    rd.filter_map(Result::ok)
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x == "json")
        })
        .count()
}

/// Diff each project script under `scripts/` against the template at
/// `<skill_root>/templates/scaffold/<path>`. Returns one `TemplateDiff`
/// per script that differs; empty when project matches template (the
/// healthy state). Silently skips when either side is missing — the
/// drift detector is best-effort and must never fail postmortem.
fn compute_template_diffs(project: &Path, skill_root: &Path) -> Vec<TemplateDiff> {
    const TRACKED: &[&str] = &[
        "scripts/run-metrics.sh",
        "scripts/audit.sh",
        "scripts/risk-gate.sh",
    ];
    let template_root = skill_root.join("templates/scaffold");
    let mut out: Vec<TemplateDiff> = Vec::new();
    for rel in TRACKED {
        let project_path = project.join(rel);
        let template_path = template_root.join(rel);
        if !project_path.exists() || !template_path.exists() {
            continue;
        }
        // `diff -u` exits 0 if identical, 1 if differing, ≥2 on error.
        // We capture only the "differing" case; everything else (including
        // missing `diff` binary) yields nothing.
        let Ok(output) = Command::new("diff")
            .arg("-u")
            .arg("--label")
            .arg(format!("template:{rel}"))
            .arg("--label")
            .arg(format!("project:{rel}"))
            .arg(&template_path)
            .arg(&project_path)
            .output()
        else {
            continue;
        };
        if output.status.code() != Some(1) {
            continue;
        }
        let diff_body = String::from_utf8_lossy(&output.stdout).into_owned();
        if diff_body.is_empty() {
            continue;
        }
        let mut hasher = Sha256::new();
        hasher.update(diff_body.as_bytes());
        let diff_sha256 = format!("{:x}", hasher.finalize());
        out.push(TemplateDiff {
            path: (*rel).to_owned(),
            diff_sha256,
            diff_body,
        });
    }
    out
}

#[allow(clippy::too_many_arguments)] // postmortem rendering — flat data passthrough is clearer than a struct here
fn write_postmortem(
    path: &Path,
    slug: &str,
    head_sha: &str,
    rows: &[ResultsRow],
    counts: &BTreeMap<&'static str, usize>,
    receipt_count: usize,
    capsule_count: usize,
    notes: &[String],
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = String::new();
    out.push_str(&format!("# Postmortem — {slug}\n\n"));
    out.push_str(&format!("HEAD: `{head_sha}`\n\n"));
    out.push_str("## Iteration breakdown\n\n");
    out.push_str(&format!("- total: {}\n", rows.len()));
    for (k, v) in counts {
        out.push_str(&format!("- {k}: {v}\n"));
    }
    out.push_str(&format!("- receipts on disk: {receipt_count}\n"));
    out.push_str(&format!("- failure capsules: {capsule_count}\n\n"));
    if !rows.is_empty() {
        out.push_str("## results.tsv\n\n");
        out.push_str("| iter | commit | metric | ACs | status | description |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        for r in rows {
            let desc = r.description.replace('|', "\\|");
            out.push_str(&format!(
                "| {} | `{}` | {} | {}/{} | {} | {} |\n",
                r.iteration, r.commit, r.metric, r.ac_pass, r.ac_total, r.status, desc
            ));
        }
        out.push('\n');
    }
    if notes.is_empty() {
        out.push_str("## Notes\n\nNo concerns surfaced from this run.\n");
    } else {
        out.push_str("## Notes\n\n");
        for n in notes {
            out.push_str(&format!("- {n}\n"));
        }
    }
    fs::write(path, out)?;
    Ok(())
}

fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return Path::new(&home).join(rest);
        }
    }
    PathBuf::from(s)
}
