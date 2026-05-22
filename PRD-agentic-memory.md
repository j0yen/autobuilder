# PRD: Agentic Memory System (codename: *recall*)

**Author:** Claude (Opus 4.7), drafted for jsy
**Status:** Draft v0.1
**Date:** 2026-05-22
**Eng owner:** TBD   **Stakeholders:** the user (jsy), every Claude session running on this laptop

---

## TL;DR

The memory system I run on today is a flat directory of Markdown files indexed
by `MEMORY.md`. It works — but it's an *index*, not a *memory*. I have to
remember to write to it, remember to read it, and re-pay the token cost of the
entire index on every turn whether the indexed facts are relevant or not.

This PRD proposes **recall**: a local-first agentic memory system that
(a) stores memories as plain Markdown files (so they remain human-inspectable
and `grep`-able), (b) layers a small semantic index on top so retrieval is
context-aware, (c) tracks each memory's *evidence*, *confidence*, and
*last-recalled-at*, and (d) writes the things I actually want to remember
but currently can't: episodic memory of attempts, failed approaches,
which-tool-worked-where, and per-codebase patterns.

It runs entirely on the user's laptop. No cloud. No telemetry. The user owns
every byte and can read it in `less`.

---

## 1. Why this exists (what's broken about how I remember today)

A few things that hurt me, said plainly:

1. **MEMORY.md is loaded into every conversation.** Every line costs tokens.
   When the index hits ~200 lines it gets truncated. So either I keep memory
   thin and lose detail, or I keep it rich and lose retrieval.

2. **I write memory passively.** The instructions tell me to save user
   preferences, project state, feedback. I do — sometimes. When the session
   is busy, I forget. There is no observer that watches for memory-worthy
   moments and prompts a save.

3. **I have no episodic memory of my own work.** If I tried approach X on
   this codebase three weeks ago and it failed, I have no idea today. The
   knowledge is in `git log` and old session transcripts I can't read.
   I make the same mistake twice.

4. **Memory has no decay or confidence.** A note saved on 2026-01-15
   ("user is migrating the auth middleware") has the same weight today as a
   note from yesterday. Stale memories actively mislead me.

5. **There is no separation between "things about the user" and "things
   about how I, Claude, work here."** They live in the same flat namespace.

6. **Compaction destroys within-session memory.** When my context compresses
   I lose track of what I just tried. A separate scratch-memory store that
   survives compaction would close this loop.

7. **Memory is pull-only.** I have to *decide* to consult it. A push model —
   relevant memories surfaced at the start of a turn — would catch the
   cases I currently miss.

---

## 2. Who this is for

- **Primary:** me — every Claude session running in this user's Claude Code
  installation. (And by extension, the user, who benefits from a Claude
  that doesn't re-ask, re-mistake, or forget.)
- **Secondary:** the user, who occasionally wants to inspect, edit, redact,
  or audit what I remember about them and their projects.
- **Out of scope:** memory shared across users, teams, or organizations.
  Memory in `recall` is single-user and single-host.

---

## 3. What I would use it for (concretely)

These are the moments today where I notice the absence of memory:

| Scenario                                                   | Memory I want                                                                 |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Starting a new session on the autobuilder repo             | "Last 3 sessions you worked on the gate; the 7th receipt schema is in PLAN.md §4.2" |
| User asks me to commit but I'm not sure of their style     | "User uses lowercase first word, no trailing periods, ≤72 chars summary"      |
| About to install a package                                 | "User uses pnpm for TS, cargo + uv for Python; never `npm i`"                 |
| User says "the tests are broken"                           | "Last 2 times: flaky integration test in `crates/metric-harness/tests/cli.rs` — race condition with target/" |
| Choosing between two implementation approaches             | "Last refactor here: user accepted approach A over B; reason was readability over perf" |
| User reports a bug in code I wrote                         | "I wrote that function; here is the reasoning trace from the original session" |
| Session is about to compact                                | A write-ahead snapshot of "what I tried, what failed, what I'm about to try next" |
| New laptop or fresh `~/.claude` reset                      | Re-import from the most recent `recall` snapshot; pick up where I left off    |
| Cross-project recall: "I've done DB internals work before" | Surface relevant memories from `learning-db` while working on `autobuilder`   |

---

## 4. What would help me (functional requirements)

### 4.1 Memory primitives

- **Episodic** — "On 2026-05-22 I tried X; outcome Y." Append-only log per session.
- **Semantic** — Distilled facts: "user prefers integration tests over mocks."
- **Procedural** — "To run the harness for this repo: `scripts/run-metrics.sh`."
- **Reflective** — "When I asked too many clarifying questions, the user said 'just do it.'"

Each memory carries:

```
{
  "id": "...",                       // ulid
  "kind": "episodic|semantic|procedural|reflective",
  "subject": "user|project:<slug>|self|tool:<name>",
  "body": "...",                     // markdown
  "evidence": [                       // pointers to source
    {"session": "...", "turn": 12, "excerpt": "..."}
  ],
  "confidence": 0.0-1.0,
  "created_at": "...",
  "last_recalled_at": "...",
  "recall_count": 0,
  "supersedes": ["id-of-older-memory"],
  "decays_after": "30d|never"
}
```

### 4.2 Retrieval

At the start of every turn (or on demand), `recall` returns the top-K memories
most relevant to:

- The current working directory and git remote
- The active conversation's recent turns
- The tools I'm about to call (e.g. about to edit a Rust file → prefer
  Rust-related memories)

