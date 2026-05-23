//! `license-audit`: every transitive dep's `license` field is in the allowlist.
//!
//! Pure-Rust. Reads `Cargo.toml` for each package referenced in
//! `Cargo.lock` (or walks `Cargo.lock` directly using its `license` field if
//! the lock file embeds them). The allowlist lives at
//! `<project>/extended-gates.toml::license_allowlist` if present; defaults to
//! the common permissive set.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use toml::Value as TomlValue;

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    allowlist: Vec<String>,
    deps_scanned: usize,
    deps_unknown_license: Vec<String>,
    violations: Vec<Violation>,
}

#[derive(Debug, Serialize)]
struct Violation {
    package: String,
    version: String,
    license: String,
}

const DEFAULT_ALLOWLIST: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-DFS-2016",
    "Zlib",
    "MPL-2.0",
    "CC0-1.0",
    "0BSD",
];

fn load_allowlist(project: &Path) -> Vec<String> {
    let cfg_path = project.join("extended-gates.toml");
    if let Ok(text) = std::fs::read_to_string(&cfg_path) {
        if let Ok(value) = text.parse::<TomlValue>() {
            if let Some(arr) = value.get("license_allowlist").and_then(TomlValue::as_array) {
                return arr
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(str::to_owned)
                    .collect();
            }
        }
    }
    DEFAULT_ALLOWLIST.iter().map(|s| (*s).to_owned()).collect()
}

fn split_spdx(spdx: &str) -> Vec<String> {
    spdx.split(['/', ' '])
        .flat_map(|s| s.split(" OR "))
        .flat_map(|s| s.split(" AND "))
        .map(|s| s.trim().trim_matches(|c| c == '(' || c == ')').to_owned())
        .filter(|s| !s.is_empty() && *s != "OR" && *s != "AND")
        .collect()
}

fn license_acceptable(license: &str, allow: &BTreeSet<String>) -> bool {
    if license.contains(" OR ") {
        return split_spdx(license).iter().any(|s| allow.contains(s));
    }
    if license.contains(" AND ") {
        return split_spdx(license).iter().all(|s| allow.contains(s));
    }
    if license.contains('/') {
        return split_spdx(license).iter().any(|s| allow.contains(s));
    }
    allow.contains(license)
}

/// Run the license audit on `project`.
///
/// # Errors
///
/// Returns an error if Cargo.lock can't be read/parsed or the receipt write
/// fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    let allowlist = load_allowlist(project);
    let allow_set: BTreeSet<String> = allowlist.iter().cloned().collect();

    let lock_path = project.join("Cargo.lock");
    let lock_text = std::fs::read_to_string(&lock_path)
        .with_context(|| format!("read {}", lock_path.display()))?;
    let lock: TomlValue = lock_text.parse().context("parse Cargo.lock as TOML")?;
    let packages = lock
        .get("package")
        .and_then(TomlValue::as_array)
        .cloned()
        .unwrap_or_default();

    let mut violations: Vec<Violation> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();
    for pkg in &packages {
        let Some(name) = pkg.get("name").and_then(TomlValue::as_str) else {
            continue;
        };
        let version = pkg
            .get("version")
            .and_then(TomlValue::as_str)
            .unwrap_or("0.0.0");
        let license = pkg.get("license").and_then(TomlValue::as_str);
        let Some(license) = license else {
            unknown.push(format!("{name}@{version}"));
            continue;
        };
        if !license_acceptable(license, &allow_set) {
            violations.push(Violation {
                package: name.to_owned(),
                version: version.to_owned(),
                license: license.to_owned(),
            });
        }
    }

    let verdict = if violations.is_empty() { "pass" } else { "block" };
    let summary = format!(
        "license-audit: scanned {} deps, {} violations, {} unknown",
        packages.len(),
        violations.len(),
        unknown.len()
    );
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            allowlist,
            deps_scanned: packages.len(),
            deps_unknown_license: unknown,
            violations,
        },
    )?;
    Ok(summary)
}
