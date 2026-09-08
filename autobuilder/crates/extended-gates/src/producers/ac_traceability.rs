//! `ac-traceability`: every PRD AC id has ≥1 Rust test fn referencing it.
//!
//! Parses the PRD at `<project>/PRD-*.md` (configurable via
//! `extended-gates.toml::prd_path`), extracts `AC<N>` and `AC-<slug>.<N>`
//! identifiers, then walks `tests/` and `src/` for `#[test] fn` whose name
//! or docstring contains the id (case-insensitive).
//!
//! `<project>` is now (PRD-build-extend-gate-nested-crate-project) whatever
//! `extend-gate.sh`/`extended-receipts.sh` resolved as the actual Cargo
//! project root — for a nested-crate repo like rustbuild itself that is
//! `<repo-root>/autobuilder`, not `<repo-root>`, where the PRD file and
//! `extended-gates.toml` actually live. `locate_prd` therefore checks
//! `<project>` first (unchanged behavior for root-level-Cargo.toml repos,
//! where project *is* the repo root) and falls back to `<project>`'s
//! parent directory when nothing is found directly — mirroring
//! `supply-audit`'s existing multi-candidate lookup for the same
//! self-hosting layout.
//!
//! A second AC-id source: the /build contract also numbers ACs as plain
//! numbered lines under a `## Acceptance criteria` heading (also spelled
//! `## Acceptance` or `## Acceptance tests`), e.g. `1. P0 — Given …`. Each
//! such numbered line under that heading yields `AC<N>` (line 1 → `AC1`).
//!
//! Pairing also accepts ac-judge 0.2.1's file-naming convention: a test file
//! named `tests/ac<N>_*.rs` or the zero-padded `tests/ac<0N>_*.rs` (e.g.
//! `tests/ac01_foo.rs` for `AC1`) counts as coverage even if no test fn body
//! mentions the id.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

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

