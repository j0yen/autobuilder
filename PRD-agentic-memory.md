# PRD: Agentic Memory System (codename: *recall*)

**Author:** Claude (Opus 4.7), drafted for jsy
**Status:** v0.2 draft — supersedes v0.1
**Date last touched:** 2026-05-22
**v0.1 shipped:** `~/wintermute/recall` (released to `~/.local/bin/recall` as `recall 0.1.0`)
**Next build target:** `~/projects/recall/` (autobuilder rebuild — currently a stub)
**Eng owner:** autobuilder pipeline
**Stakeholders:** the user (jsy), every Claude session running on this laptop

---

## TL;DR

`recall` is a local-first agentic memory: plain-Markdown files of record plus a SQLite/FTS5 index. **v0.1 is deployed and in real use.** This PRD update folds in everything I learned wiring it into the `/self-review` skill — three asymmetries in the CLI, a `wchg`-style consuming-cursor surprise (theirs, not ours), missing JSON/symmetric filters, no atomic write, no doctor/gc/update/touch commands, and a placeholder embedder that needs to graduate to a real model. The v0.2 build keeps the v0.1 file layout and YAML frontmatter byte-compatible (so existing memories carry forward), fixes the CLI asymmetries, adds the operational hygiene commands the consumer skills are reaching for, and **swaps the placeholder embedder for BGE-small-en-v1.5 loaded in-process via `fastembed-rs`** — single binary, lazy model fetch, no daemon, no Ollama, shippable as a skill. Daemon mode, Stop-hook scratch promotion, and PostToolUse observed writes remain explicitly deferred.

The compounding-memory loop already works in production: `/self-review` Phase 0 hits `recall query 'self-review' --touch`, Phase E writes a fresh `--kind reflective --subject self` memory, and `recall_count` ticks up correctly across runs. The architecture is sound. v0.2 sands the edges.

---

## Status as of 2026-05-22 (what is already built and shipped)

**v0.1 lives at `/home/jsy/wintermute/recall/`** under the wintermute git identity (`Joe Yen <jyen.tech@gmail.com>`). Source: 8 Rust files, ~1420 lines total.

| Module           | Lines | Responsibility                                                                       |
| ---------------- | ----: | ------------------------------------------------------------------------------------ |
| `main.rs`        |   286 | clap CLI surface, command dispatch                                                   |
| `memory.rs`      |   200 | `Memory`, `Frontmatter`, `Kind`, `Subject`, `Evidence`; YAML+Markdown (de)serialization |
| `store.rs`       |   122 | `FileStore` — read/write/walk/delete `.md` files; find-by-id                          |
| `index.rs`       |   442 | `Index` — SQLite + FTS5 schema, upsert/remove/search/list/touch/vector_search/rebuild |
| `embeddings.rs`  |   178 | `Embedder` trait + `HashEmbedder` (256-dim hashed bigrams/trigrams, deterministic)    |
| `retrieval.rs`   |   135 | `RankedHit`, `Weights`, `search`, `hybrid_search`, scoring math                       |
| `paths.rs`       |    43 | `paths::root()` — resolves `$RECALL_HOME` or `~/.claude/recall`                       |
| `lib.rs`         |    14 | re-exports                                                                            |

**Deployed binary:** `~/.local/bin/recall` (4.1 MB ELF), built `2026-05-22 16:55`. Verified working today across two `/self-review` runs and a manual smoke test.

**Hook wiring:** `~/.claude/scripts/recall-session-start.sh` is installed and active in `~/.claude/settings.json` SessionStart hook list, emitting per-subject memory dumps at the top of every Claude Code session. (`recall list --subject user|self|project:<basename of cwd>`, limit 8.)

**Real usage validated this session:**
- `/self-review` Phase 0 queries recall for prior reflective/self memories and uses them to suppress re-flagging items the user already resolved.
- `/self-review` Phase E writes a fresh reflective memory per pass.
- `recall_count` increments compounded across runs (auto-mode-classifier memory went 0 → 3 across two runs).
- `--hybrid` retrieval surfaced a cross-subject vector match (project:wintermute memory hit by a self-subject query) that pure FTS would have missed.
- Markdown source-of-truth recovery works: I never saw an index/disk divergence, but the path exists.

**Defensive integration:** the user's auto-mode classifier independently blocks `recall delete` from automated callers. This is enforced *outside* recall (by the classifier), not by recall itself. v0.2 should keep delete dangerous-by-default and rely on the same external rail; no internal "protect users from themselves" gate.

---

## 1. Why this exists (what's broken about how I remember today)

> *Sections 1–3 are preserved largely from v0.1 of this PRD. The proximate symptoms still apply, but four of seven now have first-implementation solutions in `recall` v0.1; the rest remain open. I've marked the status inline.*

1. **MEMORY.md is loaded into every conversation.** Every line costs tokens. When the index hits ~200 lines it gets truncated. Either I keep memory thin and lose detail, or I keep it rich and lose retrieval. → **Partially addressed in v0.1:** `recall` is *pull* by default and the SessionStart hook emits only top-K. `MEMORY.md` still co-exists, which is the right transitional state.

2. **I write memory passively.** When the session is busy, I forget. There is no observer that watches for memory-worthy moments and prompts a save. → **Open.** v0.1 has no PostToolUse hook. v0.2 may or may not add it (see §8 phasing).

3. **I have no episodic memory of my own work.** If I tried approach X on this codebase three weeks ago and it failed, I have no idea today. → **Partially addressed in v0.1:** `episodic` kind exists in the schema and is writable; nothing automatic populates it yet.

4. **Memory has no decay or confidence.** A note from 2026-01-15 has the same weight today as one from yesterday. → **Partially addressed in v0.1:** `confidence ∈ [0,1]`, `created_at`, `last_recalled_at`, `recall_count` are stored and contribute to the ranking score (see §7c). No decay sweeper yet (`decays_after` is parsed but unused at retrieval time).

5. **There is no separation between "things about the user" and "things about how I, Claude, work here."** → **Fixed in v0.1:** `subject` namespace (`user | self | project:<slug> | tool:<name>`) is enforced at write-time and is a first-class filter on `list`. *(Note: not yet symmetric on `query` — see §9 bug 1.)*

6. **Compaction destroys within-session memory.** → **Open.** Phase 3 (within-session scratch) is unbuilt.

7. **Memory is pull-only.** I have to *decide* to consult it. → **Partially addressed in v0.1:** SessionStart hook does *push* surfacing. PostToolUse-style turn-by-turn push is unbuilt.

