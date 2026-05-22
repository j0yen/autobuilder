//! Gated self-evolution aggregator.
//!
//! Reads `~/.claude/skills/autobuilder/proposals/evolution-proposal-*.json`,
//! filters out proposals already applied (per `applied.log` next to them),
//! optionally filters by `--since <ISO>`, ranks the remainder by total
//! `iters_advance + iters_crash + failure_capsule_count` (a coarse "how
//! disruptive was this run" proxy until something better lands), and emits:
//!
//!   `proposals/evolve-report-<date>.md`   — human-readable summary
//!   `proposals/evolve-diff-<date>.patch`  — currently empty stub; a future
//!                                            iteration generates real diffs
//!                                            against SKILL.md / rules /
//!                                            templates.
//!
//! Never auto-applies. The output is for user review.

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
    #[arg(
        long,
        default_value = "~/.claude/skills/autobuilder/proposals"
    )]
    pub proposals_dir: String,
}

struct LoadedProposal {
    path: PathBuf,
    value: Value,
    score: f64,
}

#[allow(clippy::needless_pass_by_value)] // owned `Args` matches the clap-dispatched subcommand contract
pub(crate) fn run(args: Args) -> Result<()> {
    let dir = expand_tilde(&args.proposals_dir);
    if !dir.is_dir() {
        return Err(anyhow!(
            "proposals dir does not exist at {}; run `autobuilder postmortem` first",
            dir.display()
        ));
    }
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

    let mut report = String::new();
    report.push_str(&format!("# Evolution report — {date}\n\n"));
    report.push_str(&format!(
        "Scanned {} (after filtering: since={:?}, applied={} excluded).\n\n",
        dir.display(),
        args.since,
        applied.len()
    ));
    if proposals.is_empty() {
        report.push_str("No actionable proposals.\n");
    } else {
        report.push_str(&format!(
            "Top {} proposals (ranked by iters_advance + iters_crash + failure_capsule_count):\n\n",
            proposals.len()
        ));
        report.push_str("| # | slug | iters | advance | revert | crash | capsules | source |\n");
        report.push_str("|---|---|---|---|---|---|---|---|\n");
        for (i, p) in proposals.iter().enumerate() {
            let slug = p
                .value
                .get("intent_slug")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let iters = field_u64(&p.value, "iters_total");
            let advance = field_u64(&p.value, "iters_advance");
            let revert = field_u64(&p.value, "iters_revert");
            let crash = field_u64(&p.value, "iters_crash");
            let caps = field_u64(&p.value, "failure_capsule_count");
            let src = p.path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
            report.push_str(&format!(
                "| {} | {slug} | {iters} | {advance} | {revert} | {crash} | {caps} | `{src}` |\n",
                i + 1
            ));
        }
        report.push_str("\nReview each row; if the trend is real, file a follow-up that updates SKILL.md, rules/bad-rust.md, or templates/scaffold/ accordingly. Mark proposals applied by appending the file's basename to `applied.log` in this directory.\n");
    }
    fs::write(&report_path, report)
        .with_context(|| format!("cannot write {}", report_path.display()))?;

    // Stub the patch file so a future iteration replaces it with a real diff.
    fs::write(
        &diff_path,
        "# evolve diff -- placeholder; future iterations emit real patches\n",
    )
    .with_context(|| format!("cannot write {}", diff_path.display()))?;

    println!(
        "evolve: scanned={} top={} report={} diff={}",
        applied.len() + proposals.len(),
        proposals.len(),
        report_path.display(),
        diff_path.display()
    );
    Ok(())
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
        // The receipt counters are well below 2^52 in practice (one entry
        // per iteration of one loop run), so cast to f64 for tie-breaking
        // without worrying about precision.
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
