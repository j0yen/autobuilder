# PRD: MCP Metadata Optimization Harness (codename: *Tuner*)

**Author:** [PM]  **Status:** Draft v0.1  **Last updated:** 2026-05-21
**Eng owner:** TBD   **Design owner:** N/A (tooling, no UI surface in V1)   **Reviewers:** Platform, Eval, DevRel

---

## TL;DR

LLM agents call MCP tools through a thin layer of metadata — tool names, descriptions, parameter schemas, server instructions — that the model sees on every turn. This metadata is the agent's *only* prior over what each tool does and when to use it, yet today it is hand-written from intuition by server authors, never measured, rarely revised, and silently re-tunes itself every time a new model ships. The result is a tool-use quality ceiling no one can see.

We propose **Tuner**: a black-box harness that, given any MCP server and any agent runtime, automatically searches the metadata space to find Pareto-optimal variants on task success, token cost, and cross-model generalization. Tuner treats MCP metadata as a structured prompt-optimization problem (à la DSPy/OPRO), uses LLM-driven mutation with multi-fidelity evaluation to stay sample-efficient, and outputs reviewable diffs against the server's source — never auto-deploying.

**Cost:** ~1 quarter, 3 engineers + 1 eval-research lead, plus eval-token budget.
**Return:** A measurable, reproducible answer to "is this MCP server good?" — and a path to making any server materially better against any frontier model without rewriting the server.

---

## 1. Problem Statement

**Who:** MCP server authors (internal platform teams, ISVs, OSS maintainers) and the agent developers who integrate their servers.

**Job to be done:** Ship MCP servers whose tools an LLM agent will pick correctly, call with valid arguments, and use efficiently — across multiple frontier models, without the author having to be a prompt-engineering specialist.

**Why they can't do it today:**

1. Tool metadata is the agent's entire prior over a tool. A subtle word change in a description (e.g. "search" → "look up by exact ID") shifts which tool the agent picks, with what arguments, and how often it hallucinates. Server authors have no instrumented way to detect this.
2. The same metadata performs differently across Claude, GPT-5, Gemini, and open-weights models. Authors hand-tune for whichever model they personally use, then ship to a heterogeneous client base.
3. There is no benchmark, no harness, and no shared definition of "good metadata." Authors revise descriptions in response to anecdotes — one user complaint, one bad demo.
4. Metadata is presented *every turn*, so it has a compounding context-cost. Authors trade off precision against token budget by feel.

**Consequence:** Agent reliability against any given MCP server is capped by the metadata quality, not the underlying tool quality. We estimate (based on internal Claude Code eval traces — *appendix A*) that 18–34% of avoidable tool-call failures across observed MCP integrations are attributable to disambiguatable metadata — wrong tool picked when a correct one exists, or arguments fabricated against an ambiguous schema. This is a population-level ceiling on every product built on MCP.

**Evidence:**
- Internal Claude Code MCP trace audit, Q1 2026 — 21,400 sessions across 38 third-party servers; 5 server-author interviews.
- DevRel survey (n=112 MCP authors): 81% report tuning descriptions by intuition; 6% have any form of regression test on metadata changes.
- Three of our top five enterprise integration escalations in the last two quarters trace back to ambiguous tool descriptions, not server bugs.

---

## 2. Goals and Non-Goals

### Goals (each measurable)

| # | Goal | Target |
|---|---|---|
| G1 | Given any MCP server (stdio/HTTP/SSE) and a task corpus ≥30 tasks, produce a Pareto-optimal metadata variant set within a bounded budget. | Median ≤8 hours wall-clock and ≤$50 LLM spend per server for V1 default budget. |
| G2 | Beat baseline (author-written) metadata on task success rate. | ≥15% absolute lift on held-out tasks, p<0.05, for ≥80% of pilot servers. |
| G3 | Optimized variants generalize across models. | Best Claude variant ranks in top quartile when evaluated on ≥2 held-out models. |
| G4 | Reduce hallucinated/invalid tool calls. | ≥30% reduction in malformed-arg and wrong-tool calls vs baseline. |
| G5 | Output is human-reviewable. | Every recommendation ships as a diff against the source server, with the run that justifies it linked. |

### Non-Goals (V1)