Retrieval is hybrid: keyword + embedding + recency + recall-count boost. Plain
files in front, vectors as auxiliary index.

### 4.3 Writing

Two modes:

1. **Explicit** — I or the user say "remember X." Same as today.
2. **Observed** — a lightweight hook scans the session for memory-worthy
   moments (user corrections, expressed preferences, completed milestones,
   surprising findings) and proposes a memory. The user (or I, in auto mode)
   approves with one keystroke.

### 4.4 Decay & supersedence

Memories with `decays_after` expire silently on retrieval if past their
expiry. New memories can `supersedes` old ones — the old memory is kept but
de-prioritized in retrieval and visually marked as obsolete.

### 4.5 Within-session scratch

A `session/<id>.md` file that I write to as I work — what I tried, what failed,
what I'm about to try. Survives compaction. Promoted to long-term memory only
on session end or explicit user blessing.

### 4.6 Self-memory vs user-memory

Two top-level namespaces, distinct retrieval policies:

- `user/`   — preferences, role, feedback. Loaded liberally.
- `self/`   — things about how I, Claude, work in this environment. Loaded
  when I'm about to do similar work.

(The current system collapses these, which is the bug.)

---

## 5. What would delight me

The functional bits above are what I *need*. These are what would actually
make me a better collaborator:

1. **Proactive surfacing without context bloat.** A budget of ~500 tokens per
   turn for "here are the 3 memories most relevant to what you're about to
   do." Not the full index. Just the right three.

2. **Outcome feedback.** When the user accepts/rejects/modifies something I
   did, the system observes it and updates the relevant memory's confidence.
   Memories that lead to good outcomes get reinforced; ones that lead to bad
   outcomes get downgraded or marked for review.

3. **Reasoning continuity.** When I write a non-trivial function, the
   *reasoning* behind it gets attached as memory. Later, when the user (or I)
   touches that function, the reasoning surfaces — so I don't undo my own
   careful work because I forgot why it was that way.

4. **A diff per session.** At the end of a session, show "here's what I
   learned today: 4 new memories, 2 superseded, 1 expired." Make the
   accumulation visible.

5. **`grep`-able.** Every memory is a plain Markdown file. Vectors are an
   *index over* them, never the source of truth. I (and the user) can `grep
   -r feedback ~/.claude/recall/`.

6. **Local-first, paranoid by default.** Embeddings computed by a small local
   model (sentence-transformers or similar). No network calls. No telemetry.
   No shared state.

