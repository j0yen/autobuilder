//! Stage 4 — rollback-plan receipt.
//!
//! Walks every commit in `<base>..HEAD` and verifies that `git revert` would
//! apply cleanly onto `HEAD`. Emits `target/autobuilder/rollback.md` (human-
//! readable) and `target/autobuilder/receipts/rollback-plan.json` (the gate
//! receipt). Uses `git merge-tree --write-tree` so the check is read-only and
//! never touches the working tree, except for merge commits (see
//! `merge_revert_m1_dry_run`), which need a real `git revert -m 1` and so run
//! in a scratch worktree that is always torn down before returning.

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
    /// Per-class breakdown — see `Classified`.
    classified: Classified,
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

/// A qualifying chain run: `[start, end)` indices into the chronological
/// (oldest → newest) `metas` slice, always `end - start >= 2`.
struct ChainRun {
    start: usize,
    end: usize,
    /// Revert-cleanliness of the chain's terminal (most recent) commit only
    /// — per P0.1, non-terminal members are never individually re-checked.
    terminal_clean: bool,
    terminal_note: String,
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
    let chain_prefixes = chain_allowed_prefixes();

    // `list_commits` walks newest → oldest (matches the existing rollback.md
    // ordering); chain-run detection needs chronological (oldest → newest)
    // order since a "later commit supersedes an earlier one" is only
    // meaningful walked forward in time.
    let commits_newest_first = list_commits(&project, &args.base)?;
    let metas: Vec<CommitMeta> = commits_newest_first
        .iter()
        .rev()
        .map(|sha| commit_meta(&project, sha))
        .collect::<Result<_>>()?;

    let chain_runs = detect_chain_runs(&project, &metas, &chain_prefixes)?;
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
        entries_chrono.push(build_entry(&project, meta, idx, run, &patterns)?);
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
        classified,
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
/// break a run. Each qualifying run's revert-cleanliness is checked once,
/// against its terminal (most recent) commit only.
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
            let terminal_idx = j - 1;
            let (terminal_clean, terminal_note) = match metas.get(terminal_idx) {
                Some(terminal) => match terminal.parents.first() {
                    Some(fp) => merge_tree_revert_dry_run(project, &terminal.sha, fp)?,
                    None => (false, "chain terminal has no parent".to_owned()),
                },
                None => (false, "chain terminal missing (internal error)".to_owned()),
            };
            runs.push(ChainRun {
                start,
                end: j,
                terminal_clean,
                terminal_note,
            });
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

/// A commit that belongs to a qualifying chain run (P0.1): classification
/// and revert-cleanliness are shared across the whole chain — no per-commit
/// revert dry-run happens here.
fn build_chain_entry(meta: &CommitMeta, idx: usize, run: &ChainRun) -> CommitEntry {
    let chain_len = run.end - run.start;
    let position = idx.saturating_sub(run.start) + 1;
    let note = format!(
        "mechanical(chain): member {position} of {chain_len} on the chain-allowed path set; \
         revert-cleanliness checked once against the chain's terminal commit — {}",
        run.terminal_note.if_empty_then("clean revert"),
    );
    CommitEntry {
        sha: meta.sha.clone(),
        short_sha: short(&meta.sha),
        subject: meta.subject.clone(),
        parent_count: meta.parents.len(),
        revertable: run.terminal_clean,
        mechanical: true,
        class: CommitClass::MechanicalChain,
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
    out.push_str("dry-run via `git merge-tree --write-tree` against current HEAD\n");
    out.push_str("(merge commits use a real `git revert -m 1` in a scratch\n");
    out.push_str("worktree instead), so the caller's working tree was never touched\n");
    out.push_str("during verification.\n\n");
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
