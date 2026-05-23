//! Shared test helpers for per-producer integration tests.
//!
//! Provides `init_git_project`, `read_receipt`, and one-line Cargo.lock /
//! Cargo.toml builders so each per-producer test file stays focused on the
//! AC it asserts.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

pub(crate) fn init_git(dir: &Path) {
    let _ = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["init", "-q", "-b", "main"])
        .status();
    let _ = Command::new("git")
        .arg("-C")
        .arg(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@e.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@e.com")
        .args(["commit", "-q", "--allow-empty", "-m", "init"])
        .status();
}

pub(crate) fn commit_all(dir: &Path, msg: &str) {
    let _ = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["add", "-A"])
        .status();
    let _ = Command::new("git")
        .arg("-C")
        .arg(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@e.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@e.com")
        .args(["commit", "-q", "--allow-empty", "-m", msg])
        .status();
}

pub(crate) fn read_receipt(project: &Path, file_name: &str) -> Value {
    let path = project.join("target/autobuilder/receipts").join(file_name);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_slice(&bytes).unwrap()
}

pub(crate) fn verdict_of(value: &Value) -> &str {
    value.get("verdict").and_then(Value::as_str).unwrap_or("?")
}

/// Write a minimal Cargo.lock at the project root with the named packages.
/// Each entry: (name, version, license).
pub(crate) fn write_cargo_lock(project: &Path, packages: &[(&str, &str, Option<&str>)]) {
    let mut s = String::from("version = 3\n\n");
    for (name, version, license) in packages {
        s.push_str("[[package]]\n");
        s.push_str(&format!("name = \"{name}\"\n"));
        s.push_str(&format!("version = \"{version}\"\n"));
        if let Some(license) = license {
            s.push_str(&format!("license = \"{license}\"\n"));
        }
        s.push('\n');
    }
    std::fs::write(project.join("Cargo.lock"), s).unwrap();
}

pub(crate) fn write_cargo_toml(project: &Path, rust_version: Option<&str>) {
    let mut s = String::from("[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
    if let Some(rv) = rust_version {
        s.push_str(&format!("rust-version = \"{rv}\"\n"));
    }
    std::fs::write(project.join("Cargo.toml"), s).unwrap();
}

/// Strip the two fields that legitimately differ between runs even on
/// identical input: `captured_at` (timestamp moves) and `receipt_digest`
/// (recomputed over canonical bytes which include `captured_at`).
pub(crate) fn strip_volatile(value: &mut Value) {
    if let Some(obj) = value.as_object_mut() {
        obj.remove("captured_at");
        obj.remove("receipt_digest");
    }
}

/// Set of top-level keys in a receipt JSON object.
pub(crate) fn key_set(value: &Value) -> std::collections::BTreeSet<String> {
    value
        .as_object()
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default()
}