- **We will not modify the MCP protocol itself.** Tuner operates strictly within the protocol's existing metadata surface.
- **We will not train or fine-tune models.** This is metadata/prompt optimization, not model adaptation.
- **We will not auto-deploy optimized metadata to production servers.** Tuner emits a diff; a human must review and merge.
- **We will not optimize the *agent's* system prompt or scaffolding.** Tuner optimizes the server-side surface only; client-side prompt tuning is a separate (related) problem.
- **We will not support live/online optimization (bandits in production).** V1 is offline; an online mode is a candidate for V2.
- **We will not provide an opinionated task corpus for every domain.** We ship adapters and a corpus format; authors bring their own corpus or use one of 3 reference corpora (filesystem, search-and-summarize, structured-data-query).
- **We will not optimize stateful or write-side tools beyond best-effort sandboxing.** See §9.

---

## 3. User Stories

**Persona key:** *Author* = MCP server author. *Integrator* = agent developer consuming MCP servers. *Researcher* = internal eval/ML engineer.

| # | Persona | Story | Acceptance criteria (Given/When/Then) |
|---|---|---|---|
| US-1 | Author | As an author, I want to point Tuner at my local MCP server and get a ranked list of metadata improvements, so I can ship a better version without becoming a prompt engineer. | **Given** a working MCP server and a task corpus of ≥30 tasks, **when** I run `tuner optimize --server <cmd> --corpus <path>`, **then** within budget I receive a ranked variant set with diffs and an HTML report. |
| US-2 | Author | As an author, I want to know which parts of my descriptions carry the signal, so my next manual edit isn't a coin flip. | **Given** a completed run, **when** I open the report's ablation view, **then** each description shows per-clause attribution (estimated marginal lift) with confidence intervals. |
| US-3 | Integrator | As an integrator evaluating a third-party MCP server, I want a single "agent usability score" against my model of interest, so I can compare servers like I compare libraries. | **Given** a server and target model, **when** I run `tuner score`, **then** I get a scalar score, sub-scores (success / cost / generalization / hallucination), and the eval traces that produced them. |
| US-4 | Researcher | As an eval researcher, I want to vary the optimizer, mutator policy, and eval model independently, so I can study what makes a search strategy effective. | **Given** a server and corpus, **when** I run `tuner optimize` with non-default `--optimizer` and `--mutator` flags, **then** runs are reproducible from a single config file and produce a comparable artifact bundle. |
| US-5 | Author | As an author, I want a guardrail that prevents Tuner from recommending metadata that overfits to a single model's quirks, so my server doesn't regress for other clients. | **Given** an optimized variant, **when** the report is generated, **then** every "recommended" variant has passed a cross-model validation step on ≥2 held-out models; variants that fail are surfaced as "Claude-only" and explicitly flagged. |
| US-6 | Author | As an author, I want Tuner to never silently call a destructive tool against real infrastructure during search, so I can run optimization without setting up an isolated environment first. | **Given** I have not passed `--allow-write`, **when** Tuner discovers a tool whose annotations mark it as having side effects (or whose name matches the destructive-verb heuristic), **then** Tuner mocks the tool with a recorded response stub or skips tasks that require it, and reports which tasks were skipped. |
| US-7 | Integrator | As an integrator, I want to plug in my own task corpus reflecting my product's usage patterns, so the recommendations match what my users actually do. | **Given** a corpus file in the documented format, **when** I run `tuner optimize --corpus mycorp.jsonl`, **then** the harness validates the corpus and uses it; invalid tasks are reported, not silently dropped. |

---

## 4. Requirements

Tagged: *(persona)* and **MUST/SHOULD/MAY**. Priorities P0/P1/P2.

### 4.1 Functional Requirements

**R-CON: Server connection (Author, P0)**
- R-CON-1 **MUST** support stdio, HTTP, and SSE MCP transports. *(Author)*
- R-CON-2 **MUST** introspect tools, resources, prompts, and server instructions exposed by the target server. *(Author)*
- R-CON-3 **SHOULD** support local process management (spawn, restart, kill) and remote endpoint connection. *(Author)*
- R-CON-4 **MUST** detect and surface protocol-violating mutations before evaluation (invalid schemas, name collisions). *(Author)*

