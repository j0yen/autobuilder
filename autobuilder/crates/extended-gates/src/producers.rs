//! The 16 producer modules.
//!
//! Each producer exports `pub fn run(spec, project) -> Result<String>` that
//! audits the project, writes its receipt via
//! [`crate::prelude::write_receipt`], and returns a one-line summary.
//!
//! Producers are minimal-but-real: each implements **the invariant its
//! planted-failure AC asserts**, not full feature parity with the equivalent
//! standalone tool. See `PRD-extended-gates.md` § 9 (non-goals).

pub mod supply_audit;
pub mod license_audit;
pub mod secrets_scan;
pub mod sbom;
pub mod determinism;
pub mod hermetic_build;
pub mod msrv_verify;
pub mod binary_size;
pub mod cold_build_time;
pub mod bench_delta;
pub mod semver_check;
pub mod cli_surface;
pub mod schema_compat;
pub mod ac_traceability;
pub mod mutation_kill;
pub mod flake_audit;
