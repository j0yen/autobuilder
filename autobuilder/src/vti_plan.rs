//! Stage 4 — vti-plan receipt (Vertical Test Index).
//!
//! Routes every path changed in `<base>..HEAD` through `agent/proof-lanes.toml`
//! and emits `target/autobuilder/receipts/vti-plan.json`. A path is acceptable
//! only when at least one lane glob matches (`confidence = 1.0`); zero matches
//! is a block. The receipt also surfaces the union of `required_commands` so
//! the loop runner / risk gate can verify they were exercised.

use crate::receipt;
use anyhow::{Context, Result, anyhow};
use clap::Args as ClapArgs;
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Project directory containing agent/proof-lanes.toml and the git repo.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// Range start: changes in `<base>..HEAD` will be routed.
    #[arg(long, default_value = "main")]
    pub base: String,

    /// Minimum confidence per path (default 0.70 per PLAN.md Stage 4).
    #[arg(long, default_value_t = 0.70)]
    pub min_confidence: f64,
}

#[derive(Debug, Deserialize)]
struct LanesFile {
    lane: Vec<LaneSpec>,
}

#[derive(Debug, Deserialize)]
struct LaneSpec {
    id: String,
    globs: Vec<String>,
    #[serde(default)]
    required_commands: Vec<String>,
}

struct CompiledLane {
    spec: LaneSpec,
    globset: GlobSet,
}

#[derive(Debug, Serialize)]
struct PathRoute {
    path: String,
    lanes: Vec<String>,
    confidence: f64,
}

#[derive(Debug, Serialize)]
struct ReceiptDoc {
    schema: &'static str,
    head_sha: String,
    base_ref: String,
    base_sha: String,
    proof_lanes_path: String,
    proof_lanes_sha256: String,
    min_confidence: f64,
    routes: Vec<PathRoute>,
    unrouted_paths: Vec<String>,
    required_commands: Vec<String>,
    verdict: &'static str,
    captured_at: String,
    receipt_digest: String,
}

#[allow(clippy::needless_pass_by_value)] // owned `Args` matches the clap-dispatched subcommand contract
#[allow(clippy::too_many_lines)] // a single linear pipeline (load → route → emit); splitting would hide the flow
pub(crate) fn run(args: Args) -> Result<()> {
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;

    let lanes_path = project.join("agent/proof-lanes.toml");
    let lanes_text = fs::read_to_string(&lanes_path)
        .with_context(|| format!("missing proof-lanes at {}", lanes_path.display()))?;
    let lanes_file: LanesFile =
        toml::from_str(&lanes_text).context("agent/proof-lanes.toml is not valid TOML")?;
    let lanes_sha256 = sha256_hex(lanes_text.as_bytes());

    if lanes_file.lane.is_empty() {
        return Err(anyhow!(
            "agent/proof-lanes.toml has no [[lane]] entries; nothing to route against"
        ));
    }
    let compiled = compile_lanes(lanes_file.lane)?;

    let head_sha = git_rev_parse(&project, "HEAD")?;
    let base_sha = git_rev_parse(&project, &args.base)
        .with_context(|| format!("could not resolve --base {}", args.base))?;
    let changed_files = git_changed_files(&project, &format!("{}..HEAD", args.base))?;

    let mut routes: Vec<PathRoute> = Vec::with_capacity(changed_files.len());
    let mut unrouted: Vec<String> = Vec::new();
    let mut commands: BTreeSet<String> = BTreeSet::new();

    for path in &changed_files {
        let mut matched: Vec<String> = Vec::new();
        for lane in &compiled {
            if lane.globset.is_match(path) {
                matched.push(lane.spec.id.clone());
                for cmd in &lane.spec.required_commands {
                    commands.insert(cmd.clone());
                }
            }
        }
        let confidence = if matched.is_empty() { 0.0 } else { 1.0 };
        if matched.is_empty() {
            unrouted.push(path.clone());
        }
        routes.push(PathRoute {
            path: path.clone(),
            lanes: matched,
            confidence,
        });
    }

    let all_above_threshold = routes.iter().all(|r| r.confidence >= args.min_confidence);
    let verdict = if all_above_threshold && unrouted.is_empty() {
        "pass"
    } else {
        "block"
    };

    let doc = ReceiptDoc {
        schema: "autobuilder.vti_plan_receipt.v1",
        head_sha: head_sha.clone(),
        base_ref: args.base.clone(),
        base_sha,
        proof_lanes_path: "agent/proof-lanes.toml".to_owned(),
        proof_lanes_sha256: lanes_sha256,
        min_confidence: args.min_confidence,
        routes,
        unrouted_paths: unrouted.clone(),
        required_commands: commands.into_iter().collect(),
        verdict,
        captured_at: receipt::now_rfc3339()?,
        receipt_digest: String::new(),
    };
    let value = serde_json::to_value(&doc)?;
    let receipt_path = project.join("target/autobuilder/receipts/vti-plan.json");
    receipt::write(&receipt_path, value)?;

    println!(
        "vti-plan: head={head_sha} base={} files={} unrouted={} verdict={verdict}",
        args.base,
        doc.routes.len(),
        doc.unrouted_paths.len(),
    );

    if verdict == "block" {
        return Err(anyhow!(
            "{} of {} changed paths are unrouted (no matching lane)",
            doc.unrouted_paths.len(),
            doc.routes.len()
        ));
    }
    Ok(())
}

fn compile_lanes(specs: Vec<LaneSpec>) -> Result<Vec<CompiledLane>> {
    let mut out = Vec::with_capacity(specs.len());
    for spec in specs {
        if spec.globs.is_empty() {
            return Err(anyhow!("lane {} has no globs", spec.id));
        }
        let mut builder = GlobSetBuilder::new();
        for g in &spec.globs {
            let glob = Glob::new(g)
                .with_context(|| format!("lane {} has invalid glob {g}", spec.id))?;
            builder.add(glob);
        }
        let globset = builder
            .build()
            .with_context(|| format!("failed to compile globs for lane {}", spec.id))?;
        out.push(CompiledLane { spec, globset });
    }
    Ok(out)
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
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}
