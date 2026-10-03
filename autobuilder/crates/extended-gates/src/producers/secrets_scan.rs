//! `secrets-scan`: scan tracked source files for high-confidence secret patterns.
//!
//! Pure-Rust. Walks the project tree (skipping `target/`, `.git/`, vendored
//! reference dirs, and common Python/JS build-tooling dirs — see
//! [`skip_dir`]), reads each file's text, and matches a regex set tuned for
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
//!
//! An optional `--files-from <manifest>` CLI flag (plumbed through
//! [`run_with_files_from`]) restricts the scan to exactly the files listed in
//! `manifest` (one path per line, relative to `project`) instead of walking
//! the whole tree. Each listed path still has [`skip_dir`] applied to its
//! directory components, and a listed path that doesn't exist (or isn't a
//! file) is skipped and counted rather than erroring the whole scan.

use std::path::{Component, Path, PathBuf};

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

/// Directory names pruned from the walk — build output, VCS metadata,
/// vendored reference trees, and Python/JS tooling caches that otherwise
/// drag third-party secret-shaped material (e.g. a `cryptography` package's
/// own test PEMs under a `.venv`) into the scan. Only applied to entries
/// that are actually directories (see the `filter_entry` closure in
/// [`walk_all_files`]) — a file that happens to share one of these names
/// (e.g. a `build` shell script) is still scanned. `--files-from` applies
/// the same names to each listed path's directory components via
/// [`path_has_skipped_component`].
fn skip_dir(name: &str) -> bool {
    matches!(
        name,
        "target"
            | ".git"
            | "node_modules"
            | "autoresearch-macos"
            | "jankurai"
            | "jeryu"
            | "vendor"
            | ".venv"
            | "venv"
            | "__pycache__"
            | ".tox"
            | ".mypy_cache"
            | ".pytest_cache"
            | ".ruff_cache"
            | "dist"
            | "build"
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

/// `true` if any normal (non-root, non-`..`) component of `rel` is one of
/// [`skip_dir`]'s pruned directory names. Used by `--files-from` to apply
/// the same directory-pruning a tree walk would have applied, to a path
/// that was handed to us explicitly rather than discovered by walking.
fn path_has_skipped_component(rel: &Path) -> bool {
    rel.components().any(|c| match c {
        Component::Normal(os) => skip_dir(&os.to_string_lossy()),
        _ => false,
    })
}

/// Collect every file under `project`, pruning [`skip_dir`] directories
/// during the walk (so their contents are never descended into, not merely
/// filtered out after the fact).
fn walk_all_files(project: &Path) -> Vec<PathBuf> {
    WalkDir::new(project)
        .into_iter()
        .filter_entry(|e| {
            // Only directory entries are pruned by name; a file that
            // happens to share a name with a pruned directory is still
            // scanned.
            if !e.file_type().is_dir() {
                return true;
            }
            !skip_dir(&e.file_name().to_string_lossy())
        })
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_owned())
        .collect()
}

/// Resolve `--files-from <manifest>` into the list of files to scan.
///
/// `manifest` is read as one relative path per line (blank lines and `#`
/// comments ignored); each is joined onto `project`. A listed path that
/// doesn't exist, isn't a plain file, or has a [`skip_dir`]-pruned
/// directory component is skipped (not scanned, not an error) and counted
/// in the returned `skipped` total.
fn resolve_files_from(project: &Path, manifest: &Path) -> Result<(Vec<PathBuf>, usize)> {
    let text = std::fs::read_to_string(manifest)
        .with_context(|| format!("read --files-from manifest {}", manifest.display()))?;
    let mut files = Vec::new();
    let mut skipped = 0usize;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let rel = Path::new(line);
        if path_has_skipped_component(rel) {
            skipped += 1;
            continue;
        }
        let abs = project.join(rel);
        if !abs.is_file() {
            skipped += 1;
            continue;
        }
        files.push(abs);
    }
    Ok((files, skipped))
}

/// Run the secrets-scan audit on `project`, scanning its whole tree.
///
/// # Errors
///
/// Returns an error if the regex set fails to compile or the receipt write
/// fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    run_with_files_from(spec, project, None)
}

