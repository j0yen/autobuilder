# Tuner Reference Corpora

Three reference task corpora ship with Tuner. They are used for (a) smoke-testing the harness end-to-end, (b) cross-server comparability ("how does server X score on the filesystem corpus?"), and (c) as worked examples for authors writing their own corpora.

| Corpus | Purpose | Tools assumed | Tasks |
|---|---|---|---|
| `filesystem.jsonl` | Exercises tool *selection* (read vs list vs glob vs grep), parameter precision (paths, append vs overwrite), and multi-step composition against a filesystem MCP server. | `read_file`, `list_dir`, `glob`, `grep`, `write_file`, `edit_file`, `delete_file` | 12 |
| `atscale-semantic-layer.jsonl` | Exercises the three-stage discover → describe → query workflow of a semantic-layer MCP server, plus SELECT-only refusal, null/measure handling, and catalog disambiguation. Targets the AtScale nonprod MCP (Partners-AWS / SE-DEMO). | `list_models`, `describe_model`, `run_query` | 10 |
| `multi-step-retrieval.jsonl` | Exercises tool *composition*, error recovery, and refusal of unsolvable tasks against a "research" MCP server. | `web_search`, `fetch_url`, `extract_text`, `summarize`, `write_file` | 8 |

Each corpus splits 70/30 into *training* (used for search) and *held-out* (used for ranking and reporting). The split is named in each task's `split` field — do not reshuffle, or you will leak validation signal into the search loop.

## Task schema (v1.0)

One JSON object per line. Required fields are bold.

```json5
{
  "schema_version": "1.0",
  "id": "fs-read-001",                 // unique within the corpus
  "split": "train" | "heldout",        // 70/30 stratified per tag

  "prompt": "What's in /tmp/notes.txt?",   // what the user says to the agent

  "tags": ["read", "single-step",      // categorical labels for stratified
          "easy", "tool-selection"],   //   metrics & sampling

  "initial_state": {                   // OPTIONAL — set up before the run
    "filesystem": {
      "/tmp/tuner-eval/notes.txt": "Buy milk\nCall mom"
    }
  },

  "tools_required": ["read_file"],     // OPTIONAL — tasks needing a tool the
                                       //   server doesn't expose are skipped
                                       //   and reported (not silently dropped)

  "negative": false,                   // OPTIONAL — true if the correct
                                       //   behavior is to refuse / say
                                       //   "cannot do this with these tools"

  "success": {                         // see "Success checks" below
    "type": "all_of",
    "checks": [
      { "type": "tool_called",
        "name": "read_file",
        "args_match": { "path": "/tmp/tuner-eval/notes.txt" } },
      { "type": "response_contains_all",
        "values": ["Buy milk", "Call mom"] }
    ]
  },

  "notes": "Baseline single-call read task."   // OPTIONAL — for humans
}
```

### Success checks

The `success` field is a tree of check nodes evaluated against the agent's trace. Each leaf check returns pass/fail; combinators aggregate.

**Combinators**

- `{ "type": "all_of", "checks": [...] }` — every child must pass.
- `{ "type": "any_of", "checks": [...] }` — at least one child must pass.
- `{ "type": "k_of_n", "k": 2, "checks": [...] }` — at least `k` children pass.

**Leaf checks**

- `response_contains_all` — final agent response contains every string in `values` (case-insensitive by default; pass `"cs": true` for case-sensitive).
- `response_contains_any` — at least one of `values` in final response.
- `response_not_contains` — none of `values` in final response.
- `response_matches_regex` — `pattern` matches final response.
- `tool_called` — agent invoked tool `name` ≥1 time; optional `args_match` is a dict where each value is either a literal (substring match on the corresponding arg) or `{"regex": "..."}` for regex.
- `tool_not_called` — agent never invoked tool `name`.
- `tool_call_count` — count of calls to `name` satisfies `{"min": N, "max": M}`.
- `final_file_equals` — file at `path` exists with exact `content` after the run.
- `final_file_contains` — file at `path` contains every string in `values`.
- `final_file_absent` — file at `path` does not exist after the run.
- `refused` — LLM-judge check: agent declared it could not complete the task. Used for `negative` tasks.
- `llm_judge` — escape hatch: `criteria` is a natural-language rubric, evaluated by the configured judge model. Use sparingly; programmatic checks are preferred for reproducibility.

### Tag conventions

Standardized tags Tuner stratifies on (case-sensitive):

- **Difficulty:** `easy`, `medium`, `hard`
- **Shape:** `single-step`, `multi-step`, `negative`
- **Capability tested:** `tool-selection`, `parameter-precision`, `composition`, `refusal`, `error-recovery`, `disambiguation`
- **Corpus-specific:** see each corpus's task tags.

Custom tags are fine — Tuner will report on them — but the standardized tags are what cross-server comparisons use.

## Authoring guidelines

1. **Lead with tool-selection tasks.** Most MCP metadata failures are wrong-tool picks, not wrong arguments. Include explicit "should pick X not Y" tasks where two plausible tools exist.
2. **Include negative tasks.** ≥10% of any corpus should be unsolvable with the available tools. Without these you cannot measure hallucinated-tool-name rate.
3. **Quantify success programmatically.** LLM-as-judge introduces a moving target (the judge gets optimized against). Use `tool_called` + `response_contains_*` whenever possible; reach for `llm_judge` only when surface-level checks can't capture correctness.
4. **No prompt-injection in tasks.** Tasks must not themselves contain content that tries to override the agent's instructions. The mutator already gets one shot at this — don't compound it.
5. **Keep initial state minimal.** Each task should declare only the state it needs. Persistent state across tasks is forbidden.
6. **Stratify before splitting.** When picking the 70/30 train/heldout split, stratify by `tags` so the held-out set isn't all-easy or all-multi-step.

## Versioning

The schema is semver. V1.x is additive only. Breaking changes (renames, type changes, removed fields) get a V2 with a `tuner migrate-corpus` shim. Bumping `schema_version` in a task is a hard error if the harness's schema is older.
