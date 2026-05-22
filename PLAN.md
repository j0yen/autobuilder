# Autobuilder — A Skill for PRD-driven, Rigorously Validated Rust Code

## Context

The user has just walked through three repos that take opposite philosophical bets on "how do you trust agent-authored software":

- **`miolini/autoresearch-macos`** (Karpathy's `autoresearch`, MPS fork): collapses the problem to a single editable file + one unfakeable metric (`val_bpb`) + a 9-step git advance-or-revert loop. No governance, no schema, no review. Trust = empirical metric + git history.
- **`neverhuman/jankurai`**: a Rust audit CLI + a 0.9.0-versioned "standard" with ~40 HLT-* rule IDs, a 1485-line BAD_RUST.md anti-pattern catalog, proof-lane / owner-map / test-map TOML+JSON schemas, and a merge-witness JSON schema. Trust = repository-local evidence receipts.
- **`neverhuman/jeryu`**: a Rust-first Git/CI control plane with six proof primitives — Supersedence, Impact, Evidence Capsule (`FailureCapsule`), Confidence Gate (VTI confidence ≥ 0.70), Risk Gate (`RiskGateDecision { Allow, Deny, Escalate }`), Proof Receipt — plus five separable proof-scoped crates (`cargo-witness`, `cargo-vrc`, `cargo-aer`, `witness-rt`, `arc-bench`). Trust = N required receipts on a gate, signed and digest-bound.

The intent: **combine these into a single Claude Code skill called `autobuilder` that takes a PRD as input, drives an autonomous iterate-and-prove loop on a Rust codebase, and continually improves itself.**

Goal: every Rust artifact emitted by `autobuilder` is (a) generated from a structured Intent Card derived from the PRD via 4/5-Whys, (b) iterated under a narrow falsifiable loop (autoresearch model), (c) accompanied by a `FailureCapsule`/`EvidencePack`-style receipt bundle (jeryu model), (d) gated by lifted jankurai rules and a risk gate before "ready to ship," and (e) used as input to a postmortem that updates the skill itself.

## Architecture

```
PRD (file / pasted text)
  └─> Stage 1: Intake & 5-Whys → intent-card.json
       └─> Stage 2: Scaffold (cargo new + locked metric harness + lints)
            └─> Stage 3: Iterate-and-Prove Loop (advance-or-revert)
                 └─> Stage 4: Risk Gate (require 7 receipts)
                      └─> Stage 5: Postmortem + Self-Evolve
```

### Stage 1 — PRD Intake (4/5-Whys)

A structured interview that runs against the PRD until the root motivation is named. Emits an `intent-card.json` shaped after jeryu's `EvidencePack.intake` slot:

```jsonc
{
  "schema": "autobuilder.intent_card.v1",
  "prd_source": "path/to/prd.md",
  "root_motivation": "...",          // surfaced by 5-Whys
  "user_persona": "...",
  "unfakeable_metric": {              // autoresearch's load-bearing concept
    "name": "e.g. tests_pass_count",
    "lower_is_better": false,
    "harness_command": "scripts/run-metrics.sh"
  },
  "acceptance_criteria": [            // MUST/SHOULD/MAY, each testable
    {"id": "AC1", "level": "MUST", "test": "..."},
    ...
  ],
  "scope": ["..."],
  "non_goals": ["..."],
  "hard_constraints": {
    "rust_edition": "2024",
    "deny_unsafe": true,
    "target": "cli|lib|service",
    "max_deps": null
  },
  "five_whys_trace": [
    {"why": 1, "q": "...", "a": "..."},
    ... up to 5
  ]
}
```

Refuses to proceed if ambiguity remains after 5 Whys — surfaces what's missing and asks the user (jankurai's `kickoff` pattern).

### Stage 2 — Scaffold (Locked Harness + Editable Surface)

