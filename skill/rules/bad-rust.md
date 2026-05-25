# BAD_RUST — Curated Subset for autobuilder

Curated subset of `jankurai/docs/BAD_RUST.md` (1484 lines). This file is the source for the audit checks in `rules/audit-checks.sh` and for the BAD_RUST scan that runs as a Stage-3 hard gate.

The full canonical catalog lives at `../../jankurai/docs/BAD_RUST.md` in any checkout of `github.com/j0yen/autobuilder`. Vendor here is intentionally narrow: ~8 categories, ~100 items, each chosen because it is (a) high-signal, (b) mechanically detectable, or (c) already a well-known soundness or correctness footgun.

A finding is `BLOCKING` (hard-gate fail) unless explicitly marked `advisory:` — advisory findings surface in the EvidencePack but do not fail the gate.

---

## 1. Borrow-checker bypasses that lie (HLT-029)

1. `mem::transmute` to extend a lifetime.
2. Turning a borrowed reference into `'static` because an API "wanted static."
3. Using `Box::leak` as a lazy lifetime fix in long-running code.
4. Using raw pointers to create aliasing that references would reject.
5. Using `Rc<RefCell<_>>` or `Arc<Mutex<_>>` merely to dodge ownership design.
6. Creating self-referential structs with raw pointers without a proven pinning/drop story.
7. Storing references into containers while pretending moves cannot happen.
8. Using `unsafe` because "the borrow checker is too strict" without proving the compiler-rejected case is actually valid.

## 2. Unsafe blocks and traits (HLT-029)

1. Using `unsafe` as a way to bypass the borrow checker rather than encode the real invariant.
2. Writing an unsafe block without a nearby `SAFETY:` explanation.
3. Writing a `SAFETY:` comment that restates the code instead of proving the preconditions.
4. Writing a false or unverifiable `SAFETY:` comment (`SAFETY: should be fine`, `SAFETY: tested`, `SAFETY: AI generated this`).
5. Putting many unrelated unsafe operations into one block.
6. `unsafe fn` bodies that perform unsafe operations without explicit local unsafe blocks (Rust 2024 default).
7. Making a function safe when the caller must uphold memory-safety preconditions.
8. Hiding unsafe code in macros, build scripts, proc macros, or generated files.
9. Adding `#![allow(unsafe_code)]` or broad lint suppressions instead of reviewing each unsafe site.
10. `unsafe impl Send` or `unsafe impl Sync` because "it seems fine."
11. Marking a type `Send` because "it compiles except for this raw pointer."
12. Marking a type `Sync` while it contains unsynchronized interior mutability.
13. Implementing unsafe traits like `TrustedLen`, allocator traits, or FFI-related traits without satisfying every invariant.

## 3. Panic discipline (HLT-029)

1. `unwrap()` or `expect()` on user input, network input, filesystem input, env vars, DB rows, CLI args, IPC, untrusted JSON, decrypted data, or anything attacker-influenced.
2. `expect("works")`, `expect("should not fail")`, or `expect("lol")`.
3. `panic!` as normal error handling in libraries.
4. Panicking in public APIs without documenting the contract.
5. Panicking in `Drop`.
6. Panicking across FFI.
7. Panicking inside async tasks where the `JoinHandle` is ignored.
8. Calling `todo!()`, `unimplemented!()`, or `unreachable!()` in reachable production paths.
9. Depending on `debug_assert!` for security or memory safety.
10. Using `assert!` for untrusted input validation when the right behavior is a recoverable error.
11. Documenting a function as infallible when it can panic.

## 4. Error swallowing (HLT-029)

1. Discarding `Result` with `let _ = ...` (advisory: when the comment justifies it).
2. Calling `.ok()` or `.err()` just to throw information away.
3. `map_err(|_| Error::Failed)` that destroys critical context.
4. Logging an error and continuing as if the operation succeeded.
5. Returning `Option` for a failure that needs an error explanation.
6. Returning `bool` for a failure that has multiple causes.
7. Swallowing `JoinError`, channel-close errors, task failures, flush failures, fsync failures, or serialization failures.
8. Retrying forever without backoff, limit, or cancellation.
9. Converting all errors to strings too early.
10. Using `anyhow::Error` as a public library API when callers need structured recovery.
11. Calling `.unwrap_or_default()` to hide a failure.

## 5. Secrets (HLT-010)

1. Hardcoded API keys, tokens, passwords, private keys, salts, or seed phrases.
2. Secrets in tests that are also valid in production.
3. Secrets in logs, panics, telemetry, metrics labels, traces, `Debug`, `Display`, or error messages.
4. Deriving `Debug`, `Clone`, `Serialize`, or `Deserialize` on secret-holding types without thinking.
5. Storing passwords as plaintext.
6. Comparing secrets with ordinary equality when constant-time comparison is needed.
7. Keeping sensitive data in memory longer than necessary.
8. Assuming `drop` zeroes memory.
9. Using `String`/`Vec<u8>` for secrets without a zeroization story in high-risk code.
10. Sending secrets to an LLM or external tool.

