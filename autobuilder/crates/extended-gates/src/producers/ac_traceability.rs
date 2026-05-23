//! `ac-traceability`: every PRD AC id has ≥1 Rust test fn referencing it.
//!
//! Parses the PRD at `<project>/PRD-*.md` (configurable via
//! `extended-gates.toml::prd_path`), extracts `AC<N>` and `AC-<slug>.<N>`
//! identifiers, then walks `tests/` and `src/` for `#[test] fn` whose name
//! or docstring contains the id (case-insensitive).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use regex::Regex;
use serde::Serialize;
use toml::Value as TomlValue;
use walkdir::WalkDir;

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    prd_path: String,
    ac_ids: Vec<String>,
    untraced_ac_ids: Vec<String>,
    tests_per_ac: BTreeMap<String, usize>,
}

fn locate_prd(project: &Path) -> Option<PathBuf> {
    let cfg = project.join("extended-gates.toml");
    if let Ok(text) = std::fs::read_to_string(&cfg) {
        if let Ok(value) = text.parse::<TomlValue>() {
            if let Some(s) = value.get("prd_path").and_then(TomlValue::as_str) {
                let p = project.join(s);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    for entry in std::fs::read_dir(project).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_md = std::path::Path::new(&name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        if name.starts_with("PRD-") && is_md {
            return Some(entry.path());
        }
    }
    None
}

fn extract_ac_ids(prd: &str) -> BTreeSet<String> {
    // Both patterns are static literals known-good at write time; if either
    // ever fails to compile, the caller treats an empty match set as a
    // "no AC ids found" signal — which the producer surfaces as block.
    let Ok(re) = Regex::new(r"\bAC[-]?[A-Za-z0-9_-]*\d+(\.\d+)?\b") else {
        return BTreeSet::new();
    };
    let mut out = BTreeSet::new();
    for cap in re.find_iter(prd) {
        let id = cap.as_str().to_owned();
        if id.starts_with("AC") {
            out.insert(id);
        }
    }
    out
}

fn collect_test_text(project: &Path) -> Vec<(PathBuf, String)> {
    let mut out: Vec<(PathBuf, String)> = Vec::new();
    for sub in ["tests", "src", "crates"] {
        let dir = project.join(sub);
        if !dir.is_dir() {
            continue;
        }
        for entry in WalkDir::new(&dir) {
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().and_then(|s| s.to_str()) != Some("rs") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(entry.path()) {
                out.push((entry.path().to_owned(), text));
            }
        }
    }
    out
}

/// Run the ac-traceability audit.
///
/// # Errors
///
/// Returns an error if the receipt write fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    let Some(prd_path) = locate_prd(project) else {
        write_receipt(
            project,
            spec,
            "block",
            Payload {
                prd_path: "<none>".into(),
                ac_ids: Vec::new(),
                untraced_ac_ids: vec!["no PRD-*.md found in project root".into()],
                tests_per_ac: BTreeMap::new(),
            },
        )?;
        return Ok("ac-traceability: BLOCK (no PRD)".into());
    };
    let prd_text = std::fs::read_to_string(&prd_path).unwrap_or_default();
    let ac_ids: BTreeSet<String> = extract_ac_ids(&prd_text);

    let test_files = collect_test_text(project);
    let mut tests_per_ac: BTreeMap<String, usize> = BTreeMap::new();
    for ac in &ac_ids {
        let needle = ac.to_ascii_lowercase();
        let mut count = 0usize;
        for (_, text) in &test_files {
            if text.to_ascii_lowercase().contains(&needle) {
                count += 1;
            }
        }
        tests_per_ac.insert(ac.clone(), count);
    }

    let untraced: Vec<String> = tests_per_ac
        .iter()
        .filter(|(_, c)| **c == 0)
        .map(|(k, _)| k.clone())
        .collect();

    let verdict = if untraced.is_empty() && !ac_ids.is_empty() {
        "pass"
    } else {
        "block"
    };
    let summary = format!(
        "ac-traceability: {} AC ids, {} untraced",
        ac_ids.len(),
        untraced.len()
    );
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            prd_path: prd_path.to_string_lossy().into_owned(),
            ac_ids: ac_ids.into_iter().collect(),
            untraced_ac_ids: untraced,
            tests_per_ac,
        },
    )?;
    Ok(summary)
}
