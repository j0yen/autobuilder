//! Digest-bound JSON receipt-writing primitives for autobuilder's risk gate.
//!
//! Every receipt is a JSON object that ends up under `target/autobuilder/receipts/`
//! with a `receipt_digest` field bound to its own canonicalized contents, plus a
//! `captured_at` UTC timestamp. Centralizing the digest + timestamp logic keeps
//! every receipt in the gate honest about what it attests to.
//!
//! Two public entry points:
//!
//! - [`write`] — stamp `receipt_digest`, write pretty-printed JSON bytes to a path.
//! - [`now_rfc3339`] — current UTC instant as an RFC3339 string the receipts use
//!   for `captured_at`.
//!
//! Self-binding invariant: the digest is computed over the canonicalized JSON
//! with `receipt_digest` set to the empty string, then written back into the
//! field. Any post-hoc edit to the receipt changes the recomputed digest,
//! making forgery detectable by re-running the same algorithm.

#![cfg_attr(not(test), forbid(unsafe_code))]

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use sha2::{Digest, Sha256};

/// Compute and stamp `receipt_digest` on the JSON object, then write
/// pretty-printed bytes to `path`.
///
/// The digest field is set to the empty string before hashing so the receipt
/// is self-binding: re-running the same algorithm over the on-disk bytes
/// reproduces the same digest exactly when no post-hoc edits occurred.
///
/// # Errors
///
/// - Returns an error if `value` is not a JSON object (digest binding only
///   makes sense for top-level objects).
/// - Returns an error if the parent directory cannot be created or the file
///   cannot be written.
pub fn write(path: &Path, mut value: serde_json::Value) -> Result<()> {
    if !value.is_object() {
        return Err(anyhow!("receipt must be a JSON object"));
    }
    set_digest(&mut value)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(&value)?;
    fs::write(path, bytes)?;
    Ok(())
}

/// Current UTC instant as an RFC3339 string (`YYYY-MM-DDTHH:MM:SSZ`).
///
/// # Errors
///
/// Returns an error if the system clock reports a time before the UNIX epoch.
pub fn now_rfc3339() -> Result<String> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX epoch")?
        .as_secs();
    Ok(secs_to_rfc3339(secs))
}

fn set_digest(value: &mut serde_json::Value) -> Result<()> {
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "receipt_digest".into(),
            serde_json::Value::String(String::new()),
        );
    }
    let canonical = canonical_json_bytes(value);
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    let digest = format!("sha256:{:x}", hasher.finalize());
    let obj = value
        .as_object_mut()
        .ok_or_else(|| anyhow!("receipt is not a JSON object"))?;
    obj.insert("receipt_digest".into(), serde_json::Value::String(digest));
    Ok(())
}

/// Compute the canonicalized-JSON byte representation of `value`.
///
/// "Canonical" here means recursive object-key sorting; nothing more. Two
/// JSON values that differ only in key order produce identical bytes.
#[must_use]
pub fn canonical_json_bytes(value: &serde_json::Value) -> Vec<u8> {
    let sorted = sort_keys(value);
    serde_json::to_vec(&sorted).unwrap_or_default()
}

/// Recursively sort the keys of every JSON object inside `value`.
///
/// Arrays preserve their order (positional semantics matter for arrays);
/// primitives are returned unchanged.
#[must_use]
pub fn sort_keys(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted = serde_json::Map::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for k in keys {
                if let Some(v) = map.get(k) {
                    sorted.insert(k.clone(), sort_keys(v));
                }
            }
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(sort_keys).collect())
        }
        other => other.clone(),
    }
}

/// Convert a UNIX-epoch second count to an RFC3339 UTC string.
#[must_use]
pub fn secs_to_rfc3339(secs: u64) -> String {
    let day = secs / 86_400;
    let rem = secs % 86_400;
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;
    let days = i64::try_from(day).unwrap_or(0);
    let (year, month, mday) = civil_from_days(days);
    format!("{year:04}-{month:02}-{mday:02}T{hour:02}:{minute:02}:{second:02}Z")
}

// Howard Hinnant's `civil_from_days` (public domain).
// Converts days-since-1970-01-01 to (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = u64::try_from(z - era * 146_097).unwrap_or(0);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = i64::try_from(yoe).unwrap_or(0) + era * 400;
    let day_of_year = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let d = u32::try_from(day_of_year - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}
