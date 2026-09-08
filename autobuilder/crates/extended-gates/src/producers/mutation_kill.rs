//! `mutation-kill`: a small mutation-operator pass on `src/lib.rs` causes the
//! test suite to fail.
//!
//! The audit's invariant: a project whose tests don't observe trivial
//! mutations (arithmetic-flip, comparison-flip) has trivial tests. The
//! producer copies the source tree to a temp directory, applies one of three
//! mutation operators to each `.rs` file in `src/`, runs `cargo test` on the
//! mutant, and counts how many mutations caused a test failure ("kills").
//! The kill rate must clear `extended-gates.toml::mutation_kill_min_pct`
//! (default 50%).
//!
//! Candidate mutation sites are found by a small comment/string-aware
//! scanner: occurrences inside `//`, `///`, `//!` line comments, `/* */`
//! block comments (nested), or `"..."` string literals are never mutated —
//! a mutation there is a no-op that would be misreported as a "surviving"
//! mutant. Up to 5 candidate code sites are tried per operator, not just the
//! first.
//!
//! Heavy operation; skippable via `AUTOBUILDER_SKIP_HEAVY=1`. The fixture
//! `tests/fixtures/trivial-tests/` has tests that don't observe arithmetic,
//! so its mutation-kill rate is 0% → verdict=block.

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use serde::Serialize;
use toml::Value as TomlValue;

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    mutations_attempted: usize,
    mutations_killed: usize,
    kill_pct: f64,
    min_pct: f64,
    survivors: Vec<String>,
}

fn min_pct(project: &Path) -> f64 {
    let cfg = project.join("extended-gates.toml");
    if let Ok(text) = std::fs::read_to_string(&cfg) {
        if let Ok(value) = text.parse::<TomlValue>() {
            if let Some(n) = value
                .get("mutation_kill_min_pct")
                .and_then(TomlValue::as_float)
            {
                return n;
            }
        }
    }
    50.0
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in walkdir::WalkDir::new(src) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src).unwrap_or(entry.path());
        if rel
            .components()
            .any(|c| c.as_os_str() == "target" || c.as_os_str() == ".git")
        {
            continue;
        }
        let to = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&to)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
enum Operator {
    AddToSub,
    EqToNeq,
}

fn mutations() -> Vec<(Operator, &'static str, &'static str)> {
    vec![
        (Operator::AddToSub, " + ", " - "),
        (Operator::EqToNeq, " == ", " != "),
    ]
}

/// Find up to `max_sites` byte offsets where `pattern` occurs in "real code"
/// — i.e. not inside a `//`/`///`/`//!` line comment, a (possibly nested)
/// `/* */` block comment, a `"..."` string literal, or a `'x'` char literal.
/// `'` that isn't a char literal (a lifetime like `'a`) is treated as a
/// single ordinary character so it doesn't wrongly swallow the rest of the
/// file into "string" state.
fn candidate_sites(text: &str, pattern: &str, max_sites: usize) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut sites = Vec::new();
    let mut i = 0usize;
    let mut in_line_comment = false;
    let mut block_comment_depth = 0u32;
    let mut in_string = false;
    let mut in_char = false;
    let mut escape = false;

    while let Some(&b) = bytes.get(i) {
        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }
        if block_comment_depth > 0 {
            if text.get(i..).is_some_and(|s| s.starts_with("/*")) {
                block_comment_depth += 1;
                i += 2;
                continue;
            }
            if text.get(i..).is_some_and(|s| s.starts_with("*/")) {
                block_comment_depth -= 1;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if in_char {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'\'' {
                in_char = false;
            }
            i += 1;
            continue;
        }

        // "Real code" region.
        if text.get(i..).is_some_and(|s| s.starts_with("//")) {
            in_line_comment = true;
            i += 2;
            continue;
        }
        if text.get(i..).is_some_and(|s| s.starts_with("/*")) {
            block_comment_depth = 1;
            i += 2;
            continue;
        }
        if b == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        if b == b'\'' {
            // Distinguish a char literal ('a', '\n', '\'') from a lifetime
            // ('a, 'static): a char literal is either an escape sequence or
            // exactly one char before the closing quote.
            let next = bytes.get(i + 1).copied();
            let next2 = bytes.get(i + 2).copied();
            let is_char_literal = next == Some(b'\\') || next2 == Some(b'\'');
            if is_char_literal {
                in_char = true;
            }
            i += 1;
            continue;
        }
        if text.get(i..).is_some_and(|s| s.starts_with(pattern)) {
            sites.push(i);
            if sites.len() >= max_sites {
                return sites;
            }
            i += pattern.len();
            continue;
        }
        i += 1;
    }
    sites
}

fn apply_mutation_at(text: &str, idx: usize, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    if let Some(head) = text.get(..idx) {
        out.push_str(head);
    }
    out.push_str(to);
    if let Some(tail) = text.get(idx + from.len()..) {
        out.push_str(tail);
    }
    out
}

