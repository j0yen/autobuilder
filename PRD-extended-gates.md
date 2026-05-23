# PRD: autobuilder-extended-gates — 16 new receipt producers under adversarial discipline

**Author:** Claude (Opus 4.7), drafted with jsy
**Status:** Draft v0.1 — mega-PRD; expands the risk gate from 8 → 24 receipts
**Date:** 2026-05-23
**Sibling to:** `PRD-gate.md` (extends `RECEIPT_SPECS`), `PRD-receipt.md` (every new producer uses `autobuilder-receipt::write`)

---

## TL;DR

`autobuilder-extended-gates` is a Rust workspace crate (`autobuilder/crates/extended-gates/`)
that ships **16 new receipt producers**, organized into five categories, each
auditing a class of failure mode the current 8-receipt gate does not catch:

| # | Producer | Category | What it proves |
|---|----------|----------|----------------|
| 1 | `supply-audit` | supply-chain | No Cargo.lock dep has a RUSTSEC advisory at vendored db ref |
| 2 | `license-audit` | supply-chain | Every transitive dep's license is in the allowlist |
| 3 | `secrets-scan` | supply-chain | No diff line matches the secret regex set |
| 4 | `sbom` | supply-chain | CycloneDX-shape SBOM materializes for the workspace |
| 5 | `determinism` | reproducibility | Two cold `cargo build --release` runs produce identical artifact sha256 |
| 6 | `hermetic-build` | reproducibility | No outbound socket during `cargo build --offline` (audited via `/proc/<pid>/net`) |
| 7 | `msrv-verify` | reproducibility | Declared `rust-version` actually compiles + tests |
| 8 | `binary-size` | performance | `target/release/*` under per-bin budget |
| 9 | `cold-build-time` | performance | Clean `cargo build --release` wall-time under budget |
| 10 | `bench-delta` | performance | Criterion benches do not regress >X% vs frozen baseline |
| 11 | `semver-check` | API contract | Public API diff between `HEAD~1` and `HEAD` is semver-compatible |
| 12 | `cli-surface` | API contract | `<bin> --help` output hash matches snapshot (or snapshot bumped intentionally) |
| 13 | `schema-compat` | API contract | Receipt JSON schemas added/changed are additive-only |
| 14 | `ac-traceability` | test quality | Every PRD AC ID has ≥1 test fn referencing it |
| 15 | `mutation-kill` | test quality | `syn`-driven mutation operators applied to lib produce ≥N% test failures |
| 16 | `flake-audit` | test quality | `cargo test` rerun K times produces identical outcomes |

Each producer:
- Lives as a `[[bin]]` target in `autobuilder/crates/extended-gates/` and a
  matching `pub fn run(args, project) -> Result<()>` in the lib.
- Emits `target/autobuilder/receipts/<name>.json` matching schema
  `autobuilder.<name>_receipt.v1`.
- Embeds `head_sha`, `captured_at`, and `receipt_digest` via
  `autobuilder_receipt::write` (digest-binding is reused, not reinvented).
- Is registered in `autobuilder_gate::RECEIPT_SPECS` so `autobuilder gate`
  aggregates all 24 into `release-receipt.json`.

The crate is **pure-Rust in-tree**: no shelling to external CLIs like
`cargo-audit`, `cargo-deny`, `gitleaks`, or `cargo-semver-checks`. It may
invoke `cargo` itself (the toolchain) and `git` (already a build dep). All
audit logic is implemented against in-tree crates (`syn`, `cargo_metadata`,
`sha2`, `regex`, `serde_json`, `proptest`).

---

## 1. Why this exists (what the current 8 receipts miss)

With all 8 existing receipts green, the following classes of shipping defect
remain undetected:

| Failure class | Example | Current gate behaviour |
|---|---|---|
| Supply-chain CVE | `serde_derive=1.0.171` (sleeper-RCE precedent) ships in lock file | Pass |
| License violation | A GPL dep gets pulled into the MIT-licensed crate | Pass |
| Secret leak | `AKIA…` accidentally committed in a test fixture | Pass |
| Non-deterministic build | Embedded `__DATE__` macro varies build-to-build | Pass |
| Hidden network in build | A `build.rs` calls `curl` and silently fails open | Pass |
| MSRV regression | `let-else` accidentally requires 1.65 in a 1.85-MSRV crate | Pass (1.85 happens to compile it) |
| Binary bloat | `serde_json` accidentally pulled into a parser crate | Pass |
| Semver-breaking change | A pub field type changes between patches | Pass |
| CLI surface break | `--config` becomes `--cfg`, downstream scripts break | Pass |
| AC-test drift | A PRD AC has no test; `cargo test` is green but the AC is unproven | Pass |
| Test triviality | Tests exist and pass but mutations don't break them | Pass |
| Flake masked by retry | `cargo test` is green only on retry; CI hides the flap | Pass |

The unfakeable scalar `stage4_receipt_producers_callable` would not move for
any of these defects, because that metric measures producer presence, not
producer truth. The extended gate closes the gap by producing **truth**
receipts — each with a forgery-resistant digest, a head_sha binding, and an
adversarial AC suite that proves the producer flips its verdict when the
audit's target failure mode is planted in a fixture.

---

## 2. Public surface

```rust
// autobuilder/crates/extended-gates/src/lib.rs

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

/// One row per producer; consumed by autobuilder-gate's RECEIPT_SPECS extension.
pub struct ProducerSpec {
    pub name: &'static str,                 // "supply-audit"
    pub schema: &'static str,               // "autobuilder.supply_audit_receipt.v1"
    pub file_name: &'static str,            // "supply-audit-receipt.json"
    pub pass_verdicts: &'static [&'static str], // typically &["pass"]
}

pub const PRODUCER_SPECS: &[ProducerSpec] = &[ /* 16 entries */ ];

/// Each module exposes:
///   pub fn run(project: &Path) -> anyhow::Result<()>;
/// which writes target/autobuilder/receipts/<file_name> via
/// autobuilder_receipt::write (digest-binding, head_sha embedding).
```

Each binary `autobuilder/crates/extended-gates/src/bin/<name>.rs`:

```rust
fn main() -> anyhow::Result<()> {
    let args = <ClapArgsForThisProducer>::parse();
    extended_gates::<module>::run(&args.project)
}
```

Each `--help` exits 0 and prints the producer's purpose + flags. This is the
unfakeable-presence check the existing meta-scalar relies on.

---

## 3. Receipt schemas

Every receipt is a JSON object with this common envelope (digest-bound by
`autobuilder_receipt::write`):

```json
{
  "schema": "autobuilder.<name>_receipt.v1",
  "head_sha": "<40-char hex>",
  "verdict": "pass" | "block",
  "captured_at": "<RFC3339 UTC>",
  "receipt_digest": "<sha256 hex>",
  "<producer-specific fields>": "..."
}
```

Producer-specific payloads (one example per category):

```json
// supply-audit-receipt.json
{
  "schema": "autobuilder.supply_audit_receipt.v1",
  "verdict": "pass",
  "advisory_db_ref": "rustsec/2026-05-22T00:00:00Z",
  "deps_scanned": 187,
  "advisories_found": [],
  "ignored_advisories": []
}

// semver-check-receipt.json
{
  "schema": "autobuilder.semver_check_receipt.v1",
  "verdict": "pass",
  "base_ref": "HEAD~1",
  "head_ref": "HEAD",
  "compatibility": "patch" | "minor" | "major",
  "expected_bump": "patch",
  "breaking_changes": []
}

// ac-traceability-receipt.json
{
  "schema": "autobuilder.ac_traceability_receipt.v1",
  "verdict": "pass",
  "prd_path": "PRD-extended-gates.md",
  "ac_ids": ["AC1", "AC2", "..."],
  "untraced_ac_ids": [],
  "tests_per_ac": { "AC1": 2, "AC2": 1, "...": 0 }
}
```

