---
name: autobuilder
description: PRD-driven, rigorously validated Rust code generation. Use when the user wants to build a Rust CLI or library from a Product Requirements Document under an autonomous iterate-and-prove loop with structured receipts and a 25-receipt release gate. Synthesizes autoresearch's locked-harness model, jankurai's anti-pattern catalog, and jeryu's proof-receipt gate into one pipeline.
---

# autobuilder

## What this skill does

Takes a PRD (file path or pasted text) and drives a 5-stage pipeline that yields a Rust project where every artifact is (a) generated from a structured `intent-card.json` derived via 4/5-Whys, (b) iterated under a narrow falsifiable advance-or-revert loop, (c) accompanied by per-iteration `EvidencePack` receipts and `FailureCapsule`s on crash, (d) gated by 7 receipts before declaring "ready," and (e) fed into a postmortem that queues self-improvement proposals.

## When to invoke

Invoke when the user:
- Hands you a PRD and asks for a Rust project (CLI or library).
- Says "build me a Rust X" with concrete acceptance criteria.
- Asks to dogfood autobuilder against one of its own sub-tools.

Do NOT invoke for:
- Greenfield non-Rust projects.
- Surgical edits to an existing Rust crate (use direct tools).
- Web services, WASM, or embedded targets (v1 is `--target cli|lib` only).

## Resolved decisions (locked 2026-05-21)

1. **Skill + companion Rust binary.** Skill orchestrates Claude subagents; the companion `autobuilder` binary (Cargo workspace at `../autobuilder/` in the repo, installed via `cargo install --path autobuilder`) owns the metric harness, receipt writing, risk gate, and experiment-loop runner. Skill shells out to the binary. Binary is itself dogfooded.
2. **Target scope: CLIs + library crates.** `--target cli` or `--target lib`. For libs add cargo-semver-checks, docs-coverage, `cargo public-api` diff. Service/WASM/embedded → v2.
3. **Hybrid autonomy.** Loop and risk gate run fully autonomous. Human checkpoint only when the agent wants to add/relax a MUST acceptance criterion, widen hard constraints, or modify the skill itself. Trigger via `intent_card_amendment_request.json`.
4. **First PRD is the metric harness.** Build `autobuilder-metric-harness` (input: project path; output: normalized `metrics.json`) before throwing external PRDs at autobuilder.

## Pipeline

```
PRD ──► Stage 1: Intake (5-Whys)         ──► intent-card.json
        Stage 2: Scaffold (locked harness) ──► <project>/ tree
        Stage 3: Iterate-and-Prove loop    ──► EvidencePack per iter, FailureCapsule on crash
        Stage 4: Risk Gate (7 receipts)    ──► ready / blocked + diagnostic
        Stage 5: Postmortem + Self-Evolve  ──► gated proposal queued for review
```

### Stage 1 — Intake (5-Whys)

Run `prompts/prd-intake-5whys.md` against the PRD. Output validates against `schemas/intent-card.schema.json`. Refuse to proceed if ambiguity remains after 5 Whys — surface what's missing and ask the user.

### Stage 2 — Scaffold

Generate a project where harness is read-only, agent edits only `src/`:

```
<project>/
├── Cargo.toml, clippy.toml, deny.toml, rust-toolchain.toml
├── src/                                  ← agent edits ONLY this
├── tests/acceptance_*.rs                 ← one test per AC (read-only)
├── tests/proptest_invariants.rs          ← read-only
├── tests/fuzz/                           ← cargo-fuzz harness
├── scripts/run-metrics.sh                ← read-only; emits metrics.json
├── scripts/audit.sh                      ← read-only; BAD_RUST scan
├── scripts/risk-gate.sh                  ← read-only; checks 7 receipts
├── agent/
│   ├── AUTOBUILDER_PROGRAM.md            ← autoresearch-style instructions
│   ├── intent-card.json
│   ├── owner-map.json
│   ├── test-map.json
│   └── proof-lanes.toml
├── target/autobuilder/
│   ├── receipts/<sha>.json               ← EvidencePack per iteration
│   ├── failure-capsules/                 ← FailureCapsule per crash
│   ├── results.tsv
│   └── postmortem.md
└── .github/workflows/                    ← CI mirror of local gate
```

### Stage 3 — Iterate-and-Prove

```
LOOP UNTIL all-MUST-ACs-green AND risk-gate-passes OR budget-exhausted:
  1. git state check; branch is autobuilder/<intent-slug>
  2. Edit src/ ONLY (edit-agent generates the diff)
  3. git commit -m "iter-<n>: <hypothesis>"
  4. scripts/run-metrics.sh > target/autobuilder/run.log 2>&1
  5. Parse target/autobuilder/metrics.json
  6. If crash: tail -n 50 run.log; ≤3 fix attempts; else FailureCapsule + status=crash
  7. Append to results.tsv: <sha> <quality_score> <ac_passing> <status> <description>
  8. Advance if: all hard gates pass AND quality_score improved AND no MUST-AC regression
     Else: git reset --hard HEAD~1; status=discard
  9. Emit EvidencePack JSON for the iteration
 10. (Optional) Adversarial sub-step: spawn the adversarial-agent
     (prompts/adversarial-agent.md) to write tests/adversarial_<id>.rs
     attempting to falsify the AC against its English description, not
     the implementation. If any adversarial test fails on the
     implementation, downgrade verdict from advance to concern and
     surface the failure to the next edit-agent iteration. Closes the
     "edit-agent wrote both impl and test" tautology gap.
```

