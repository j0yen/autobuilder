//! `secrets-scan`: scan tracked source files for high-confidence secret patterns.
//!
//! Pure-Rust. Walks the project tree (skipping `target/`, `.git/`, vendored
//! reference dirs), reads each file's text, and matches a regex set tuned for
//! low false-positive: AWS access keys, GitHub PATs, private-key PEM headers,
//! Slack webhook URLs. The planted-failure fixture
//! (`tests/fixtures/leaked-key/`) embeds a synthetic AKIA pattern; the
//! producer must surface it with `verdict=block`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use regex::RegexSet;
use serde::Serialize;
use walkdir::WalkDir;

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    files_scanned: usize,
    findings: Vec<Finding>,
}

#[derive(Debug, Serialize)]
struct Finding {
    path: String,
    line: usize,
    pattern: &'static str,
}

const PATTERNS: &[(&str, &str)] = &[
    ("aws-access-key", r"AKIA[0-9A-Z]{16}"),
    ("github-pat", r"ghp_[A-Za-z0-9]{36}"),
    ("private-key-pem", r"-----BEGIN (RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----"),
    ("slack-webhook", r"https://hooks\.slack\.com/services/T[A-Z0-9]+/B[A-Z0-9]+/[A-Za-z0-9]+"),
];

fn skip_dir(name: &str) -> bool {
    matches!(
        name,
        "target" | ".git" | "node_modules" | "autoresearch-macos" | "jankurai" | "jeryu" | "vendor"
    )
}

/// Run the secrets-scan audit on `project`.
///
/// # Errors
///
/// Returns an error if the regex set fails to compile or the receipt write
/// fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    let patterns: Vec<&str> = PATTERNS.iter().map(|(_, p)| *p).collect();
    let set = RegexSet::new(&patterns).context("compile secrets-scan regex set")?;

    let mut files_scanned = 0usize;
    let mut findings: Vec<Finding> = Vec::new();

    for entry in WalkDir::new(project).into_iter().filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        !skip_dir(&name)
    }) {
        let Ok(entry) = entry else {
            continue;
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path: PathBuf = entry.path().to_owned();
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if text.len() > 5_000_000 {
            continue;
        }
        files_scanned += 1;
        for (line_idx, line) in text.lines().enumerate() {
            for m in set.matches(line) {
                if let Some((name, _)) = PATTERNS.get(m) {
                    findings.push(Finding {
                        path: path
                            .strip_prefix(project)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .into_owned(),
                        line: line_idx + 1,
                        pattern: name,
                    });
                }
            }
        }
    }

    let verdict = if findings.is_empty() { "pass" } else { "block" };
    let summary = format!(
        "secrets-scan: scanned {files_scanned} files, {} findings",
        findings.len()
    );
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            files_scanned,
            findings,
        },
    )?;
    Ok(summary)
}