Full schema specs land in `autobuilder/crates/extended-gates/SCHEMAS.md`
during Phase 0.

---

## 4. Acceptance criteria

All MUST. Unfakeable scalar `extended_gates_ac_passing` (target=70) counts
AC-N.K + cross-cutting ACs.

### Per-producer ACs (4 each × 16 producers = 64 ACs)

For each producer `<name>` in {supply-audit, license-audit, secrets-scan, sbom,
determinism, hermetic-build, msrv-verify, binary-size, cold-build-time,
bench-delta, semver-check, cli-surface, schema-compat, ac-traceability,
mutation-kill, flake-audit}:

**AC-<name>.1 (MUST) — happy-path receipt**
On a clean fixture project that satisfies the producer's audit, running
`<name>` writes `target/autobuilder/receipts/<name>-receipt.json` with
`verdict=pass`, schema `autobuilder.<name>_receipt.v1`, and a digest that
verifies under `autobuilder_receipt::verify`.
**Test:** `tests/acceptance_<name>_happy.rs`

**AC-<name>.2 (MUST) — failure detection**
On a fixture project where the audit's target failure mode is **planted**
(see fixture catalog below), the producer writes `verdict=block` with a
non-empty `notes` (or producer-specific failure field).
**Test:** `tests/acceptance_<name>_planted.rs`

**AC-<name>.3 (MUST) — idempotency**
Running the producer twice on the same project produces byte-identical
receipt JSON (modulo `captured_at`, which is allowed to differ; `receipt_digest`
must match because the digest computation zeroes the digest field and the
captured_at field is included in canonical form — so this AC tests
**deterministic-modulo-timestamp** equivalence by stripping both fields and
comparing).
**Test:** `tests/acceptance_<name>_idempotent.rs`

**AC-<name>.4 (MUST) — schema stability**
The receipt's top-level keys are exactly the set declared in `SCHEMAS.md`
for this producer. No accidental fields. Proptest fuzzes producer inputs
(where applicable) and asserts the key set never grows beyond the declared
set.
**Test:** `tests/acceptance_<name>_schema.rs`

### Planted-failure fixtures (used by AC-N.2)

| Producer | Planted defect |
|---|---|
| supply-audit | `tests/fixtures/cve-cargo-lock/` pins a dep at a version listed in vendored RUSTSEC advisory `RUSTSEC-2020-0036` (test-only fixture; not a real prod dep) |
| license-audit | `tests/fixtures/gpl-dep/Cargo.toml` lists a GPL-3.0 crate against an MIT allowlist |
| secrets-scan | `tests/fixtures/leaked-key/` contains a synthetic `AKIA` + 16 chars (not a real key) |
| sbom | `tests/fixtures/broken-lock/Cargo.lock` is malformed — producer must emit `verdict=block`, not panic |
| determinism | `tests/fixtures/nondeterministic-buildrs/` writes `SystemTime::now()` into a generated file — second build differs from first |
| hermetic-build | `tests/fixtures/network-buildrs/build.rs` opens a TCP socket to 127.0.0.1:0 during build |
| msrv-verify | `tests/fixtures/msrv-violation/` declares `rust-version = "1.60"` but uses `let-else` (1.65) |
| binary-size | `tests/fixtures/bloated/` pulls in a heavy dep that pushes the bin over a 1MB budget configured in `extended-gates.toml` |
| cold-build-time | `tests/fixtures/slow-build/` includes a `compile_error!` after a deliberate `std::thread::sleep(60s)` in build.rs (test runs the producer with a 5s budget; producer should report budget exceeded, not block on the sleep) |
| bench-delta | `tests/fixtures/regressed-bench/` ships a frozen baseline JSON; the criterion run reports a 50% slowdown |
| semver-check | `tests/fixtures/semver-break/` has `HEAD~1` with `pub struct S { pub x: u32 }` and `HEAD` with `pub struct S { pub x: u64 }` — major change against `patch` expectation |
| cli-surface | `tests/fixtures/cli-rename/` snapshot says `--config`, current `--help` says `--cfg` |
| schema-compat | `tests/fixtures/schema-break/` removes a required field between v1 and v2 of a receipt schema |
| ac-traceability | `tests/fixtures/missing-ac/` declares `AC9` in PRD but has no test fn referencing `AC9` |
| mutation-kill | `tests/fixtures/trivial-tests/` has a lib with mutable arithmetic and a test that doesn't observe the result |
| flake-audit | `tests/fixtures/flaky-test/` has a test that fails with 30% probability based on hashing the timestamp |

