# Verifying quality of /autobuilder-generated Rust code

**Author:** Claude (Opus 4.7), for jsy
**Date:** 2026-05-28
**Status:** Research report (not a PRD; not buildable)
**Scope:** What does it mean for /autobuilder's output to be "good," what does the
current pipeline already verify, what slips through, and what tests to add.

---

## TL;DR

/autobuilder already gets the easy half right: deny-lints prevent the
classic foot-guns (zero `.unwrap()` in shipped src/, no `unsafe` outside FFI,
no `todo!()` / `panic!()`), and acceptance tests pair to PRD ACs rather than
re-asserting whatever the implementation happened to compute. The Stage 4
"7 receipts" gate covers `cargo check / clippy -D / test / deny`, an
adversarial sub-agent that writes falsifying tests, and an independent
reviewer-agent verdict — a solid harness for syntactic and structural
quality.

What slips through is the LLM-specific layer: **specification drift** (PRD
asserts behavior the named upstream tool doesn't actually have — observed in
cadence-bind-letters), **test surface narrowness** (the AC's test passes but
mutation-killing density is uncomputed — `mutants_alive_count: null` across
every receipt sampled), **reviewer-agent "concern" verdicts that don't
block** (agorabus shipped with `decision_observed: "concern"`), and
**hardware-gated ACs that simply mark themselves deferred** rather than
being satisfiable via documented fakes. The proposal below adds a
spec-drift probe, mutation testing, a semantic AC-↔-test judge, a
hardware-mock convention, and a graduating reviewer-agent gate — five
concrete tests/gates, each scoped to a clear failure mode that the current
evidence shows is not yet caught.

The goal is not to verify "the code is correct" in the abstract — that's
not achievable. The goal is to verify *the specific failure modes
LLM-generated code is prone to that the existing gate misses*.

---

## 1. What the pipeline verifies today (evidence)

Reading `~/.claude/skills/autobuilder/SKILL.md` Stage 3–4 and one full
release receipt at `~/wintermute/agorabus/target/autobuilder/release-receipt.json`:

**Hard gates (every iter):**
- `cargo check --workspace`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `cargo deny check`
- `cargo +nightly miri test` (when `--allow-unsafe`)
- BAD_RUST audit (`rules/bad-rust.md` + `rules/audit-checks.sh`)
- Proof-lane routing: every changed path resolves to ≥1 lane, all lanes green

**Quality score (advance/revert tiebreak):**
```
score = 10*ac_passing + 3*coverage_pct + 2*proptest_density + 1*doc_coverage
      − 2*audit_findings − 1*clippy_warnings
```

**Stage 4 receipts (block on missing):**
| Receipt | Pass condition |
|---|---|
| `intake` | `intent-card.json` validates; all MUST-ACs declared |
| `vti-plan` | every changed path routed via `proof-lanes.toml`; confidence ≥ 0.70 |
| `proof-receipt` | test/proptest/fuzz/miri/deny green on HEAD |
| `risk-gate` | BAD_RUST audit clean (or only `advisory` with waivers) |
| `reviewer-agent` | independent Claude review; `{pass, concern, block}` |
| `rollback-plan` | every commit `git revert`-clean |
| `ci-checks` | `.github/workflows/` green on fresh clone |

**Adversarial sub-step** (optional, Stage 3 step 10): spawn an
adversarial-agent that writes `tests/adversarial_<id>.rs` attempting to
*falsify* each AC against its English description, not the implementation.
Closes the "edit-agent wrote both impl and test" tautology gap.

**Lint discipline in shipped crates** (sampled agorabus, episodic-observer,
daily-receipt, memlog, wintermute-audio): every Cargo.toml has
```
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"
unsafe_code = "deny"
```
Zero violations in `src/` across the sample. Lints are deny, not warn —
code fails to compile if violated.

**Tests are behavioral.** Spot-check on `agorabus/tests/acceptance_ac3.rs`,
`episodic-observer/tests/acceptance_ac10.rs`, `daily-receipt/tests/acceptance_ac1.rs`:
each test asserts a PRD-stated invariant against a fixture, not
"the function returned the value the function just computed."

**This is a strong harness.** The remaining critique is about what it
*doesn't* yet cover, not what it does.

---

## 2. What the evidence shows is missing or weak

Three signals from the sampled artifacts and journal:

### 2a. Receipts allow nullable thoroughness metrics

`~/wintermute/agorabus/target/autobuilder/metrics.json` includes
`mutants_alive_count: null`. The schema accepts a number; this build
didn't compute one. Same for branch coverage. The quality score above has
weights for proptest density and test coverage — but if both can be null
without blocking, the score is a soft signal, not a gate.

