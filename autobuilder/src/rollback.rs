//! Stage 4 — rollback-plan receipt.
//!
//! Walks every commit in `<base>..HEAD` and verifies that `git revert` would
//! apply cleanly onto `HEAD`. Emits `target/autobuilder/rollback.md` (human-
//! readable) and `target/autobuilder/receipts/rollback-plan.json` (the gate
//! receipt). Uses `git merge-tree --write-tree` so the check is read-only and
//! never touches the working tree.

use crate::receipt;
use anyhow::{Context, Result, anyhow};
use clap::Args as ClapArgs;
use regex::Regex;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Project directory. The git repo at this path is the one inspected.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// Range start: commits in `<base>..HEAD` are checked.
    #[arg(long, default_value = "main")]
    pub base: String,
}

#[derive(Debug, Serialize)]
struct CommitEntry {
    sha: String,
    short_sha: String,
    subject: String,
    parent_count: usize,
    revertable: bool,
    /// True when the commit matches one of the known mechanical shapes (see
    /// `mechanical_patterns`) both by subject line AND by its own
    /// changed-file set being a subset of that pattern's expected files.
    /// Mechanical commits never count toward `blocking_count`/`verdict`.
    mechanical: bool,
    note: String,
}

#[derive(Debug, Serialize)]
struct ReceiptDoc {
    schema: &'static str,
    head_sha: String,
    base_ref: String,
    base_sha: String,
    rollback_md: String,
    commit_count: usize,
    revertable_count: usize,
    /// Commits classified `mechanical` (revert-clean or not) — see
    /// `CommitEntry::mechanical`.
    mechanical_count: usize,
    blocking_count: usize,
    verdict: &'static str,
    commits: Vec<CommitEntry>,
    captured_at: String,
    receipt_digest: String,
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

/// A commit is `mechanical` when its subject matches one of `patterns` AND
/// its own changed-file set (`files`) is a subset of that pattern's
/// `allowed_files`.
fn classify_mechanical(subject: &str, files: &[String], patterns: &[MechPattern]) -> bool {
    patterns.iter().any(|p| {
        p.subject_re.is_match(subject)
            && files.iter().all(|f| p.allowed_files.contains(&f.as_str()))
    })
}

#[allow(clippy::needless_pass_by_value)] // owned `Args` matches the clap-dispatched subcommand contract
pub(crate) fn run(args: Args) -> Result<()> {
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;

    let head_sha = git_rev_parse(&project, "HEAD")?;
    let base_sha = git_rev_parse(&project, &args.base).with_context(|| {
        format!("could not resolve --base {} in {}", args.base, project.display())
    })?;

    let patterns = mechanical_patterns()?;
    let commits = list_commits(&project, &args.base)?;
    let mut entries: Vec<CommitEntry> = Vec::with_capacity(commits.len());
    let mut blocking = 0usize;
    let mut mechanical_count = 0usize;
    for sha in &commits {
        let entry = check_commit(&project, sha, &patterns)?;
        if entry.mechanical {
            mechanical_count += 1;
        } else if !entry.revertable {
            // Only substantive (non-mechanical) commits count toward
            // `blocking_count`/`verdict` — a mechanical commit that fails
            // its revert dry-run stays visible in rollback.md but never
            // blocks (see requirements P0.2).
            blocking += 1;
        }
        entries.push(entry);
    }

    let rollback_md_rel = PathBuf::from("target/autobuilder/rollback.md");
    let rollback_md_abs = project.join(&rollback_md_rel);
    write_rollback_md(&rollback_md_abs, &head_sha, &args.base, &base_sha, &entries)?;

    let revertable_count = entries.iter().filter(|e| e.revertable).count();
    let verdict = if blocking == 0 { "pass" } else { "block" };

    let doc = ReceiptDoc {
        schema: "autobuilder.rollback_plan_receipt.v1",
        head_sha: head_sha.clone(),
        base_ref: args.base.clone(),
        base_sha,
        rollback_md: rollback_md_rel.to_string_lossy().into_owned(),
        commit_count: entries.len(),
        revertable_count,
        mechanical_count,
        blocking_count: blocking,
        verdict,
        commits: entries,
        captured_at: receipt::now_rfc3339()?,
        receipt_digest: String::new(),
    };
    let value = serde_json::to_value(&doc)?;
    let receipt_path = project.join("target/autobuilder/receipts/rollback-plan.json");
    receipt::write(&receipt_path, value)?;

    println!(
        "rollback-plan: head={head_sha} base={} commits={} revertable={revertable_count} mechanical={mechanical_count} verdict={verdict}",
        args.base,
        doc.commit_count
    );

    if blocking > 0 {
        return Err(anyhow!(
            "{blocking} of {} commits are not git-revert-clean; see {}",
            doc.commit_count,
            rollback_md_rel.display()
        ));
    }
    Ok(())
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

fn check_commit(project: &Path, sha: &str, patterns: &[MechPattern]) -> Result<CommitEntry> {
    let subject = run_git(project, &["log", "-1", "--format=%s", sha])?
        .trim()
        .to_owned();
    let parents_str = run_git(project, &["log", "-1", "--format=%P", sha])?;
    let parents: Vec<&str> = parents_str.split_whitespace().collect();
    let parent_count = parents.len();
    let files = commit_files(project, sha)?;
    let mechanical = classify_mechanical(&subject, &files, patterns);

    // The mainline parent used as `theirs` for the revert-merge.
    let Some(first_parent) = parents.first() else {
        return Ok(CommitEntry {
            sha: sha.to_owned(),
            short_sha: short(sha),
            subject,
            parent_count: 0,
            revertable: false,
            mechanical,
            note: "root commit (no parent) cannot be reverted".to_owned(),
        });
    };

    let merge_arg = format!("--merge-base={sha}");
    let (status, _stdout, stderr) = run_git_capturing(
        project,
        &["merge-tree", "--write-tree", &merge_arg, "HEAD", first_parent],
    )?;

    let revertable = status == 0;
    let note = if revertable {
        if parent_count > 1 {
            "merge commit; revert with `git revert -m 1`".to_owned()
        } else {
            "clean revert".to_owned()
        }
    } else {
        match stderr.lines().next() {
            Some(line) if !line.trim().is_empty() => format!("conflicts: {}", line.trim()),
            _ => "conflicts during revert".to_owned(),
        }
    };

    Ok(CommitEntry {
        sha: sha.to_owned(),
        short_sha: short(sha),
        subject,
        parent_count,
        revertable,
        mechanical,
        note,
    })
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

fn write_rollback_md(
    path: &Path,
    head_sha: &str,
    base_ref: &str,
    base_sha: &str,
    entries: &[CommitEntry],
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = String::new();
    out.push_str("# Rollback plan\n\n");
    out.push_str(&format!("HEAD: `{head_sha}`\n"));
    out.push_str(&format!("Base: `{base_ref}` (`{base_sha}`)\n\n"));
    out.push_str("Reverts are listed newest → oldest. Each `git revert` was\n");
    out.push_str("dry-run via `git merge-tree --write-tree` against current HEAD,\n");
    out.push_str("so the working tree was not touched during verification.\n\n");
    if entries.is_empty() {
        out.push_str("No commits in range.\n");
    } else {
        out.push_str("| # | sha | revertable | command | subject |\n");
        out.push_str("|---|---|---|---|---|\n");
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
            let subject = e.subject.replace('|', "\\|");
            out.push_str(&format!(
                "| {n} | `{sha}` | {mark} | {cmd} | {subj} |\n",
                n = i + 1,
                sha = e.short_sha,
                subj = subject,
            ));
        }
        out.push_str(
            "\n`(M)` marks a commit classified `mechanical` (matches a known housekeeping \
             shape — intent-card refresh, Cargo.lock version-field sync, or parallel-integrate \
             version bump — by both subject line and changed-file set) — non-revert-clean but \
             excluded from `blocking_count`/`verdict`.\n",
        );
        out.push_str("\n## Notes\n\n");
        for e in entries {
            out.push_str(&format!("- `{}` — {}\n", e.short_sha, e.note));
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