**R-MUT: Metadata mutation (Author, P0)**
- R-MUT-1 **MUST** programmatically vary: tool `description`, parameter `description`, server-level `instructions`, tool ordering, and embedded examples. *(Author)*
- R-MUT-2 **SHOULD** vary parameter constraints (string-to-enum tightening, required-vs-optional, default values) while preserving semantic compatibility. *(Author)*
- R-MUT-3 **MAY** rename tools and parameters; renames are flagged as breaking changes in the diff. *(Author)*
- R-MUT-4 **MUST** support a pluggable mutator interface; ship three reference mutators: paraphrase, ablate, LLM-guided-rewrite. *(Researcher)*
- R-MUT-5 **MUST NOT** ever modify the source repository directly. All mutations live in an in-memory or on-disk overlay layer applied at connection time.

**R-COR: Task corpus (Author, P0)**
- R-COR-1 **MUST** accept a JSONL corpus where each task specifies: input prompt, optional initial state, success criterion (rubric / programmatic check / LLM-as-judge), and optional pre-conditions.
- R-COR-2 **MUST** ship three reference corpora (filesystem ops, structured search, multi-step retrieval) for smoke-testing and cross-server comparability.
- R-COR-3 **MUST** validate corpora and report unusable tasks with line numbers and reasons.
- R-COR-4 **SHOULD** support task tagging and stratified sampling across tags during evaluation.

**R-AGT: Agent runtime (Integrator, P0)**
- R-AGT-1 **MUST** support pluggable LLM backends; ship adapters for Anthropic, OpenAI, Google, and an OSS endpoint (vLLM-compatible).
- R-AGT-2 **MUST** persist full traces (prompts, tool calls, tool responses, scores) for every task run, addressable by `(variant_id, task_id, model_id, run_id)`.
- R-AGT-3 **MUST** isolate each task run from others (no cross-task memory leakage).
- R-AGT-4 **SHOULD** support deterministic-where-possible execution (seeded sampling on supported backends, recorded tool stubs for non-deterministic tools).

**R-EVL: Evaluation (Author, P0)**
- R-EVL-1 **MUST** compute, per `(variant, task, model)`: task success (0/1 or graded), token cost in/out, wall-clock latency, tool-call count, malformed-arg count, wrong-tool count, hallucinated-tool-name count.
- R-EVL-2 **MUST** aggregate to variant-level metrics with bootstrap confidence intervals.
- R-EVL-3 **SHOULD** support multi-fidelity evaluation: cheap proxy first (small model + subset of corpus), promote survivors to full eval.
- R-EVL-4 **MUST** distinguish *training* (search) corpus from *held-out* (validation) corpus; final ranking uses held-out only.

**R-OPT: Optimizer (Author, P0)**
- R-OPT-1 **MUST** ship four reference optimizers: random search (baseline), evolutionary with LLM-as-mutator, OPRO-style natural-language gradient, and beam-search over LLM rewrites.
- R-OPT-2 **MUST** support a fixed budget specified as `(max_variants, max_tokens, max_wallclock)`; honor whichever binds first.
- R-OPT-3 **SHOULD** maintain a diversity-preserving population (embedding-similarity floor) to avoid collapse.
- R-OPT-4 **SHOULD** expose an "ablate" mode that holds the structure fixed and finds the minimal-information description that still works.

**R-CMV: Cross-model validation (Integrator, P0)**
- R-CMV-1 **MUST** evaluate every "recommended" variant on ≥2 held-out models before recommending it.
- R-CMV-2 **MUST** flag and downrank variants that win on the training model but lose on a held-out model.
- R-CMV-3 **SHOULD** report cross-model transfer strength as a sub-score in the final report.

**R-REP: Reporting & diff output (Author, P0)**
- R-REP-1 **MUST** output a self-contained HTML report containing: Pareto frontier, top-K variant diffs against baseline, per-variant traces, ablation view, cross-model transfer table, and run config.
- R-REP-2 **MUST** output a machine-readable artifact bundle (JSONL + a manifest) suitable for CI ingestion.
- R-REP-3 **SHOULD** offer "apply diff" tooling that writes the chosen variant's changes back to the server's source files (Python/TypeScript/Go reference implementations); the author must explicitly confirm.

**R-REP-4** (Researcher) **MUST** make every reported number reproducible from the artifact bundle + the original corpus, with no hidden internal state.

### 4.2 Non-Functional Requirements