## 6. False thread-safety and async traps

1. `unsafe impl Send` / `unsafe impl Sync` on raw pointers, FFI handles, ref-counted non-threadsafe state, or hidden mutation without proof.
2. Sharing non-thread-safe FFI handles across threads.
3. Atomics used as decoration without a memory-ordering argument.
4. Mixing atomics and non-atomic access to the same memory.
5. Holding a `std::sync::MutexGuard`, `RwLockGuard`, or `RefCell` borrow across `.await`.
6. Blocking an async runtime thread with `std::thread::sleep`, blocking filesystem/network, blocking crypto, CPU-heavy loops, or synchronous clients.
7. Spawning tasks and ignoring their `JoinHandle` when task failure matters.
8. Fire-and-forget background tasks with no shutdown path.
9. Unbounded channels fed by external input.
10. `tokio::select!` code that is not cancellation-safe.
11. Creating a new runtime inside an existing runtime.
12. Mixing async runtimes accidentally.
13. Using sleeps as synchronization.
14. Holding locks while calling user code, callbacks, logging hooks, or async code.
15. Lock-order inversions.
16. `Arc<Mutex<_>>` as default architecture for shared mutable state.
17. Using `static mut`.
18. Using `Relaxed` because it is faster, not because it is correct.
19. Ignoring poisoning or pretending poisoning is recovery.

## 7. Performance and correctness traps

1. Cloning because you do not understand the borrow error.
2. Adding `.to_string()`, `.to_owned()`, `.clone()`, or `.collect()` until the compiler stops complaining (advisory in hot paths).
3. Using `Box::leak` to get a `'static` lifetime.
4. Using `lazy_static`, `OnceCell`, `static`, or globals to avoid passing state explicitly.
5. Using `get_unchecked` when the index can be influenced by input.
6. Using `unreachable_unchecked` for a branch that is merely "unlikely."
7. Using `assume`, unchecked math, unchecked indexing, or unchecked UTF-8 as performance tricks without proof.
8. Integer overflow in size calculations.
9. Allocating based on untrusted lengths without caps.
10. Missing timeouts on network calls.
11. Missing body-size limits on HTTP endpoints.
12. Missing rate limits on expensive endpoints.
13. Regexes vulnerable to catastrophic backtracking.
14. Using `HashMap` where hash-flooding matters and the hasher choice was changed casually.

## 8. Testing dishonesty

1. Treating Miri, tests, fuzzing, or "it ran once" as proof of soundness.
2. Weakening tests to match generated output.
3. Deleting failing tests because generated code changed behavior.
4. Accepting generated benchmarks that do not measure release builds, realistic data, or optimized-away work.
5. Snapshot-only proof of changed behavior.
6. Tautological tests that assert the implementation against itself.
7. Tests that pass under timing assumptions ("flaky → retry").
8. Tests that share mutable global state without isolation.
9. Tests skipped/focused for changed behavior (`#[ignore]`, `--exact`, `it.only`).
10. Tests that use mocks where the integration is the load-bearing risk.

---

## The master rule (from the source catalog)

Any of these in changed code is automatically `BLOCKING`:

1. **Lies to the compiler** about lifetimes, aliasing, initialization, layout, thread-safety, or ownership.
2. **Lies to users** by exposing a safe API that secretly requires unsafe preconditions.
3. **Lies to reviewers** by hiding risk behind `unsafe`, broad `allow`, vague comments, generated code, or "it compiles."
4. **Turns expected failure into process failure** without a documented contract.
5. **Treats AI output as authority** rather than as untrusted draft code.
6. **Ships code whose safety/security story the author cannot explain.**

## Generated-code variants (specifically for autobuilder)

These deserve extra scrutiny because the iterate-loop is the generator:

1. Adding `clone()`, `Arc<Mutex<_>>`, `Box::leak`, `Rc<RefCell<_>>`, `static mut`, `unsafe`, or `transmute` as a fix for the borrow checker.
2. Heap-allocating to remove lifetime parameters.
3. Replacing clear ownership with shared mutable state.
4. Replacing type errors with `as` casts.
5. "Fixing" async code by adding `spawn`, `block_on`, `spawn_blocking`, unbounded channels, or global state without an execution model.
6. Weakening tests, removing assertions, broadening tolerances, or ignoring flaky failures.
7. Replacing precise error types with `anyhow::Error` or strings everywhere in a library API.
8. Turning recoverable errors into panics because the iteration could not thread `Result` through.
9. Turning panics into `Result` while silently losing the invariant the panic was meant to expose.
10. `as` casts as a numeric-coercion shortcut where `TryFrom` is required.

A finding in this section is always `BLOCKING` and the iteration is reverted regardless of the metric.
