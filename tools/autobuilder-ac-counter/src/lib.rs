use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Per-layout breakdown of discovered acceptance criteria.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layouts {
    /// Number of `tests/acceptance_*.rs` files (one AC each).
    pub split_file: usize,
    /// Number of `fn (ac|new_ac|ext)[0-9]+_…` functions inside `tests/acceptance.rs`.
    pub monolithic_fns: usize,
    /// Number of `tests/mocks/ac<N>.rs` files (one AC each).
    pub mock_files: usize,
}

/// Complete acceptance-criteria inventory for a crate.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcInventory {
    /// Total AC count (split_file + monolithic_fns + mock_files).
    pub total: usize,
    /// Per-layout breakdown.
    pub by_layout: Layouts,
    /// Every matched AC identifier, sorted ascending.
    pub names: Vec<String>,
}

// ─── internal helpers ──────────────────────────────────────────────────────

/// Returns true if `name` matches the pattern `acceptance_<rest>` where rest
/// is non-empty and contains only `[a-z0-9_]` chars.
fn is_split_file(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("acceptance_") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Returns true if `name` matches `ac<N>.rs` in the mocks dir.
fn is_mock_file(stem: &str) -> bool {
    let Some(rest) = stem.strip_prefix("ac") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

/// Extract all AC function names from the content of a monolithic `acceptance.rs`.
///
/// Matches lines whose trimmed form is:
///   `fn (ac|new_ac|ext)[0-9]+_[a-z0-9_]+`
/// (visibility keywords, `async`, `#[…]` attributes on prior lines, etc. are ignored;
/// we only need the fn-name token.)
fn extract_monolithic_names(content: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // Strip optional `pub`, `async`, `pub(crate)`, etc. before `fn`
        let after_fn = if let Some(rest) = trimmed.strip_prefix("fn ") {
            rest
        } else if let Some(rest) = strip_visibility_and_fn(trimmed) {
            rest
        } else {
            continue;
        };
        // after_fn is the rest of the line after "fn " — grab the identifier
        let fn_name: &str = after_fn.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').next().unwrap_or("");
        if fn_name_is_ac(fn_name) {
            names.push(fn_name.to_string());
        }
    }
    names
}

/// Strip leading visibility / async / unsafe / etc. tokens before `fn`.
fn strip_visibility_and_fn(s: &str) -> Option<&str> {
    // Try common prefixes: `pub fn`, `pub(crate) fn`, `async fn`, `pub async fn`, etc.
    let stripped = s
        .strip_prefix("pub(crate) async fn ")
        .or_else(|| s.strip_prefix("pub(crate) fn "))
        .or_else(|| s.strip_prefix("pub async fn "))
        .or_else(|| s.strip_prefix("pub fn "))
        .or_else(|| s.strip_prefix("async fn "))
        .or_else(|| s.strip_prefix("unsafe fn "))?;
    Some(stripped)
}

/// Returns true if the identifier matches `(ac|new_ac|ext)[0-9]+_[a-z0-9_]+`
fn fn_name_is_ac(name: &str) -> bool {
    // Try each family prefix
    for prefix in &["new_ac", "ext", "ac"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            // rest must start with at least one digit, then `_`, then at least one char
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                continue;
            }
            let after_digits = &rest[digits.len()..];
            if let Some(suffix) = after_digits.strip_prefix('_') {
                if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                    return true;
                }
            }
        }
    }
    false
}

// ─── public API ───────────────────────────────────────────────────────────