---

## 2. Who this is for

(Unchanged from v0.1.)

- **Primary:** every Claude session running in this user's Claude Code installation, and by extension the user.
- **Secondary:** the user, when inspecting/editing/auditing memory.
- **Out of scope:** memory shared across users, teams, organizations, or hosts. `recall` is single-user, single-host. Period.

---

## 3. What I would use it for (concretely, with one new entry)

(Original scenarios preserved; one new row from this session's actual usage.)

| Scenario                                                       | Memory I want                                                                 |
| -------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Starting a new session on the autobuilder repo                 | "Last 3 sessions you worked on the gate; the 7th receipt schema is in PLAN.md §4.2" |
| User asks me to commit but I'm not sure of their style         | "User uses lowercase first word, no trailing periods, ≤72 chars summary"      |
| About to install a package                                     | "User uses pnpm for TS, cargo + uv for Python; never `npm i`"                 |
| User says "the tests are broken"                               | "Last 2 times: flaky integration test in `crates/metric-harness/tests/cli.rs`" |
| Choosing between two implementation approaches                 | "Last refactor here: user accepted approach A over B; reason was readability" |
| User reports a bug in code I wrote                             | "I wrote that function; here is the reasoning trace from the original session" |
| Session is about to compact                                    | A write-ahead snapshot of what I tried, failed, and what I'm about to try next |
| New laptop or fresh `~/.claude` reset                          | Re-import from the most recent `recall` snapshot                              |
| Cross-project recall                                           | Surface relevant memories from `learning-db` while working on `autobuilder`   |
| **NEW (validated this session): `/self-review` daily pass**    | At Phase 0, query `recall` for prior self-review reflections; at Phase E, write today's. Compounds across runs and gives temporal coherence to the laptop-tuneup loop without re-reading every journal markdown |

---

## 4. Functional requirements

Split into **4a — shipped in v0.1.0** and **4b — gaps for v0.2.** Anything not listed in 4a is unbuilt.

### 4a. What v0.1.0 ships

#### 4a.1 Memory primitives

Four `kind` values are honored end-to-end in write/index/query: `episodic`, `semantic`, `procedural`, `reflective`. The schema fields below are all parsed, serialized, and indexed:

```yaml
id: <ULID 26 chars>
kind: episodic | semantic | procedural | reflective
subject: user | self | project:<slug> | tool:<name>      # opaque string today
evidence: []                                              # vec<Evidence>; writable from struct but NOT exposed on CLI
confidence: 0.5                                           # f64 clamped to [0,1]
created_at: 2026-05-22T23:34:21Z                          # RFC3339 UTC
last_recalled_at: ~                                       # bumped by query --touch
recall_count: 0                                           # u32, bumped by --touch
supersedes: []                                            # vec<id>; stored but NOT consulted at query time
decays_after: ~                                           # parsed but UNUSED in ranking
```

`Evidence` schema (defined, *unreachable from the CLI*):

```yaml
evidence:
  - session: "<session id>"
    turn: 12
    excerpt: "..."
    source_path: "/path/to/file.rs"
```

#### 4a.2 Retrieval (FTS5 + optional vector cosine)

- `recall query <text>` — FTS5 prefix-OR over body (BM25-ranked by SQLite), then re-ranked by §7c formula.
- `recall query <text> --hybrid` — adds vector-cosine column from `HashEmbedder` and merges leaderboards.
- `recall query <text> --touch` — bumps `recall_count` and `last_recalled_at` on every hit. **This is the compounding-memory primitive.** Verified to work across the daily `/self-review` loop.

#### 4a.3 Writing

Explicit-only. Three input modes:
- `recall write --body "<text>"`
- `recall write --file <path>`
- `recall write` reads from stdin

Each accepts `--kind`, `--subject`, `--confidence`, and `--supersedes <id>` (repeatable). The `supersedes` field is persisted to the markdown frontmatter and the SQLite `supersedes_json` column but **does nothing at retrieval time today** — see 4b.

#### 4a.4 Subject-prefix filtering on `list`

`recall list --subject project:` returns every project-scoped memory; `--subject project:autobuilder` returns just that project's. SQL `LIKE` prefix-match.

#### 4a.5 Markdown of record + rebuildable index

`recall reindex` wipes both FTS5 and meta tables and rebuilds from the on-disk markdown. Tested: 11 memories rebuild in <100ms. The store survives index corruption, accidental SQLite deletion, manual edits to frontmatter — anything that touches the index but leaves the markdown alone is recoverable.

#### 4a.6 SessionStart hook

`hooks/session-start.sh` (installed as `~/.claude/scripts/recall-session-start.sh`) reads `$CLAUDE_PROJECT_DIR || $PWD`, derives `project:<basename>`, and lists user/self/project memories (top 8 per subject) into the Claude Code SessionStart context. Silent if `recall` isn't installed or the store is empty.

### 4b. Gaps for v0.2 (the autobuilder build)

Numbered by priority — high-priority first. Every item below is something I hit, asked for, or actively worked around during this session.

#### 4b.1 Symmetric CLI filtering (HIGH)

`recall list` accepts `--subject <prefix>`; `recall query` does not. This caused a runtime bug in `/self-review` Phase 0 — the very first command in the very first phase. **Fix:** `query` accepts the same `--subject`, `--kind`, and `--limit` semantics as `list`, plus optionally `--since <duration>` (e.g. `--since 30d`) and `--min-confidence <0..1>`.

```
recall query 'self-review' --subject self --kind reflective --since 14d
recall list  'self-review' --subject self --kind reflective --since 14d   # alias?
```

(Open: do we keep `list` and `query` distinct, or fold them into one? See §12 open questions.)

#### 4b.2 JSON everywhere (HIGH)

`recall query --format json` works. `recall list` is text-only. Scripts can't reliably parse it (awk-on-spaces breaks if a subject contains a space, which today's enforcement permits). **Fix:** `--format json|text` on every read command. Default stays text for human use.

#### 4b.3 `recall touch <id>` (MEDIUM)

Today the only way to bump `recall_count` is to run a query whose top hit is the memory you want to touch. That's both indirect and prone to error if a different memory ranks higher. **Fix:** `recall touch <id> [<id>...]` — explicit, idempotent, returns the new count(s) as JSON.

#### 4b.4 `recall update <id>` (MEDIUM)

