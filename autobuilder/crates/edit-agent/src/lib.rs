#![allow(missing_docs)] // public API types intentionally documented at the struct level rather than field-by-field

//! Native Rust edit-agent for autobuilder campaigns.
//!
//! When the iterate-and-prove loop produces a `revert` or `crash` verdict,
//! the campaign driver hands the resulting `FailureCapsule` to this crate.
//! [`session::run`] opens an Anthropic Messages API conversation, advertises
//! four sandboxed tools (`read_file`, `write_file`, `edit_file`, `bash`),
//! and drives the model to convergence (`stop_reason == end_turn`) or to
//! one of the configured budget cutoffs.
//!
//! What this crate intentionally is not:
//! - **Not async.** Blocking HTTP via `ureq`; one request at a time.
//! - **Not multimodal.** Only text + tool-use + tool-result content blocks.
//! - **Not streaming.** Whole responses only.
//! - **Not a hardened sandbox.** [`sandbox::Sandbox`] gates direct file I/O
//!   via the three file tools; the `bash` tool runs subprocesses with the
//!   parent's environment and full filesystem access (no seccomp, no
//!   namespaces). See [`sandbox`] for details.

pub mod api;
pub mod sandbox;
pub mod session;
pub mod tools;

pub use api::{AnthropicClient, MessagesApi, MessagesRequest, MessagesResponse};
pub use sandbox::Sandbox;
pub use session::{SessionInput, SessionOutcome, run};