fn cargo_test_passes(dir: &Path) -> bool {
    let output = Command::new("cargo")
        .args(["test", "--quiet"])
        .current_dir(dir)
        .output();
    match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

/// Run the mutation-kill audit.
///
/// # Errors
///
/// Returns an error if temp dirs can't be created or the receipt write fails.
#[allow(clippy::too_many_lines)]
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    if std::env::var("AUTOBUILDER_SKIP_HEAVY").is_ok() {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                mutations_attempted: 0,
                mutations_killed: 0,
                kill_pct: 0.0,
                min_pct: min_pct(project),
                survivors: vec!["AUTOBUILDER_SKIP_HEAVY set".into()],
            },
        )?;
        return Ok("mutation-kill: skipped (AUTOBUILDER_SKIP_HEAVY)".into());
    }
    let lib = project.join("src/lib.rs");
    if !lib.is_file() {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                mutations_attempted: 0,
                mutations_killed: 0,
                kill_pct: 0.0,
                min_pct: min_pct(project),
                survivors: vec!["no src/lib.rs".into()],
            },
        )?;
        return Ok("mutation-kill: skipped (no lib)".into());
    }

    let original = std::fs::read_to_string(&lib).unwrap_or_default();
    let ops = mutations();
    let mut attempted = 0usize;
    let mut killed = 0usize;
    let mut survivors: Vec<String> = Vec::new();

    for (op, from, to) in &ops {
        let sites = candidate_sites(&original, from, 5);
        for idx in sites {
            let mutated = apply_mutation_at(&original, idx, from, to);
            attempted += 1;
            let tmp = tempfile::tempdir()?;
            copy_tree(project, tmp.path())?;
            std::fs::write(tmp.path().join("src/lib.rs"), &mutated)?;
            let passed = cargo_test_passes(tmp.path());
            if passed {
                survivors.push(format!("{op:?} @byte{idx}"));
            } else {
                killed += 1;
            }
        }
    }

    let kill_pct = if attempted == 0 {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        {
            (killed as f64 / attempted as f64) * 100.0
        }
    };
    let min = min_pct(project);
    let verdict = if attempted > 0 && kill_pct >= min {
        "pass"
    } else if attempted == 0 {
        "skipped"
    } else {
        "block"
    };
    let summary = format!(
        "mutation-kill: {killed}/{attempted} = {kill_pct:.1}% (min {min:.1}%)"
    );
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            mutations_attempted: attempted,
            mutations_killed: killed,
            kill_pct,
            min_pct: min,
            survivors,
        },
    )?;
    Ok(summary)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn candidate_sites_skips_doc_comment_and_finds_real_code() {
        let text = "\
//! adds a + b together (this doc-comment `+` must not be a candidate)
fn add(a: i32, b: i32) -> i32 { a + b }
";
        let sites = candidate_sites(text, " + ", 5);
        assert_eq!(sites.len(), 1, "expected exactly one real-code candidate site");
        let idx = sites[0];
        // The chosen site must be on the `fn add` line, not the doc comment.
        let before = &text[..idx];
        assert!(
            before.contains("fn add"),
            "expected the candidate site to be after `fn add`, got context: {before:?}"
        );
    }

    #[test]
    fn candidate_sites_skips_line_comment() {
        let text = "let x = 1; // a + b in a comment\nlet y = 2 + 3;\n";
        let sites = candidate_sites(text, " + ", 5);
        assert_eq!(sites.len(), 1);
        assert!(text[sites[0]..].starts_with(" + 3"));
    }

    #[test]
    fn candidate_sites_skips_block_comment() {
        let text = "/* a + b block comment */\nlet y = 2 + 3;\n";
        let sites = candidate_sites(text, " + ", 5);
        assert_eq!(sites.len(), 1);
        assert!(text[sites[0]..].starts_with(" + 3"));
    }

    #[test]
    fn candidate_sites_skips_string_literal() {
        let text = "let s = \"a + b\";\nlet y = 2 + 3;\n";
        let sites = candidate_sites(text, " + ", 5);
        assert_eq!(sites.len(), 1);
        assert!(text[sites[0]..].starts_with(" + 3"));
    }

    #[test]
    fn candidate_sites_caps_at_max_sites() {
        let text = "1 + 2 + 3 + 4 + 5 + 6 + 7\n";
        let sites = candidate_sites(text, " + ", 5);
        assert_eq!(sites.len(), 5);
    }

    #[test]
    fn candidate_sites_does_not_choke_on_lifetimes() {
        let text = "fn f<'a>(x: &'a str) -> &'a str { x }\nlet y = 2 + 3;\n";
        let sites = candidate_sites(text, " + ", 5);
        assert_eq!(sites.len(), 1);
    }

    #[test]
    fn apply_mutation_at_replaces_only_the_chosen_site() {
        let text = "let y = 2 + 3;\n";
        let idx = text.find(" + ").unwrap();
        let mutated = apply_mutation_at(text, idx, " + ", " - ");
        assert_eq!(mutated, "let y = 2 - 3;\n");
    }
}