### Cross-cutting ACs (6)

**AC-X1 (MUST) — all 16 binaries respond to `--help`**
`autobuilder/crates/extended-gates/target/release/<name> --help` exits 0
for all 16 producer names. Tested via `scripts/run-metrics.sh`.

**AC-X2 (MUST) — PRODUCER_SPECS table integrity**
`PRODUCER_SPECS.len() == 16`. Every entry has a unique `name`, a schema
string starting with `"autobuilder."` and ending with `"_receipt.v1"`, and
a non-empty `pass_verdicts`.

**AC-X3 (MUST) — gate aggregator picks up new specs**
After Phase 6 (extending `autobuilder_gate::RECEIPT_SPECS` to 24), running
`autobuilder gate --project <synth>` on a tree where all 24 receipts are
present and valid emits `release-receipt.json` with `pass_count == 24`,
`block_count == 0`, `verdict == "pass"`. The aggregator's permutation-
invariance (gate AC6) still holds.

**AC-X4 (MUST) — receipt directory layout**
Every producer writes to `target/autobuilder/receipts/<file_name>` where
`<file_name>` matches `PRODUCER_SPECS[i].file_name`. No producer writes
elsewhere.

**AC-X5 (MUST) — head_sha consistency**
Every receipt's `head_sha` matches `git rev-parse HEAD` in the project
directory. Proptest runs the full producer set against three commits and
asserts the head_sha threading.

**AC-X6 (MUST) — digest-roundtrip on every receipt**
For each of the 16 new receipts, `autobuilder_receipt::verify(path)`
returns true; mutating any non-digest, non-captured_at field and calling
`verify` returns false. Adversarial: proptest-mutates one byte at a time.

---

## 5. Hard constraints

- `rust_edition = "2024"`
- `target_kind = "lib + 16 bins"`
- `deny_unsafe = true` for the library; producer audits may not introduce
  `unsafe` blocks. (Unsafe in the crates being audited is a different
  concern, scoped out — see Non-goals.)
- `max_deps_top_level = 8` — `anyhow`, `serde`, `serde_json`, `sha2`,
  `clap`, `regex`, `syn`, `cargo_metadata`. Each is justified in
  `Cargo.toml` comments. `proptest` is dev-only.
- `msrv = "1.85"` (matches workspace).
- `max_lib_lines_per_module = 400` — each producer module ≤ 400 LoC; the
  17th module (`mod prelude`) holds shared helpers.
- Workspace clippy lints: `unwrap_used`, `expect_used`, `panic = deny`
  outside `#[cfg(test)]`. Every integration test file has the file-level
  allow per the memory note.
- **No shell-out** to standalone CLIs other than `cargo` (toolchain) and
  `git` (already a workspace dep). `cargo-audit`, `cargo-deny`,
  `cargo-semver-checks`, `gitleaks`, `cargo-mutants` are explicitly
  re-implemented in-tree at the depth required by the ACs above (not full
  feature parity — only the audit invariant each AC names).
- Vendored RUSTSEC advisory db: `autobuilder/crates/extended-gates/vendor/rustsec/`
  contains a snapshot of the advisory-db TOML files as of the build date.
  Refresh is out-of-band (a separate `extended-gates refresh-rustsec`
  subcommand, scoped under Non-goals for v1).