fn locate_prd_in(dir: &Path) -> Option<PathBuf> {
    let cfg = dir.join("extended-gates.toml");
    if let Ok(text) = std::fs::read_to_string(&cfg) {
        if let Ok(value) = text.parse::<TomlValue>() {
            if let Some(s) = value.get("prd_path").and_then(TomlValue::as_str) {
                let p = dir.join(s);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
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

/// Checks `project` itself, then (self-hosting nested-crate layout) its
/// parent directory — the actual repo root when `project` was resolved to
/// a nested Cargo project one level down (e.g. rustbuild's `autobuilder/`).
/// A repo whose Cargo.toml lives at its own root never needs the fallback:
/// `project` there already *is* the repo root, so the first check succeeds.
fn locate_prd(project: &Path) -> Option<PathBuf> {
    locate_prd_in(project).or_else(|| project.parent().and_then(locate_prd_in))
}

/// Matches `AC1`, `AC-foo.1`, etc. anywhere in the PRD body.
static TOKEN_RE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"\bAC[-]?[A-Za-z0-9_-]*\d+(\.\d+)?\b").ok());

/// Matches a `## Acceptance criteria` / `## Acceptance` / `## Acceptance
/// tests` heading (case-insensitive, exact h2 level).
static HEADING_RE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)^##\s+Acceptance(?:\s+(?:criteria|tests))?\s*$").ok());

/// Matches a numbered AC line under that heading: `1. P0 — Given …`.
static NUMBERED_RE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^\s*(\d+)\.\s+P[0-2]\s+—").ok());

fn extract_token_ac_ids(prd: &str) -> BTreeSet<String> {
    // The pattern is a static literal known-good at write time; if it ever
    // fails to compile, the caller treats an empty match set as a "no AC ids
    // found" signal — which the producer surfaces as block.
    let Some(re) = TOKEN_RE.as_ref() else {
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

/// Parse numbered `1. P0 — …` lines under a `## Acceptance...` heading into
/// `AC<N>` ids.
fn extract_numbered_ac_ids(prd: &str) -> BTreeSet<String> {
    let (Some(heading_re), Some(numbered_re)) = (HEADING_RE.as_ref(), NUMBERED_RE.as_ref())
    else {
        return BTreeSet::new();
    };
    let mut out = BTreeSet::new();
    let mut in_section = false;
    for line in prd.lines() {
        if heading_re.is_match(line) {
            in_section = true;
            continue;
        }
        if in_section && line.trim_start().starts_with('#') {
            // Any other heading (any level) ends the acceptance-criteria section.
            in_section = false;
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some(caps) = numbered_re.captures(line) {
            if let Some(n) = caps.get(1) {
                out.insert(format!("AC{}", n.as_str()));
            }
        }
    }
    out
}

fn extract_ac_ids(prd: &str) -> BTreeSet<String> {
    let mut out = extract_token_ac_ids(prd);
    out.extend(extract_numbered_ac_ids(prd));
    out
}

fn collect_test_text(project: &Path) -> Vec<(PathBuf, String)> {
    let mut out: Vec<(PathBuf, String)> = Vec::new();
    let mut roots: Vec<PathBuf> = ["tests", "src", "crates"]
        .into_iter()
        .map(|sub| project.join(sub))
        .collect();
    // Self-hosting layout: this repo has no root `Cargo.toml` (confirmed at
    // every commit in history) — its own crates live nested one level down
    // at `autobuilder/crates/*` (see supply-audit's identical
    // `autobuilder/crates/extended-gates/vendor/rustsec` fallback candidate
    // for the same root cause). An AC's test coverage can land in any
    // sibling crate under there (e.g. a producer-crate AC test paired with
    // a gate-crate receipt-acceptance test), so walk the whole `autobuilder/`
    // tree rather than only `extended-gates`.
    let autobuilder_dir = project.join("autobuilder");
    if autobuilder_dir.is_dir() {
        roots.push(autobuilder_dir);
    }
    for dir in roots {
        if !dir.is_dir() {
            continue;
        }
        let walker = WalkDir::new(&dir).into_iter().filter_entry(|e| {
            !(e.file_type().is_dir() && e.file_name() == std::ffi::OsStr::new("target"))
        });
        for entry in walker {
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

/// If `ac` is a pure numeric id (`AC<N>`), the ac-judge 0.2.1 file-pairing
/// filename prefixes that count as coverage: unpadded (`ac1_`) and
/// zero-padded to 2 digits (`ac01_`).
fn file_pairing_prefixes(ac: &str) -> Option<(String, String)> {
    let n: &str = ac.strip_prefix("AC")?;
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let num: u64 = n.parse().ok()?;
    Some((format!("ac{num}_"), format!("ac{num:02}_")))
}

/// True if any file under a `tests/` directory component has a filename
/// (case-insensitive) starting with one of `ac<N>_*.rs` or `ac<0N>_*.rs`.
fn has_test_file_pairing(ac: &str, test_files: &[(PathBuf, String)]) -> bool {
    let Some((plain_prefix, padded_prefix)) = file_pairing_prefixes(ac) else {
        return false;
    };
    test_files.iter().any(|(path, _)| {
        let under_tests = path
            .components()
            .any(|c| c.as_os_str() == std::ffi::OsStr::new("tests"));
        if !under_tests {
            return false;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return false;
        };
        let name_lower = name.to_ascii_lowercase();
        name_lower.starts_with(&plain_prefix) || name_lower.starts_with(&padded_prefix)
    })
}

/// Count of test-file matches for one AC id: content mentions (case
/// insensitive substring) plus the ac-judge filename-pairing convention.
fn count_test_matches(ac: &str, test_files: &[(PathBuf, String)]) -> usize {
    let needle = ac.to_ascii_lowercase();
    let mut count = test_files
        .iter()
        .filter(|(_, text)| text.to_ascii_lowercase().contains(&needle))
        .count();
    if has_test_file_pairing(ac, test_files) {
        count += 1;
    }
    count
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
        tests_per_ac.insert(ac.clone(), count_test_matches(ac, &test_files));
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn numbered_acs_under_heading_are_parsed() {
        let prd = "\
# PRD

Some intro text.

## Acceptance criteria

1. P0 — Given a valid input, when run, then it succeeds.
2. P1 — Given an invalid input, when run, then it errors.

## Non-goals

3. P0 — This numbered line is outside the section and must not count.
";
        let ids = extract_numbered_ac_ids(prd);
        assert_eq!(
            ids,
            BTreeSet::from(["AC1".to_owned(), "AC2".to_owned()])
        );
    }

    #[test]
    fn numbered_acs_accepts_acceptance_and_acceptance_tests_headings() {
        let prd_plain = "## Acceptance\n\n1. P0 — Given x, when y, then z.\n";
        assert_eq!(
            extract_numbered_ac_ids(prd_plain),
            BTreeSet::from(["AC1".to_owned()])
        );

        let prd_tests = "## Acceptance tests\n\n1. P2 — Given x, when y, then z.\n";
        assert_eq!(
            extract_numbered_ac_ids(prd_tests),
            BTreeSet::from(["AC1".to_owned()])
        );
    }

    #[test]
    fn extract_ac_ids_unions_token_and_numbered_forms() {
        let prd = "\
AC-legacy1 is referenced in prose.

## Acceptance criteria

1. P0 — Given a, when b, then c.
";
        let ids = extract_ac_ids(prd);
        assert!(ids.contains("AC-legacy1"));
        assert!(ids.contains("AC1"));
    }

    #[test]
    fn zero_padded_test_file_pairs_with_numeric_ac_id() {
        let test_files = vec![(
            PathBuf::from("/proj/tests/ac01_foo.rs"),
            "// no textual mention of the id".to_owned(),
        )];
        assert!(has_test_file_pairing("AC1", &test_files));
        assert_eq!(count_test_matches("AC1", &test_files), 1);
    }

    #[test]
    fn unpadded_test_file_pairs_with_numeric_ac_id() {
        let test_files = vec![(
            PathBuf::from("/proj/tests/ac12_bar.rs"),
            "// no textual mention of the id".to_owned(),
        )];
        assert!(has_test_file_pairing("AC12", &test_files));
    }

    #[test]
    fn test_file_outside_tests_dir_does_not_pair() {
        let test_files = vec![(
            PathBuf::from("/proj/src/ac01_foo.rs"),
            "// no textual mention".to_owned(),
        )];
        assert!(!has_test_file_pairing("AC1", &test_files));
        assert_eq!(count_test_matches("AC1", &test_files), 0);
    }

    #[test]
    fn non_numeric_ac_id_has_no_file_pairing() {
        let test_files = vec![(
            PathBuf::from("/proj/tests/ac01_foo.rs"),
            "unrelated".to_owned(),
        )];
        assert!(!has_test_file_pairing("AC-legacy.1", &test_files));
    }
}
