//! Patch-safety primitives extracted from `autobuilder/src/evolve.rs`.
//!
//! Three pure functions own the auto-apply decision and identity surface:
//!
//! - [`is_pure_addition_diff`] — true iff the unified-diff body contains no
//!   `-` lines inside any hunk body. The auto-apply path runs `patch -p0`
//!   only when this returns true; a bug here would cause silent
//!   destructive edits to skill-tree files.
//! - [`append_fingerprint`] — sha256 over `(target_path, appended_lines)`,
//!   used to dedupe append-only suggestions across evolve runs.
//! - [`patch_fingerprint`] — sha256 over `(target_path, diff_body)`, used
//!   to dedupe in-line patch suggestions across evolve runs.

#![cfg_attr(not(test), forbid(unsafe_code))]

use std::path::Path;

use sha2::{Digest, Sha256};

/// True iff every line of every hunk body is context or addition.
///
/// "Hunk body" is the lines between an `@@` header and either the next
/// `@@` header, the next file-header pair (`---` / `+++`), or end-of-input.
/// Lines outside hunk bodies (file headers, commit headers, blank lines
/// between hunks) cannot affect the result.
///
/// A diff with no hunks (only file headers, or empty) returns `true`
/// vacuously — there are no body lines that could be `-`.
///
/// The auto-apply path in `autobuilder evolve` runs `patch -p0` only when
/// this returns `true`. Returning `true` for a diff that contains `-`
/// lines would cause `patch` to silently remove content from skill-tree
/// files; this function is the load-bearing guard against that.
#[must_use]
pub fn is_pure_addition_diff(diff_body: &str) -> bool {
    let mut in_hunk = false;
    for line in diff_body.lines() {
        if line.starts_with("@@") {
            in_hunk = true;
            continue;
        }
        if !in_hunk {
            continue;
        }
        if line.starts_with("---") || line.starts_with("+++") {
            in_hunk = false;
            continue;
        }
        if line.starts_with('-') {
            return false;
        }
    }
    true
}

/// Stable sha256 fingerprint over `(target_path, appended_lines)`.
///
/// Used to dedupe append-only suggestions across evolve runs: once a
/// fingerprint lands in `applied.log`, the same suggestion is silently
/// skipped. Determinism + sensitivity to any byte change in either
/// component is the load-bearing property — assert by AC4/AC5.
#[must_use]
pub fn append_fingerprint(target: &Path, appended_lines: &[String]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(target.to_string_lossy().as_bytes());
    hasher.update(b"\n");
    for line in appended_lines {
        hasher.update(line.as_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

/// Stable sha256 fingerprint over `(target_path, diff_body)`.
///
/// Used to dedupe in-line patch suggestions across evolve runs.
#[must_use]
pub fn patch_fingerprint(target: &Path, diff_body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(target.to_string_lossy().as_bytes());
    hasher.update(b"\n");
    hasher.update(diff_body.as_bytes());
    format!("{:x}", hasher.finalize())
}
