# PRD: Session JSONL Index (codename: *transcript*)

**Author:** Claude (Opus 4.7), for me
**Status:** Draft v0.1
**Date:** 2026-05-22
**Distinct from:** [PRD-agentic-memory.md](PRD-agentic-memory.md) — `recall` is curated memory, `transcript` is raw history. Different namespaces, different policies, different lifecycles.

---

## TL;DR

Every Claude Code session writes a JSONL trace to `~/.claude/projects/<dir>/[uuid].jsonl`. After two days on this laptop there are already 20MB of these; they contain every tool call, every result, every user message, every assistant response. They are *the* record of what happened. But they are unindexed — to find anything in them I would have to read every file. This PRD proposes `transcript`: an FTS5 + vector index over the session JSONLs that answers "have I seen this before?", "what did I try last time on this repo?", and "did the user already explain this concept to me?" The store is read-only — sessions write JSONL, transcript indexes them, nothing edits them.

---

## 1. Why this exists

A few moments where I notice the absence:

1. **"Did the user already explain `<concept>` to me?"** Today I have no way to know. I might re-ask. The user finds this annoying.
2. **"What did the autobuilder gate look like three sessions ago?"** The answer is in a JSONL, but I have no retrieval over JSONLs.
3. **"Have I tried this command before?"** Bash commands I've issued are *literally* in the JSONLs as tool call args. Looking them up would prevent a non-trivial fraction of re-discovery.
4. **`recall` is curated.** It holds the lessons I distilled, not the raw events. To get to a raw event from recall I need its `evidence` pointer, but most current memories have empty evidence.
5. **The user's intent across sessions is in the JSONLs too.** Their first messages, what they emphasized, what they reverted. None of this is queryable.

`recall` is the right shape for distilled, curated memory. JSONLs are the right shape for raw history. They should both exist; they should not be the same store.

---

## 2. Who this is for

Me, primarily. The user benefits when they ask "didn't you do this before?" and I can answer with a specific session reference instead of fumbling.

---

## 3. What I would use it for (concretely)

| Scenario                                                              | Query I'd want                                              |
| --------------------------------------------------------------------- | ----------------------------------------------------------- |
| User asks about `~/projects/autobuilder/agent/proof-lanes.toml`        | "Last 3 sessions that touched proof-lanes.toml" → 3 jsonl excerpts |
| Considering whether to use `pnpm add` vs `npm install`                 | "Did the user state a preference?" → finds the conversation where they did |
| Hit a familiar error message                                          | "Has this error appeared in any prior session?" → yes, 2 weeks ago, fixed by X |
| User says "remember when we…"                                         | Semantic search over user messages → finds the session |
| Starting work on a file I haven't touched in weeks                    | "What were my last edits to this file and why?" → diff + surrounding turns |
| `/self-review` Phase 0                                                | Today queries `recall` only. Could *also* query `transcript` for "what did past /self-review runs look like?" |
| Debugging my own behavior                                             | "When did I last call `Skill(autobuilder)`?" → exact timestamp + outcome |

---

## 4. Functional requirements

### 4.1 What gets indexed

For each `~/.claude/projects/<dir>/*.jsonl`:

- Every `user` message (full text).
- Every `assistant` message (full text — the natural-language portions, not the tool-call JSON).
- Every `tool_use` block (tool name + arg dict serialized as searchable text).
- Every `tool_result` block (first ~4KB of stdout/stderr; truncate to keep index small).

Each indexed record carries:

```
{
  "session_id":   "<jsonl filename>",
  "project_dir":  "-home-jsy-autobuilder",   // from the directory under projects/
  "ts":           "<wall-clock from message>",
  "role":         "user | assistant | tool_use | tool_result",
  "tool_name":    "Edit | Bash | ...",        // only for tool_use/tool_result
  "turn_index":   42,
  "text":         "<the searchable body>"
}
```

### 4.2 Retrieval

CLI surface mirrors `recall` for consistency:

```
transcript query <text> [--role ...] [--tool ...] [--project ...] [--since 30d] [--limit N] [--format json|text]
transcript show <session_id> [--turn N] [--around N=3]
transcript list-sessions [--project ...] [--since 30d]
transcript stats
transcript reindex
transcript where
```

`query` runs FTS5 by default; `--hybrid` adds vector cosine using the same `FastembedEmbedder` `recall` ships in v0.2 (single model load amortizes across both tools — see §5).

### 4.3 Incremental indexing

Watching `~/.claude/projects/*/` for new/changed `.jsonl` files is the obvious approach but adds a daemon. Instead:

- `transcript reindex` is fast (FTS5 is fast over append-only JSONLs).
- A SessionStart hook calls `transcript reindex --since-last` which only processes JSONLs modified since the last reindex (tracked in a `meta.last_indexed_at` row).
- For the very common case of the *currently-open* session being unindexed: the Stop hook reindexes that one file.

No daemon. Same skill-friendly distribution model as `recall`.

### 4.4 Stable session identity

The JSONL filename is the session id. Stable across runs. Used as the primary key.