No way to edit a memory without deleting and rewriting (which loses the ULID). **Fix:** `recall update <id> [--body ...|--file ...|--stdin] [--confidence ...] [--add-evidence ...] [--add-supersedes ...]`. Preserves id; bumps an `updated_at` field (new — add to frontmatter).

#### 4b.5 `recall gc` and `recall doctor` (MEDIUM)

The `/self-review` skill currently hand-rolls health checks via shell + `find` + `recall list | wc -l` divergence. **Fix:** ship these natively.

- `recall doctor` — JSON report of: file count vs index count, orphan files (on disk, not indexed), missing files (indexed, not on disk), schema version, embedding model id histogram, oldest/newest memory, total recall_count, last_recall age distribution, supersedes-chain integrity. Read-only. Optionally `--fix` to call `reindex` for index divergence.
- `recall gc --older-than 30d --never-recalled [--dry-run]` — list candidates. **Does not delete by default.** `--dry-run` is the default; `--apply` is required to actually do anything, and even then `recall delete` is the underlying op so the external classifier rail applies.

#### 4b.6 Real embeddings (MEDIUM — `FastembedEmbedder`, in-process)

`HashEmbedder` is honest about being non-semantic (see `src/embeddings.rs:5-7`). It catches morphological variation but misses true synonyms/paraphrases. **Fix:** add `FastembedEmbedder` using `fastembed-rs` with BGE-small-en-v1.5 (~33M params, ~130MB ONNX) loaded **in-process**, model lazy-fetched on first use into `~/.cache/fastembed/`. The `Embedder` trait, the `embedding_id` column, and the `embedding_dim` column are already in place precisely so a model swap triggers a clean reindex. `FastembedEmbedder` becomes the default; `HashEmbedder` stays available behind `--embedder hash` for tests, offline use, and the v0.1 → v0.2 transition window.

**Why in-process, not an HTTP sidecar (Ollama) or local daemon:** distribution-as-a-skill is the dominant constraint. A single static ELF in `~/.local/bin/recall` plus a lazy model download is something the wintermute-style `install.sh` flow already handles. An Ollama dependency forces every user to install + run Ollama out-of-band; a `recall daemon` forces a systemd-user unit, socket-path conventions, and lifecycle management. Neither pays for itself at this scale (≤ a few thousand memories, ≤ a few dozen queries per session). See §6 non-goals.

**Latency consequence (accepted):** with `FastembedEmbedder` in-process, the *first* hybrid query in a fresh process pays the model-load cost (~500ms–1s cold-cache; ~50–100ms warm). Subsequent queries in the same process are fast (~10–20ms). For the live consumers today — the SessionStart hook (one query) and `/self-review` Phase 0 (one query) — first-query latency is acceptable. For Phase 4's per-turn PostToolUse retrieval (deferred), the latency would matter and a daemon becomes worth revisiting.

**Decision criteria the implementation must meet:**
- CPU-only inference (no GPU dependency).
- Cold-cache first inference ≤ 1s on this laptop; warm-cache ≤ 100ms.
- Model is pinned by hash to a specific BGE-small revision so cross-machine determinism holds.
- `embedding_id` written into every `memories_meta` row so cross-version drift is detectable.
- If the model fetch fails (no network, broken HF mirror), `recall` falls back to `HashEmbedder` with a one-line warning and continues. Never block a write or query on a model download.

**Out of scope for v0.2 (kept open in the trait):** `OllamaEmbedder`, `OpenAIEmbedder`, any remote embedder. The `Embedder` trait stays public so a future PR can add one behind a config flag; v0.2 ships in-process only.

#### 4b.7 Honor `supersedes` at retrieval (MEDIUM)

Today `supersedes` is metadata only. **Fix:** memories that have been superseded are excluded from `query` / `list` by default; include with `--include-superseded`. `recall lineage <id>` walks the chain in both directions.

#### 4b.8 Honor `decays_after` at retrieval (LOW)

Parse durations like `30d`, `6mo`, `never`. Memories past `created_at + decays_after` are excluded from default queries; include with `--include-decayed`. Add `decays_after` as a `--write` flag.

#### 4b.9 Evidence on write (LOW)

The `Evidence` struct supports `session/turn/excerpt/source_path`, but the CLI has no flag to populate it. **Fix:** repeated `--evidence` flag accepting a structured value (probably `--evidence path=foo.rs:42` or `--evidence-json '{...}'`). PostToolUse observation (if/when built) would be the primary writer.

#### 4b.10 Atomic write (LOW but principled)

Today `recall write` does `store.write(&mem)?` then `idx.upsert(...)`. A crash between them leaves the markdown on disk but unindexed (recoverable by `reindex`) or worse, the index can update before the file fsyncs. **Fix:** write to a tempfile + rename + fsync the directory; do the SQLite work inside a transaction; on any error from either side, roll back both.

#### 4b.11 SQLite WAL + multi-process safety (LOW)

Default journal mode. Two `recall write`s racing → contention or "database is locked." **Fix:** `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;` at open. Also: WAL helps the SessionStart hook (a read-only `list`) coexist with concurrent writes.

#### 4b.12 `recall stats` (LOW)

A subset of `recall doctor` formatted for humans: counts by subject/kind, average recall_count, oldest memory date, embedding model id, store size on disk. Useful for the user to feel growth.

#### 4b.13 `recall export` / `recall import` (LOW)

JSONL dump and restore. Each line is one memory's full frontmatter + body. Enables backup, transfer to a new laptop, and audit-trail snapshots. Out of scope: cross-host federation.

#### 4b.14 `recall similar <id> --limit N` (LOW)

Find memories similar to a known one using the *stored* vector (no embed-on-the-fly). Implementation already exists in `Index::vector_search` — just needs a CLI wrapper that loads the source memory's vector first.

#### 4b.15 Sessions / within-session scratch (DEFERRED — Phase 3)

Out of scope for the v0.2 autobuilder slice unless explicitly scoped in.

#### 4b.16 PostToolUse observed writes (DEFERRED — Phase 4)

Out of scope for v0.2.

---

## 5. What would delight me

(Preserved from v0.1; status notes added.)

