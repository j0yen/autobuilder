# autobuilder

Turn a Product Requirements Document into a working Rust project, through an autonomous *iterate-and-prove* loop guarded by a release gate that won't pass until seven independent receipts agree.

The hard part of letting an agent write code is not generating it — it's knowing when to trust what came out. autobuilder's answer is to separate the writing from the proving. The agent edits only `src/`; the harness around it — `Cargo.toml`, lints, tests, the metric script — is read-only, so the thing being measured can't rewrite its own measurement. A change advances only if it improves an unfakeable scalar metric, and a slice ships only when seven separately-produced, digest-bound receipts all attest to the same `HEAD`. The structure is the argument: nothing here asks you to trust the agent's word.

It comes in two pieces — a Claude Code skill that orchestrates the five stages, and a companion Rust binary that owns the parts too consequential to leave in shell (intent-card validation, the loop runner, receipt writing, the gate).

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

### Full install — skill + all tools + companion binary

```sh
curl -fsSL https://raw.githubusercontent.com/j0yen/autobuilder/main/skill/install.sh | bash
```

Or manually:
```sh
git clone --depth 1 https://github.com/j0yen/autobuilder.git
./autobuilder/skill/install.sh
```

When `cargo` is on your PATH, `install.sh` automatically builds and installs:
- The 4 companion CLIs (`autobuilder-ac-counter`, `autobuilder-bincov-receipt`,
  `autobuilder-harness-portability-audit`, `autobuilder-proposal-aggregator`)
- The pipeline companion binary (`autobuilder`)

Without `cargo`, the skill still works for Stages 1–2.

### Prerequisites

- Stages 1-2: `bash`, `jq`, `git`. Claude Code for the skill itself.
- Stages 3-5: `cargo` / `rustc 1.85+`, `cargo-deny`, `cargo-nextest`,
  optional `cargo +nightly miri` (only when `--allow-unsafe`).

## Tools

Four standalone Rust CLIs shipped alongside the skill. Built and installed
automatically by `install.sh` when `cargo` is present; also installable
individually via `cargo install --path tools/<name>`.

| Binary | What it does |
|--------|-------------|
| `autobuilder-ac-counter` | Counts acceptance criteria correctly across split-file (`acceptance_*.rs`), monolithic (`acceptance.rs` with `ac\|new_ac\|ext` families), and mock (`tests/mocks/`) layouts. Fixes the `run-metrics.sh` undercount. |
| `autobuilder-bincov-receipt` | Detects `[[bin]]` crates that ship a binary but have no `tests/integration_cli.rs` driving it via `std::process::Command`. Emits a `bincov.v1` receipt; `--strict` exits 3. |
| `autobuilder-harness-portability-audit` | Scans harness scripts for Linux-only idioms (`nproc`, `/proc/`, `flock`, `date -d`, `readlink -f`, `sed -i`, `stat -c`) and reports macOS-equivalent suggestions. Draft-only; `--strict` exits 4. |
| `autobuilder-proposal-aggregator` | Clusters the `proposals/*.json` pile by `target_file` + lexical-Jaccard rationale similarity, ranks by distinct-crate recurrence, filters `applied.log`. Emits `hardening-backlog.json`. |

Source for each tool lives under `tools/<name>/` in this repo.

## Repository layout

```
.
├── autobuilder/              # Cargo workspace: the autobuilder companion binary
│   ├── src/                  #   one module per pipeline stage / receipt producer
│   └── crates/metric-harness/#   reusable metric-harness crate
├── tools/                    # standalone Rust CLIs (built by install.sh)
│   ├── autobuilder-ac-counter/
│   ├── autobuilder-bincov-receipt/
│   ├── autobuilder-harness-portability-audit/
│   └── autobuilder-proposal-aggregator/
├── agent/                    # canonical agent-state files (intent-card, owner-map, …)
│   ├── intent-card.json
│   ├── owner-map.json
│   ├── proof-lanes.toml
│   └── test-map.json
├── corpora/                  # JSONL eval corpora consumed by metric-harness
├── scripts/run-metrics.sh    # emits autobuilder.metrics.v1 for this repo
└── PLAN.md                   # full skill design
```

PRDs (the inputs this pipeline consumes) live in the private companion
repo `joeyen-atscale/autobuilder-private`, not here.

The ideas `autobuilder` synthesizes were lifted from three upstream
repositories: [`miolini/autoresearch-macos`](https://github.com/miolini/autoresearch-macos)
(locked harness + a single unfakeable scalar metric), `neverhuman/jankurai`
(repository-local evidence receipts and an anti-pattern catalog), and
`neverhuman/jeryu` (N-of-N signed proof receipts on a risk gate). They were
previously vendored into this tree for reference but have been removed: the
relevant shapes are already translated into the skill files (provenance is noted
inline, e.g. "Lifted from `jankurai/agent/JANKURAI_STANDARD.md`"). Clone the
upstreams directly if you need the originals.

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
autobuilder experiment      # Stage 2.5: drive a multi-slice campaign from experiment.toml
autobuilder loop            # Stage 3: iterate-and-prove
autobuilder metric-harness  #          run a project's harness, emit metrics.json
autobuilder adversarial     # Stage 3: prepare/finalize an adversarial-agent test slot
autobuilder vti-plan        # Stage 4: route changed paths through proof-lanes.toml
autobuilder rollback-plan   # Stage 4: verify HEAD~N..HEAD is git-revert-clean
autobuilder reviewer-agent  # Stage 4: prepare/finalize the reviewer receipt
autobuilder ci-checks       # Stage 4: confirm CI is green via `gh`
autobuilder gate            # Stage 4: aggregate the 7 receipts → release receipt
autobuilder postmortem      # Stage 5: aggregate run artifacts
autobuilder evolve          # Stage 5: gated skill-self-diff
autobuilder publish         # Stage 6: publish a finished slice as its own j0yen/<slug> repo
```

Every subcommand is wired and answers `--help`; the binary builds clean under
the strict lint set below. The binary is dogfooded — it runs against its own
repo through `scripts/run-metrics.sh` (see [Dogfooding](#dogfooding)).

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

The skill source lives under `skill/` (`SKILL.md` plus its prompts, rules,
schemas, scripts, and templates); `install.sh` symlinks it into
`~/.claude/skills/autobuilder/`. Invoke it from inside a Claude Code session
with a PRD path:

```
/autobuilder path/to/prd.md
```

The skill drives the stages, shelling out to the companion binary for the
load-bearing work, and leaves every receipt under
`target/autobuilder/receipts/` for review.

## Distribution / publishing

When a slice passes the gate and is ready to share, the convention is to
publish it as its own GitHub repo at `github.com/j0yen/<slug>` rather than
import it into a monorepo. See [Stage 6 — Publish](./skill/SKILL.md#stage-6--publish-per-project-repo)
in the skill doc for the per-slice steps. The wider ecosystem is indexed
in [`j0yen/wintermute`'s REPOS.md](https://github.com/j0yen/wintermute/blob/master/REPOS.md);
its `bootstrap/install.sh` clones each published slice on a fresh machine.

## Recent

- **`autobuilder publish`** codifies the Stage-6 publish step — README/LICENSE
  generation, branch normalize, repo create, push, `REPOS.md` update — as one
  idempotent, dry-run-capable command, so publishing a slice stops being a
  hand-run checklist. See [`autobuilder/CHANGELOG.md`](./autobuilder/CHANGELOG.md)
  for the version history.

## License

MIT licensed. See [`LICENSE`](./LICENSE).