/// Run the secrets-scan audit on `project`.
///
/// When `files_from` is `Some(manifest)`, only the files listed in
/// `manifest` (one path per line, relative to `project`, [`skip_dir`]
/// pruning still applied to each) are scanned — this is what wm-build's
/// gate passes (`producers.toml`'s secrets-scan entry) to scope the scan to
/// tracked files rather than the whole worktree (e.g. a `.venv` created by
/// `uv sync` during the build). When `files_from` is `None`, the whole
/// project tree is walked.
///
/// # Errors
///
/// Returns an error if the regex set fails to compile, the manifest can't
/// be read, or the receipt write fails.
pub fn run_with_files_from(
    spec: &ProducerSpec,
    project: &Path,
    files_from: Option<&Path>,
) -> Result<String> {
    let patterns: Vec<&str> = PATTERNS.iter().map(|(_, p)| *p).collect();
    let set = RegexSet::new(&patterns).context("compile secrets-scan regex set")?;

    let allowlist = load_allowlist(project);
    let allow_res: Vec<Regex> = allowlist.iter().filter_map(|p| glob_to_regex(p)).collect();

    let (candidates, files_skipped) = match files_from {
        Some(manifest) => resolve_files_from(project, manifest)?,
        None => (walk_all_files(project), 0),
    };

    let mut files_scanned = 0usize;
    let mut findings: Vec<Finding> = Vec::new();

    for path in candidates {
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
    let mut summary = format!(
        "secrets-scan: scanned {files_scanned} files, {} findings",
        findings.len()
    );
    if files_skipped > 0 {
        summary.push_str(&format!(
            ", {files_skipped} listed files skipped (missing or pruned)"
        ));
    }
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    const PEM_HEADER: &str = "-----BEGIN RSA PRIVATE KEY-----\n";

    fn spec() -> &'static ProducerSpec {
        ProducerSpec::lookup("secrets-scan").unwrap()
    }

    fn findings_count(project: &Path) -> usize {
        let receipt_path = project
            .join("target/autobuilder/receipts")
            .join(spec().file_name);
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&receipt_path).unwrap()).unwrap();
        value
            .get("findings")
            .and_then(serde_json::Value::as_array)
            .expect("findings array")
            .len()
    }

    /// A PEM private-key header sitting inside a `.venv` directory must not
    /// be flagged — `.venv` is a pruned directory.
    #[test]
    fn venv_pem_header_is_pruned() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        std::fs::create_dir_all(project.join(".venv/x")).unwrap();
        std::fs::write(project.join(".venv/x/key.pem"), PEM_HEADER).unwrap();
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(project.join("src/ok.rs"), "fn main() {}\n").unwrap();

        run(spec(), project).unwrap();
        assert_eq!(findings_count(project), 0, "`.venv` contents must be pruned");
    }

    /// The same PEM header, outside `.venv`, must still block.
    #[test]
    fn non_venv_pem_header_still_blocks() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(project.join("src/ok.rs"), "fn main() {}\n").unwrap();
        std::fs::write(project.join("src/leak.pem"), PEM_HEADER).unwrap();

        run(spec(), project).unwrap();
        assert_eq!(
            findings_count(project),
            1,
            "a PEM header outside a pruned dir must still be found"
        );
    }

    /// `--files-from` restricts the scan to exactly the listed files: a
    /// manifest naming only `src/ok.rs` must not pick up `src/leak.pem`,
    /// even though it exists on disk.
    #[test]
    fn files_from_restricts_scan_to_listed_files() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(project.join("src/ok.rs"), "fn main() {}\n").unwrap();
        std::fs::write(project.join("src/leak.pem"), PEM_HEADER).unwrap();

        let manifest = tmp.path().join("manifest.txt");
        std::fs::write(&manifest, "src/ok.rs\n").unwrap();

        run_with_files_from(spec(), project, Some(&manifest)).unwrap();
        assert_eq!(
            findings_count(project),
            0,
            "manifest listing only src/ok.rs must not scan src/leak.pem"
        );
    }

    /// The same manifest mechanism, but listing the leaking file, must
    /// surface the finding.
    #[test]
    fn files_from_scans_listed_leak() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(project.join("src/ok.rs"), "fn main() {}\n").unwrap();
        std::fs::write(project.join("src/leak.pem"), PEM_HEADER).unwrap();

        let manifest = tmp.path().join("manifest.txt");
        std::fs::write(&manifest, "src/leak.pem\n").unwrap();

        run_with_files_from(spec(), project, Some(&manifest)).unwrap();
        assert_eq!(
            findings_count(project),
            1,
            "manifest listing src/leak.pem must surface the finding"
        );
    }

    /// A manifest entry that doesn't exist on disk is skipped, not an
    /// error, and doesn't block the scan.
    #[test]
    fn files_from_skips_missing_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(project.join("src/ok.rs"), "fn main() {}\n").unwrap();

        let manifest = tmp.path().join("manifest.txt");
        std::fs::write(&manifest, "src/ok.rs\nsrc/does_not_exist.rs\n").unwrap();

        let summary = run_with_files_from(spec(), project, Some(&manifest)).unwrap();
        assert_eq!(findings_count(project), 0);
        assert!(
            summary.contains("skipped"),
            "summary should note the skipped missing entry: {summary}"
        );
    }
}