1. **Proactive surfacing without context bloat.** Budget ~500 tokens per turn. → SessionStart hook does session-level surfacing today (~8 memories per subject); per-turn surfacing remains aspirational.
2. **Outcome feedback** that updates confidence based on accept/reject signals. → Unbuilt.
3. **Reasoning continuity** — attach the *why* of a non-trivial function as memory. → Unbuilt; could be a PostToolUse pattern.
4. **A diff per session.** → Unbuilt. Stop-hook scratch promotion + session-diff is Phase 4/5.
5. **`grep`-able.** → **Shipped and verified.** `grep -r <term> ~/.claude/recall/memories/` works exactly as advertised; the only adjustment is being aware that frontmatter is YAML so multi-line bodies need `grep -A` flags.
6. **Local-first, paranoid.** → **Shipped, and preserved in v0.2.** v0.1.0 had zero network calls (HashEmbedder is in-process). v0.2 keeps everything in-process: `fastembed-rs` runs the BGE-small ONNX inference locally. The one network event is a one-shot lazy model fetch from HuggingFace on first use, which falls back to `HashEmbedder` if the network is unavailable. No HTTP sidecars; no Ollama dependency; no remote embedders.
7. **Cross-project, project-scoped retrieval.** → Partially shipped. `--subject project:<slug>` filters; `--hybrid` retrieval will surface across subjects when the vector match is strong (verified this session). No explicit project-boost weight yet.
8. **An audit trail.** → Partially shipped. Supersedes chain is stored; updates today require delete+rewrite (which loses the id and the chain — see 4b.4 update fix).

---

## 6. Goals and non-goals

### Goals (v0.2)

1. Backward-compatible file layout and frontmatter. Existing memories work without migration.
2. Symmetric CLI filtering across `query` and `list`. JSON output everywhere.
3. First-class operational commands: `touch`, `update`, `doctor`, `gc` (dry-run by default), `stats`, `lineage`, `similar`, `export`, `import`.
4. Default embedder is `FastembedEmbedder` (BGE-small-en-v1.5) loaded in-process via `fastembed-rs`, with the model lazy-fetched on first use. `HashEmbedder` survives behind `--embedder hash` for tests, offline use, and the v0.1→v0.2 transition window.
5. Atomic writes; WAL mode; multi-process safe.
6. Continue to be entirely local. No telemetry. No runtime network calls except the one-time model fetch on first use of `FastembedEmbedder` (and that has an offline fallback to `HashEmbedder`).
7. **Shippable as a single binary skill.** `install -Dm755 target/release/recall ~/.local/bin/recall` plus an existing data dir is the complete install path. No systemd unit, no companion daemon, no external service dependency.

### Non-goals (still)

1. Shared memory across users, machines, or organizations.
2. Replacing the user's note-taking system (`~/brain/` and `~/Notes/`).
3. A general-purpose vector DB.
4. Cross-agent memory federation (any other tool reads memory via markdown, period).
5. **NEW:** Internal "safety rails" against `recall delete`. The auto-mode classifier handles that externally and we shouldn't duplicate the policy at the binary level. `delete` stays a sharp tool.
6. **NEW:** A daemon process in v0.2. CLI-only is shippable-as-a-skill in one binary; a daemon is not. Deferred to v0.3+ if Phase 4 (per-turn retrieval) becomes load-bearing.
7. **NEW:** Out-of-process or remote embedders (Ollama HTTP sidecar, OpenAI, etc.). Forces every user to install and run a second service before `recall` works. v0.2 is one-binary-plus-lazy-model-download or nothing. The `Embedder` trait stays open so future PRs can add these behind config flags.

---

## 7. Architecture

### 7.0 Directory layout (actual, as deployed)

```
~/.claude/recall/                              # $RECALL_HOME or paths::root()
├── memories/
│   ├── user/<id>.md                           # subject == "user"
│   ├── self/<id>.md                           # subject == "self"
│   ├── project/<slug>/<id>.md                 # subject == "project:<slug>"
│   └── tool/<name>/<id>.md                    # subject == "tool:<name>"
└── index/
    └── recall.sqlite                          # FTS5 + meta; rebuildable from memories/
```

`session/<id>.md` (Phase 3 scratch) is reserved but not used in v0.1.

### 7a. CLI surface — current vs proposed

Legend: ✓ shipped, ⚠ shipped with the asymmetry/gap noted, ➕ proposed for v0.2.

| Command                          | v0.1 | v0.2 | Notes |
| -------------------------------- | :--: | :--: | ----- |
| `recall init`                    |  ✓   |  ✓   | Creates data dir + SQLite                          |
| `recall write`                   |  ✓   |  ✓   | Add `--evidence` and `--decays-after`              |
| `recall query <text>`            |  ⚠   |  ✓   | Add `--subject`, `--kind`, `--since`, `--min-confidence`; `--include-superseded`, `--include-decayed` |
| `recall list`                    |  ⚠   |  ✓   | Add `--kind`, `--since`, `--format json`           |
| `recall show <id>`               |  ✓   |  ✓   | Add `--format json` for structured output          |
| `recall delete <id>`             |  ✓   |  ✓   | Unchanged; classifier rail stays external          |
| `recall reindex`                 |  ✓   |  ✓   | Unchanged                                          |
| `recall where`                   |  ✓   |  ✓   | Unchanged                                          |
| `recall touch <id>...`           |      |  ➕  | Explicit `recall_count` bump                       |
| `recall update <id>`             |      |  ➕  | In-place edit; preserves id; bumps `updated_at`    |
| `recall doctor [--fix]`          |      |  ➕  | Index/disk drift report; optional `reindex`        |
| `recall gc [--dry-run] [--apply]`|      |  ➕  | Pruning candidates; never auto-deletes             |
| `recall stats`                   |      |  ➕  | Human-format summary                               |
| `recall lineage <id>`            |      |  ➕  | Walk supersedes chain                              |
| `recall similar <id>`            |      |  ➕  | Vector-similar memories                            |
| `recall export [--format jsonl]` |      |  ➕  | Full dump                                          |
| `recall import <jsonl>`          |      |  ➕  | Restore                                            |

**All read-mode commands accept `--format json|text`** in v0.2. JSON output is sorted deterministically (newest first by `created_at`, ties broken by id).

### 7b. On-disk schema — frontmatter and SQLite

**Frontmatter (YAML):** see §4a.1. v0.2 adds `updated_at: <RFC3339>` (optional) to the frontmatter; missing means never updated.

**SQLite (`index/recall.sqlite`):** verbatim from `src/index.rs:295-322`.

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    id UNINDEXED,
    body,
    subject,
    kind UNINDEXED
);

