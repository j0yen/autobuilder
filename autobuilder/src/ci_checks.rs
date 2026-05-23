//! Stage 4 — ci-checks receipt.
//!
//! Verifies that GitHub Actions workflow runs for the current HEAD commit
//! exist and ended with `conclusion=success`. Shells out to `gh run list`
//! because that's the lowest-effort way to read the GitHub API with
//! credentials already on the developer's machine. The receipt records every
//! observed run so the gate can prove which workflows are accounted for.
//!
//! Missing `gh`, missing auth, or zero runs on HEAD all produce a `block`
//! verdict — refusing to certify CI-green without evidence is the point.
//!
//! Exception: when the project has no `origin` remote configured (typical
//! for greenfield scaffolds before a `git push`), there is nothing to ask
//! GitHub about. In that case we emit `verdict=skipped` with a `skip_reason`
//! string, mirroring the `session-trace` "tracer-unavailable" pattern.
//! The gate counts `skipped` as passing for this receipt.

use crate::receipt;
use anyhow::{Context, Result, anyhow};
use clap::Args as ClapArgs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Project directory (used to scope `gh -R` to the correct repo).
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// Path to the `gh` binary. Defaults to whatever is on PATH.
    #[arg(long, default_value = "gh")]
    pub gh: String,

    /// Override the repo gh queries (owner/name). Defaults to the project's `origin`.
    #[arg(long)]
    pub repo: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct GhRun {
    #[serde(rename = "databaseId")]
    id: u64,
    #[serde(rename = "workflowName", default)]
    workflow_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    conclusion: String,
    #[serde(default)]
    url: String,
    #[serde(rename = "headBranch", default)]
    head_branch: String,
}

#[derive(Debug, Serialize)]
struct ReceiptDoc {
    schema: &'static str,
    head_sha: String,
    repo: String,
    run_count: usize,
    success_count: usize,
    failure_count: usize,
    pending_count: usize,
    runs: Vec<GhRun>,
    verdict: &'static str,
    /// Populated only when `verdict == "skipped"`; explains the structural
    /// reason the check could not be performed (e.g. no `origin` remote).
    /// Omitted from serialization when None so existing pass/block receipts
    /// stay byte-identical to today's shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    skip_reason: Option<String>,
    captured_at: String,
    receipt_digest: String,
}

#[allow(clippy::needless_pass_by_value)] // owned `Args` matches the clap-dispatched subcommand contract
pub(crate) fn run(args: Args) -> Result<()> {
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;

    let head_sha = git_rev_parse(&project, "HEAD")?;
    let repo = match args.repo {
        Some(r) => Some(r),
        None => try_detect_repo(&project)?,
    };
    let Some(repo) = repo else {
        return emit_skipped(
            &project,
            &head_sha,
            "no GitHub remote configured; ci-checks cannot be performed",
        );
    };

    let runs = list_runs(&args.gh, &repo, &head_sha)?;
    let mut success = 0usize;
    let mut failure = 0usize;
    let mut pending = 0usize;
    for r in &runs {
        match r.conclusion.as_str() {
            "success" => success += 1,
            "" if r.status != "completed" => pending += 1,
            _ => failure += 1,
        }
    }

    let verdict = if !runs.is_empty() && failure == 0 && pending == 0 {
        "pass"
    } else {
        "block"
    };

    let doc = ReceiptDoc {
        schema: "autobuilder.ci_checks_receipt.v1",
        head_sha: head_sha.clone(),
        repo: repo.clone(),
        run_count: runs.len(),
        success_count: success,
        failure_count: failure,
        pending_count: pending,
        runs,
        verdict,
        skip_reason: None,
        captured_at: receipt::now_rfc3339()?,
        receipt_digest: String::new(),
    };
    let value = serde_json::to_value(&doc)?;
    let receipt_path = project.join("target/autobuilder/receipts/ci-checks.json");
    receipt::write(&receipt_path, value)?;

    println!(
        "ci-checks: head={head_sha} repo={repo} runs={} success={success} failure={failure} pending={pending} verdict={verdict}",
        doc.run_count
    );

    match verdict {
        "pass" => Ok(()),
        _ if doc.run_count == 0 => Err(anyhow!(
            "no workflow runs found for {head_sha}; push the commit and wait for CI before re-running"
        )),
        _ => Err(anyhow!(
            "CI not green: {failure} failed run(s), {pending} pending run(s) for {head_sha}"
        )),
    }
}