### 2b. Reviewer-agent verdicts are advisory

`reviewer-agent` is in the 7-receipt gate, but agorabus shipped with
`decision_observed: "concern"`. The gate accepts "concern" as
non-blocking. So the independent-review step exists, but its negative
signal doesn't actually stop a ship.

### 2c. Specification drift is detected too late

The clearest live failure pattern in `~/brain/journal/build-auto.log`:

> **cadence-bind-letters iter-1:** "Found a PRD-vs-reality conflict: the
> PRD assumes `letter-curate` runs a monthly curation pass that *produces*
> a monthly-aggregate Markdown. The actual binary only **triages
> pre-existing** letters… Building the PRD as written would mean inventing
> the entire aggregation engine — explicitly out of scope."

This was caught at iter-1 because the edit-agent tried to invoke the
real tool. But the PRD got drafted, queued, and dispatched before anyone
checked. With 5-way parallel dispatch (just added to /build), four other
parallel branches could chase similar specs concurrently before the
mismatch surfaces.

### 2d. Hardware-gated ACs are marked deferred, not satisfied differently

`PRD-build-deferred-acs.md` introduces `deferred_acs:` frontmatter so
ACs needing live systemd / audio / IMAP can skip the verified-completed
check. This is the right escape hatch *for shipping*, but it means a
whole class of behavioral verification gets pushed to the human + future.
There's no "documented fake convention" that lets the same AC be
satisfied at gate-time by a mock that exercises the boundary call
sequence and signature.

### 2e. Jankurai catalog is normative, not mechanical

`~/wintermute/autobuilder/jankurai/paper/jankurai.md` defines a
substantive anti-pattern set (HLT-003 OWNERLESS-PATH, HLT-008
FALSE-GREEN-RISK, HLT-029 RUST-BAD-BEHAVIOR …). The autobuilder receipt
records a jankurai-style verdict but doesn't mechanically invoke a
jankurai CLI. The catalog is good doctrine; the enforcement is
human-review-shaped.

---

## 3. LLM-specific failure modes that the harness doesn't catch

Categorized by what's prone to slip through given the evidence:

| Failure mode | What it looks like | Caught today? |
|---|---|---|
| **Spec drift** | PRD names a tool/API that doesn't have the assumed surface | Partially — only at iter-1 if the agent happens to invoke it |
| **Tautological test breadth** | Tests pass; mutation score would be low; one input class explored | No — mutation untested |
| **Edge-case under-coverage** | Happy path solid; empty/giant/unicode/concurrent inputs untested | Partially — proptest density weighted but not gated |
| **API hallucination (semantic)** | Right method name, wrong contract (e.g. "returns Result" but assumed Option semantics) | `cargo check` only catches structural; semantic slip lands as a runtime bug |
| **Reviewer concern → ship** | Independent reviewer flags "concern"; pipeline ships anyway | No — concern is advisory |
| **Deferred AC accumulation** | Hardware-gated ACs `#[ignore]`-marked indefinitely; no behavioral verification mechanism even via mocks | No — deferral is the escape hatch |
| **Cross-version drift** | Code works on the dep version pinned at build; breaks on a minor bump | No — pinned-version tests only |
| **Test-as-documentation incoherence** | Test name says one thing, body checks another | No — only the adversarial agent partially covers this |
| **AC-text-↔-impl-semantics mismatch** | AC's English says X; impl does X-but-with-a-quiet-Y; tests assert X | Adversarial agent partially; not gated |

---

## 4. Proposed tests / gates (concrete, in order of leverage)

### Test 1 — **Spec-drift probe (pre-iter-1, new Stage 2.5)**

**Failure mode:** §2c / cadence-bind-letters pattern.

**Design:** Before Stage 3, the autobuilder runs a small "ground-truth
probe" against every external tool / API the PRD names:
- Parse PRD body for backticked tool invocations
  (`recall list --since 7d`, `letter-curate aggregate`, etc.).
- For each, run `<tool> --help` and `<tool> <verb> --help` if applicable.
- Diff the actual subcommand+flag surface against what the PRD assumes.
- Emit `target/autobuilder/spec-drift.json` with verdict
  `{matched, missing_verbs[], extra_verbs[]}` per tool.