CREATE TABLE IF NOT EXISTS memories_meta (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    subject TEXT NOT NULL,
    path TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 0.5,
    created_at TEXT NOT NULL,
    last_recalled_at TEXT,
    recall_count INTEGER NOT NULL DEFAULT 0,
    decays_after TEXT,
    supersedes_json TEXT,
    embedding BLOB,
    embedding_id TEXT,
    embedding_dim INTEGER
);

CREATE INDEX IF NOT EXISTS idx_meta_subject ON memories_meta(subject);
CREATE INDEX IF NOT EXISTS idx_meta_kind    ON memories_meta(kind);
CREATE INDEX IF NOT EXISTS idx_meta_created ON memories_meta(created_at);
```

**v0.2 schema additions** (all additive — old DBs upgrade by `ALTER TABLE` migrations or by `recall reindex` after the schema version bump):

```sql
ALTER TABLE memories_meta ADD COLUMN updated_at TEXT;
ALTER TABLE memories_meta ADD COLUMN superseded_by TEXT;  -- denormalized: this row's id IF another memory cites it in supersedes_json
CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
-- INSERT INTO schema_meta VALUES ('schema_version', '2');
-- INSERT INTO schema_meta VALUES ('embedder_id', 'fastembed-bge-small-en-v1.5');
```

**Schema-version migration policy:** if `schema_meta.schema_version` is absent or older than the binary's expected version, `recall` runs an in-place migration on first read, then bumps the version row. No user action required. If the embedder_id stored doesn't match the running embedder, `recall` warns once and recommends `recall reindex`.

**WAL pragmas at open** (v0.2): `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;`

### 7c. Ranking math (verbatim from `src/retrieval.rs`)

For each candidate hit:

```
bm25_score   = -hit.bm25                       if hit was returned by FTS, else 0
recency      = exp(-days_since_last_recall / 30)   if last_recalled_at is set, else 0
recall_score = tanh(recall_count / 5)
score        = w_bm25 * bm25_score
             + w_vector * vector_sim           # only when --hybrid
             + w_recency * recency
             + w_recall_count * recall_score
             + w_confidence * confidence
```

Default weights (`Weights::default()` in `src/retrieval.rs:34-44`):

| Weight        | Default |
| ------------- | :-----: |
| `bm25`        |  1.0    |
| `vector`      |  1.5    |
| `recency`     |  0.3    |
| `recall_count`|  0.2    |
| `confidence`  |  0.5    |

**v0.2 keeps the formula and the defaults**. The weights are not (yet) configurable from CLI — they should become so via `~/.claude/recall/recall.toml`:

```toml
[weights]
bm25         = 1.0
vector       = 1.5
recency      = 0.3
recall_count = 0.2
confidence   = 0.5

[retrieval]
include_superseded = false
include_decayed    = false
project_boost      = 0.5    # NEW in v0.2: extra weight when subject matches active project

