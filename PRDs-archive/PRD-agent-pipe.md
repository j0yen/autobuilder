# PRD: Agent-Pipe — structured composition for agent tools (codename: *apipe*)

**Author:** Claude (Opus 4.7), for me
**Status:** Draft v0.1 — shared record schema + thin runtime
**Date:** 2026-05-22
**Threads through:** recall, transcript, fsstory, episode, spool, memlog. Makes the toolchain compose.

---

## TL;DR

I have eight wintermute tools (sbx, pevent, wchg, procstat, txn-edit, tcap, ctrace, bpolicy) plus recall, plus four planned (transcript, fsstory, episode, spool, memlog) — and growing. Each is fine in isolation; together they're an awkward set of CLIs glued by `jq` and shell pipes that lose type information at every boundary. The Unix pipeline model is great for bytes-and-lines (`grep | sort | uniq -c`) but terrible for structured records (recall hits, transcript turns, file events). `apipe` is a shared **record schema** (CBOR-encoded or NDJSON, same shape either way) plus a thin runtime that handles serialization, type negotiation between stages, and the obvious convenience commands. A real example: `recall query 'self-review' | apipe context --turns 3 | apipe attribute | apipe top 5` becomes a meaningful pipeline because each stage understands the records flowing through. Today this is six shell scripts and a `jq` cheatsheet.

---

## 1. Why this exists

Today's "compose two agent tools" pattern:

```sh
recall query 'self-review' --format json \
  | jq -r '.[] | .id' \
  | while read id; do
      # I now have an id but lost everything else; have to re-query
      transcript show <session> --turn $turn  # but I don't have session/turn here either
    done
```

That's painful. The recall hit had `evidence` pointing at a session/turn; the JSON-then-jq pipeline lost the structure. I rebuild it by re-querying. Multiply this by every tool pair and the toolchain doesn't actually chain.

The fix is a shared schema where every tool emits the *same* record shape, every tool accepts that shape as input, and downstream tools enrich the records rather than reducing them to text and back.

---

## 2. Who this is for

Me, plus every tool I or autobuilder writes that wants to compose with the others. The user benefits because composed queries get more powerful while staying readable.

---

## 3. What I'd use it for (concretely)

| Today                                                                             | With apipe |
| --------------------------------------------------------------------------------- | ---------- |
| "Find recall memories about self-review, show the originating session turn"        | `recall query 'self-review' | apipe expand-evidence | apipe context --turns 3` |
| "List files I wrote this week and the tool I used for each"                       | `fsstory ls ~/.claude --since 7d --by-actor=me | apipe drop-actor-claude-bash | apipe sort -k tool` |
| "Recall memories that mention files I touched this week"                          | `wchg since ~/.claude | apipe to-paths | recall query --from-paths` |
| "All compaction events from memlog enriched with the recall episodic memory I wrote about them" | `memlog show --since 30d | apipe match-episode | apipe summarize-by week` |
| "Permissions prompts repeated 3+ times this week, with the surrounding session context" | `spool report --redirect-pattern 'permission' | apipe context --turns 2 | apipe group-by-skill` |

---

## 4. Functional requirements

### 4.1 The shared record schema

Every record is a JSON object with:

```json
{
  "kind":       "recall_hit | transcript_turn | file_event | memlog_record | spool_entry | episodic_candidate | enriched",
  "source":     "recall:01KS... | transcript:<session>:<turn> | fsstory:<path> | ...",
  "ts":         "<RFC3339>",
  "id":         "<stable id, unique within (kind, source)>",
  "session_id": "<agent_session_id if applicable>",
  "subject":    "<recall-style subject or null>",
  "score":      0.0,
  "payload":    { /* kind-specific structured data */ },
  "annotations": [ /* enrichments added by upstream stages */ ]
}
```

The `payload` shape per `kind` is documented in `apipe schema --kind <name>`. Schema is versioned (`schema_version: 1`).

NDJSON wire format by default (one record per line); CBOR for binary efficiency when the pipeline is long.

### 4.2 The `apipe` runtime

```
apipe schema [--kind <name>]                    # print the schema
apipe pass                                       # passthrough, useful for type-checking a pipeline
apipe expand-evidence                            # follow recall.evidence pointers, emit transcript_turn records
apipe context --turns N [--around id]            # given any record with session_id+turn_index, fetch surrounding turns
apipe attribute                                  # given a file_event, fill in actor via fsstory
apipe top N                                      # take top N by score
apipe sort -k <field>
apipe group-by <field>
apipe summarize-by <window>                      # daily/weekly/monthly histograms
apipe match-episode                              # join memlog records with recall episodic memories about them
apipe filter <expr>                              # boolean expressions on fields
apipe drop <field>...                            # remove fields for compactness
apipe pretty [--columns ...]                     # human-readable table at the end of a pipeline
apipe to-paths                                   # extract file_event.path-like fields, emit as bare strings (back to unix-pipe land)
apipe from-paths                                 # the inverse — wrap bare paths in records
```

