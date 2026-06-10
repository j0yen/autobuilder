# autobuilder-proposal-aggregator

Clusters autobuilder self-evolve proposal JSONs into a ranked `hardening-backlog.json` by recurrence across distinct build slugs.

## TL;DR

Autobuilder Stage-5 writes one `evolution-*.json` per run into `~/.claude/skills/autobuilder/proposals/`. This CLI reads that pile plus `applied.log`, clusters suggestions by `target_file` and rationale similarity (lexical Jaccard ≥ 0.5), ranks by distinct-slug recurrence, and emits a backlog — so you can answer "what's the most-recurring unaddressed harness gap?" without eyeballing 21+ files.

## Usage

```
autobuilder-proposal-aggregator \
  --proposals-dir <dir>     # default ~/.claude/skills/autobuilder/proposals
  [--applied-log <file>]    # default <proposals-dir>/applied.log
  [--min-recurrence 1]      # only show clusters hit by >= N distinct slugs
  [--format json|human]
```

### Example output (JSON)

```json
{
  "backlog": "hardening.v1",
  "generated_proposals_read": 21,
  "clusters": [
    {
      "target_file": "templates/scaffold/tests/integration_cli.rs.tmpl",
      "kind": "template_addition",
      "recurrence": 2,
      "slugs": ["mqo-mcp-server", "mqo-spec"],
      "exemplar_rationale": "Subprocess-orchestration projects have binary dispatch arms cargo test cannot reach.",
      "status": "open"
    }
  ],
  "coverage": { "applied_filtered": 3, "unparseable_skipped": 0, "clusters_total": 9 }
}
```

## Acceptance criteria

| # | Description |
|---|-------------|
| AC1 | Two proposals for distinct slugs targeting the same file → one cluster with `recurrence:2` and both slugs |
| AC2 | `suggestions[]` shape and top-level `PatchSuggestion` shape both normalized and clustered |
| AC3 | `applied-suggestion:<sha>` in `applied.log` → filtered out, counted in `coverage.applied_filtered` |
| AC4 | `#REJECTED: <id>` in `applied.log` → suppresses proposal |
| AC5 | `--min-recurrence 2` omits single-slug clusters from output (still counted in `coverage.clusters_total`) |
| AC6 | Unparseable `.json` skipped with stderr note, counted in `coverage.unparseable_skipped`, run continues |
| AC7 | Output deterministic: clusters sorted `(recurrence desc, target_file asc)`, slugs sorted within each cluster |

## Install

```bash
cargo install --path .
```

Or build in-place:

```bash
cargo build --release
# binary at: target/release/autobuilder-proposal-aggregator
```

## Implementation notes

- **No network, no embedder.** Lexical Jaccard on whitespace+punctuation token sets (threshold 0.5). Runs fully offline.
- **Tolerant parser.** Tries three schema shapes in sequence: `suggestions[]` array, top-level `PatchSuggestion` with `target_file`, flat single-record with `target`. Unrecognized files are skipped with a stderr note — never aborts.
- **`applied.log` honoured.** Lines matching `applied-suggestion:<sha>` and `#REJECTED: <id>` both suppress matching suggestions from the output clusters.
- **Deps:** `clap 4`, `serde`, `serde_json`, `tempfile` (dev).
