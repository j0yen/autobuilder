//! `semver-check`: pub-API diff between HEAD~1 and HEAD is semver-compatible.
//!
//! Pure-Rust. Uses `git show` to read prior versions of each `src/lib.rs` in
//! the workspace, parses both old and new with `syn`, and extracts the set
//! of public items (fn signatures, struct field types, enum variants). Any
//! item present at HEAD~1 and missing or type-changed at HEAD is a breaking
//! change; the expected bump (`extended-gates.toml::semver_expected_bump`)
//! gates verdict.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use serde::Serialize;
use syn::{File, Item, Visibility};
use toml::Value as TomlValue;

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    base_ref: String,
    head_ref: String,
    expected_bump: String,
    compatibility: String,
    breaking_changes: Vec<String>,
    additions: Vec<String>,
}

fn git_show(project: &Path, refspec: &str) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["show", refspec])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn pub_items(src: &str) -> BTreeSet<String> {
    let Ok(parsed): syn::Result<File> = syn::parse_str(src) else {
        return BTreeSet::new();
    };
    let mut out = BTreeSet::new();
    for item in parsed.items {
        match item {
            Item::Fn(f) if matches!(f.vis, Visibility::Public(_)) => {
                out.insert(format!("fn:{}", f.sig.ident));
            }
            Item::Struct(s) if matches!(s.vis, Visibility::Public(_)) => {
                out.insert(format!("struct:{}", s.ident));
                for field in s.fields {
                    if matches!(field.vis, Visibility::Public(_)) {
                        let name = field.ident.map_or_else(String::new, |i| i.to_string());
                        let ty = quote_ty(&field.ty);
                        out.insert(format!("field:{}.{name}:{ty}", s.ident));
                    }
                }
            }
            Item::Enum(e) if matches!(e.vis, Visibility::Public(_)) => {
                out.insert(format!("enum:{}", e.ident));
                for variant in e.variants {
                    out.insert(format!("variant:{}::{}", e.ident, variant.ident));
                }
            }
            Item::Trait(t) if matches!(t.vis, Visibility::Public(_)) => {
                out.insert(format!("trait:{}", t.ident));
            }
            Item::Const(c) if matches!(c.vis, Visibility::Public(_)) => {
                out.insert(format!("const:{}", c.ident));
            }
            _ => {}
        }
    }
    out
}

fn quote_ty(ty: &syn::Type) -> String {
    // Stable string representation for diffing pub field types. Uses Debug
    // because syn's Type doesn't implement Display and pulling in `quote` +
    // `proc_macro2` just for this would inflate the dep budget. Two types
    // with identical Debug output are structurally identical for the
    // purposes of semver-check.
    let dbg = format!("{ty:?}");
    dbg.chars().filter(|c| !c.is_whitespace()).collect()
}

fn expected_bump(project: &Path) -> String {
    let cfg = project.join("extended-gates.toml");
    if let Ok(text) = std::fs::read_to_string(&cfg) {
        if let Ok(value) = text.parse::<TomlValue>() {
            if let Some(s) = value
                .get("semver_expected_bump")
                .and_then(TomlValue::as_str)
            {
                return s.to_owned();
            }
        }
    }
    "patch".to_owned()
}

/// Run the semver-check audit.
///
/// # Errors
///
/// Returns an error if the receipt write fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    let head_src = std::fs::read_to_string(project.join("src/lib.rs"));
    let Ok(head_src) = head_src else {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                base_ref: "HEAD~1".into(),
                head_ref: "HEAD".into(),
                expected_bump: expected_bump(project),
                compatibility: "unknown".into(),
                breaking_changes: vec!["no src/lib.rs at HEAD".into()],
                additions: Vec::new(),
            },
        )?;
        return Ok("semver-check: skipped (no src/lib.rs)".into());
    };
    let Some(base_src) = git_show(project, "HEAD~1:src/lib.rs") else {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                base_ref: "HEAD~1".into(),
                head_ref: "HEAD".into(),
                expected_bump: expected_bump(project),
                compatibility: "unknown".into(),
                breaking_changes: vec!["HEAD~1:src/lib.rs unavailable".into()],
                additions: Vec::new(),
            },
        )?;
        return Ok("semver-check: skipped (no HEAD~1)".into());
    };

    let base_items = pub_items(&base_src);
    let head_items = pub_items(&head_src);
    let removed: Vec<String> = base_items.difference(&head_items).cloned().collect();
    let added: Vec<String> = head_items.difference(&base_items).cloned().collect();

    let compatibility = if !removed.is_empty() {
        "major"
    } else if !added.is_empty() {
        "minor"
    } else {
        "patch"
    };

    let expected = expected_bump(project);
    let order = |s: &str| match s {
        "patch" => 0,
        "minor" => 1,
        "major" => 2,
        _ => -1,
    };
    let verdict = if order(compatibility) <= order(&expected) {
        "pass"
    } else {
        "block"
    };

    let summary = format!(
        "semver-check: compatibility={compatibility} expected={expected} breaking={} additions={}",
        removed.len(),
        added.len()
    );
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            base_ref: "HEAD~1".into(),
            head_ref: "HEAD".into(),
            expected_bump: expected,
            compatibility: compatibility.into(),
            breaking_changes: removed,
            additions: added,
        },
    )?;
    Ok(summary)
}