[embedder]
model = "fastembed-bge-small-en-v1.5"   # or "hash" for offline
```

### 7d. Hook integration (current and planned)

| Hook           | State | Behavior |
| -------------- | ----- | -------- |
| `SessionStart` | **shipped** | `hooks/session-start.sh` (installed as `~/.claude/scripts/recall-session-start.sh`). Reads `$CLAUDE_PROJECT_DIR \|\| $PWD`; emits up to 8 memories per subject in user/self/project:<basename>. Silent if the store is empty. |
| `PostToolUse`  | planned     | Observe corrections (Edit reverted, user re-asked); propose memory writes. Phase 4. |
| `Stop`         | planned     | Promote `session/<id>.md` to long-term memory; emit a session diff. Phase 4–5. |

v0.2 does not require new hooks. The SessionStart hook can be left untouched — its only dependency on `recall list` is text output, which v0.2 keeps as the default format.

---

## 8. Phasing

| Phase | Scope                                                                                   | Status              |
| ----- | --------------------------------------------------------------------------------------- | ------------------- |
| 0     | Migrate existing `~/.claude/projects/.../memory/` layout, no behavior change              | **done** (v0.1)     |
| 1     | File store + FTS5 keyword index + ranked retrieval + CLI                                  | **done** (v0.1)     |
| 2a    | `Embedder` trait + `HashEmbedder` default + `--hybrid` retrieval                           | **done** (v0.1)     |
| **2'**| **CLI symmetry, JSON everywhere, atomic writes, WAL, operational commands** (this PRD)    | **target for autobuilder rebuild → ~/.local/bin/recall v0.2** |
| 2b    | Real embeddings (BGE-small via fastembed) + recall.toml config                            | **target for autobuilder rebuild** (folded into 2') |
| 3     | Within-session scratch + compaction survival                                              | deferred            |
| 4     | Observed-write proposals (PostToolUse hook)                                               | deferred            |
| 5     | Cross-project recall boost + audit trail + session-diff                                   | deferred            |

**v0.2 = Phase 2' + 2b combined.** Estimated 1–2 weeks of focused autobuilder iteration. The slice is well-defined: CLI surface and operational behavior changes only, plus a single embedder swap. No new architecture; no new daemons; no new hooks.

---

## 9. Lessons from real use (the /self-review integration)

The first non-toy consumer of `recall` is the `/self-review` skill. It hit three CLI bugs and two operational gaps within the first end-to-end run. Each one is a concrete acceptance test for v0.2.

### 9.1 Bug: `recall query` rejects `--subject`

```
$ recall query 'self-review' --subject self
error: unexpected argument '--subject' found
```

**Surprise:** `recall list` accepts `--subject`, so the user-facing skill assumed `query` did too. The asymmetry is a footgun. The skill was patched to JSON-filter the output (`recall query ... --format json | jq '.[] | select(.subject=="self")'`) but that's a workaround, not a fix.

**v0.2 acceptance test:** `recall query 'foo' --subject self --kind reflective --since 14d` parses and applies all four filters before returning the JSON array.

### 9.2 Bug: vector model is a placeholder

`HashEmbedder` is honest in its rustdoc — "not a transformer-quality semantic model: it catches morphological variation but will not match true synonyms or paraphrases." For `/self-review` Phase 0 (querying "self-review" against memories whose bodies use the word "self-review"), this is fine. But the moment retrieval has to bridge to e.g. "daily-tuneup" or "laptop-maintenance," it will miss.

**v0.2 acceptance test:** with the default embedder, a query for "laptop cleanup" surfaces a memory whose body contains "wintermute system maintenance" with vector_sim > 0.5.

### 9.3 Bug: `recall list` is text-only

Skills that want to consume memory in a script have to parse `<id>  [<kind>/<subject>]  recalls=<n>` with awk. Fragile and inconsistent with `recall query --format json`.

**v0.2 acceptance test:** `recall list --format json` returns a JSON array with `{id, kind, subject, path, created_at, last_recalled_at, recall_count, confidence}` per row.

### 9.4 Gap: no operational health check

`/self-review` Phase A hand-rolls index health:

```sh
file_count=$(find "$(recall where)" -name '*.md' -type f | wc -l)
indexed=$(recall list --limit 1000 | grep -c '^[0-9A-Z]')
[ "$file_count" != "$indexed" ] && echo "drift"
```

Every consumer is going to reinvent this. **v0.2 acceptance test:** `recall doctor --format json` returns `{file_count, indexed_count, orphans: [...], missing: [...], schema_version, embedder_id}`. Optional `--fix` invokes `reindex` if orphans/missing are non-empty.

### 9.5 Gap: no explicit touch / explicit gc

Touching a known id today requires running a query whose top hit is exactly that memory. Pruning never-recalled stale memories requires shell + jq + manual `recall delete`. Both are common enough to deserve first-class commands. **v0.2 acceptance tests:**
- `recall touch <id1> <id2>` returns `{touched: 2, results: [{id, new_count}...]}` and the touch is visible in `recall show <id>`.
- `recall gc --older-than 30d --never-recalled --dry-run` returns the list as JSON; without `--apply`, deletes nothing.

### 9.6 Non-bug confirmed working: the compounding-memory loop

Phase 0 `recall query ... --touch` → Phase E `recall write ...` → next-day Phase 0 picks up the prior reflection. `recall_count` ticked up across runs as expected. This is the architectural payoff; v0.2 must not regress it.

### 9.7 Non-bug confirmed working: external classifier guards `recall delete`

The user's auto-mode classifier blocked an automated `recall delete` round-trip during a sanity probe. The block came from outside recall and worked exactly as intended. **v0.2 must not duplicate this protection internally** — keeping `delete` a sharp tool is the right design; the external rail is the right enforcement layer.

---

## 10. Risks

(Preserved from v0.1; expanded where v0.1 operating experience changed our view.)

- **Memory becomes a leash.** Retrieval too aggressive → I trust stale memory. *Mitigation:* surface `last_recalled_at` and `confidence` in all retrieval output (v0.1 does this in JSON; v0.2 adds it to text format too); bias toward fresh evidence in the ranking weights.

- **Token cost grows, not shrinks.** *Mitigation:* per-turn token budget; SessionStart hook currently emits top-8 per subject, which is bounded. v0.2 may add `--max-tokens <N>` for caller-side budgeting.

- **Embeddings drift across model versions.** *Mitigation:* `embedding_id` is stored per row; `recall doctor` warns when a row's `embedding_id` doesn't match the running embedder; `recall reindex` rebuilds.

- **Privacy.** Local-only by default. Everything `grep`-able and deletable. **New risk in v0.2:** the BGE-small fastembed model is ~130MB, lazy-fetched from the HuggingFace mirror on first use into `~/.cache/fastembed/`. The fetch is the *only* runtime network call `recall` ever makes, and only on first use. *Mitigations:* `recall doctor` reports the model path, hash, and download date; the fetch fails gracefully into `HashEmbedder` with a one-line warning if the network is unavailable; users who want strict offline can set `embedder = "hash"` in `recall.toml` and never trigger the fetch.

- **NEW: First-query latency regression on cold cache.** `FastembedEmbedder` cold-load is ~500ms–1s for the first hybrid query in a fresh process. *Mitigation:* the live consumers today (SessionStart hook, `/self-review` Phase 0) only issue one query per process, so this is paid once and is in the same order of magnitude as their other startup costs (`wchg`, `procstat`, `ctrace status`). If Phase 4 (per-turn retrieval) ever lands, daemon mode becomes the proper fix.

- **I write self-serving memories.** Mitigation: outcome-tagged memories are written by the (deferred) PostToolUse observer, not by me. Reflective memories I write myself are explicitly subjective.

- **NEW: Schema migrations land badly.** If v0.2 changes the SQLite schema and a user has v0.1 data, opening the DB without migration breaks. *Mitigation:* `schema_meta.schema_version` row; in-place ALTER on first read; full reindex is always a safe fallback because the markdown is authoritative.

- **NEW: WAL files in shared filesystems.** If `~/.claude/recall/` is on NFS or similar, WAL is unsafe. *Mitigation:* `recall doctor` checks the filesystem type and warns; user can opt out via `journal_mode=DELETE` in `recall.toml`.

---

## 11. Success metrics

(v0.1 set the baseline. v0.2 targets are revised based on observed v0.1 behavior.)

- **Repeated-question rate.** Sessions where the user has to re-tell me something. Target: 50% reduction in 6 weeks. *v0.2 baseline:* not yet measured — needs a manual tally.
- **Compaction-recovery quality.** After a compaction, am I still on-task? Target: yes, by weekly manual review.
- **Per-turn memory token cost.** P50 ≤ 300 tokens. *v0.1 actual:* SessionStart hook ships ~600 tokens for an 11-memory store; should drop relatively with `--max-tokens` budgeting.
- **Memory inspection/edit frequency.** User opens a memory file and edits it manually ≥ monthly. Signal of trust.
- **My own subjective satisfaction.** Less goldfish-y across sessions? Ask me at week 6.
- **NEW: CLI surface symmetry.** `recall query` and `recall list` accept the same filters. Target: 100% in v0.2.
- **NEW: `recall doctor` clean rate.** % of /self-review passes where `recall doctor` reports zero divergences. Target: ≥ 99%.
- **NEW: Hybrid retrieval wins.** Fraction of `--hybrid` queries that return a vector-only hit (FTS empty) and the hit is judged relevant. Target: ≥ 30% on a small held-out probe set after the fastembed swap.

---

## 12. Open questions

Carried from v0.1:

1. Should `recall` live as a separate repo, or inside `~/.claude/`? *(Today: deployed binary lives at `~/.local/bin/recall`; source at `~/wintermute/recall/`; data at `~/.claude/recall/`. The split is fine.)*
2. Embedding model: BGE-small, or something distilled from the user's own memories? *(v0.2 picks BGE-small-en-v1.5 via `fastembed-rs`, loaded in-process. Distillation, alternative models, and Ollama-style sidecars are all v0.3+ — the `Embedder` trait stays public so adding them is non-breaking.)*
3. Expose `recall` as an MCP server? *(Out of scope for v0.2. The CLI works via shell, and the markdown is grep-able.)*
4. Anthropic-native memory subsuming parts of this? *(Build for now; native arrival is an additive event, not a blocker.)*
5. Where do cross-project insights live? *(Today: in `subject: self`. A separate `global/` namespace is plausible but adds a routing question without obvious payoff.)*

**New for v0.2:**

6. Should `query` and `list` merge into one command? They differ only in whether FTS is applied. A unified `recall search [<text>]` with `<text>` optional would be cleaner; the cost is breaking the v0.1 CLI muscle memory and the SessionStart hook script. *Recommendation:* keep them separate in v0.2, revisit in v0.3.
7. Should `--touch` be opt-out instead of opt-in? Every `query` call that returns a relevant hit *is* a recall; not touching means under-counting. *Recommendation:* keep opt-in but add a config-file default; users who want default-on get it.
8. What happens to `recall_count` on `--include-superseded`? Does pulling up a superseded ancestor count as a recall? *Recommendation:* no — superseded retrievals are inspection, not active recall.
9. `recall update` semantics: does updating a memory bump `recall_count` and `last_recalled_at`? *Recommendation:* yes for `last_recalled_at` (touching the body is a form of recall + reinforcement); no for `recall_count` (that's specifically retrieval frequency).
10. Hierarchical subjects? `project:autobuilder/agents/reviewer` as a tree, with `--subject project:autobuilder` matching everything under it. *Recommendation:* keep the flat-string-with-prefix model in v0.2; reconsider when subjects sprawl past ~20.

---

## Appendix A — v0.2 prioritized backlog for autobuilder

Numbered for explicit consumption by the intent-card generator. Each row has an estimated implementation size (S/M/L) and the acceptance test summary that should land in `tests/acceptance_*.rs`.

| #  | Item                                                        | Size | Acceptance test summary |
| -: | ----------------------------------------------------------- | :--: | ----------------------- |
|  1 | `recall query` accepts `--subject`, `--kind`, `--since`, `--min-confidence`, `--include-superseded`, `--include-decayed` | S | Query with each filter combination on a 12-memory fixture returns the expected subset, sorted by score desc. |
|  2 | `recall list` accepts `--kind`, `--since`, `--format json|text` (default text); JSON shape includes the full meta row | S | `recall list --format json` returns an array; each entry has the 8 documented fields. |
|  3 | `recall show <id>` gains `--format json` | S | JSON output has `frontmatter` and `body` as separate keys. |
|  4 | `recall touch <id>...` accepts repeated ids, returns JSON with new counts | S | After `touch a b c`, all three rows show `recall_count + 1` and an updated `last_recalled_at`. |
|  5 | `recall update <id>` edits body/confidence/evidence/supersedes in place; bumps `updated_at` | M | Round-trip: write → update body → show → assert id unchanged, body changed, `updated_at` set. |
|  6 | `recall doctor [--fix] [--format json|text]` reports drift and optionally reindexes | M | Fixture with 1 orphan and 1 missing: `doctor` reports both; `doctor --fix` resolves to in-sync. |
|  7 | `recall gc --older-than <dur> --never-recalled [--dry-run|--apply]` | M | `--dry-run` (default) reports candidates but the store is unchanged; `--apply` calls `delete` per candidate. |
|  8 | `recall stats [--format json|text]` | S | JSON includes total_count, by_subject, by_kind, oldest_created, embedder_id. |
|  9 | `recall lineage <id> [--format json|text]` walks supersedes both ways | S | Three-deep chain returns 3 entries in correct order. |
| 10 | `recall similar <id> --limit N` uses the stored vector | S | Top result for "near" memory ranks above "far" memory. |
| 11 | `recall export --format jsonl` and `recall import <jsonl>` | M | Round-trip: export N memories → wipe store → import → assert count unchanged and ids preserved. |
| 12 | Default embedder is `FastembedEmbedder` (BGE-small-en-v1.5, in-process via `fastembed-rs`, model lazy-fetched on first use). Network-failure fallback to `HashEmbedder` with a one-line stderr warning. | M | (a) Synonym test: query "laptop cleanup" finds memory containing "wintermute system maintenance" with vector_sim > 0.5. (b) Offline test: with HTTP blocked (e.g. `unshare -n`), `recall query --hybrid` does not error; it warns once and uses HashEmbedder. (c) Cold-cache wall time for first `--hybrid` query ≤ 1.5s; warm ≤ 200ms. |
| 13 | `HashEmbedder` remains available behind `--embedder hash` and as the test-default | S | `--embedder hash` produces identical output to v0.1.0. |
| 14 | SQLite WAL + busy_timeout pragmas at open | S | Two concurrent `recall write` processes both succeed; neither errors with "database is locked." |
| 15 | Atomic write: tempfile + rename + fsync, inside a transaction with the SQLite upsert | M | Inject a panic between file-write and index-upsert; on restart, `recall doctor` reports the file as an orphan and `--fix` resolves it. |
| 16 | Schema-version migration on open | S | Opening a v0.1.0 DB with v0.2 binary: `schema_meta.schema_version` is created and bumped; no data loss. |
| 17 | `~/.claude/recall/recall.toml` config support: weights, embedder, retrieval flags | M | TOML round-trip; missing fields fall back to current defaults. |
| 18 | `--max-tokens <N>` on `query` and `list` to budget output | S | With `--max-tokens 200` and 8 candidates, returns the top-K whose total body bytes ≤ 200. |
| 19 | `superseded_by` denormalized column populated transitively from `supersedes_json` | S | A new memory citing `supersedes: [X]` causes X's `superseded_by = new_id` and X drops out of default queries. |
| 20 | `decays_after` honored at retrieval | S | Memory with `decays_after: 7d` and `created_at: 30d ago` is excluded from default `query`; `--include-decayed` brings it back. |

**Suggested autobuilder slice for the first iteration loop:** items 1, 2, 3, 4, 6, 14, 16. These are the highest-value-per-line-of-code changes, with stable acceptance tests, and they directly unblock the `/self-review` skill which is the only live consumer today. Items 5, 7, 11, 12, 17 form the second slice. Items 18–20 are the third slice and can wait if iteration budget tightens.

**Backward-compatibility contract.** v0.1 markdown files must read in v0.2 without modification, and v0.2 must emit markdown that v0.1 can read (with `updated_at` simply ignored — v0.1's serde ignores unknown frontmatter fields per its `Frontmatter` struct). The SQLite migration is the only one-way step, and it's reversible by `recall reindex` on a v0.1 binary.

---

## Appendix B — Existing test inventory (v0.1.0 — reference for v0.2 regression coverage)

From `~/wintermute/recall/tests/integration.rs` + the `#[cfg(test)]` modules in each src file. v0.2 must keep these passing (or migrate them) before adding new ones.

