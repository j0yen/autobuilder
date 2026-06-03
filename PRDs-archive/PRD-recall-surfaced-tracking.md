# PRD: recall-surfaced-tracking — separate "memory surfaced" from "memory recalled"

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** [visions/fidelity.md](visions/fidelity.md)
build_target: rust-extend
build_into: /home/jsy/wintermute/recall
**Version target:** `recall v0.7.1` (patch — adds column + subcommand
flag; existing surfaces unchanged). If recall-doctor-claims ships first
and consumes v0.7.0, this stays at v0.7.1 cleanly.

---

## TL;DR

Today `recall_count` tracks `query --touch` calls (API-level retrieval),
not hook-injected surfacing. The Stop hook treats every id in
`~/.cache/recall-weather/<sid>/recalled.json` as deserving the same
`+0.02 confidence` bump as a real API recall. We need a separate
`surfaced_count` so that "the hook layer injected this into a session's
context" is a first-class signal — distinguishable from "the user
explicitly asked recall about this" and from "the memory was useful."

This PRD adds the column, the `recall feedback --surfaced <id>...`
subcommand mode, and teaches the SessionStart load + UserPromptSubmit
search-inject hooks to write `surfaced.json` alongside `recalled.json`.
Stop hook applies `--surfaced` on those ids. **No behavior change in
ranking yet** — this is the data plumbing slice. Downstream PRDs
(`recall-use-evidence`, `recall-stop-hook-discriminate`) consume it.

---

## 1. Why this exists

1. **The data model conflates two semantics.** `recall_count` increments
   on `query --touch` (user-driven recall), not on hook injection
   (system-driven surface). The Stop hook's blanket-accept treats them
   the same. Source: `src/index.rs` lines 46 / 62 (struct fields) and
   `hooks/stop.sh` (Stop hook reading `recalled.json`).
2. **Hook surface accumulates without separate accounting.** 158
   `~/.cache/recall-weather/<sid>/recalled.json` files exist as of
   2026-05-28T06:11Z, each one fired the same uniform `+accept` step.
   There's no way to say "this memory was surfaced 50 times but only
   pulled by `query` twice" — both are conflated under `recall_count`.
3. **Adding `surfaced_count` is a small SQLite migration.** No schema
   break: `ALTER TABLE memories_meta ADD COLUMN surfaced_count INTEGER
   NOT NULL DEFAULT 0;` plus a new struct field with `#[serde(default)]`
   for older markdown frontmatter compat.
4. **Hooks already write per-session JSON.** `recalled.json` (the
   accept-list) is written by `recall-session-start.sh` and consumed
   by `recall-stop.sh`. Adding `surfaced.json` is the same pattern,
   different file path.

---

## 2. What this builds

### 2.1 Schema migration

`src/index.rs`:

- Add `surfaced_count: u32` to both `MemoryFront` and `MemoryMeta`
  structs (with `#[serde(default)]`).
- Migration on `Index::open`: `ALTER TABLE memories_meta ADD COLUMN
  surfaced_count INTEGER NOT NULL DEFAULT 0` (idempotent — check via
  PRAGMA table_info or use `IF NOT EXISTS` pattern from prior
  feedback_count migration at index.rs:129–166).
- `MemoryMeta::roundtrip` and the `INSERT … ON CONFLICT` upsert
  include the new column.

### 2.2 New `recall feedback --surfaced` mode

```
recall feedback --surfaced <id> [<id>...]   # increment surfaced_count, no confidence change
```

- Pure counter increment. Does NOT touch confidence, does NOT
  increment `feedback_count` (that column is for accept/reject only).
- Updates SQLite row + markdown frontmatter.
- Existing `--accept` / `--reject` / `--abstain` / `--decay-sweep`
  flags untouched.

### 2.3 Hook surface

- **SessionStart hook** (`~/.claude/scripts/recall-session-start.sh`,
  external to recall repo): already writes `recalled.json` listing the
  ids of memories it loaded. Add a write of `surfaced.json` with the
  same ids. (For SessionStart, surfaced = recalled — they're the
  same event. The separation matters for the search-inject hook,
  next.)
- **UserPromptSubmit hook**
  (`~/.claude/scripts/recall-search-inject.sh`): currently surfaces
  top-5 query matches per prompt but writes nothing. Add a write that
  appends to `surfaced.json` (NOT `recalled.json` — these are
  mid-session surfacings, not start-of-session loads). Use `jq` to
  read, append, dedup, write atomically.
- **Stop hook** (`~/.claude/scripts/recall-stop.sh`): currently calls
  `recall feedback --accept` on `recalled.json` ids. Add a call to
  `recall feedback --surfaced` on `surfaced.json` ids BEFORE the
  accept step. Subsequent PRDs change the accept step; this PRD only
  adds the surfaced increment.

### 2.4 CHANGELOG / version bump

- `recall` patch bump 0.6.0 → 0.7.1. (v0.7.0 reserved for
  recall-doctor-claims if it ships first; if not, this can grab
  v0.7.0 — first to land wins, the other rebases.)
- CHANGELOG.md entry under `## v0.7.1` with the new column + flag.

### 2.5 Out of scope

- **No behavior change in feedback.** Stop hook still applies
  `+accept` on every `recalled.json` id. (Next PRD changes that.)
- **No transcript scanning.** Use-evidence detection lands in the
  next PRD.
- **No doctor surface for the new column.** Doctor utility lands in
  PRD #4 of the fleet.