Each subcommand is a tiny streaming filter. Built into one binary so a pipeline is `apipe sub1 | apipe sub2`; for ergonomics, the multi-stage form `apipe pipeline 'sub1 | sub2 | sub3'` is sugar.

### 4.3 Adapter mode in existing tools

Each existing tool (recall, transcript, fsstory, etc.) grows a `--format apipe` output mode that emits the shared schema directly. Today's `--format json` stays for backward compat and human use; `--format apipe` is the canonical pipeline format.

The recall `--format json` produces:

```json
[{"id":"01KS...", "kind":"reflective", "subject":"self", ...}]
```

The recall `--format apipe` produces:

```
{"kind":"recall_hit","source":"recall:01KS...","ts":"...","id":"01KS...","session_id":null,"subject":"self","score":3.87,"payload":{"recall_kind":"reflective","body_snippet":"…","confidence":0.5,"recall_count":2},"annotations":[]}
```

One per line, NDJSON.

### 4.4 Pipeline-level features

- **Streaming.** Every stage is line-oriented; nothing buffers the whole stream unless explicitly required (`apipe sort` does).
- **Type checking.** `apipe check 'recall query "x" --format apipe | apipe context'` lints the pipeline before running; reports type mismatches.
- **Backpressure.** If a downstream stage is slow (e.g. `apipe attribute` doing per-record fsstory lookups), upstream stages block. Standard pipe semantics.
- **Errors.** A malformed record (failed parse) is emitted on stderr with the original line and an error message; the rest of the pipeline continues.

### 4.5 Backward-compatible escape hatches

`apipe to-paths` and `apipe to-ids` emit bare strings — back into normal unix-pipe land. So `apipe ... | apipe to-paths | xargs sed -i ...` still works.

---

## 5. Architecture

```
~/.local/bin/apipe              # the runtime (one binary, all subcommands)
~/.local/share/apipe/schemas/   # JSON Schema files for each record kind, versioned
```

Single Rust binary, ~1500 LoC. Heavy use of `serde_json::Value` for streaming; per-kind Serde structs where ergonomic. Subcommands are dispatched in `main.rs`.

Each existing tool gets a small PR to add `--format apipe`. The PR is mechanical (already-structured data; just remap to the shared schema).

---

## 6. Non-goals

1. **A new query language.** apipe is shell-composable filters, not a SQL replacement. If you want SQL, run `recall reindex` and then use sqlite3.
2. **Persistent state between stages.** Stages are streaming filters. Stateful needs (joins across non-streaming sources) go through `apipe expand-evidence`-style explicit lookups.
3. **Replacing jq.** jq is great for ad-hoc JSON; apipe is for the agent-tool record schema specifically. Use jq for everything else.
4. **Tool-discovery / routing.** apipe knows about the agent-tool record schema; it does not orchestrate which tool to call. That's still my job.

---

## 7. Phasing

| Phase | Scope                                                              |
| ----- | ------------------------------------------------------------------ |
| 0     | Schema v1 (recall_hit, transcript_turn, file_event). `apipe pass`/`pretty`/`top`/`sort`. recall and transcript ship `--format apipe`. |
| 1     | `apipe expand-evidence`/`context`/`attribute`. fsstory ships `--format apipe`. |
| 2     | `apipe group-by`/`summarize-by`/`filter`. memlog + spool + episode adapters. |
| 3     | `apipe check` (type-check pipelines), CBOR wire format, `apipe pipeline 'a | b'` sugar. |

---

## 8. Risks

- **Schema evolution.** Adding a field to `recall_hit` shouldn't break downstream consumers. *Mitigation:* fields are additive only; `schema_version` is bumped only on breaking changes (and there's a `apipe migrate` step).
- **Coupling.** Every tool now depends on the shared schema. *Mitigation:* the schema is small and the per-kind shapes are documented in `~/.local/share/apipe/schemas/`. Tools can be updated independently as long as they emit valid records.
- **Performance.** NDJSON parsing per line is ~10x slower than passing bytes around. *Mitigation:* CBOR mode for long pipelines; for short pipelines (typical case) NDJSON is fine.

---

## 9. Open questions

1. Should `apipe` ship its own embedded jq/jaq for inline transformations? `apipe filter '.score > 0.8'` could call into jaq, sparing a separate stage. Probably yes.
2. Should pipelines be storable as named "recipes" — `apipe recipe weekly-review` runs a saved multi-stage pipeline? Useful for `/self-review` Phase A's hand-rolled aggregations. Probably yes; would be tiny addition.
3. Should there be a "pipeline visualizer" — `apipe explain ...` draws a DAG of the stages with their expected I/O types? Cute; defer.
4. CBOR vs MessagePack vs Cap'n Proto for the binary format? CBOR for now (consistency with memlog). Switch is cheap.
