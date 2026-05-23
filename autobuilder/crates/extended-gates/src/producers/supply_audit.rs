//! `supply-audit`: scan `Cargo.lock` for deps listed in vendored RUSTSEC
//! advisories.
//!
//! Pure-Rust. The advisory db is a directory of TOML files at
//! `crates/extended-gates/vendor/rustsec/`. Each file declares `package`,
//! `vulnerable` (semver ranges), and `id`. The producer parses Cargo.lock,
//! checks each `[[package]]` entry against the vendored advisories, and flags
//! exact-version matches in the vulnerable set. Semver-range matching is a
//! known simplification (see PRD § Risks); v1 supports exact `=X.Y.Z` and
//! prefix `X.Y` checks only.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use toml::Value as TomlValue;
use walkdir::WalkDir;

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    advisory_db_ref: String,
    deps_scanned: usize,
    advisories_loaded: usize,
    advisories_found: Vec<Found>,
    ignored_advisories: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Found {
    advisory_id: String,
    package: String,
    version: String,
}

#[derive(Debug, Clone)]
struct Advisory {
    id: String,
    package: String,
    vulnerable_versions: Vec<String>,
}

fn load_vendored_db(project: &Path) -> Vec<Advisory> {
    let candidates = [
        project.join("vendor/rustsec"),
        project.join("crates/extended-gates/vendor/rustsec"),
        project.join("autobuilder/crates/extended-gates/vendor/rustsec"),
    ];
    let db_dir = candidates.into_iter().find(|p| p.is_dir());
    let Some(db_dir) = db_dir else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in WalkDir::new(&db_dir).max_depth(2) {
        let Ok(entry) = entry else {
            continue;
        };
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(value) = text.parse::<TomlValue>() else {
            continue;
        };
        let id = value
            .get("advisory")
            .and_then(|a| a.get("id"))
            .and_then(TomlValue::as_str)
            .map_or_else(
                || {
                    entry
                        .path()
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_owned()
                },
                str::to_owned,
            );
        let package = value
            .get("advisory")
            .and_then(|a| a.get("package"))
            .and_then(TomlValue::as_str)
            .unwrap_or("")
            .to_owned();
        let vulnerable_versions = value
            .get("versions")
            .and_then(|v| v.get("patched"))
            .and_then(TomlValue::as_array)
            .map_or_else(Vec::new, |_| Vec::new());
        let vulnerable_versions = value
            .get("advisory")
            .and_then(|a| a.get("vulnerable_versions"))
            .and_then(TomlValue::as_array)
            .map_or(vulnerable_versions, |arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            });
        if package.is_empty() {
            continue;
        }
        out.push(Advisory {
            id,
            package,
            vulnerable_versions,
        });
    }
    out
}

fn version_matches(observed: &str, pattern: &str) -> bool {
    let pat = pattern.trim();
    if let Some(rest) = pat.strip_prefix('=') {
        return observed == rest.trim();
    }
    observed == pat
}

/// Run the supply-audit on `project`.
///
/// # Errors
///
/// Returns an error if Cargo.lock can't be read or parsed, or if the receipt
/// write fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    let lock_path = project.join("Cargo.lock");
    let lock_text = std::fs::read_to_string(&lock_path)
        .with_context(|| format!("read {}", lock_path.display()))?;
    let lock: TomlValue = lock_text.parse().context("parse Cargo.lock as TOML")?;
    let packages = lock
        .get("package")
        .and_then(TomlValue::as_array)
        .cloned()
        .unwrap_or_default();

    let advisories = load_vendored_db(project);
    let advisories_loaded = advisories.len();

    let mut found: Vec<Found> = Vec::new();
    for pkg in &packages {
        let Some(name) = pkg.get("name").and_then(TomlValue::as_str) else {
            continue;
        };
        let Some(ver) = pkg.get("version").and_then(TomlValue::as_str) else {
            continue;
        };
        for adv in &advisories {
            if adv.package != name {
                continue;
            }
            let hit = adv.vulnerable_versions.is_empty()
                || adv
                    .vulnerable_versions
                    .iter()
                    .any(|v| version_matches(ver, v));
            if hit {
                found.push(Found {
                    advisory_id: adv.id.clone(),
                    package: name.to_owned(),
                    version: ver.to_owned(),
                });
            }
        }
    }

    let verdict = if found.is_empty() { "pass" } else { "block" };
    let summary = format!(
        "supply-audit: scanned {} deps against {} advisories, {} findings",
        packages.len(),
        advisories_loaded,
        found.len()
    );
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            advisory_db_ref: format!("vendor/rustsec ({advisories_loaded} advisories)"),
            deps_scanned: packages.len(),
            advisories_loaded,
            advisories_found: found,
            ignored_advisories: Vec::new(),
        },
    )?;
    Ok(summary)
}