- **Gate:** any `missing_verbs[]` entry blocks Stage 3 with a clear
  diagnostic ("PRD assumes `letter-curate aggregate` but `--help` shows
  only `triage`, `list`, `show`").

**Cost:** O(seconds). Pure I/O. Adds one receipt to the gate set
(8 receipts).

**Confidence:** high — the cadence-bind-letters journal entry is the
exact pattern this catches in <1s.

### Test 2 — **Mutation testing (Stage 3 metric, then Stage 4 gate)**

**Failure mode:** §2a + tautological tests + edge-case under-coverage.

**Design:** Run `cargo mutants --in-place --no-shuffle --jobs $(nproc)`
after the standard `cargo test` passes. Capture:
- `mutants_total` (count of mutations generated)
- `mutants_killed` (count caught by a failing test)
- `mutants_alive` (slipped through — survivors are evidence of test
  thinness)
- `kill_rate = killed / total`

**Phased gate:**
- **Phase 1 (telemetry):** populate the receipt's `mutants_alive_count`
  field. No block. Calibrate threshold from the first 20 shipped crates.
- **Phase 2 (gate at calibrated threshold):** block if
  `kill_rate < threshold` (initial guess: 0.60). Bypass via PRD
  frontmatter `mutation_kill_rate_floor: 0.40` for crates where mutation
  testing is expensive (large input matrices).

**Cost:** mutation testing is slow (1–10× test-suite wall time). Cache by
src/ + tests/ hash so iter-N re-runs are cheap when no mutation-relevant
files changed.

**Confidence:** high — mutation testing is the gold standard for "do
your tests actually test." LLM-gen tests are exactly the population
where false-green is plausible.

### Test 3 — **Semantic AC↔test judge (Stage 4 receipt #8)**

**Failure mode:** AC-text-↔-impl-semantics mismatch + test-as-doc
incoherence + tautology beyond what the adversarial agent catches.

**Design:** A small judge LLM call (Sonnet 4.6, prompt-cached system) gets:
- AC i's English text from the PRD
- The test file paired to AC i (test name + body, ~50 lines)

…and answers, strictly:
1. Does the test exercise the behavior the AC describes? (yes / no /
   partial)
2. Is the test asserting the AC's stated invariant, or merely re-running
   the implementation and confirming the implementation's return value?
   (asserts-invariant / restates-impl / mixed)
3. Confidence (0.0 – 1.0).

**Gate:** any AC where `behavior_match: no` OR `assertion_kind:
restates-impl` AND confidence ≥ 0.7 blocks. Verdicts are recorded in
`target/autobuilder/ac-semantic-judge.json` as a new 8th receipt.

**Cost:** ~$0.005 per AC at Sonnet rates with system caching, ~$0.05
per crate (10 ACs). Negligible.

**Confidence:** medium-high — judge-LLM patterns work for narrow boolean
questions; the failure mode is judge bias toward the impl. Mitigation:
run the judge against a small golden set of known-good / known-bad pairs
as a calibration suite (~20 pairs hand-curated, expanded as crates ship).

### Test 4 — **Hardware-mock convention (replaces blanket deferred_acs)**

**Failure mode:** §2d — deferred ACs accumulate.

**Design:** For every "needs live X" AC, the PRD must either:
- (a) Declare a mock under `tests/mocks/<ac>.rs` that exercises the
  full call sequence + signature that the real hardware path would,
  asserting the same invariant against the mock; or
- (b) Declare `deferred_acs: [N]` AND a paired `mock_unjustified_for: [N]`
  prose explanation (one sentence: why a mock isn't tractable).

**Gate:** Stage 4 verified-completed check #5 now accepts EITHER a real
test pass OR a mock test pass + the (b) justification. ACs that have
neither remain a hard fail.

**Cost:** authoring effort per PRD. Saves wall-clock on the "hardware
will eventually verify this" loop.

**Confidence:** medium. The hardest part is keeping the mock honest —
mock drift from real hardware behavior is a known anti-pattern (cf.
your own memory `feedback_use_local_toolkit.md` discipline). Mitigate by
including a `cargo test --features=real-hardware` job that runs the same
ACs against the actual device and reports drift.

### Test 5 — **Reviewer-agent verdict promotion**

**Failure mode:** §2b — "concern" verdicts ship.

**Design:** Phased graduation, not flag-day:
- **Phase A (today):** concern is advisory; we collect verdicts in a
  table.
- **Phase B (after 30 crates):** correlate concern verdicts with
  post-ship issues. If concern ≈ "real issue" >50% of the time, promote
  to **soft-block** (requires user override via
  `reviewer_override: true` in PRD frontmatter, with one-line reason).
- **Phase C (after another 30):** concern → hard block, full stop.

**Cost:** none today (data already collected). The promotion is a
SKILL.md edit.

**Confidence:** high — this is calibration discipline, not new
infrastructure.

### Bonus — cross-version smoke (Stage 4, lower priority)

Run `cargo update --aggressive` + `cargo test` in a separate worktree
post-ship. Capture deltas. Non-blocking telemetry for v1; gate later if
breakage rate justifies.

---

## 5. Implementation roadmap

| Order | Test | Why first | Effort |
|---|---|---|---|
| 1 | Spec-drift probe | Cheapest, highest ROI, blocks today's main failure mode | 1 PRD; <1 day |
| 2 | Mutation testing (Phase 1) | Pure telemetry; calibrates the gate of Test 2 | 1 PRD; <1 day |
| 3 | Reviewer-agent Phase A→B promotion | No new infra, just calibration | <1 hour SKILL.md edit |
| 4 | Semantic AC↔test judge | Adds 8th receipt, depends on judge calibration set | 1 PRD; ~2 days incl. calibration |
| 5 | Hardware-mock convention | Touches PRD frontmatter shape; coordinate with the 5 hardware-deferred wintermute crates | 1 PRD; ~1 day + per-PRD authoring |
| 6 | Mutation Phase 2 gate | After Test 2 telemetry calibrates | <1 hour |
| 7 | Cross-version smoke | Lowest urgency; nice telemetry | 1 PRD; <1 day |

Each is a single-PRD-shaped change to the autobuilder skill or to a
sibling check. None require new tooling beyond `cargo-mutants` (already
on crates.io). All compose; none conflict.

---

## 6. Open questions (call before implementing)

1. **Mutation testing threshold.** Is 0.60 kill-rate too tight? Too
   loose? Cadence-bind-letters would have killed mutants well — the spec
   drift is upstream. Compute on the 20 most recent crates before
   picking a number.
2. **Judge LLM identity.** Should the AC↔test judge be the SAME model
   as the edit-agent (cheaper, may have shared blind spots) or a
   DIFFERENT model family (more expensive, more independent)? The
   reviewer-agent receipt already uses an independent Claude session;
   reuse that pattern? Or use Haiku for the judge as a cost-effective
   second opinion?
3. **Spec-drift probe scope.** Limit to backticked CLI invocations in
   the PRD, or extend to docstring-mentioned Rust APIs (e.g.
   "uses `tokio::sync::broadcast`")? The latter blows up scope; v1 is
   CLI-only.
4. **Hardware-mock convention drift detection.** Is a separate
   `cargo test --features=real-hardware` CI job tractable on this laptop
   when /build runs unattended at 21:30, or does it need a manual ritual?
5. **Reviewer promotion timeline.** "After 30 crates" is a guess. Could
   be 10 or 100 depending on signal density. Decide by inspection of the
   first 10 concern-shipped crates' fate.

---

## 7. What this report deliberately doesn't recommend

- **A new test framework.** `cargo test` + proptest + cargo-mutants is
  sufficient. Building a custom harness adds maintenance burden without
  qualitative improvement.
- **Coverage gating at 80/90/100%.** Coverage is a weak signal in
  isolation (LLM-gen code tends to be coverage-dense, mutation-thin).
  Track it; don't gate on it.
- **Formal verification (Kani, Prusti).** Worth a future research note;
  the cost-benefit doesn't pencil for the current crate corpus
  (~500 LOC each, mostly I/O glue).
- **Forbidding the deferred_acs mechanism.** It's a legitimate escape
  hatch for genuinely hardware-bound ACs. The fix is the mock
  convention (Test 4), not removal.
- **A separate "AI code quality" tool.** The five tests above are all
  things you'd run on human-written code too. The LLM-specific framing
  is about *failure mode prior*, not new categories of test.

---

## Appendix — evidence map

| Claim | Source |
|---|---|
| 7 receipts + hard gates | `~/.claude/skills/autobuilder/SKILL.md:96-127` |
| Quality score formula | same, lines 106-113 |
| Adversarial sub-step | same, lines 87-93 |
| 0 unwrap in src across 5 crates | survey by Explore agent, 2026-05-28 |
| Lint deny set in Cargo.toml | `~/wintermute/agorabus/Cargo.toml:30-45` (and siblings) |
| Behavioral test example | `~/wintermute/episodic-observer/tests/acceptance_ac10.rs` |
| `mutants_alive_count: null` | `~/wintermute/agorabus/target/autobuilder/metrics.json` |
| Reviewer-agent "concern" shipped | `~/wintermute/agorabus/target/autobuilder/release-receipt.json` |
| Spec-drift example | `~/brain/journal/build-auto.log`, cadence-bind-letters iter-1 |
| Deferred ACs convention | `~/wintermute/autobuilder/PRDs-archive/PRD-build-deferred-acs.md` |
| Jankurai catalog | `~/wintermute/autobuilder/jankurai/paper/jankurai.md` |