Hard gates (all must pass to advance):
- `cargo check --workspace`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `cargo deny check`
- `cargo +nightly miri test` (when `--allow-unsafe`)
- BAD_RUST audit scan (`rules/bad-rust.md` + `rules/audit-checks.sh`)
- Proof-lane routing: every changed path resolves to ≥1 lane, all lanes green

Quality score (drives advance/revert tiebreak):
```
score = 10*ac_passing_count
      +  3*test_coverage_pct
      +  2*proptest_density
      +  1*doc_coverage_pct
      -  2*audit_findings_count
      -  1*clippy_warning_count
```

### Stage 4 — Risk Gate (7 receipts)

| Receipt | Source | Pass condition |
|---|---|---|
| `intake` | Stage 1 | `intent-card.json` validates; all MUST-ACs declared |
| `vti-plan` | Stage 2/3 | every changed path routed via `proof-lanes.toml`; confidence ≥ 0.70 |
| `proof-receipt` | Stage 3 | test/proptest/fuzz/miri/deny green on `HEAD` |
| `risk-gate` | Stage 3 | BAD_RUST audit clean (or only `advisory` findings with waivers) |
| `reviewer-agent` | sub-agent | independent Claude review of `HEAD~N..HEAD` vs intent-card; ∈ `{pass, concern, block}` |
| `rollback-plan` | Stage 2 | every commit `git revert`-clean; steps in `target/autobuilder/rollback.md` |
| `ci-checks` | Stage 2 | `.github/workflows/` green on a fresh worktree clone |

Missing receipts → block + machine-readable diagnostic. No self-approval.

### Stage 5 — Postmortem & Self-Evolve

`target/autobuilder/postmortem.md` summarizes the run. A run-level `evolution-proposal.json` queues in `~/.claude/skills/autobuilder/proposals/`. `autobuilder evolve` aggregates across the last K runs and emits a diff against `SKILL.md` / `rules/bad-rust.md` / `templates/scaffold/`.

**Auto-apply (default).** Each `Suggestion` is append-only by construction. `evolve` writes the appended lines to the target file in the skill tree, commits the change in the skill_root git repo when present (one commit per suggestion, message `evolve: <rationale>`), and records `applied-suggestion:<sha256-of-target-and-appended-lines>` in `proposals/applied.log` so the same suggestion does not re-emit on subsequent runs.

**Template-drift auto-apply (pure-additions only).** Postmortem captures `template_diffs` for every project script that has diverged from `templates/scaffold/scripts/*`. When the same diff body appears across ≥2 distinct slugs, evolve groups them and inspects the diff direction: a pure-additions diff (no `-` lines in hunk bodies) unambiguously means projects added content the template lacks → promoted to a `PatchSuggestion` and auto-applied via `patch --dry-run` guard + `patch -p0` + git commit. Anything with `-` lines is direction-ambiguous and surfaces as a `TemplateDriftAdvisory` for manual review.

Use `evolve --dry-run` to inspect both suggestion types without applying.

**Manual rejection still supported.** Add a basename to `applied.log` with a `#REJECTED:` comment block (existing convention) to suppress the source proposal entirely. Use this for suggestions whose underlying issue should be resolved elsewhere rather than by appending to the skill.

### Stage 6 — Publish (per-project repo)

