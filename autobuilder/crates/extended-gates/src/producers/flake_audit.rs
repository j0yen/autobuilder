//! `flake-audit`: `cargo test` rerun K times produces identical outcomes.
//!
//! Runs `cargo test --quiet` K times (default K=3, configurable via
//! `extended-gates.toml::flake_audit_runs`, or overridden per-invocation via
//! `--passes`) and asserts every run's exit code is identical. Heavy;
//! respects `AUTOBUILDER_SKIP_HEAVY`.
//!
//! `--only-changed-since <base>` narrows the run to the test binaries whose
//! sources changed in `<base>..HEAD` (matched against `tests/<name>.rs` and
//! `tests/<name>/main.rs`); any changed path outside `tests/` falls back to
//! the full suite.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;
use toml::Value as TomlValue;

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    runs: usize,
    exit_codes: Vec<Option<i32>>,
    deterministic: bool,
    passes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    changed_scope: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    changed_tests: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    git_diff_error: Option<String>,
}

fn runs_count(project: &Path) -> usize {
    let cfg = project.join("extended-gates.toml");
    if let Ok(text) = std::fs::read_to_string(&cfg) {
        if let Ok(value) = text.parse::<TomlValue>() {
            if let Some(n) = value.get("flake_audit_runs").and_then(TomlValue::as_integer) {
                if (1..=20).contains(&n) {
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    return n as usize;
                }
            }
        }
    }
    3
}

/// Outcome of classifying `git diff --name-only <base>..HEAD` against
/// `tests/` inside the project.
enum ChangedKind {
    /// `git diff` itself failed (e.g. unknown base rev) -- an INFRA-class
    /// failure, never a pass; carries stderr.
    GitError(String),
    /// A changed path lay outside `tests/`; carries the offending path as
    /// the fallback reason.
    Full(String),
    /// Every changed path matched `tests/<name>.rs` or `tests/<name>/main.rs`
    /// (possibly empty, e.g. only test files were deleted).
    Tests(Vec<String>),
}

/// Classify the paths changed in `<base>..HEAD` inside `project` into either
/// a set of `--test <name>` targets or a full-suite fallback.
///
/// # Errors
///
/// Returns an error if `git` cannot be spawned at all (binary missing).
/// A non-zero `git diff` exit (e.g. unknown `base` rev) is reported via
/// `ChangedKind::GitError`, not an `Err`.
fn classify_changed_since(project: &Path, base: &str) -> Result<ChangedKind> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["diff", "--name-only"])
        .arg(format!("{base}..HEAD"))
        .output()
        .context("spawn git diff --name-only")?;
    if !output.status.success() {
        return Ok(ChangedKind::GitError(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut names = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(rest) = line.strip_prefix("tests/") else {
            return Ok(ChangedKind::Full(line.to_owned()));
        };
        if let Some(name) = rest.strip_suffix(".rs") {
            if !name.is_empty() && !name.contains('/') {
                names.push(name.to_owned());
                continue;
            }
        }
        if let Some(name) = rest.strip_suffix("/main.rs") {
            if !name.is_empty() {
                names.push(name.to_owned());
                continue;
            }
        }
        // Unrecognized shape inside tests/ (e.g. a nested helper module);
        // it isn't itself a runnable `--test` target and doesn't force a
        // full-suite fallback -- only a path outside tests/ does that.
    }
    names.sort();
    names.dedup();
    Ok(ChangedKind::Tests(names))
}

/// Run the flake-audit.
///
/// # Errors
///
/// Returns an error if the receipt write fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    run_with_options(spec, project, None, None)
}

