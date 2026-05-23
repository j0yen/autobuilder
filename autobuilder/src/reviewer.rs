//! Stage 4 — reviewer-agent receipt launcher.
//!
//! The actual review is performed by an independent Claude subagent (see
//! `~/.claude/skills/autobuilder/prompts/reviewer-agent.md`). This binary
//! cannot spawn that subagent itself — that responsibility belongs to the
//! orchestrating Claude session running the autobuilder skill. What this
//! module owns:
//!
//! * `prepare`  — assemble a deterministic `review-request.json` packet (HEAD
//!                sha, base, intent-card digest, commit range, changed files)
//!                that the orchestrator hands to the subagent as context.
//! * `finalize` — read the subagent's decision JSON, validate it matches the
//!                schema declared in `prompts/reviewer-agent.md`, digest-bind
//!                it, and write the gate receipt at
//!                `target/autobuilder/receipts/reviewer-agent.json`.

use crate::receipt;
use anyhow::{Context, Result, anyhow};
use clap::{Args as ClapArgs, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    #[command(subcommand)]
    pub action: Action,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Action {
    /// Assemble target/autobuilder/review-request.json from HEAD + intent-card + diff range.
    Prepare(PrepareArgs),
    /// Validate the subagent's JSON output and write target/autobuilder/receipts/reviewer-agent.json.
    Finalize(FinalizeArgs),
}

#[derive(Debug, ClapArgs)]
pub(crate) struct PrepareArgs {
    /// Project directory containing agent/intent-card.json and the git repo.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// Range start: changes in `<base>..HEAD` will be reviewed.
    #[arg(long, default_value = "main")]
    pub base: String,
}

#[derive(Debug, ClapArgs)]
pub(crate) struct FinalizeArgs {
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// Path to the JSON the reviewer subagent produced.
    #[arg(long)]
    pub input: PathBuf,
}

#[allow(clippy::needless_pass_by_value)] // owned `Args` matches the clap-dispatched subcommand contract
pub(crate) fn run(args: Args) -> Result<()> {
    match args.action {
        Action::Prepare(a) => prepare(a),
        Action::Finalize(a) => finalize(a),
    }
}

#[derive(Debug, Serialize)]
struct ReviewRequest {
    schema: &'static str,
    head_sha: String,
    base_ref: String,
    base_sha: String,
    intent_card_sha256: String,
    intent_card_path: String,
    commit_count: usize,
    commits: Vec<CommitSummary>,
    changed_files: Vec<String>,
    captured_at: String,
}

#[derive(Debug, Serialize)]
struct CommitSummary {
    sha: String,
    subject: String,
}

#[allow(clippy::needless_pass_by_value)] // owned `PrepareArgs` matches the clap-dispatched contract
fn prepare(args: PrepareArgs) -> Result<()> {
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;

    let head_sha = git_rev_parse(&project, "HEAD")?;
    let base_sha = git_rev_parse(&project, &args.base)
        .with_context(|| format!("could not resolve --base {}", args.base))?;

    let intent_card_path = project.join("agent/intent-card.json");
    let intent_card_bytes = fs::read(&intent_card_path)
        .with_context(|| format!("missing intent-card at {}", intent_card_path.display()))?;
    let intent_card_sha256 = sha256_hex(&intent_card_bytes);

    let range = format!("{}..HEAD", args.base);
    let commits = git_commit_summaries(&project, &range)?;
    let changed_files = git_changed_files(&project, &range)?;

    let request = ReviewRequest {
        schema: "autobuilder.review_request.v1",
        head_sha: head_sha.clone(),
        base_ref: args.base.clone(),
        base_sha,
        intent_card_sha256,
        intent_card_path: "agent/intent-card.json".to_owned(),
        commit_count: commits.len(),
        commits,
        changed_files,
        captured_at: receipt::now_rfc3339()?,
    };

    let out_path = project.join("target/autobuilder/review-request.json");
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&out_path, serde_json::to_vec_pretty(&request)?)?;

    println!(
        "reviewer-agent prepare: head={head_sha} base={} commits={} files={} wrote={}",
        args.base,
        request.commit_count,
        request.changed_files.len(),
        out_path.display()
    );
    println!("Next: spawn the reviewer subagent with:");
    println!("  prompt: ~/.claude/skills/autobuilder/prompts/reviewer-agent.md");
    println!("  context: {}", out_path.display());
    println!("Then: autobuilder reviewer-agent finalize --input <subagent-output>.json");
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct ReviewerOutput {
    schema: String,
    head_sha: String,
    intent_card_sha: String,
    decision: String,
    #[serde(default)]
    block_reasons: Vec<String>,
    #[serde(default)]
    concern_reasons: Vec<ConcernEntry>,
    falsification: Falsification,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ConcernEntry {
    id: String,
    note: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Falsification {
    test_audit: String,
    panic_audit: String,
    unsafe_audit: String,
    public_api_audit: String,
    deps_audit: String,
    drift_audit: String,
    /// Counter-attack: a plausible attack the test suite would catch,
    /// plus an executable test skeleton. Back-compat: deserializes from
    /// either the old prose-only `String` shape OR the new structured
    /// `{description, test_skeleton}` shape. Empty `test_skeleton` is
    /// honest (reviewer couldn't construct one) and downgrades the
    /// decision to concern during finalize.
    counter_attack: CounterAttack,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum CounterAttack {
    /// New shape (post-2026-05-23). The orchestrator surfaces
    /// `test_skeleton` to the adversarial-agent for `tests/adversarial_*.rs`.
    Structured {
        description: String,
        test_skeleton: String,
    },
    /// Legacy prose-only shape from receipts written before
    /// the reviewer-agent.md prompt was upgraded.
    Prose(String),
}

impl CounterAttack {
    fn description(&self) -> &str {
        match self {
            Self::Structured { description, .. } => description,
            Self::Prose(s) => s,
        }
    }
    fn test_skeleton(&self) -> &str {
        match self {
            Self::Structured { test_skeleton, .. } => test_skeleton,
            // Legacy receipts have no test_skeleton; treat as empty so
            // the finalize downgrade rule fires uniformly.
            Self::Prose(_) => "",
        }
    }
}

#[derive(Debug, Serialize)]
struct FinalReceipt {
    schema: &'static str,
    head_sha: String,
    intent_card_sha: String,
    decision: String,
    block_reasons: Vec<String>,
    concern_reasons: Vec<ConcernEntry>,
    falsification: Falsification,
    captured_at: String,
    receipt_digest: String,
}

#[allow(clippy::needless_pass_by_value)] // owned `FinalizeArgs` matches the clap-dispatched contract
#[allow(clippy::too_many_lines)] // a single linear pipeline (parse → validate → bind → write); splitting would hide the flow
fn finalize(args: FinalizeArgs) -> Result<()> {
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;

    let raw = fs::read_to_string(&args.input)
        .with_context(|| format!("missing reviewer output at {}", args.input.display()))?;
    let parsed: ReviewerOutput =
        serde_json::from_str(&raw).context("reviewer output is not a valid receipt JSON")?;

    if parsed.schema != "autobuilder.reviewer_agent_receipt.v1" {
        return Err(anyhow!(
            "reviewer output schema mismatch: got {} expected autobuilder.reviewer_agent_receipt.v1",
            parsed.schema
        ));
    }
    if !matches!(parsed.decision.as_str(), "pass" | "concern" | "block") {
        return Err(anyhow!(
            "reviewer decision must be one of pass|concern|block, got {}",
            parsed.decision
        ));
    }
    for ConcernEntry { id, note } in &parsed.concern_reasons {
        if id.is_empty() || note.is_empty() {
            return Err(anyhow!(
                "concern_reasons entries must have non-empty id and note"
            ));
        }
    }
    let f = &parsed.falsification;
    let prose_sections: [(&str, &String); 6] = [
        ("test_audit", &f.test_audit),
        ("panic_audit", &f.panic_audit),
        ("unsafe_audit", &f.unsafe_audit),
        ("public_api_audit", &f.public_api_audit),
        ("deps_audit", &f.deps_audit),
        ("drift_audit", &f.drift_audit),
    ];
    for (name, body) in prose_sections {
        if body.trim().is_empty() {
            return Err(anyhow!(
                "falsification.{name} is empty; reviewer-agent.md requires every section to be non-empty"
            ));
        }
    }
    if f.counter_attack.description().trim().is_empty() {
        return Err(anyhow!(
            "falsification.counter_attack.description is empty; reviewer-agent.md requires a plausible attack to be described"
        ));
    }
    // Downgrade pass → concern when test_skeleton is empty. An empty
    // skeleton means the reviewer either couldn't construct a falsifying
    // input (legitimate, but worth flagging) or skipped writing one
    // (lazy, also worth flagging). Either way, "pass" overclaims
    // confidence; "concern" is honest and lets the gate continue
    // (pass_verdicts for reviewer-agent includes both).
    let mut effective_decision = parsed.decision.clone();
    let mut downgrade_concerns = parsed.concern_reasons.clone();
    if f.counter_attack.test_skeleton().trim().is_empty()
        && effective_decision == "pass"
    {
        effective_decision = "concern".to_string();
        downgrade_concerns.push(ConcernEntry {
            id: "counter-attack-skeleton-missing".to_string(),
            note: "reviewer-agent supplied counter_attack prose but no executable test_skeleton; downgrading to concern so the adversarial-agent has nothing to consume — the next iteration cannot reuse the attack".to_string(),
        });
    }

    let head_sha = git_rev_parse(&project, "HEAD")?;
    if parsed.head_sha != head_sha {
        return Err(anyhow!(
            "reviewer head_sha={} does not match current HEAD={}",
            parsed.head_sha,
            head_sha
        ));
    }
    let intent_card_bytes = fs::read(project.join("agent/intent-card.json"))
        .context("missing agent/intent-card.json")?;
    let intent_card_sha = sha256_hex(&intent_card_bytes);
    if parsed.intent_card_sha != intent_card_sha {
        return Err(anyhow!(
            "reviewer intent_card_sha={} does not match current intent-card sha256={}",
            parsed.intent_card_sha,
            intent_card_sha
        ));
    }

    let doc = FinalReceipt {
        schema: "autobuilder.reviewer_agent_receipt.v1",
        head_sha: parsed.head_sha,
        intent_card_sha: parsed.intent_card_sha,
        decision: effective_decision,
        block_reasons: parsed.block_reasons,
        concern_reasons: downgrade_concerns,
        falsification: parsed.falsification,
        captured_at: receipt::now_rfc3339()?,
        receipt_digest: String::new(),
    };
    let value = serde_json::to_value(&doc)?;
    let receipt_path = project.join("target/autobuilder/receipts/reviewer-agent.json");
    receipt::write(&receipt_path, value)?;

    println!(
        "reviewer-agent finalize: decision={} head={head_sha} wrote={}",
        doc.decision,
        receipt_path.display()
    );
    if doc.decision == "block" {
        return Err(anyhow!(
            "reviewer decision is block; risk gate cannot pass until reasons are resolved"
        ));
    }
    Ok(())
}

fn git_rev_parse(project: &Path, refname: &str) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["rev-parse", refname])
        .output()
        .with_context(|| format!("failed to spawn git rev-parse {refname}"))?;
    if !output.status.success() {
        return Err(anyhow!(
            "git rev-parse {refname} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git_commit_summaries(project: &Path, range: &str) -> Result<Vec<CommitSummary>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["log", "--first-parent", "--format=%H %s", range])
        .output()
        .with_context(|| format!("failed to spawn git log {range}"))?;
    if !output.status.success() {
        return Err(anyhow!(
            "git log {range} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut out = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let (sha, subject) = line.split_once(' ').unwrap_or((line, ""));
        out.push(CommitSummary {
            sha: sha.to_owned(),
            subject: subject.to_owned(),
        });
    }
    Ok(out)
}

fn git_changed_files(project: &Path, range: &str) -> Result<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["diff", "--name-only", range])
        .output()
        .with_context(|| format!("failed to spawn git diff {range}"))?;
    if !output.status.success() {
        return Err(anyhow!(
            "git diff {range} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}