When the slice passes the gate and the user wants to distribute it, the slice becomes its own GitHub repo under `github.com/j0yen/<slug>` rather than being squash-imported into a monorepo. (This replaces the older "Import into `~/wintermute/`" convention; see `j0yen/wintermute`'s [`REPOS.md`](https://github.com/j0yen/wintermute/blob/master/REPOS.md) for the live index and `bootstrap/install.sh` for the bootstrap installer that picks up new repos.)

Per-slice steps:

1. **README + LICENSE.** Generate `README.md` from `agent/intent-card.json` (root_motivation as overview, MUST-level acceptance criteria as the AC list). Drop dual `LICENSE-MIT` + `LICENSE-APACHE` files into the repo root.
2. **Rename branch to `main`.** The autobuilder scaffold uses `autobuilder/<slug>` as the working branch. Before publishing: delete any stale `main` (the iter-0 scaffold baseline) and rename `autobuilder/<slug>` → `main`. Skip this if you want to preserve the autobuilder branch name on GitHub.
3. **Commit + push.** Single commit "Prep for standalone distribution: README + dual MIT/Apache-2.0 license" using the `Joe Yen <jyen.tech@gmail.com>` identity for wintermute-ecosystem repos, then `gh repo create j0yen/<slug> --public --source . --remote origin --push`.
4. **Update wintermute's `REPOS.md`** with a one-line description and category (pipeline/runtime/memory/session/artist). The bootstrap installer will then pick up the new repo on next `install.sh` run.

The autobuilder companion binary does not yet automate Stage 6; it is a manual convention. A future `autobuilder publish` subcommand may codify this.

## Reused skills

- `/loop` — long-running experiment cadence.
- `/verify` — final end-to-end app-run check (Stage 4).
- `/code-review` — `reviewer-agent` receipt.

## Local tool integration

The companion binary owns the inner loop, but Claude running this skill
manually (or extending the binary) should reach for these wrappers
rather than rolling equivalents:

- **`wchg watch <project>`** at Stage 2 scaffold-complete; **`wchg
  since <project>`** between iterations as a write-scope guard. Stage 3
  promises the agent edits only `src/`. If `wchg since` surfaces a
  write outside `src/` (or outside the target/autobuilder/ receipts
  dir), that's an escape: emit a `FailureCapsule` with type
  `scope_escape`, revert via git, and don't count it as an iteration.
  Catches a class of failures git status would miss because the diff
  might also be reverted by the test command itself.
- **`sbx --no-net --profile pwd -- <cmd>`** wrapping
  `scripts/run-metrics.sh` and any `cargo test`/`cargo run` invocations
  in Stage 3. PRD-generated test code is untrusted-by-construction;
  isolating its network + write surface is the right default. Add
  `--bind <project>/target` so cargo's incremental build cache
  persists across iterations. Skip `--no-net` only if a specific
  acceptance test explicitly requires network egress (rare).
- **`txn-edit snap`** around the Stage 3 edit-agent diff application
  when the agent touches >1 file in one iteration. `git reset --hard
  HEAD~1` (the current revert path) loses partial progress on the
  iteration; `txn-edit rollback` preserves the diff for the next
  edit-agent prompt to learn from.
- **`procstat snap <cargo-pid>`** during long Stage 3 iterations.
  Capture peak `vm_rss_bytes` and `io_write_bytes` into the
  EvidencePack — a regression in resource cost is a quality signal the
  current score function doesn't reward but matters for tools shipped
  to `~/.local/bin/`.
- **`pevent run <cargo test --release>`** for the Stage 4
  full-test-suite verification when `cargo test` is expected to run
  longer than a single conversational turn. The pevent record survives
  Claude session boundaries; later turns retrieve the verdict via
  `pevent wait`/`pevent log` rather than re-running the suite.
- **`ctrace`** is already enabled at the session level (SessionStart
  hook). Stage 5 postmortem should grep `~/.cache/ctrace/sessions/` for
  the iteration's ndjson and aggregate openat/execve counts as a
  ground-truth complement to the harness's self-reported metrics.

## Layout

```
~/.claude/skills/autobuilder/
├── SKILL.md                              ← this file
├── prompts/
│   ├── prd-intake-5whys.md
│   ├── reviewer-agent.md
│   ├── edit-agent.md
│   ├── postmortem-writer.md
│   └── evolve.md
├── templates/
│   ├── scaffold/                         ← project skeleton
│   └── AUTOBUILDER_PROGRAM.md.tmpl
├── rules/
│   ├── bad-rust.md                       ← curated subset lifted from jankurai
│   ├── hlt-rules.toml                    ← HLT-* IDs we adopt
│   └── audit-checks.sh                   ← grep + clippy-restriction implementations
├── schemas/
│   ├── intent-card.schema.json
│   ├── evidence-pack.schema.json
│   ├── failure-capsule.schema.json
│   ├── proof-receipt.schema.json
│   └── merge-witness.schema.json
├── scripts/
│   ├── intake.sh
│   ├── scaffold.sh
│   ├── experiment-loop.sh
│   ├── metric-harness.sh
│   ├── risk-gate.sh
│   ├── postmortem.sh
│   └── evolve.sh
└── proposals/                            ← accumulated evolution proposals (gated)
```

## Reference repos (vendored in this repo as read-only context)

The autobuilder repo ships three sibling research trees as upstream
reference material. They live alongside the skill in any checkout of
`j0yen/autobuilder`:

- `autoresearch-macos/` — locked-harness loop model
- `jankurai/` — anti-pattern catalog, HLT rule IDs, proof-lane format
- `jeryu/` — 7-receipt gate, EvidencePack, FailureCapsule, cargo-witness/vrc/aer crates

When invoked outside a repo checkout (skill installed via curl|bash
sparse-clone), these dirs are absent. The skill still functions for
Stages 1-2; Stages 3-5 expect the companion binary, whose own crates
embed the parts of these repos it depends on.

## Status

**v0.1 — Phase A in progress.** Schemas, rules, prompts being scaffolded. The Rust binary (Phase B) and the first meta-PRD run (Phase C) follow.

## Known block — recall-memory-linter

Reviewer flagged: 1, 7.
Investigate the underlying cause and either fix the implementation
or amend the intent-card if the AC was wrong. Re-run the gate before
shipping.
