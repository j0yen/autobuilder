# autobuilder

PRD-driven, rigorously validated Rust code generation. A Claude Code skill plus
a companion Rust binary that turn a Product Requirements Document into a
working Rust project through an autonomous *iterate-and-prove* loop guarded by
a 25-receipt release gate.

`autobuilder` is the synthesis of three opinionated takes on "how do you trust
agent-authored software":

| Source                         | Idea lifted                                                  |
| ------------------------------ | ------------------------------------------------------------ |
| `miolini/autoresearch-macos`   | locked harness + a single *unfakeable* scalar metric         |
| `neverhuman/jankurai`          | repository-local evidence receipts and an anti-pattern catalog |
| `neverhuman/jeryu`             | N-of-N signed proof receipts on a risk gate                  |

The full design is in [`PLAN.md`](./PLAN.md). The first PRD the skill dogfooded
against itself — the *MCP Metadata Optimization Harness* — is in
[`PRD-mcp-metadata-tuner.md`](./PRD-mcp-metadata-tuner.md).

## Install

### Skill only — `bash` + `jq` (covers Stages 1-2)

One-liner — clones the skill into a temp dir, symlinks it into
`~/.claude/skills/autobuilder/`, exits clean:

```sh
curl -fsSL https://raw.githubusercontent.com/j0yen/autobuilder/main/skill/install.sh | bash
```

Or the manual two-step:

```sh
git clone --depth 1 https://github.com/j0yen/autobuilder.git
./autobuilder/skill/install.sh
```

Claude Code picks up the skill on the next session start.
`/autobuilder <PRD-path>` invokes it.

### Full pipeline — skill + companion binary (covers Stages 3-5)

The metric harness, iterate-and-prove loop, risk gate, postmortem,
and evolve subcommands all live in the companion Rust crate. Install:

```sh
git clone --depth 1 https://github.com/j0yen/autobuilder.git
cd autobuilder
./skill/install.sh
cargo install --path autobuilder
```

The binary lands in `~/.cargo/bin/`. The skill's shell scripts shell
out to it when present and fall back gracefully (Stage 1-2 only)
when not.

### Prerequisites

- Stages 1-2: `bash`, `jq`, `git`. Claude Code for the skill itself.
- Stages 3-5: `cargo` / `rustc 1.85+`, `cargo-deny`, `cargo-nextest`,
  optional `cargo +nightly miri` (only when `--allow-unsafe`).

## Repository layout

```
.
├── autobuilder/              # Cargo workspace: the autobuilder companion binary
│   ├── src/                  #   one module per pipeline stage / receipt producer
│   └── crates/metric-harness/#   reusable metric-harness crate
├── agent/                    # canonical agent-state files (intent-card, owner-map, …)
│   ├── intent-card.json
│   ├── owner-map.json
│   ├── proof-lanes.toml
│   └── test-map.json
├── corpora/                  # JSONL eval corpora consumed by metric-harness
├── scripts/run-metrics.sh    # emits autobuilder.metrics.v1 for this repo
├── autoresearch-macos/       # vendored MPS fork of Karpathy's autoresearch
├── jankurai/                 # vendored audit-standard repo
├── jeryu/                    # vendored proof-receipt control plane
├── PLAN.md                   # full skill design
└── PRD-mcp-metadata-tuner.md # the first dogfooded PRD
```

The `autoresearch-macos/`, `jankurai/`, and `jeryu/` directories are upstream
reference material vendored alongside the skill, not part of the autobuilder
crate.

## The pipeline

```
PRD ──► Stage 1: Intake & 5-Whys ──► intent-card.json
         └─► Stage 2: Scaffold (cargo new + locked harness + lints)
              └─► Stage 3: Iterate-and-Prove Loop (advance-or-revert)
                   └─► Stage 4: Risk Gate (7 receipts must agree)
                        └─► Stage 5: Postmortem + Self-Evolve
```

Stage 3 also runs `scripts/run-mutants.sh` (cargo-mutants telemetry, Phase 1)
when the crate has tests: it merges `mutation_kill_rate` and mutant counts into
`metrics.json` to catch tests that pass but cover only the implementation's
happy path. It is telemetry-only today (never blocks); a calibrated kill-rate
gate is a follow-on. See PRD `autobuilder-mutation-testing`.

The **agent edits only `src/`**. Everything else — `Cargo.toml`, `clippy.toml`,
`deny.toml`, `tests/`, `scripts/run-metrics.sh` — is read-only harness,
mirroring autoresearch's `prepare.py`/`train.py` separation. The skill ships
the BAD_RUST audit and risk-gate driver scripts in
`~/.claude/skills/autobuilder/{rules/audit-checks.sh,scripts/risk-gate.sh}`
rather than per-project, so they stay versioned in one place.

### The 7 receipts

