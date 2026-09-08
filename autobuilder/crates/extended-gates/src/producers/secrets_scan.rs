//! `secrets-scan`: scan tracked source files for high-confidence secret patterns.
//!
//! Pure-Rust. Walks the project tree (skipping `target/`, `.git/`, vendored
//! reference dirs), reads each file's text, and matches a regex set tuned for
//! low false-positive: AWS access keys, GitHub PATs, private-key PEM headers,
//! Slack webhook URLs. The planted-failure fixture
//! (`tests/fixtures/leaked-key/`) embeds a synthetic AKIA pattern; the
//! producer must surface it with `verdict=block`.
//!
//! An optional `<project>/extended-gates.toml::secrets_scan_allowlist`
//! (array of path-glob strings, relative to `<project>`) names files to skip
//! before pattern-matching — e.g. a producer's own planted-secret test
//! fixture, which would otherwise re-trigger this scanner when it walks the
//! whole crate tree rather than just that test's own isolated tempdir. This
//! follows `license_audit.rs`'s `extended-gates.toml` read pattern exactly.
//! Missing file or missing key behaves exactly as before this was added
//! (empty allowlist, no files skipped).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use regex::{Regex, RegexSet};
use serde::Serialize;
use toml::Value as TomlValue;
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

/// Read `<project>/extended-gates.toml::secrets_scan_allowlist` (array of
/// path-glob strings). Missing file or missing key returns an empty list —
/// same read pattern as `license_audit.rs::load_allowlist`.
fn load_allowlist(project: &Path) -> Vec<String> {
    let cfg_path = project.join("extended-gates.toml");
    if let Ok(text) = std::fs::read_to_string(&cfg_path) {
        if let Ok(value) = text.parse::<TomlValue>() {
            if let Some(arr) = value
                .get("secrets_scan_allowlist")
                .and_then(TomlValue::as_array)
            {
                return arr
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(str::to_owned)
                    .collect();
            }
        }
    }
    Vec::new()
}

/// Translate a simple glob (`*` = any run of characters, `?` = any single
/// character, everything else literal) into an anchored regex matching the
/// whole path. Returns `None` if the resulting pattern fails to compile
/// (e.g. pathological input); callers should skip an unusable entry rather
/// than error the whole scan.
fn glob_to_regex(pattern: &str) -> Option<Regex> {
    let mut re = String::from("^");
    for c in pattern.chars() {
        match c {
            '*' => re.push_str(".*"),
            '?' => re.push('.'),
            other => re.push_str(&regex::escape(&other.to_string())),
        }
    }
    re.push('$');
    Regex::new(&re).ok()
}

fn is_allowlisted(rel_path: &str, allow: &[Regex]) -> bool {
    allow.iter().any(|re| re.is_match(rel_path))
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

    let allowlist = load_allowlist(project);
    let allow_res: Vec<Regex> = allowlist.iter().filter_map(|p| glob_to_regex(p)).collect();

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

        let rel = path
            .strip_prefix(project)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        if is_allowlisted(&rel, &allow_res) {
            // Config-driven skip: still counted in files_scanned above, just
            // not pattern-matched — a real secret elsewhere still blocks.
            continue;
        }

        for (line_idx, line) in text.lines().enumerate() {
            for m in set.matches(line) {
                if let Some((name, _)) = PATTERNS.get(m) {
                    findings.push(Finding {
                        path: rel.clone(),
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
