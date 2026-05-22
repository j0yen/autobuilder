//! Gated self-evolution aggregator.
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
//! Never auto-applies. Output is for user review.

use crate::receipt;
use anyhow::{Context, Result, anyhow};
use clap::Args as ClapArgs;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

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
    let applied: HashSet<String> = load_applied_log(&dir);
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

    println!(
        "evolve: scanned={} top={} suggestions={} report={} diff={}",
        applied.len() + proposals.len(),
        proposals.len(),
        suggestions.len(),
        report_path.display(),
        diff_path.display()
    );
    Ok(())
}

#[allow(clippy::too_many_lines)] // one branch per known proposal-pattern; splitting hides the rule set
fn derive_suggestions(proposals: &[LoadedProposal], skill_root: &Path) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = Vec::new();
    let skill_md = skill_root.join("SKILL.md");
    let bad_rust = skill_root.join("rules/bad-rust.md");
    let template_harness = skill_root.join("templates/scaffold/scripts/run-metrics.sh");

    let mut total_crash = 0u64;
    let mut total_revert = 0u64;
    let mut total_advance = 0u64;
    let mut total_capsules = 0u64;
    let mut zero_advance_runs: Vec<String> = Vec::new();
    let mut crash_note_runs: Vec<String> = Vec::new();
    let mut harness_abort_seen = false;

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
        if iters_advance == 0 && field_u64(&p.value, "iters_total") > 0 {
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
            target: skill_md,
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

fn load_applied_log(dir: &Path) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    let Ok(text) = fs::read_to_string(dir.join("applied.log")) else {
        return out;
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            out.insert(trimmed.to_owned());
        }
    }
    out
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
    Ok(out)
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