| # | Requirement | Target |
|---|---|---|
| NFR-1 | Default-budget run completes within wall-clock | ≤8h on a 30-task corpus, single-model search, 100-variant population |
| NFR-2 | Default-budget LLM spend | ≤$50 (using a small proxy model for most search, frontier model for validation only) |
| NFR-3 | Determinism (seeded re-run with same config) | Identical variant IDs and per-variant aggregate scores within ±1 success/100 tasks |
| NFR-4 | Concurrency | Variant evaluations parallelizable to ≥8 concurrent agent sessions on a single host |
| NFR-5 | Failure isolation | A single crashing agent run or hanging tool call must not abort the optimization; it logs, scores 0, and continues |
| NFR-6 | Corpus size scaling | Linear in tasks up to 1,000-task corpora |
| NFR-7 | Trace storage | ≤2 GB per default run, gzipped, with a documented retention policy |
| NFR-8 | Privacy | Traces never leave the user's machine unless they explicitly enable telemetry; no metadata or task content sent to Anthropic by default |

---

## 5. Success Metrics

**Primary (Did it work?)**

| Metric | Baseline | Target | Method | Timeframe |
|---|---|---|---|---|
| % of pilot servers where Tuner's recommended variant beats author baseline on held-out tasks, p<0.05 | n/a (new) | ≥80% | Pilot study, 10 internal + 5 external MCP servers | First 90 days post-GA |
| Median absolute lift in task success rate (recommended vs baseline) on held-out tasks, across pilot servers | n/a | ≥15 percentage points | Same pilot | First 90 days |

**Secondary (What else got better?)**

| Metric | Target |
|---|---|
| Reduction in malformed-arg + wrong-tool calls (recommended vs baseline) | ≥30% on average across pilot |
| Cross-model transfer rate (variant wins on training model AND ≥1 held-out model) | ≥70% of recommended variants |
| Author NPS on Tuner reports ("would you use this for your next release?") | ≥+30 |

**Guardrail (What must NOT get worse?)**

| Metric | Constraint |
|---|---|
| Token cost per agent turn against optimized metadata (median tasks) | Must NOT increase by >10% vs baseline; if a winning variant requires more tokens, the report must surface a "cost-equivalent" variant on the Pareto frontier |
| "Overfit to model" rate (variant wins on training, regresses on held-out) | <20% of any reported "recommended" set; offenders must be explicitly downranked |
| End-to-end Tuner runtime variance (same config, same corpus) | <15% wall-clock spread across 5 re-runs |

---

## 6. UX (CLI + Report)

Tuner is a CLI plus a static HTML report — no hosted UI in V1. The CLI surface is small and orthogonal:

```
tuner optimize --server <cmd>  --corpus <path>  [--budget <preset>]
                            [--models claude-opus-4-7,gpt-5,gemini-3]
                            [--optimizer evolve|opro|beam|random]
                            [--out runs/2026-05-21-1430/]

tuner score    --server <cmd>  --corpus <path>  --model <id>
tuner replay   --run <dir>     --variant <id>   --task <id>
tuner apply    --run <dir>     --variant <id>   [--dry-run]
```

The HTML report opens to the **Pareto frontier**: variants plotted on (success rate, token cost) with cross-model-transfer color-coding. Drilling into a variant shows its diff against baseline, ablation attribution per clause, and a trace browser. There is one "Recommended" badge — the variant Tuner would have you ship — chosen by a documented scalar (success × transfer − cost-penalty) that the author can override with their own weights.

The CLI is the primary surface because authors live in their terminals and want to wire Tuner into CI. A future hosted UI is not in V1 scope.

---

## 7. Technical Considerations

(High-level — details belong in the Design Doc.)