---

## 6. Five whys

1. **Why one mega-PRD vs sixteen?** Jsy chose maximalist scope (see memory
   `feedback-ambitious-scope-pure-rust`). The crate is one coherent
   surface (`PRODUCER_SPECS`), the gate consumes one extension point, the
   postmortem is one document. Sixteen PRDs would split the discipline
   without splitting the design.
2. **Why pure-Rust in-tree vs shelling to existing tools?** Receipt
   forgery-resistance depends on every byte the producer emits being
   traceable to in-tree code. A subprocess's stdout is opaque; even if we
   parsed it, the operator can substitute a malicious `cargo-audit` on the
   PATH and the receipt would pass. The pure-Rust path also keeps the
   build hermetic (see hermetic-build, AC-hermetic-build.1).
3. **Why 16 producers and not 19 (the list I floated)?** Doc-coverage and
   unsafe-census overlap with the existing `bad-rust-audit` receipt
   (jankurai anti-pattern catalog already covers undocumented pub items
   and unjustified unsafe). Coverage-delta merges into mutation-kill: a
   mutation-kill rate that doesn't fall implies coverage of the mutated
   lines, which is the load-bearing property anyway.
4. **Why extend `RECEIPT_SPECS` instead of a parallel gate?** The release
   verdict is a single bit. Two gates means two bits, which means the
   downstream consumer of `release-receipt.json` has to learn a new shape.
   Extending preserves the existing contract; the only diff is that
   `pass_count + block_count` is now 24 instead of 8.
5. **Why now?** The 8-receipt gate has stabilized (gate-extraction commit
   `0da0615` landed). Building on that floor is cheaper than building it
   in parallel with the gate's own discipline.

---

## 7. Phasing

| Phase | Scope | ACs proved |
|-------|-------|-----------|
| 0 | PRD + intent-card + workspace-member scaffold + 16 stub bins + `PRODUCER_SPECS` table + `SCHEMAS.md`. All producers emit `verdict=block`, `notes=["not implemented"]`. | X1, X2, X4 |
| 1 | Supply-chain cluster: supply-audit, license-audit, secrets-scan, sbom. | 16 (4 per producer) |
| 2 | API-contract cluster: semver-check, cli-surface, schema-compat. | 12 |
| 3 | Test-quality cluster: ac-traceability, mutation-kill, flake-audit. | 12 |
| 4 | Reproducibility cluster: determinism, hermetic-build, msrv-verify. | 12 |
| 5 | Performance cluster: binary-size, cold-build-time, bench-delta. | 12 |
| 6 | Extend `autobuilder_gate::RECEIPT_SPECS` from 8 to 24. Cross-cutting ACs X3, X5, X6. Run `autobuilder gate` end-to-end against a synth tree of 24 valid receipts. | X3, X5, X6 |

