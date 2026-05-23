//! Re-export shim for the `autobuilder-receipt` crate.
//!
//! The digest-binding and timestamp primitives now live in
//! `crates/receipt/` under their own intent-card and an adversarial
//! proptest suite (see that crate's `tests/`). This module keeps the
//! `crate::receipt::{write, now_rfc3339}` call sites compiling without
//! touching every caller.

pub(crate) use autobuilder_receipt::{now_rfc3339, write};