/// Discover all acceptance criteria declared under `<crate_dir>/tests/`.
///
/// Pure filesystem read; no cargo invocation, no test execution.
pub fn discover(crate_dir: &Path) -> io::Result<AcInventory> {
    let tests_dir = crate_dir.join("tests");

    let mut split_file = 0usize;
    let mut monolithic_fns = 0usize;
    let mut mock_files = 0usize;
    let mut names: Vec<String> = Vec::new();

    // If tests/ does not exist, return empty inventory (AC6).
    if !tests_dir.exists() {
        return Ok(AcInventory::default());
    }

    // Read top-level entries in tests/
    let entries = fs::read_dir(&tests_dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let fname = file_name.to_string_lossy();

        if path.is_file() {
            if let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) {
                if fname.ends_with(".rs") {
                    if stem == "acceptance" {
                        // Monolithic layout
                        let content = fs::read_to_string(&path)?;
                        let fn_names = extract_monolithic_names(&content);
                        monolithic_fns += fn_names.len();
                        names.extend(fn_names);
                    } else if is_split_file(&stem) {
                        // Split-file layout: each acceptance_*.rs = 1 AC
                        split_file += 1;
                        names.push(stem.clone());
                    }
                }
            }
        } else if path.is_dir() && fname == "mocks" {
            // Mock layout: tests/mocks/ac<N>.rs
            let mock_entries = fs::read_dir(&path)?;
            for mock_entry in mock_entries {
                let mock_entry = mock_entry?;
                let mock_path = mock_entry.path();
                if mock_path.is_file() {
                    if let Some(stem) = mock_path.file_stem().map(|s| s.to_string_lossy().into_owned()) {
                        if mock_path.extension().map(|e| e == "rs").unwrap_or(false) && is_mock_file(&stem) {
                            mock_files += 1;
                            names.push(format!("mocks/{stem}"));
                        }
                    }
                }
            }
        }
    }

    let total = split_file + monolithic_fns + mock_files;
    names.sort();

    Ok(AcInventory {
        total,
        by_layout: Layouts { split_file, monolithic_fns, mock_files },
        names,
    })
}

/// Parse `cargo test` stdout and count passing AC tests across all name families.
///
/// Counts lines matching:
/// - `test (ac|new_ac|ext)[0-9]+_[a-z0-9_]+ ... ok`
/// - `test acceptance_[a-z0-9_]+ ... ok`
pub fn count_passing(test_stdout: &str) -> usize {
    let mut count = 0usize;
    for line in test_stdout.lines() {
        let trimmed = line.trim();
        // Must start with "test " and end with " ... ok"
        let Some(rest) = trimmed.strip_prefix("test ") else { continue };
        let Some(name_part) = rest.strip_suffix(" ... ok") else { continue };
        // name_part should be the test name with no spaces
        if name_part.contains(' ') { continue; }
        if test_name_is_passing_ac(name_part) {
            count += 1;
        }
    }
    count
}

/// Returns true if the test name matches an AC passing pattern.
fn test_name_is_passing_ac(name: &str) -> bool {
    // Split-file: acceptance_[a-z0-9_]+
    if let Some(rest) = name.strip_prefix("acceptance_") {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            return true;
        }
    }
    // Monolithic families: (ac|new_ac|ext)[0-9]+_[a-z0-9_]+
    fn_name_is_ac(name)
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn fn_name_is_ac_variants() {
        assert!(fn_name_is_ac("ac1_foo"));
        assert!(fn_name_is_ac("new_ac1_bar"));
        assert!(fn_name_is_ac("ext1_baz"));
        assert!(fn_name_is_ac("ac42_long_name"));
        assert!(!fn_name_is_ac("helper"));
        assert!(!fn_name_is_ac("test_something"));
        assert!(!fn_name_is_ac("ac_no_number"));
        assert!(!fn_name_is_ac("ac1"));  // no underscore suffix
    }

    #[test]
    fn extract_monolithic_basic() {
        let content = r#"
#[test]
fn ac1_first() {}

#[test]
fn new_ac1_second() {}

#[test]
fn ext1_third() {}

fn helper_not_ac() {}
"#;
        let names = extract_monolithic_names(content);
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"ac1_first".to_string()));
        assert!(names.contains(&"new_ac1_second".to_string()));
        assert!(names.contains(&"ext1_third".to_string()));
    }

    #[test]
    fn test_name_passing_acceptance_split() {
        assert!(test_name_is_passing_ac("acceptance_my_test"));
        assert!(!test_name_is_passing_ac("acceptance_"));
    }
}