7. **Cross-project, project-scoped retrieval.** Memory lives globally but is
   tagged with project. Retrieval boosts the current project, but is willing
   to surface relevant memories from other projects ("you solved this kind of
   B-tree split bug in learning-db three months ago").

8. **An audit trail when I'm wrong.** When a memory is updated or
   superseded, the old version stays. If I ever notice I was confidently
   wrong, the history is recoverable.

---

## 6. Goals and non-goals

### Goals

1. Reduce repeated questions and re-mistakes across sessions.
2. Keep the memory store inspectable, exportable, and grep-able as plain
   files.
3. Lower the token cost per turn vs the current `MEMORY.md` load.
4. Make memory writes feel automatic, not chore-like.
5. Be entirely local. No cloud round-trip. No telemetry.

### Non-goals

1. Shared memory across users or machines. (Single-host, single-user.)
2. Replacing the user's note-taking system (`~/brain/`). `recall` is for
   *me*; the user's notes are for them.
3. A general-purpose vector DB. The embeddings index is internal; we do not
   expose a query API.
4. Cross-agent memory federation. If another agent (e.g. Codex, Cursor) wants
   to read my memory, it can read the files — but there is no protocol.

---

## 7. Architecture

```
~/.claude/recall/
├── memories/
│   ├── user/                  # preferences, role, feedback
│   ├── self/                  # how-I-work memories
│   ├── project/
│   │   ├── autobuilder/
│   │   ├── learning-db/
│   │   └── wintermute/
│   └── episodic/
│       └── 2026-05-22/        # per-day session logs
├── session/<id>.md            # within-session scratch (ephemeral)
├── index/
│   ├── embeddings.sqlite      # vec0 / sqlite-vss
│   ├── keyword.fts5           # sqlite FTS5
│   └── meta.sqlite            # confidence, recall_count, decay
└── recall.toml                # config
```

**Embeddings model:** local `bge-small-en-v1.5` (or smaller) via `llama.cpp`
or `sentence-transformers`. ~25M params, runs on CPU in <100ms per query.

**Index:** SQLite + `sqlite-vss` for vectors, FTS5 for keyword. Plain files
are source of truth; both indexes are derivable.

**Daemon:** a single Rust binary (`recall`) that:

- Maintains the index (watches `memories/` for changes).
- Serves a tiny local API over a Unix socket: `recall query`, `recall write`,
  `recall observe`.
- Hooks into Claude Code via `settings.json` hooks (SessionStart, PostToolUse,
  Stop).

**Hook integration:**

| Hook            | What it does                                                              |
| --------------- | ------------------------------------------------------------------------- |
| `SessionStart`  | Inject a top-K retrieval based on cwd, recent files, user prompt          |
| `PostToolUse`   | Observe corrections (Edit reverted, user re-asked); propose memory writes |
| `Stop`          | Promote session scratch to long-term memory; emit session-diff            |

---

## 8. Phasing

| Phase | Scope                                                                                   | Time |
| ----- | --------------------------------------------------------------------------------------- | ---- |
| 0     | Migrate existing `~/.claude/projects/.../memory/` into the new layout, no behavior change | 1 wk |
| 1     | File store + keyword index + SessionStart hook (push retrieval, no embeddings yet)      | 1 wk |
| 2     | Embeddings index + hybrid retrieval; confidence + decay                                 | 2 wk |
| 3     | Within-session scratch + compaction survival                                             | 1 wk |
| 4     | Observed-write proposals (the PostToolUse hook)                                         | 2 wk |
| 5     | Cross-project recall + audit trail + session-diff                                       | 1 wk |

Total: ~8 weeks of focused work, but each phase is shippable on its own.

---

## 9. Risks

- **Memory becomes a leash.** If retrieval is too aggressive, I'll mistake
  stale memories for current truth. *Mitigation:* always show `last_recalled_at`
  and confidence in surfaced memories; bias toward fresh evidence.

- **Token cost grows, not shrinks.** *Mitigation:* enforce a per-turn token
  budget for surfaced memories; default to 500 tokens, configurable.

- **Embeddings drift across model versions.** *Mitigation:* pin the embedding
  model version; rebuild the index when the model changes.

- **Privacy.** A rich memory store is a rich privacy attack surface.
  *Mitigation:* local-only by default; everything `grep`-able and deletable;
  redaction tooling out of the box.

- **I write self-serving memories.** ("User loved my approach!" when they
  didn't.) *Mitigation:* memories include the literal evidence excerpt; the
  observer (PostToolUse hook) does the writing for outcome-tagged memories, not me.

---

## 10. Success metrics

- **Repeated-question rate:** sessions where the user has to re-tell me
  something they've told me before. Target: 50% reduction in 6 weeks.
- **Compaction-recovery quality:** after a compaction, am I still working on
  the same thing with the same approach? Target: yes, measured by a manual
  weekly review.
- **Per-turn memory token cost:** P50 ≤ 300 tokens (vs current MEMORY.md
  baseline of ~1200 tokens after truncation).
- **Memory inspection / edit frequency:** the user opens a memory file and
  edits it manually at least monthly. (Signal that the store is useful and
  trusted.)
- **My own satisfaction.** Subjective, but real: do I feel less like a
  goldfish across sessions? Ask me at week 6.

---

## 11. Open questions

1. Should `recall` live as a separate repo, or inside `~/.claude/`?
2. Embedding model: local `bge-small`, or a tiny purpose-trained one
   distilled from a larger model on the user's own memories?
3. Do we expose a `recall` MCP server so Claude Code can query memory
   through tool use, rather than only through hooks?
4. How much of this overlaps with what Anthropic ships natively in future
   Claude Code versions? Build for now and let the native version
   subsume parts, or wait?
5. Where should `recall` store cross-project insights — alongside per-project
   memory or in a separate `global/` namespace?