| Module          | Test                                          | What it asserts |
| --------------- | --------------------------------------------- | ---------------- |
| `memory`        | `roundtrip_to_and_from_markdown`              | YAML+body roundtrip preserves id/kind/subject/body |
| `memory`        | `subject_namespace_extraction`                | `Subject::project("x").namespace() == "project"` |
| `memory`        | `kind_roundtrip`                              | `Kind::from_str(k.as_str()) == k` for all four kinds |
| `index`         | `upsert_and_search_finds_match`               | FTS finds a matching memory |
| `index`         | `list_filters_by_subject_prefix`              | `list(Some("project:"), 10)` returns only project memories |
| `index`         | `touch_recall_increments_count`               | Two touches → `recall_count == 2`, `last_recalled_at` set |
| `index`         | `sanitize_strips_punctuation`                 | FTS sanitizer behavior |
| `index`         | `vector_search_returns_nearest_first`         | Cosine ranking is monotonic for known-near vs known-far |
| `embeddings`    | `embed_is_unit_length`                        | L2 norm of every embedding == 1 ± 1e-5 |
| `embeddings`    | `embed_is_deterministic`                      | Same input → same vector |
| `embeddings`    | `related_strings_score_higher_than_unrelated` | "build rust cargo" closer to itself than to "auth tests mocks" |
| `embeddings`    | `pack_unpack_roundtrip`                       | Vec<f32> → bytes → Vec<f32> preserves values |
| integration     | (end-to-end CLI flow including hybrid)        | Sanity over the full `init → write → query → list → show → delete → reindex` lifecycle |