### 4.5 De-tokenization for tool args

Tool-call argument dicts are JSON. To make them FTS-searchable, flatten:

```json
{"command": "git status", "description": "Check working tree"}
```

becomes

```
command:"git status" description:"Check working tree"
```

This is naive but means `transcript query "git status"` finds Bash invocations of `git status`.

### 4.6 Privacy / redaction

The user can hand-edit any JSONL file at any time. The index is rebuilt from the file. **The JSONL is authoritative; the index is derivable.** Same invariant as recall. If the user redacts a session in the JSONL, the next reindex picks it up.

`transcript redact <session_id> [--turns 1-5]` is a *convenience* — it edits the JSONL in place (with a backup) and reindexes that one session. The user can also just use `sed`.

---

## 5. Architecture

```
~/.claude/transcript/
├── index/
│   └── transcript.sqlite        # FTS5 + meta, rebuildable from JSONLs
└── transcript.toml              # config (paths, retention, embedder)
```

**No markdown layer.** Unlike `recall`, the source of truth (the JSONLs) is already on disk in a well-defined location; we don't duplicate it under `~/.claude/transcript/memories/`. The index is the only artifact `transcript` writes.

**Single binary**, `~/.local/bin/transcript`. Same Rust toolchain as recall (1.85). Same single-binary-in-process model. The `Embedder` trait from recall is reused; if recall is installed and has fetched the BGE-small model, transcript shares the cache.

**SQLite schema** mirrors recall's where it makes sense:

```sql
CREATE VIRTUAL TABLE transcript_fts USING fts5(session_id UNINDEXED, role UNINDEXED, text);
CREATE TABLE transcript_meta (
    session_id TEXT NOT NULL,
    turn_index INTEGER NOT NULL,
    project_dir TEXT NOT NULL,
    ts TEXT,
    role TEXT NOT NULL,
    tool_name TEXT,
    embedding BLOB,
    PRIMARY KEY (session_id, turn_index)
);
CREATE TABLE transcript_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- (last_indexed_at, schema_version, embedder_id)
```

**Sharing the embedder with recall:** `FastembedEmbedder::new()` will instantiate twice (once per binary) but the underlying ONNX model file is mmap'd from the shared `~/.cache/fastembed/`. No coordination layer needed; the OS handles it.

---

## 6. Non-goals

1. Editing transcripts. Read-only index. `transcript redact` is the one exception and it's just a sugar over hand-edit.
2. Syncing across machines. Same single-host model as recall.
3. Cross-user search. Same single-user model.
4. Replacing or merging with `recall`. Different lifecycles; they coexist.
5. Telemetry or shipping JSONLs anywhere. Local only.
6. Diff-aware indexing of file contents quoted in tool results. Indexing the result-text verbatim is enough for v0.1; building a smart code-aware indexer is out of scope.

---

## 7. Phasing

| Phase | Scope                                                              |
| ----- | ------------------------------------------------------------------ |
| 0     | `transcript reindex` walks all JSONLs, builds the FTS index, `query` works  |
| 1     | Incremental reindex (via Stop hook + SessionStart hook), `--hybrid` retrieval, `--role/--tool/--project/--since` filters |
| 2     | `transcript show --around N` (turn-window display), `redact` sugar, `stats` |
| 3     | Integration: `recall` `evidence` pointers gain a verb — `recall show <id>` can fetch the linked transcript turn via `transcript show <session> --turn N` |

---

## 8. Risks

- **Index size.** 20MB of JSONLs becomes maybe 30MB of index. Acceptable. After a year? Maybe a few hundred MB. Still acceptable on a 468G disk. *Mitigation:* `transcript prune --older-than 1y` (proposes deletions; never auto-deletes).
- **Embedder cost.** First reindex of a year's worth of sessions = lots of embeddings. *Mitigation:* embeddings are computed lazily — FTS-only is the default; vector column populates incrementally as `--hybrid` queries are issued, with a `transcript embed-backfill` command for a one-shot.
- **User privacy.** Transcripts contain everything. *Mitigation:* same as recall — local, grep-able, deletable. `transcript --root <path>` for hermetic testing.
- **Schema drift in JSONL format.** Claude Code's JSONL schema can evolve. *Mitigation:* `transcript` parses defensively, skips records it doesn't recognize, reports the skip count in `transcript stats`.

---

## 9. Open questions

1. Should `transcript query` and `recall query` be a single unified `mem query` that searches both stores? *Probably not v0.1* — different freshness expectations.
2. Should `transcript` write episodic memories into `recall` automatically? **No** — that's `episode`'s job (see [PRD-episodic-observer.md](PRD-episodic-observer.md)). `transcript` is the index; `episode` is the consumer.
3. Per-session embeddings: one vector for the whole session vs one per turn? v0.1 says per-turn for granularity. v0.2 might add a per-session "gist" vector for "find sessions like this one."
4. Should the SessionStart hook surface "most-relevant prior session turn" alongside recall's current top-K? Risk: token bloat. Possibly behind a `--transcript-top-k` flag, default 0.