- **No `--decay-on-surfaced`.** Decay-on-surfaced-alone is a v2
  proposal; v1 keeps decay separated.

---

## 3. Acceptance criteria

1. **AC1 — schema migration is idempotent.** Fresh DB and existing DB
   both end with `surfaced_count` column at default 0. Re-running
   `Index::open` on a migrated DB is a no-op (no error, no spurious
   write). Test: `tests/migration_surfaced_count.rs` opens a v0.6.0
   DB fixture twice and asserts column presence + idempotency.
2. **AC2 — `recall feedback --surfaced <id>` increments by 1, leaves
   confidence + feedback_count unchanged.** Test:
   `feedback::tests::surfaced_increments_only_surface_count`.
3. **AC3 — markdown frontmatter round-trips
   `surfaced_count: N`.** Read a memory, set surfaced_count to 7,
   write, read again. Test:
   `feedback::tests::surfaced_roundtrip_markdown`.
4. **AC4 — SessionStart hook writes `surfaced.json` with the same ids
   as `recalled.json`.** Smoke: invoke recall-session-start.sh in a
   tmpfs sandbox; assert both files exist and contain identical
   JSON arrays.
5. **AC5 — UserPromptSubmit hook appends to `surfaced.json` without
   touching `recalled.json`.** Smoke: pre-seed `recalled.json` with
   `["A","B","C"]` and `surfaced.json` with `["A","B","C"]`, then
   invoke recall-search-inject.sh with a prompt that finds id "D";
   post-state `recalled.json` unchanged, `surfaced.json` =
   `["A","B","C","D"]`.
6. **AC6 — Stop hook applies `--surfaced` before `--accept`.**
   Smoke: 3 surfaced ids (A, B, C), all 3 also in `recalled.json`.
   After stop, each id's `surfaced_count` incremented by exactly 1
   AND `confidence` incremented by `accept_delta` exactly once
   (i.e., the surfaced increment doesn't double-fire feedback).
7. **AC7 — `recall doctor --format json` field listing includes
   `surfaced_count`** for at least one memory after a smoke session.
   (No utility ratio yet — just expose the count for observability.)

---

## 4. Implementation notes

### 4.1 Migration pattern (idempotent ALTER)

```rust
// In Index::open or new()
conn.execute_batch(
    "PRAGMA user_version;
     -- check / set as needed; or use IF NOT EXISTS by introspecting"
)?;

// Idempotent column-add via PRAGMA table_info inspection
let has_col: bool = conn
    .prepare("SELECT 1 FROM pragma_table_info('memories_meta') WHERE name = 'surfaced_count'")?
    .exists([])?;
if !has_col {
    conn.execute(
        "ALTER TABLE memories_meta ADD COLUMN surfaced_count INTEGER NOT NULL DEFAULT 0",
        [],
    )?;
}
```

(The feedback_count migration at index.rs:129–166 uses an INSERT … ON
CONFLICT pattern; reuse the same idempotent approach.)

### 4.2 Hook script changes (best-effort, silent on failure)

`recall-session-start.sh` currently writes `recalled.json` only.
Adding the `surfaced.json` write is a one-line `cp` since the ids are
identical for this hook. Use `mv` + temp-file pattern for atomicity:

```bash
printf '%s\n' "$ids_json" > "$weather_dir/surfaced.json.tmp"
mv "$weather_dir/surfaced.json.tmp" "$weather_dir/surfaced.json"
```

`recall-search-inject.sh` currently writes nothing. Adding the
append-dedup pattern needs `jq`:

```bash
new_ids="$(printf '%s\n' "$result" | grep -oE '^[0-9A-Z]{26}')"
[ -n "$new_ids" ] || exit 0
existing="$(jq '. // []' "$surfaced_file" 2>/dev/null || echo '[]')"
merged="$(printf '%s\n%s' "$existing" "$new_ids" | \
    jq -s 'add | unique' 2>/dev/null)"
printf '%s\n' "$merged" > "$surfaced_file.tmp"
mv "$surfaced_file.tmp" "$surfaced_file"
```

### 4.3 Determine session id in inject hook

UserPromptSubmit hook receives `.session_id` in JSON stdin (same
pattern as Stop hook's v0.5.1 fix). Read it via the same `jq -r
'.session_id // empty'` idiom. Without sid the inject hook still
surfaces memories (current behavior) but skips the surfaced.json
write — degrades gracefully.

---

## 5. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Hook latency: SessionStart adds one file write per session | Negligible (<5ms); already writes recalled.json. |
| UserPromptSubmit hook adds `jq` invocation per prompt | jq is fast (<10ms); guarded by existing `[ -x "$JQ" ]` check. |
| Schema migration on opened DB | Idempotent ALTER + PRAGMA introspection; tested in AC1. |
| Inject hook fires for slash commands (currently skipped) | Existing case statement skips `/*` prompts; surfaced.json write only happens after the surfacing succeeds, so consistent. |
| Stop hook reads surfaced.json that doesn't exist | Existing `[ -f "$recalled_file" ]` guard pattern; replicate for surfaced_file. |

---

## 6. Phasing

- **v0.7.1** (this PRD): schema migration + `--surfaced` flag + hook
  writes + Stop hook surfaced increment. No behavior change in feedback.
- v0.7.2 (next PRD, recall-use-evidence): transcript scan, used.json.
- v0.7.3 (recall-stop-hook-discriminate): replace blanket accept with
  used-vs-surfaced split.