Generates a Rust project where the **harness is read-only** and the **agent edits only `src/`** (autoresearch's `prepare.py` / `train.py` separation, generalized):

```
<project>/
├── Cargo.toml                 # locked workspace + deny.toml
├── clippy.toml                # strict lints; deny warnings
├── deny.toml                  # supply-chain (lifted from jeryu)
├── rust-toolchain.toml        # pinned
├── src/                       # ← agent edits ONLY this
├── tests/                     # ← read-only integration + proptest
│   ├── acceptance_*.rs        # one test per AC from intent-card
│   ├── proptest_invariants.rs
│   └── fuzz/                  # cargo-fuzz harness
├── scripts/
│   ├── run-metrics.sh         # ← read-only; emits metrics.json
│   ├── audit.sh               # ← read-only; runs BAD_RUST scan
│   └── risk-gate.sh           # ← read-only; checks 7 receipts
├── agent/                     # lifted-from-jankurai layout
│   ├── AUTOBUILDER_PROGRAM.md # the autoresearch-style program file
│   ├── intent-card.json       # the source of truth
│   ├── owner-map.json         # path → owner
│   ├── test-map.json          # path → required validation command
│   └── proof-lanes.toml       # change-class → required lanes
├── target/autobuilder/        # receipts dir
│   ├── receipts/<sha>.json    # one EvidencePack per iteration
│   ├── failure-capsules/      # one FailureCapsule per crash
│   ├── results.tsv            # autoresearch-style log
│   └── postmortem.md          # final summary
└── .github/workflows/         # CI mirror of the local gate
```

### Stage 3 — Iterate-and-Prove Loop (advance-or-revert)

Generalizes autoresearch's 9-step loop. Emits an EvidencePack receipt every iteration.

```
LOOP UNTIL all-MUST-ACs-green AND risk-gate-passes OR budget-exhausted:
  1. git state check; current branch is autobuilder/<intent-slug>
  2. Edit src/ ONLY (an Autobuilder Edit Agent generates the diff)
  3. git commit -m "iter-<n>: <hypothesis>"
  4. scripts/run-metrics.sh > target/autobuilder/run.log 2>&1
  5. jq < target/autobuilder/metrics.json → parse
  6. If crash: tail -n 50 run.log; ≤3 fix attempts; else write FailureCapsule + status=crash
  7. Append row to results.tsv: <sha> <quality_score> <ac_passing> <status> <description>
  8. Advance if: all hard gates pass AND quality_score improved AND no MUST-AC regression
     Else: git reset --hard HEAD~1; status=discard
  9. Emit EvidencePack JSON for the iteration
```

Hard gates (must all pass before any advance):
- `cargo check --workspace`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `cargo deny check`
- `cargo +nightly miri test` (if `--allow-unsafe`)
- BAD_RUST audit scan (lifted catalog, grep + clippy-restriction lints)
- proof-lanes routing: every changed path resolves to ≥1 lane, all lanes green

Quality score (soft, drives advance/revert tiebreak):
```
score = weighted_sum(
  ac_passing_count,          # weight 10
  test_coverage_pct,          # weight 3
  proptest_density,           # weight 2
  doc_coverage_pct,           # weight 1
  -audit_findings_count,      # weight 2
  -clippy_warning_count       # weight 1
)
```

### Stage 4 — Risk Gate (jeryu's 7 receipts)

Before declaring the project "ready," require all 7 receipts present and digest-bound to `HEAD`:

| Receipt | Source | Pass condition |
|---|---|---|
| `intake` | Stage 1 | `intent-card.json` validates against schema, all MUST-ACs declared |
| `vti-plan` | Stage 2/3 | every changed path routed via `proof-lanes.toml`; confidence ≥ 0.70 |
| `proof-receipt` | Stage 3 | test/proptest/fuzz/miri/deny green on `HEAD` |
| `risk-gate` | Stage 3 | BAD_RUST audit clean (or only `advisory`-severity findings with waivers) |
| `reviewer-agent` | new sub-agent | independent Claude review of `HEAD~N..HEAD` against intent-card; decision ∈ `{pass, concern, block}` |
| `rollback-plan` | Stage 2 | every commit is `git revert`-clean; rollback steps written to `target/autobuilder/rollback.md` |
| `ci-checks` | Stage 2 | `.github/workflows/` green on a fresh clone of the worktree |

Missing receipts → block + machine-readable diagnostic. No self-approval (jeryu rule 3).

### Stage 5 — Postmortem & Self-Evolve

After each completed PRD run:
- `target/autobuilder/postmortem.md`: what worked, where the loop got stuck, what anti-pattern recurred, what new lint should be added, what scaffold tweak would have prevented N retries.
- A run-level `evolution-proposal.json` queued in `~/.claude/skills/autobuilder/proposals/`.
- The `/autobuilder --evolve` mode: aggregates proposals across the last K runs, surfaces a diff against `SKILL.md` / `rules/bad-rust.md` / `templates/scaffold/` for **user review**. Never self-applies. (This is the "improving the toolset" half of the request; making self-modification gated keeps the meta-loop honest.)

## Skill Layout (Claude Code skill)

```
~/.claude/skills/autobuilder/
├── SKILL.md                          # ~5KB — entry point, when-to-invoke, the 5-stage pipeline summary
├── prompts/
│   ├── prd-intake-5whys.md            # 4/5-Whys interview script
│   ├── reviewer-agent.md              # independent diff reviewer
│   ├── edit-agent.md                  # iteration coder prompt
│   ├── postmortem-writer.md
│   └── evolve.md                      # self-improvement aggregator
├── templates/
│   ├── scaffold/                      # full project skeleton (Cargo.toml, clippy, deny, harness, tests/, scripts/, agent/, .github/)
│   └── AUTOBUILDER_PROGRAM.md.tmpl    # autoresearch-program-md analogue, instantiated per project
├── rules/
│   ├── bad-rust.md                    # curated subset lifted from jankurai/docs/BAD_RUST.md
│   ├── hlt-rules.toml                 # HLT-* IDs we adopt (HLT-001, -002, -004, -010, -021, -029...)
│   └── audit-checks.sh                # grep + clippy-restriction implementations
├── schemas/
│   ├── intent-card.schema.json
│   ├── evidence-pack.schema.json      # lifted from jeryu/.jeryu/autonomy/schemas/
│   ├── failure-capsule.schema.json    # lifted from jeryu/src/capsule.rs (translated)
│   ├── proof-receipt.schema.json      # lifted from jankurai/schemas/proofmark-receipt.schema.json
│   └── merge-witness.schema.json      # lifted from jankurai/schemas/merge-witness.schema.json
├── scripts/
│   ├── intake.sh                      # runs 5-Whys interview, emits intent-card.json
│   ├── scaffold.sh                    # cargo-new + templating
│   ├── experiment-loop.sh             # the 9-step LOOP
│   ├── metric-harness.sh              # delegates to project's scripts/run-metrics.sh, normalizes output
│   ├── risk-gate.sh                   # checks 7 receipts
│   ├── postmortem.sh
│   └── evolve.sh                      # aggregates proposals
└── proposals/                         # accumulated evolution proposals (gated, not auto-applied)
```

## What I Vendor vs. Reference

| Asset | Source | Action | Reason |
|---|---|---|---|
| `BAD_RUST.md` catalog | jankurai/docs/BAD_RUST.md | **Vendor curated subset** (~300 lines of the 1485) | Stable, prose, immediately useful; full version is too much for context |
| HLT-* rule table | jankurai/agent/JANKURAI_STANDARD.md | Vendor as `rules/hlt-rules.toml` | Stable IDs; we pick the ~15 most-relevant |
| `proof-lanes.toml` schema | jankurai/agent/proof-lanes.toml | Vendor template | Reusable structure |
| `merge-witness.schema.json` | jankurai/schemas/merge-witness.schema.json | Vendor verbatim | Concrete JSON schema for the "are we ready" decision |
| `FailureCapsule` struct | jeryu/src/capsule.rs | Translate to JSON schema, vendor | Receipt format for crashes |
| `EvidencePack` schema | jeryu/.jeryu/autonomy/schemas/evidence-pack.schema.json | Vendor verbatim | Receipt format for iterations |
| `release.policy.toml` 7-receipt pattern | jeryu/release.policy.toml + src/release/gate.rs | Adopt as `scripts/risk-gate.sh` logic | The gate model itself |
| `program.md` structure | autoresearch/program.md | Vendor as `templates/AUTOBUILDER_PROGRAM.md.tmpl` | Per-project instructions for the iterating agent |
| `results.tsv` schema | autoresearch/program.md | Adopt verbatim, add columns | Concrete per-iteration log |
| 9-step LOOP | autoresearch/program.md | Adopt nearly verbatim, generalize | The core trust mechanic |
| `jankurai-proofbind` crate | jankurai/crates/jankurai-proofbind | Reference by path (don't vendor) | Live crate, would age in a vendor copy |
| `cargo-aer` / `cargo-vrc` / `cargo-witness` | jeryu/crates/ | Reference by path (don't vendor) | Live crates; optional invocations |

## Reused Existing Skills

The skill does NOT reinvent; it calls into existing skills/tooling where available:
- `/loop` (interval execution) — for long-running experiment cadence
- `/verify` — for the final end-to-end app-run verification step in Stage 4
- `/code-review` — for the `reviewer-agent` receipt
- `/init` — for cargo-new scaffolding shells out, but autobuilder owns its own template path

## Tools to Build (concrete, during implementation)

1. **`intake.sh` + `prd-intake-5whys.md`** — Claude-driven structured interview, JSON output.
2. **`scaffold.sh`** — templated `cargo new` + harness installation.
3. **`scripts/run-metrics.sh`** (in each scaffolded project) — single-script orchestrator emitting one `metrics.json`.
4. **`audit-checks.sh`** — BAD_RUST grep + clippy-restriction lint runner. Translates the prose catalog into runnable checks where mechanizable (e.g., `Box::leak` grep, `unsafe impl Send` grep, `mem::transmute` grep, `unwrap` density), advisory-only where not (e.g., "API design honesty").
5. **`experiment-loop.sh`** — the 9-step LOOP runner. Calls `metric-harness.sh`, makes advance/revert decisions, writes receipts.
6. **`risk-gate.sh`** — checks 7 receipt files exist + are digest-bound to `HEAD`.
7. **`reviewer-agent` prompt** — independent subagent that reviews `HEAD~N..HEAD` diff against `intent-card.json` and decides pass/concern/block.
8. **`postmortem.sh`** — aggregates results.tsv + FailureCapsules + EvidencePacks into a markdown summary and an evolution proposal.
9. **`evolve.sh`** — gated self-improvement: never auto-modifies SKILL.md; surfaces a diff for user approval.

## Verification (How to Test the Built Skill)

Smoke test (single PRD, single platform):
- PRD: "A CLI that reverses stdin and exits 0; rejects binary input with exit 2."
- Expect: scaffold compiles, MUST-ACs all green after ≤5 iterations, all 7 receipts produced, postmortem emitted.

Adversarial tests:
- **Under-specified PRD** ("make a tool that helps with files") — expect 5-Whys to refuse to proceed and surface ambiguity.
- **Conflicting PRD** ("must be zero-dep AND must use tokio") — expect intake to flag the conflict.
- **Untestable PRD** ("must feel snappy") — expect intake to demand a quantifiable proxy or refuse.

Robustness tests:
- Inject a deliberate failure into `scripts/run-metrics.sh` mid-run → expect FailureCapsule, ≤3 retries, then clean halt with status=crash.
- Inject a metric regression on an "improvement" iteration → expect git reset + status=discard.
- Skip the risk gate (mock missing receipt) → expect Stage 4 to refuse "ready" with machine-readable diagnostic.

Scale tests (after smoke):
- 3 PRDs of escalating complexity: rev-cli → in-memory KV store → tiny static-file HTTP server. Measure iterations-to-green, advance/revert ratio, # of human interventions (should be 0 if the gate model holds).

Self-evolution test:
- Run autobuilder against a deliberate PRD that exposes a gap in the BAD_RUST catalog (e.g., misuse of `tokio::spawn` returning unawaited handles). Expect postmortem to identify the new rule and queue an evolution proposal. Expect `/autobuilder --evolve` to surface it for review without auto-applying.

## Resolved Decisions (from 2026-05-21 clarification round)

The user answered the four open questions before switching Claude plans. Lock these in:

1. **Skill shape** — **Skill + companion Rust binary.** Build both: a `~/.claude/skills/autobuilder/` Claude Code skill that orchestrates Claude subagents, AND a companion Rust binary at `/Users/jsy/projects/autobuilder/autobuilder/` that owns the metric harness, receipt writing, risk gate, proof-lane routing, and the experiment-loop runner. The skill shells out to the binary; the binary is itself dogfooded under autobuilder's discipline (see decision #4). Reused crates from jeryu/jankurai are referenced by path, not vendored (per the vendor-vs-reference table).

2. **Target scope** — **CLIs + library crates.** Stage-2 scaffold must support both `--target cli` and `--target lib`. For libraries, add: cargo-semver-checks integration into the metric harness, docs-coverage check (`cargo doc --no-deps` + `RUSTDOCFLAGS="-D missing-docs"`), public-API surface diff vs. `HEAD~1` (a `cargo public-api` invocation). Web service / WASM / embedded explicitly **out of scope for v1** — defer to a `--target service` mode in v2 once v1 is shipping.

3. **Autonomy model** — **Hybrid: human checkpoint only on AC additions/changes.** The iterate-and-prove loop runs fully autonomously (advance/revert with no human input). The risk gate runs fully autonomously when all 7 receipts can be produced. The human is consulted ONLY when:
   - The agent wants to add a new acceptance criterion not in the intent-card
   - The agent wants to relax / waive a MUST acceptance criterion
   - The agent wants to widen `hard_constraints` (e.g., allow unsafe when intent-card said no)
   - An evolution proposal would modify the skill itself (Stage 5 is always gated)
   Implementation: an `intent_card_amendment_request.json` written to a known path triggers a halt + user prompt. Without an amendment, the loop ships when receipts green.

4. **First PRD** — **Autobuilder itself, meta.** Once the skill scaffolding exists (SKILL.md + minimum prompts + scaffold templates + a stub Rust binary), write a PRD for one of autobuilder's own Rust sub-tools and have autobuilder build it. Candidate first sub-tool: **the metric harness binary** (`autobuilder-metric-harness`) — it has a clean unfakeable contract (input: project path; output: a single normalized `metrics.json`), is small enough to fit in the v1 falsifiable loop, and once it exists, every subsequent autobuilder run uses it. This is maximally dogfooded and gives us a real signal whether the loop works before we throw external PRDs at it.

## Build Order (for resume)

When work resumes:

**Phase A — Skill scaffold (no Rust yet)**
1. Create `~/.claude/skills/autobuilder/SKILL.md` (frontmatter + entry-point logic)
2. Write `prompts/prd-intake-5whys.md` (the structured interview)
3. Write `schemas/intent-card.schema.json`
4. Vendor `rules/bad-rust.md` curated subset from `jankurai/docs/BAD_RUST.md` (~300 lines, selected for: borrow-checker bypasses, unsafe misuse, panic discipline, error swallowing, secrets, false thread-safety, performance traps, testing dishonesty)
5. Vendor `schemas/evidence-pack.schema.json` and `schemas/merge-witness.schema.json` from the source repos
6. Translate `FailureCapsule` (jeryu/src/capsule.rs:14) to `schemas/failure-capsule.schema.json`

**Phase B — Rust binary stub**
7. `cargo new --bin /Users/jsy/projects/autobuilder/autobuilder/` (workspace-ready)
8. Stub binary with subcommands: `intake`, `scaffold`, `loop`, `gate`, `postmortem`, `evolve` — each `unimplemented!()` initially
9. Add `Cargo.toml` workspace pointing at `crates/` for future sub-crates
10. Add `clippy.toml`, `deny.toml` (lifted from jeryu), `rust-toolchain.toml`

**Phase C — First meta-PRD: the metric harness**
11. Write `~/.claude/plans/autobuilder-prd-metric-harness.md` — a real PRD for `autobuilder-metric-harness` binary
12. Run autobuilder against it (Stage 1 5-Whys → Stage 2 scaffold → Stage 3 loop → Stage 4 gate)
13. Capture postmortem; this is the v1 acceptance test

**Phase D — Backfill**
14. Once the metric harness exists and works, use it for autobuilder's own loop (the binary now uses its own dogfooded harness)
15. Write the remaining tools (`audit-checks.sh`, `risk-gate.sh`, `experiment-loop.sh`) under autobuilder's discipline
16. Iterate `BAD_RUST.md` rules selection based on Phase C postmortem learnings

## Resume Instructions (for the next Claude session)

The next session should:
1. Read this plan file in full
2. Read the three source repos under `/Users/jsy/projects/autobuilder/{jankurai,jeryu,autoresearch-macos}/` for the artifacts to vendor/reference (specific files listed in the "What I Vendor vs. Reference" table above)
3. Confirm with the user that the resolved decisions still hold (they may have evolved between sessions)
4. Begin **Phase A** of the Build Order
5. Use `ExitPlanMode` when ready to begin actual file creation, since plan mode is currently active

Reference files the next session should read first (in order):
- `/Users/jsy/projects/autobuilder/autoresearch-macos/program.md` (the LOOP protocol, verbatim)
- `/Users/jsy/projects/autobuilder/autoresearch-macos/prepare.py` lines 40-56 + the `evaluate_bpb` signature (the locked-harness contract)
- `/Users/jsy/projects/autobuilder/jankurai/docs/BAD_RUST.md` (the anti-pattern catalog source)
- `/Users/jsy/projects/autobuilder/jankurai/agent/JANKURAI_STANDARD.md` lines 165-212 (the HLT-* rule table)
- `/Users/jsy/projects/autobuilder/jankurai/agent/proof-lanes.toml` (template format)
- `/Users/jsy/projects/autobuilder/jankurai/schemas/merge-witness.schema.json` (verbatim vendor)
- `/Users/jsy/projects/autobuilder/jankurai/schemas/proofmark-receipt.schema.json` (verbatim vendor)
- `/Users/jsy/projects/autobuilder/jeryu/.jeryu/autonomy/schemas/evidence-pack.schema.json` (verbatim vendor)
- `/Users/jsy/projects/autobuilder/jeryu/.jeryu/autonomy/schemas/agent-approval-receipt.schema.json` (verbatim vendor)
- `/Users/jsy/projects/autobuilder/jeryu/src/capsule.rs` lines 14+ (`FailureCapsule` struct — translate to JSON schema)
- `/Users/jsy/projects/autobuilder/jeryu/release.policy.toml` + `/Users/jsy/projects/autobuilder/jeryu/src/release/gate.rs` lines 62-72 (the 7-receipt gate model)
- `/Users/jsy/projects/autobuilder/jeryu/proof-lanes.toml` lines 1-60 (consumer pattern in `src/agent_surface_index.rs:62-100`)
- `/Users/jsy/projects/autobuilder/jeryu/src/test_intel/planner.rs` + `subsystem_glob.rs` (VTI confidence-scoring algorithm — for reference, not vendor)

## Status at Save Point

**Last updated: 2026-05-21 (session 2). Paths corrected from macOS to Linux: this machine is Linux at `/home/jsy/` not `/Users/jsy/`.**

### Phase A — Skill scaffold: COMPLETE

38 files under `/home/jsy/.claude/skills/autobuilder/`:
- `SKILL.md` — entry point (registers the skill with Claude Code).
- `prompts/` — 5 files: `prd-intake-5whys.md`, `edit-agent.md`, `reviewer-agent.md`, `postmortem-writer.md`, `evolve.md`.
- `rules/` — `bad-rust.md` (curated subset, ~270 lines), `hlt-rules.toml` (15 adopted IDs), `audit-checks.sh` (22 mechanizable detectors; bash-syntax-clean).
- `schemas/` — `intent-card.schema.json` (authored), `failure-capsule.schema.json` (translated from `jeryu/src/capsule.rs:14`, GitLab-CI envs generalized), plus 3 vendored verbatim: `evidence-pack`, `merge-witness`, `proof-receipt`.
- `scripts/` — 7 orchestration shims (`intake`, `scaffold`, `experiment-loop`, `metric-harness`, `risk-gate`, `postmortem`, `evolve`); each defers to the companion Rust binary when present, falls back to minimum-viable shell behavior otherwise.
- `templates/scaffold/` — full project skeleton (`Cargo.toml` with strict `[lints]`, `clippy.toml`, `deny.toml`, `rust-toolchain.toml`, `src/`, `tests/`, `scripts/run-metrics.sh + audit.sh + risk-gate.sh`, `agent/owner-map.json + test-map.json + proof-lanes.toml`, `.github/workflows/ci.yml` with SHA-pinned actions).
- `templates/AUTOBUILDER_PROGRAM.md.tmpl` — per-project inner-loop instructions.

### Phase B — Rust binary stub: COMPLETE

13 files under `/home/jsy/projects/autobuilder/autobuilder/`:
- Workspace-ready `Cargo.toml` with `[workspace.lints]` mirroring scaffold rules.
- `src/main.rs` — clap dispatch to 7 subcommand modules.
- 7 module stubs (`intake`, `scaffold`, `loop_runner`, `metric_harness`, `gate`, `postmortem`, `evolve`), each clap-typed with `unimplemented!()` body.
- `clippy.toml`, `deny.toml`, `rust-toolchain.toml` (pinned 1.85.0), `.gitignore`.

**Toolchain installed**: `rustup` at `~/.cargo/bin/`, Rust 1.85.0 auto-installed via `rust-toolchain.toml` on first invocation. `cargo check` passes clean from `/home/jsy/projects/autobuilder/autobuilder/`. PATH note: add `~/.cargo/bin` to PATH (rustup-init was invoked with `--no-modify-path`).

Visibility was tightened on the subcommand modules to `pub(crate)` to satisfy the `unreachable_pub = "warn"` workspace lint — module structs and `run()` fns are `pub(crate)`, fields are `pub` (which narrows to `pub(crate)` inside a `pub(crate)` struct).

### Phase C — First meta-PRD (`autobuilder-metric-harness`): IN PROGRESS

- **PRD written**: `/home/jsy/.claude/plans/autobuilder-prd-metric-harness.md`. 10 ACs (6 MUST, 3 SHOULD, 1 MAY). Tight constraints: deny_unsafe, max 6 deps, no git or network subprocess. Unfakeable metric: `acceptance_tests_passing_count`, target 10.
- **Stage 1 (Intake) COMPLETE.** Four open questions resolved with the user via AskUserQuestion:
  1. **Root motivation**: bootstrap a load-bearing tool — close the shell-script crack in the trust model. Until autobuilder owns its own metric harness, the loop's advance/revert decisions are only as trustworthy as a bash file with no tests.
  2. **Crate layout**: standalone crate at `crates/metric-harness/` (initially scaffolded standalone at `/home/jsy/projects/autobuilder-metric-harness/` and vendored into the workspace post-green).
  3. **`head_sha` source**: `--head-sha` CLI flag from caller; binary has no git dependency.
  4. **Canonical JSON for `output_digest`**: recursive key-sort + `serde_json::to_vec` (matches the tight-deps constraint; documented in the binary's README).
- **Intent card emitted**: `~/.claude/skills/autobuilder/proposals/intake-autobuilder-metric-harness-20260521T000000Z.json`. Structural validation passes (all required keys present, AC IDs match pattern, `intent_slug` valid, 6 MUST-ACs declared, schema field correct).
- **Stage 2 (Scaffold) COMPLETE.** Path 1 chosen (shell-fallback scaffold + hand-authored acceptance tests). Project at `/home/jsy/projects/autobuilder-metric-harness/` with baseline commit `fc3fd57` on `main`, plus `autobuilder/autobuilder-metric-harness` branch ready for the Stage 3 loop. 17 acceptance test functions (named `acceptance_ac<N>_*`) across 10 `tests/acceptance_ac<N>.rs` files. Cargo deps locked: anyhow, clap, serde, serde_json, sha2 (5 of 6 allowed). Dev-deps: proptest, tempfile.
- **Baseline run-metrics.sh output**: `ac_passing_count=0, ac_total_count=10, audit.blocking_count=0, advisory_count=0, clippy_warning_count=0` (see `target/autobuilder/metrics.json`). Stage 3's gradient is to drive `ac_passing_count` from 0 → 10 by editing `src/main.rs` only.
- **Scaffold-time fixes applied to the project's `scripts/run-metrics.sh`** (the template at `~/.claude/skills/autobuilder/templates/scaffold/scripts/run-metrics.sh` has the same bugs and should be patched during Phase D postmortem):
  - `cargo check` and `cargo test` lines now end with `|| true` so a failing gate doesn't `set -e` the script before the emit step. Mirrors AC5's invariant ("emit metrics even when exit 1") in the bash bootstrap.
  - `BLOCKING`/`ADVISORY` are defaulted with `${VAR:-0}` after the jq parse, so an empty `target/autobuilder/audit.json` (which happens when `audit-checks.sh` itself aborts before emitting) doesn't crash the final `--argjson` call.

### Stage 3 — single-iteration `autobuilder loop` runner: LANDED

Commit `5333e6c` in the autobuilder workspace. The companion binary's `loop` subcommand is implemented at `/home/jsy/projects/autobuilder/autobuilder/src/loop_runner.rs` (~350 lines incl. RFC3339 helper and canonical-JSON digest). Surface:

```
autobuilder loop --project <p> --iteration <n> --head-sha <sha> [--description <text>]
```

What it does, in order: read `<p>/agent/intent-card.json` for the unfakeable metric name + `lower_is_better`, spawn `bash <p>/scripts/run-metrics.sh`, parse the resulting `target/autobuilder/metrics.json` against schema `autobuilder.metrics.v1`, extract the scalar by name from `metrics.scalars`, compare against the last metric column of `target/autobuilder/results.tsv` (if any), decide verdict ∈ {baseline, advance, revert, crash}, append a new TSV row, write a sha256-digested per-iteration receipt (`autobuilder.iteration_receipt.v1`) to `target/autobuilder/receipts/<head_sha>.json`, and print a one-line summary to stdout.

Verdict logic (lifted from autoresearch + adapted):
- `blocking_audit > 0` → crash.
- `script_exit != 0 && previous.is_none()` → crash (no prior baseline to compare against).
- `iteration == 0` → baseline.
- Otherwise: `improved = (lower_is_better ? current < prev : current > prev)` → advance else revert.

End-to-end verified against `/home/jsy/projects/autobuilder-metric-harness/` at iter-0: results.tsv gains header + baseline row, receipt JSON written with valid `receipt_digest`, run.log captured, exit code 0.

Workspace deps used: `clap`, `anyhow`, `serde`, `serde_json`, `sha2`. Time formatting is std-only (Howard Hinnant `civil_from_days`); the `time` crate would have been cleaner but its 0.3.47 requires Rust 1.88 vs our 1.85 toolchain pin. Clippy clean on loop_runner.rs (only the other stub modules' `unimplemented!()` warnings remain workspace-wide).

What's deferred (explicit non-goals for v1, to surface in the postmortem):
- **`FailureCapsule` emission**. On verdict=crash we just set the status; we don't yet write `autobuilder.failure_capsule.v1` JSON. The schema is vendored in `~/.claude/skills/autobuilder/schemas/failure-capsule.schema.json` and ready to wire.
- **Proof-lane routing**. `agent/proof-lanes.toml` is not consulted yet — the loop runs whatever `run-metrics.sh` does, regardless of which paths changed.
- **Multi-iteration orchestration**. The binary runs ONE iteration; the caller drives the LOOP. Either Claude does it autoresearch-style, or we add an outer `autobuilder loop --iterate-until <budget>` mode that shells out to the edit-agent.
- **Edit-agent invocation**. The binary doesn't call out to Claude (or anything else) to make `src/` edits. That's the orchestrator's job.

### Resume point — Stage 3 (iterate the meta-PRD)

- Project cwd: `/home/jsy/projects/autobuilder-metric-harness/`.
- Branch: `autobuilder/autobuilder-metric-harness` (parent: `main@fc3fd57`). Iter-0 baseline already recorded.
- Only `src/main.rs` is editable per `agent/owner-map.json`; everything else is harness-readonly.
- Edit-agent contract for the metric-harness binary's CLI (locked by the test stubs):
  - `autobuilder-metric-harness <project_path> [--head-sha <sha>] [--iteration <n>] [--timeout-seconds <n>] [--pretty]`
  - Exit codes: 0 clean, 1 partial (still emits metrics), 2 missing/non-executable run-metrics.sh, 3 schema validation failure on metrics.json.
- Iter-0 metric: 0/10 (committed at `fc3fd57`, receipt at `target/autobuilder/receipts/fc3fd57edd588f7e597967b008d53d97e02417ea.json`). Target: 10/10 plus all 7 receipts on the Stage 4 risk gate.
- Next step: drive iter-1 onward. A Claude session reads the intent-card + the next failing AC test, edits `src/main.rs` to implement that AC, runs:

  ```
  cd /home/jsy/projects/autobuilder-metric-harness
  git commit -am "iter-1: <hypothesis>"
  /home/jsy/projects/autobuilder/autobuilder/target/release/autobuilder loop \
      --project . --iteration 1 --head-sha "$(git rev-parse HEAD)" \
      --description "<short>"
  ```

  Then acts on the printed verdict: advance keeps the commit, revert does `git reset --hard HEAD~1`, crash investigates run.log. Repeat until 10/10 or the budget is exhausted.

### Known issues to fold into the Phase D postmortem

1. **`templates/scaffold/scripts/run-metrics.sh`** has the same `set -e`-aborts-before-emit bug fixed in this scaffolded copy. Template needs the same `|| true` + `${BLOCKING:-0}` patches.
2. **`rules/audit-checks.sh`** itself aborts mid-run (exit 1, empty stdout) on a fresh scaffold — likely a `check_seven_receipts_present` or similar pre-condition that's not met at baseline. Should be defensive: emit valid `{findings:[], blocking_count:0, advisory_count:0}` even when no checks pass.
3. The clippy `print_stderr = "warn"` denial means the template `src/main.rs` stub (which uses `eprintln!`) cannot pass `clippy -- -D warnings`. Either relax the lint for `src/main.rs` or have the scaffold emit a different stub.
4. The bash `AC_PASSING` grep counts function-level passes; `AC_TOTAL` counts files. They're not directly comparable — fine as a coarse gradient, but the Rust harness must normalize this.
5. The `time` crate (cleaner RFC3339 path) requires rustc ≥ 1.88 in its current line; we're pinned to 1.85.0 via `rust-toolchain.toml`. Until the toolchain pin is bumped (decide whether to track stable), keep using the std-only `civil_from_days` helper in `loop_runner.rs`.

### How to resume

1. Read this `PLAN.md` in full (especially the resolved-decisions and Phase C blocks).
2. Verify Phase A files exist: `ls /home/jsy/.claude/skills/autobuilder/`.
3. Verify Phase B + Stage 3 binary builds: `cd /home/jsy/projects/autobuilder/autobuilder && cargo build --release` (toolchain auto-installs if missing).
4. Verify scaffolded project compiles + tests run + baseline metrics emit:
   - `cd /home/jsy/projects/autobuilder-metric-harness && cargo test --no-fail-fast` → expect 17 failures + 1 proptest placeholder pass
   - `/home/jsy/projects/autobuilder/autobuilder/target/release/autobuilder loop --project . --iteration 0 --head-sha "$(git rev-parse HEAD)" --description "verify baseline"` → expect `verdict=baseline`, results.tsv + receipts/ written.
5. Read the PRD: `/home/jsy/.claude/plans/autobuilder-prd-metric-harness.md`.
6. Read the intent-card: `~/.claude/skills/autobuilder/proposals/intake-autobuilder-metric-harness-20260521T000000Z.json`.
7. Begin iterating: read the next failing AC, edit `src/main.rs`, commit, invoke `autobuilder loop` with the new iteration number + head sha + a short description. Repeat.

Source repos (do not re-clone, already present):
- `/home/jsy/projects/autobuilder/autoresearch-macos/`
- `/home/jsy/projects/autobuilder/jankurai/`
- `/home/jsy/projects/autobuilder/jeryu/`