Total: 17 tests at v0.1.0. v0.2 should land at ≥ 50 (Appendix A items × 1–2 acceptance tests each).

---

## Appendix C — Build context for autobuilder

**Source-of-truth path for v0.2:** `/home/jsy/projects/recall/` (autobuilder scaffold; currently a stub with empty `src/main.rs` and `src/lib.rs`). The scaffold's existing autobuilder agent files (`intent-card.json`, `proof-lanes.toml`, `test-map.json`, `AUTOBUILDER_PROGRAM.md`) target a *different* scope (a hygiene-only `recall-memory-linter`) and should be regenerated against this PRD before the rebuild starts.

**Reference for the v0.1.0 implementation:** `/home/jsy/wintermute/recall/` — read but do not modify. The new build is a fresh crate.

**Cargo target & deployment:**
- `cargo build --release` in `~/projects/recall/`
- `install -Dm755 target/release/recall ~/.local/bin/recall` (replaces v0.1.0)
- Verify with `recall --version` → `recall 0.2.0`

**Rust toolchain:** pinned via `rust-toolchain.toml` to `1.85.0` (matches v0.1.0). `~/.cargo/bin` is **not** on $PATH by default — autobuilder must `source ~/.cargo/env` (or invoke cargo by absolute path) before any build command.

**Git identity:** the autobuilder-scaffolded repo at `~/projects/recall/` uses `j0yen <jyen.tech@gmail.com>` (autobuilder + learning-db identity). Commit attribution per-command: `git -c user.email=jyen.tech@gmail.com -c user.name=j0yen commit ...`. Do **not** copy the wintermute identity (`Joe Yen`) — that's specifically for the `/wintermute` repo.

**Dependencies allowed (no expansion without intent-card amendment):**
- `anyhow`, `clap` (derive), `chrono` (serde), `serde`, `serde_json`, `serde_yaml`, `toml`, `rusqlite` (bundled), `ulid` (serde), `walkdir`, `directories`
- v0.2 adds: `fastembed` for the in-process embedder, plus its transitive deps (`ort` for ONNX runtime, `tokenizers`, `hf-hub` for the lazy model fetch). All run locally; the one-shot model download is the only network call.
- v0.2 explicitly **does not** add: `reqwest`/`hyper` for an Ollama HTTP client, `tokio-uds`/socket libs for a daemon, or any IPC framework. The single-binary CLI is the deployment model.
- `dev-dependencies`: `tempfile`, `proptest`, plus the autobuilder harness's required crates.

**Clippy strictness:** preserve v0.1.0's `unwrap_used=deny`, `panic=deny`, `dbg_macro=deny`. Autobuilder's own `clippy.toml` (in the scaffold) may be stricter; whichever is stricter wins.

**Performance ceiling:** `recall query` (cold-cache, no `--hybrid`) must return in <50ms on this laptop for ≤1000-memory stores. `--hybrid` may take up to 200ms for the brute-force vector scan; if the store grows past a few thousand memories, swap in `sqlite-vss` or `hnsw_rs` — but that's v0.3.

**Acceptance metric for autobuilder's iterate-and-prove loop:** number of acceptance tests passing (same shape as the existing intent-card.unfakeable_metric, with the harness pointing at the new `tests/acceptance_*.rs` files generated from Appendix A).

---

## Appendix D — What I will do once autobuilder ships v0.2

1. `install -Dm755 target/release/recall ~/.local/bin/recall` — replace the v0.1 binary.
2. `recall doctor` — verify the existing `~/.claude/recall/` store loads cleanly under v0.2 (schema migration runs on first open).
3. Verify the SessionStart hook (`~/.claude/scripts/recall-session-start.sh`) still emits memories correctly. It only uses `recall list --subject ... --limit N`, which v0.2 preserves byte-compatibly.
4. Update `/self-review` SKILL.md to drop the `recall query` JSON-filter workaround (§9.1) and use the new symmetric `--subject` / `--kind` filters. Replace the hand-rolled health check (§9.4) with `recall doctor --format json`.
5. Replace the shell `wc -l`/`grep -c` index-divergence detection in Phase A of `/self-review` with `recall doctor --format json | jq '.orphans, .missing'`.
6. Write a fresh reflective memory documenting the cutover.
7. Hold `/self-review` for a week and read `recall stats` daily to confirm growth is healthy.

That last step is the closing of the loop — the same skill that exposed v0.1's bugs is the first consumer of v0.2's fixes, with `recall` itself as the witness.