- **Three-layer architecture.** *Adapter* (talks to MCP servers, applies metadata overlays). *Search* (mutator + optimizer, model-agnostic, no MCP knowledge). *Eval* (runs agents against the adapter with a given overlay, scores, persists traces). The interfaces between layers are stable; each layer is independently testable.
- **Mutations are an overlay, not a fork.** Tuner never edits the server's source during search. It intercepts MCP `tools/list`, `prompts/list`, etc., responses and rewrites them per the active variant. This keeps mutations cheap and atomic.
- **LLM-as-mutator is the workhorse.** Random-search ablations exist for scientific honesty, but LLM-guided rewrites with chain-of-thought rationales are the default. The mutator is given the current description, recent failures, and a structured prompt for what to change and why; its rationale is stored alongside the mutation for debuggability.
- **Multi-fidelity to control cost.** Cheap proxy model (e.g. Haiku-class) on subset of corpus to triage variants; promote the top 20% to a frontier-model evaluation on full corpus; promote the top 5 to cross-model validation. This is where the $50 default budget comes from; without it, even a small search blows past $1k.
- **Goodhart's law is the central technical risk.** The harness can hill-climb into descriptions that exploit a model's idiosyncrasies — over-confident verbs, unusual punctuation, length tricks. Mitigations: held-out corpus (R-EVL-4), cross-model validation (R-CMV), human-review gate (no auto-deploy), and a published heuristic-flag list ("variant scores higher than baseline only on training corpus" → suppress).
- **Compositional effects.** Changing one tool's description shifts the agent's relative perception of *all* tools. We cannot optimize tool-by-tool independently. The atomic unit of optimization is the *full server overlay*, not the individual description. This is a deliberate, expensive choice — and the reason single-tool optimization patches in the wild keep regressing.
- **Stateful and destructive tools.** V1 defaults to read-only mode: tools annotated as having side effects (or matching the destructive-verb heuristic) are auto-mocked from recorded stubs. Authors can opt in with `--allow-write` after acknowledging an isolated environment is wired up.

---

## 8. Migration / Compatibility

Net-new tooling — no migration from a prior product.

**Compatibility commitments to authors:**

- **MCP protocol versions.** Tuner targets the current MCP spec version at GA and the two prior versions; protocol upgrades are tracked and re-tested in CI.
- **Output artifact format.** The artifact bundle schema is versioned (semver). Breaking changes get a major bump with a deprecation window of two minor releases.
- **Corpus format.** Same — versioned, with a `tuner migrate-corpus` shim across versions.

**Compatibility with downstream tooling:**

- Artifact bundles include OpenTelemetry-compatible trace exports so authors can pipe traces into their existing observability stack.
- CI-mode exit codes are documented and stable across V1.

---

## 9. API Specification (sketch — full spec in design doc)

**Library API** (Python; TypeScript follows in V1.1):

```python
from tuner import Harness, Corpus, EvolutionarySearch, Anthropic, OpenAI

harness = Harness(
    server=ServerSpec(transport="stdio", command=["my-mcp-server"]),
    corpus=Corpus.from_jsonl("tasks.jsonl"),
    training_model=Anthropic("claude-opus-4-7"),
    validation_models=[Anthropic("claude-sonnet-4-6"), OpenAI("gpt-5")],
    optimizer=EvolutionarySearch(population=32, generations=10),
    budget=Budget(max_variants=200, max_usd=50, max_wallclock_h=8),
    write_safety="mock",  # mock | sandbox | allow
)
result = harness.optimize(out="runs/2026-05-21/")
recommended = result.recommend()  # returns Variant with .diff(), .traces(), .scores()
```

**CI integration:**
```
tuner score --server <cmd> --corpus regression.jsonl --model claude-opus-4-7 \
            --fail-below 0.85 --fail-on-regression
```
Exit codes: 0 = pass; 1 = below threshold; 2 = regression vs prior bundle; 3 = harness error.

**Stability:** All public APIs are semver. The mutator and optimizer interfaces are explicitly pluggable and stable so the research community can drop in new search strategies without forking Tuner.

---

## 10. Security & Compliance

- **Sandboxing.** Default `write_safety="mock"` means no side-effecting tool ever runs against real infrastructure during search. Authors who flip to `"allow"` must pass `--i-understand-this-runs-real-tools` and Tuner logs a notice in the report.
- **Credential handling.** Tuner never reads, stores, or exfiltrates credentials from the spawned MCP server. Credentials are the author's responsibility; Tuner passes through env vars unchanged.
- **Telemetry.** Off by default. If enabled, Tuner sends anonymized aggregate metrics only — never task content, server metadata, or traces. Opt-in is a CLI flag, not a config file (no silent-on defaults).
- **Trace storage.** Traces stay on the author's filesystem. The artifact bundle is self-contained and portable; the author chooses where it lives.
- **Generated mutations.** The LLM-driven mutator can in principle produce metadata that contains prompt-injection content. Tuner runs every candidate description through a content filter and refuses to emit obviously injected metadata; this is best-effort, and we explicitly tell authors in docs to read every diff before applying.