/// Emit a `verdict=skipped` ci-checks receipt and return Ok.
///
/// Used when the project has no `origin` remote and the caller did not
/// pass `--repo` — we have nothing to ask GitHub about, but the gate still
/// needs a digest-bound receipt so the 8-receipt walk stays
/// structurally complete.
fn emit_skipped(project: &Path, head_sha: &str, reason: &str) -> Result<()> {
    let doc = ReceiptDoc {
        schema: "autobuilder.ci_checks_receipt.v1",
        head_sha: head_sha.to_owned(),
        repo: String::new(),
        run_count: 0,
        success_count: 0,
        failure_count: 0,
        pending_count: 0,
        runs: Vec::new(),
        verdict: "skipped",
        skip_reason: Some(reason.to_owned()),
        captured_at: receipt::now_rfc3339()?,
        receipt_digest: String::new(),
    };
    let value = serde_json::to_value(&doc)?;
    let receipt_path = project.join("target/autobuilder/receipts/ci-checks.json");
    receipt::write(&receipt_path, value)?;
    println!("ci-checks: head={head_sha} verdict=skipped reason={reason:?}");
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

/// Try to detect `owner/name` from the project's `origin` remote.
///
/// Returns:
/// - `Ok(Some(repo))` when the remote is configured and parseable.
/// - `Ok(None)` when the remote is unset — the caller should emit a
///   skipped receipt rather than failing.
/// - `Err(_)` for other I/O failures (git binary missing, etc.).
fn try_detect_repo(project: &Path) -> Result<Option<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["remote", "get-url", "origin"])
        .output()
        .context("failed to spawn git remote get-url origin")?;
    if !output.status.success() {
        // git exits non-zero when the remote is missing. Treat this as a
        // structural skip, not an error — the caller emits a skipped receipt.
        return Ok(None);
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    parse_owner_name(&url)
        .map(Some)
        .ok_or_else(|| anyhow!("could not parse owner/name from origin URL {url}"))
}

fn parse_owner_name(url: &str) -> Option<String> {
    // Handles https://github.com/<owner>/<name>(.git)? and git@github.com:<owner>/<name>(.git)?
    let trimmed = url.trim_end_matches('/').trim_end_matches(".git");
    let after_host = trimmed
        .rsplit_once("github.com/")
        .map(|(_, rest)| rest)
        .or_else(|| {
            trimmed
                .rsplit_once("github.com:")
                .map(|(_, rest)| rest)
        })?;
    let mut parts = after_host.split('/');
    let owner = parts.next()?;
    let name = parts.next()?;
    if owner.is_empty() || name.is_empty() {
        return None;
    }
    Some(format!("{owner}/{name}"))
}

fn list_runs(gh: &str, repo: &str, head_sha: &str) -> Result<Vec<GhRun>> {
    let output = Command::new(gh)
        .args([
            "run",
            "list",
            "-R",
            repo,
            "--commit",
            head_sha,
            "--json",
            "databaseId,workflowName,name,status,conclusion,url,headBranch",
            "--limit",
            "50",
        ])
        .output()
        .with_context(|| format!("failed to spawn `{gh} run list`"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "`gh run list` failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let runs: Vec<GhRun> = serde_json::from_str(&stdout)
        .context("could not parse `gh run list` JSON output")?;
    Ok(runs)
}
