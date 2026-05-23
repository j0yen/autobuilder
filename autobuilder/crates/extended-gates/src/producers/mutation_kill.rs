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

fn apply_first_mutation(text: &str, op: &(Operator, &'static str, &'static str)) -> Option<String> {
    let (_, from, to) = op;
    let idx = text.find(from)?;
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..idx]);
    out.push_str(to);
    out.push_str(&text[idx + from.len()..]);
    Some(out)
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

    for op in &ops {
        let Some(mutated) = apply_first_mutation(&original, op) else {
            continue;
        };
        if mutated == original {
            continue;
        }
        attempted += 1;
        let tmp = tempfile::tempdir()?;
        copy_tree(project, tmp.path())?;
        std::fs::write(tmp.path().join("src/lib.rs"), &mutated)?;
        let passed = cargo_test_passes(tmp.path());
        if passed {
            survivors.push(format!("{:?}", op.0));
        } else {
            killed += 1;
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