Phases 1-5 are independent and parallelizable; the autobuilder iter loop
may take them serially per cluster if the budget tightens. Phase 6 depends
on all earlier phases (it consumes the 16 producers' schemas).

---

## 8. Unfakeable scalar

```json
{
  "name": "extended_gates_ac_passing",
  "lower_is_better": false,
  "harness_command": "scripts/run-metrics.sh",
  "target": 70
}
```

Breakdown: 4 ACs × 16 producers = 64, plus 6 cross-cutting = 70.

The harness:
1. Builds the extended-gates crate in release.
2. For each producer, runs the happy-path fixture, the planted fixture,
   the idempotency check, and the schema-stability proptest. Tally per-AC
   pass/fail.
3. Runs the 6 cross-cutting ACs.
4. Emits `extended-gates-metrics.json` with the per-AC verdict.
5. Prints `extended_gates_ac_passing: <count>` for the autobuilder
   meta-scalar consumer.

A producer that emits `verdict=pass` on the planted fixture (i.e. fails to
detect its target defect) **decrements** the count regardless of whether
its --help works. The unfakeability is that detection is asserted against
an adversarial fixture, not against the producer's own report.

---

## 9. Non-goals

1. **Full feature parity with cargo-audit, cargo-deny, cargo-semver-checks,
   gitleaks, or cargo-mutants.** We re-implement only the audit invariant
   each AC names. A cargo-audit user expecting CVSS scoring won't get it.
2. **RUSTSEC advisory-db live sync.** The vendored snapshot is the source
   of truth for v1. A refresh subcommand is future work.
3. **Audit of the extended-gates crate itself by its own producers.**
   Self-application is a known-fun rabbit hole; deferred to a follow-up
   PRD (`PRD-self-audit-extended-gates.md`).
4. **Doc-coverage and unsafe-census.** Subsumed by the existing
   `bad-rust-audit` (risk-gate) receipt; revisit only if jankurai's
   catalog is found insufficient.
5. **A GUI / dashboard for receipt browsing.** `release-receipt.json` is
   the contract; downstream rendering is out of scope.
6. **Cross-language audit.** Rust-only. A Python or TypeScript variant
   would be a separate crate; the schema strings would need a
   language-prefix discriminator.
7. **Mutation testing of the autobuilder repo's *own* tests.** The
   `mutation-kill` producer audits *target projects*, not its host. Self-
   application is non-goal #3.

---

## 10. Risks (called out for the iter loop)

- **Scaffold-from-zero is not the autobuilder skill's typical mode.** Prior
  /autobuilder runs in this repo (commits `780e912`, `0da0615`, `ed5dcfa`)
  extracted *existing* code under discipline. This PRD asks the iter loop
  to scaffold 16 new producers without an existing reference impl. If the
  loop budget proves tight, fall back to per-cluster phasing (Phase 1
  alone is a valid /autobuilder run; ship it and re-enter for Phase 2).
- **Pure-Rust mutation-kill is hard.** `cargo-mutants` is non-trivial. The
  AC commits to *the invariant* (mutating arithmetic in fixture lib causes
  fixture tests to fail), not to feature parity. Keep the producer's
  scope to a small operator set (arithmetic-flip, comparison-flip,
  return-mutation) — enough to bite the trivial-test fixture, not enough
  to claim industrial-grade mutation testing.
- **Hermetic-build audit needs OS introspection.** Reading `/proc/<pid>/net/tcp`
  is Linux-only. The AC must be gated on `cfg(target_os = "linux")` and
  the producer should emit `verdict=skip` (added to `pass_verdicts`) on
  other platforms. macOS support is a Non-goal for v1.
- **Determinism producer is brittle.** rustc's incremental cache, ASLR in
  test binaries, and timestamp embedding all bite. The fixture must use
  `--release` (no incremental) and clear `target/` between runs. Failures
  here are interesting data, not bugs.

---

## 11. What "done" looks like

```
$ cd autobuilder && cargo build --release && cd ..
$ scripts/run-metrics.sh | jq '.extended_gates_ac_passing'
70

$ autobuilder gate --project .
gate: head=<sha> receipts=24 pass=24 block=0 verdict=pass
  ✓ intake
  ✓ vti-plan
  ✓ proof-receipt
  ✓ risk-gate
  ✓ reviewer-agent
  ✓ rollback-plan
  ✓ ci-checks
  ✓ release-receipt
  ✓ supply-audit
  ✓ license-audit
  ✓ secrets-scan
  ✓ sbom
  ✓ determinism
  ✓ hermetic-build
  ✓ msrv-verify
  ✓ binary-size
  ✓ cold-build-time
  ✓ bench-delta
  ✓ semver-check
  ✓ cli-surface
  ✓ schema-compat
  ✓ ac-traceability
  ✓ mutation-kill
  ✓ flake-audit
```

When that pair of commands prints those numbers and that list, ship it.