Every receipt is a digest-bound JSON object under
`target/autobuilder/receipts/`. The gate only attests that all seven are
present, schema-valid, and bound to the current `HEAD` — each producer owns
its own work and its own digest.

| Receipt          | Schema                                         | Produced by                  |
| ---------------- | ---------------------------------------------- | ---------------------------- |
| `intake`         | `autobuilder.intent_card.v1`                   | `autobuilder intake`         |
| `vti-plan`       | `autobuilder.vti_plan_receipt.v1`              | `autobuilder vti-plan`       |
| `proof-receipt`  | `autobuilder.iteration_receipt.v1`             | `autobuilder loop`           |
| `risk-gate`      | `autobuilder.bad_rust_audit.v1`                | (BAD_RUST audit)             |
| `reviewer-agent` | `autobuilder.reviewer_agent_receipt.v1`        | `autobuilder reviewer-agent` |
| `rollback-plan`  | `autobuilder.rollback_plan_receipt.v1`         | `autobuilder rollback-plan`  |
| `ci-checks`      | `autobuilder.ci_checks_receipt.v1`             | `autobuilder ci-checks`      |

`autobuilder gate` aggregates them into `release-receipt.json` and exits
non-zero on `block`.

## The companion binary

A thin Rust 2024 / `rustc 1.85` binary. Everything load-bearing — intent-card
validation, scaffold materialization, the experiment-loop runner, evidence
writing, the 7-receipt gate, postmortem aggregation, the gated self-evolution
diff — lives here so it does not rot in shell.

### Build

```sh
cd autobuilder
cargo build --release
```

The workspace pins `rustc 1.85.0` (`rust-toolchain.toml`) and applies strict
clippy lints (`unwrap_used`, `expect_used`, `panic`, `unreachable`,
`dbg_macro`, `unsafe_code` — all `deny`).

### Subcommands

```
autobuilder intake          # Stage 1: validate intent-card.json
autobuilder scaffold        # Stage 2: materialize a project from templates/
autobuilder loop            # Stage 3: iterate-and-prove
autobuilder metric-harness  #          run a project's harness, emit metrics.json
autobuilder vti-plan        # Stage 4: route changed paths through proof-lanes.toml
autobuilder rollback-plan   # Stage 4: verify HEAD~N..HEAD is git-revert-clean
autobuilder reviewer-agent  # Stage 4: prepare/finalize the reviewer receipt
autobuilder ci-checks       # Stage 4: confirm CI is green via `gh`
autobuilder gate            # Stage 4: aggregate the 7 receipts → release receipt
autobuilder postmortem      # Stage 5: aggregate run artifacts
autobuilder evolve          # Stage 5: gated skill-self-diff
```

All subcommands are real. The bin has been bootstrapped through its own
gate (verdict=pass) and dogfooded against an external PRD
(`mcp-tuner`, 9 ACs green).

### Dogfooding

`scripts/run-metrics.sh` is the harness for this repo. Its unfakeable scalar
is `stage4_receipt_producers_callable` — how many of the Stage 4 receipt
producers respond on the freshly-built binary. Every acceptance criterion
maps 1:1 to a producer and exercises the producer's actual contract against
a tmp git fixture (writes rollback.md, routes a src/ change with confidence
1.0, blocks ci-checks when no GH run exists for HEAD, etc.) — not just
`--help`. Plus a build/test sanity AC and a digest-roundtrip AC.

```sh
./scripts/run-metrics.sh
cat target/autobuilder/metrics.json
```

## The `autobuilder` skill

`autobuilder` is also a Claude Code skill (see `.claude/`). Invoke it from
inside a Claude Code session with a PRD path:

```
/autobuilder --prd path/to/prd.md
```

…and the skill drives all five stages, leaving every receipt under
`target/autobuilder/receipts/` for human review.

## Distribution / publishing

When a slice passes the gate and is ready to share, the convention is to
publish it as its own GitHub repo at `github.com/j0yen/<slug>` rather than
import it into a monorepo. See [Stage 6 — Publish](./skill/SKILL.md#stage-6--publish-per-project-repo)
in the skill doc for the per-slice steps. The wider ecosystem is indexed
in [`j0yen/wintermute`'s REPOS.md](https://github.com/j0yen/wintermute/blob/master/REPOS.md);
its `bootstrap/install.sh` clones each published slice on a fresh machine.

## Recent

- **v0.2.0** (2026-05-30): added `autobuilder publish` subcommand — codifies the
  Stage-6 publish pipeline (README/LICENSE generation, branch normalize, `wm-publish`
  repo create, `wm-push`, `REPOS.md` update) into a deterministic, idempotent,
  dry-run-capable command (PRD-autobuilder-publish, ACs 1–9 green).

## License

Dual-licensed under MIT OR Apache-2.0.
