//! Re-export shim for the `autobuilder-receipt` crate.
//!
//! The digest + RFC3339 primitives live in `crates/receipt/` under their
//! own intent-card (`PRD-receipt.md`) and a 7-AC adversarial suite
//! (self-binding, permutation stability, forgery detection, object-required,
//! RFC3339 format, parent-repo integration, strict clippy). This module
//! keeps `crate::receipt::{write, now_rfc3339}` call sites compiling without
//! touching the 9 internal consumers.

pub(crate) use autobuilder_receipt::{now_rfc3339, write};