/// Run the flake-audit with the `--passes` / `--only-changed-since`
/// overrides. [`run`] is the zero-option convenience wrapper used by
/// [`crate::prelude::run_producer`].
///
/// # Errors
///
/// Returns an error if the receipt write fails, the project path cannot be
/// resolved, or `git` cannot be spawned at all when `only_changed_since` is
/// set.
pub fn run_with_options(
    spec: &ProducerSpec,
    project: &Path,
    passes_override: Option<u8>,
    only_changed_since: Option<&str>,
) -> Result<String> {
    let project = project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", project.display()))?;
    let project = project.as_path();

    if std::env::var("AUTOBUILDER_SKIP_HEAVY").is_ok() {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                runs: 0,
                exit_codes: Vec::new(),
                deterministic: true,
                passes: 0,
                changed_scope: None,
                changed_tests: None,
                fallback_reason: None,
                git_diff_error: None,
            },
        )?;
        return Ok("flake-audit: skipped (AUTOBUILDER_SKIP_HEAVY)".into());
    }

    let runs = runs_count(project);
    let passes = passes_override.map_or(runs, usize::from);

    let mut changed_scope: Option<&'static str> = None;
    let mut changed_tests: Option<Vec<String>> = None;
    let mut fallback_reason: Option<String> = None;
    let mut test_filter: Option<Vec<String>> = None;

    if let Some(base) = only_changed_since {
        match classify_changed_since(project, base)? {
            ChangedKind::GitError(stderr) => {
                write_receipt(
                    project,
                    spec,
                    "block",
                    Payload {
                        runs,
                        exit_codes: Vec::new(),
                        deterministic: true,
                        passes: 0,
                        changed_scope: Some("error"),
                        changed_tests: None,
                        fallback_reason: None,
                        git_diff_error: Some(stderr.clone()),
                    },
                )?;
                return Ok(format!(
                    "flake-audit: git diff --name-only {base}..HEAD failed: {stderr}"
                ));
            }
            ChangedKind::Full(reason) => {
                changed_scope = Some("full");
                fallback_reason = Some(reason);
            }
            ChangedKind::Tests(names) => {
                if names.is_empty() {
                    write_receipt(
                        project,
                        spec,
                        "pass",
                        Payload {
                            runs,
                            exit_codes: Vec::new(),
                            deterministic: true,
                            passes: 0,
                            changed_scope: Some("tests"),
                            changed_tests: Some(Vec::new()),
                            fallback_reason: None,
                            git_diff_error: None,
                        },
                    )?;
                    return Ok(format!(
                        "flake-audit: no changed test binaries since {base} \
                         (only deletions); nothing to run"
                    ));
                }
                changed_scope = Some("tests");
                changed_tests = Some(names.clone());
                test_filter = Some(names);
            }
        }
    }

    let mut codes: Vec<Option<i32>> = Vec::with_capacity(passes);
    for _ in 0..passes {
        let mut cmd = Command::new("cargo");
        cmd.args(["test", "--quiet"]);
        if let Some(names) = &test_filter {
            for name in names {
                cmd.arg("--test").arg(name);
            }
        }
        let status = cmd.current_dir(project).status();
        codes.push(status.ok().and_then(|s| s.code()));
    }
    let deterministic = codes.windows(2).all(|w| w.first() == w.get(1));
    let verdict = if deterministic && codes.first().is_some_and(|c| *c == Some(0)) {
        "pass"
    } else if codes.is_empty() {
        "skipped"
    } else {
        "block"
    };
    let summary = format!(
        "flake-audit: {passes} passes, exit codes {codes:?}, deterministic={deterministic}"
    );
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            runs,
            exit_codes: codes,
            deterministic,
            passes,
            changed_scope,
            changed_tests,
            fallback_reason,
            git_diff_error: None,
        },
    )?;
    Ok(summary)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod classify_tests {
    use super::{ChangedKind, classify_changed_since};
    use std::path::Path;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@e.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@e.com")
            .args(args)
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?} failed");
    }

    fn init_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        git(tmp.path(), &["init", "-q", "-b", "main"]);
        std::fs::create_dir_all(tmp.path().join("tests")).unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), b"pub fn f() {}\n").unwrap();
        std::fs::write(tmp.path().join("tests/alpha.rs"), b"#[test] fn t() {}\n").unwrap();
        git(tmp.path(), &["add", "-A"]);
        git(tmp.path(), &["commit", "-q", "-m", "base"]);
        tmp
    }

    #[test]
    fn only_tests_paths_changed_collects_names() {
        let tmp = init_repo();
        git(tmp.path(), &["tag", "base"]);
        std::fs::write(tmp.path().join("tests/alpha.rs"), b"#[test] fn t2() {}\n").unwrap();
        std::fs::write(tmp.path().join("tests/beta.rs"), b"#[test] fn t() {}\n").unwrap();
        git(tmp.path(), &["add", "-A"]);
        git(tmp.path(), &["commit", "-q", "-m", "touch tests"]);

        let outcome = classify_changed_since(tmp.path(), "base").unwrap();
        match outcome {
            ChangedKind::Tests(mut names) => {
                names.sort();
                assert_eq!(names, vec!["alpha".to_owned(), "beta".to_owned()]);
            }
            _ => panic!("expected ChangedKind::Tests"),
        }
    }

    #[test]
    fn src_path_changed_falls_back_to_full() {
        let tmp = init_repo();
        git(tmp.path(), &["tag", "base"]);
        std::fs::write(tmp.path().join("src/lib.rs"), b"pub fn f() { }\n").unwrap();
        git(tmp.path(), &["add", "-A"]);
        git(tmp.path(), &["commit", "-q", "-m", "touch src"]);

        let outcome = classify_changed_since(tmp.path(), "base").unwrap();
        match outcome {
            ChangedKind::Full(reason) => assert_eq!(reason, "src/lib.rs"),
            _ => panic!("expected ChangedKind::Full"),
        }
    }

    #[test]
    fn mixed_tests_and_src_still_falls_back_to_full() {
        let tmp = init_repo();
        git(tmp.path(), &["tag", "base"]);
        std::fs::write(tmp.path().join("tests/alpha.rs"), b"#[test] fn t2() {}\n").unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), b"pub fn f() { }\n").unwrap();
        git(tmp.path(), &["add", "-A"]);
        git(tmp.path(), &["commit", "-q", "-m", "touch both"]);

        let outcome = classify_changed_since(tmp.path(), "base").unwrap();
        assert!(matches!(outcome, ChangedKind::Full(_)));
    }
}