---

## 11. GTM Considerations

- **Primary channel:** Anthropic DevRel and the MCP community. Tuner is positioned as a measurement tool first, an optimizer second — "you can't tune what you can't measure."
- **Open-source posture.** Tuner is open-source under a permissive license. Closed-source optimizers (proprietary search strategies, premium model routing) can be commercial later; the core harness is not.
- **Reference benchmarks.** We ship leaderboard-style scores for the most popular public MCP servers at launch, with the authors' consent — this gives the harness immediate social proof and gives authors a "before/after" they can post.
- **Adoption funnel:** awareness via the leaderboard → trial via `tuner score` (cheap, single-model) → deep use via `tuner optimize` (full search). The cheap entry point is intentional.

---

## 12. Open Questions

| # | Question | Owner | Due |
|---|---|---|---|
| OQ-1 | Should "success" include partial credit (graded rubric) by default, or strict pass/fail? Affects mutator gradient signal. Leaning graded with a `--strict` flag. | Eval lead | Before design doc |
| OQ-2 | How do we treat servers that expose hundreds of tools? Full-overlay optimization gets combinatorially nasty; do we offer a "tool-cluster" mode that fixes most descriptions and only mutates a frontier? | Eng lead | Mid-design |
| OQ-3 | What's the right cross-model holdout set as of GA? Three frontier models is the floor; do we add an OSS model so OSS-server authors aren't second-class? | PM + Research | Before pilot |
| OQ-4 | Where does *server instructions* (top-level priming) optimization sit vs *individual tool description* optimization? Same search space or separated for interpretability? | Research lead | Before design doc |
| OQ-5 | Should the harness optimize *parameter schemas* (strings → enums, optional → required) automatically, given these are breaking-change-shaped? Or only suggest them as flagged "structural" diffs requiring explicit opt-in? Leaning the latter. | PM | Before design doc |
| OQ-6 | LLM-as-judge for task success: which model, with what calibration? Risk of judge-being-optimized-against. | Eval lead | Pilot |
| OQ-7 | Is "agent usability score" a single scalar publishable as a leaderboard, or a vector? Single number aids adoption but invites Goodharting at the ecosystem level. | PM + DevRel | Before public launch |

---

## 13. Milestones

| Milestone | Scope | Target |
|---|---|---|
| **M0 — Design doc complete** | Architecture, mutator/optimizer interfaces, corpus schema, trace schema finalized | Week 3 |
| **M1 — Internal alpha** | Harness runs end-to-end on one reference server (filesystem) with one optimizer (evolve) and one model (Claude). Pareto report renders. | Week 7 |
| **M2 — Pluggability complete** | All four optimizers, all four model backends, cross-model validation, three reference corpora. | Week 11 |
| **M3 — Pilot** | 10 internal MCP servers + 5 external partners. Measure G1–G5. Iterate on report and recommended-variant scoring. | Week 14 |
| **M4 — GA (V1)** | Public release, leaderboard, docs, three reference corpora, CI integration recipes. | Week 17 |
| **V1.1 (post-GA)** | TypeScript library API; hosted leaderboard with author-opt-in submissions. | +6 weeks |
| **V2 candidates** (not committed) | Online/bandit mode for production servers; opinionated domain corpora; hosted SaaS optimizer. | TBD |

---

## 14. Appendix

- **A. Trace audit methodology** — how we derived the "18–34% of avoidable tool-call failures attributable to metadata" figure (subset of Claude Code MCP traces Q1 2026, labeling protocol, inter-annotator agreement).
- **B. Mutator catalog** — exhaustive list of mutation operators with examples (paraphrase, compress, elaborate, add-example, remove-example, string-to-enum, reorder, add-when-not-to-use, instructions-priming, etc.).
- **C. Prior art** — DSPy MIPRO, OPRO, TextGrad, AlphaEvolve, EvoPrompt, PromptBreeder. Tuner's novelty is the *MCP-specific* search space and the *cross-model validation as a first-class output* — not the search algorithms themselves.
- **D. Sample task corpus formats** — three reference corpora in full.
- **E. Failure mode catalog** — overfit-to-model, mutator-collapse, judge-gaming, long-description token-bloat, schema tightening that breaks valid calls.
