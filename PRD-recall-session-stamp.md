# PRD: recall-session-stamp — agentns session_id on every memory

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-25
**Vision:** [visions/continuity.md](visions/continuity.md)
build_auto: false
build_target: rust-extend
build_into: /home/jsy/wintermute/recall
**Version target:** `recall v0.7.0` (minor — schema-additive)
**Coordinated with:** `recall-outcome-feedback` shipped at v0.6.0
(2026-05-27, CHANGELOG.md `## v0.6.0`). Retargeted from v0.6.0 →
v0.7.0 to avoid the collision; still a clean minor band.

---

## TL;DR

When `recall save` writes a memory file today, the file's frontmatter
captures name, description, type, but nothing about *which session*
wrote it. The provfs LSM stamps `user.prov.session` on the file as a
side effect, but that xattr is fragile (loses on `cp`, on non-provfs
filesystems, on backup/restore). This PRD adds two things: a
`session_id` field in memory frontmatter written at save time, and a
new query filter `recall query --session <id>` (plus `recall list
--session <id>`). Together they make "what did this morning's
session learn" a one-command answer, and they keep working when the
file moves.

Small, surgical extension to recall. Schema-additive (existing files
without `session_id` still parse; new files always have it).

---

## 1. Why this exists

1. **The xattr is the right primary source; the frontmatter is the
   durable copy.** From provfs: `user.prov.session` is set at write
   time by the kernel/FUSE-overlay. From recall: memories survive
   moves, backups, copies — and on a backup-restored filesystem the
   xattr is gone. Embed the same value in the file content so it
   survives.

2. **Cross-session queries are valuable today.** From self-review
   run-2 journal 2026-05-24: "Cross-session ctrace aggregate over
   all 4 today's ndjsons shows 7911 writes into ~/wintermute." The
   analogous question for memories — "every reflective memory
   written by today's `/build` sessions" — is not answerable
   without session-stamped memories.

3. **`recall touch` and `recall update` need to know who's modifying
   what.** Touching a memory currently mutates its `last_recalled`
   but not its provenance. Adding `last_touched_by_session` is the
   small extension that makes auditing usable.

---

## 2. What this builds

### 2.1 Frontmatter change

Recall memory files (`~/.claude/projects/-home-jsy/memory/*.md`)
gain optional frontmatter fields:

```yaml
---
name: feedback_no_sudo
description: …
metadata:
  type: feedback
  written_by_session: 6a4f9d2e3b1c4d8a9f0e1b2c3d4e5f60   # NEW
  written_by_intent: /build                              # NEW (optional)
  last_touched_by_session: 6a4f9d2e3b1c4d8a9f0e1b2c3d4e5f60  # NEW
---
```

All three fields are optional. Old files without them parse fine;
the loader fills `None`. New writes always populate `written_by_*`
once at save; `last_touched_by_session` updates on `touch` /
`update`.

### 2.2 Session-id resolution

Order of precedence when writing:
1. `RECALL_SESSION_ID` env var (explicit override).
2. `AGENTNS_SESSION_ID` env var (set by `agentns-claude` mock mode
   or by the SessionStart hook).
3. Read `/proc/self/agent_session` directly (kernel primary source).
4. Read `~/.claude/agentns-session-id` (hook-written side channel).
5. Fall back to `comm:<argv[0]>:pid:<pid>:uid:<uid>` (the same
   shape provfs uses for fallback).

`recall save --no-session-stamp` explicitly skips all of the above
and writes no session fields. Useful for one-off testing.

### 2.3 Query filter

```
recall query --session <id> [other-filters]
recall list --session <id>
recall list --no-session         # memories with no session_id
```

`<id>` accepts:
- a full 128-bit hex id (with or without dashes),
- a unique prefix (≥8 hex chars),
- `current` — resolved at query time using the same precedence
  chain as save,
- `latest` — the most-recent session that wrote any memory.

### 2.4 New subcommand: `recall sessions`

```
recall sessions [--since <duration>]
```

Lists distinct session_ids that wrote memories, with counts. Useful
for "which sessions wrote anything today" without grepping.

### 2.5 Schema migration story

None. Schema-additive. Old files load with `None`; on next `recall
touch` of an old file (no automatic backfill), if a current session_id
is available, write it into `last_touched_by_session` only — do not
fabricate a `written_by_session` for memories whose origin is
genuinely unknown.

---

## 3. Non-goals (v0.1)

- Backfilling historical memories from session JSONLs. Too lossy and
  too much code; if needed, a separate `recall backfill-sessions`
  one-shot can be a future micro-PRD.
- Cross-session deduplication using session_id. Out of scope.
- Daemon-mode integration. `recalld` (in flight via PRD-recall-daemon)
  picks up these fields automatically because they live in the
  on-disk frontmatter the daemon already reads.

---

## 4. Acceptance criteria

1. **AC1 — Builds, tests pass.** `cargo test --release --lib` green.
   `recall --version` reports `0.7.0`. CHANGELOG.md gets a v0.7.0
   entry.
2. **AC2 — Old files load.** Existing memory files without
   `written_by_session` parse cleanly; their accessors return
   `None`.
3. **AC3 — New writes stamp.** With `AGENTNS_SESSION_ID=deadbeef…`
   in env, `recall save --type reflective --name test --description
   "x" --body "y"` writes a file whose frontmatter contains
   `written_by_session: deadbeef…`.
4. **AC4 — Precedence.** With both `RECALL_SESSION_ID=aaaa…` and
   `AGENTNS_SESSION_ID=bbbb…` set, RECALL_SESSION_ID wins. With
   neither set and no /proc surface, falls back to `comm:…` form;
   verified by golden test against the captured frontmatter.
5. **AC5 — `--no-session-stamp`.** `recall save --no-session-stamp
   …` writes a file with no `written_by_session` field.
6. **AC6 — Query by full id.** `recall query --session deadbeef… X`
   returns only memories whose `written_by_session` matches.
7. **AC7 — Query by prefix.** `recall query --session deadbe X`
   matches the same memory (≥8 char prefix). Fewer than 8 chars
   errors with a clear message.
8. **AC8 — `--session current`.** With env set,
   `recall query --session current X` resolves to the env value and
   returns matching memories.
9. **AC9 — `recall sessions`.** After writing 3 memories across 2
   distinct session_ids, `recall sessions` prints both ids with
   counts 2 and 1.
10. **AC10 — Touch updates `last_touched_by_session`.** Touch a
    memory under env `AGENTNS_SESSION_ID=cccc…`; verify
    `last_touched_by_session: cccc…` is present; `written_by_session`
    is unchanged.
11. **AC11 [boot] — /proc fallback.** Without env vars, under
    `linux-wintermute` with the writing process in an agent
    namespace, `recall save` reads `/proc/self/agent_session` and
    stamps the file. Verified by inspecting frontmatter.
12. **AC12 — REPOS.md untouched.** Per rust-extend rules, no edit to
    `~/wintermute/REPOS.md`. Commits authored by `Joe Yen`.

---

## 5. Shape (extends `~/wintermute/recall/`)

```
~/wintermute/recall/src/
├── session.rs           NEW — resolve_session_id() with precedence chain
├── memory.rs            EDIT — add Option<String> fields to Frontmatter
├── save.rs / cli.rs     EDIT — invoke resolve_session_id() at write time
├── query.rs             EDIT — --session filter
└── lib.rs               EDIT — pub mod session;

tests/
└── session_stamp.rs     NEW — env precedence + frontmatter round-trip
```

No new dependencies. `std::env`, `std::fs::read_to_string`, the
existing serde_yaml frontmatter loader.

---

## 6. Coordination with sibling recall PRDs

- `recall-daemon` shipped (UDS ping op + GA). `recall-outcome-feedback`
  shipped at v0.6.0 (2026-05-27). This PRD lands at v0.7.0, past both;
  no collision.
- daemon reads the same on-disk frontmatter, so session-stamp fields
  flow through automatically. No daemon-side change required.

---

## 7. Open questions

- Should `written_by_intent` be embedded too, or just session_id?
  Leaning embed both — `intent` is cheap (a short string) and makes
  `recall list --intent /build` trivial without an id lookup.
  Tradeoff: redundant if session metadata is available elsewhere.
  Embed.
- Backfill for memories that today were written WITH `RECALL_SESSION_ID`
  set externally? Probably none exist; check by grep before merge.
- Naming the new fields: `written_by_session` vs `session_id` vs
  `origin_session`. Leaning `written_by_session` for clarity at the
  expense of brevity. Open.

---

## 8. Provenance

- Vision: visions/continuity.md, Fleet 1 PRD #4.
- Depends on PRD-agentns-claude (PRD #1) for primary-source
  session_id. Falls back gracefully on stock kernels.
- Coordinates with recall-daemon (shipped) and
  recall-outcome-feedback (shipped at v0.6.0) — chose v0.7.0 for this
  PRD to avoid version collision.
- Frontmatter rationale: provfs xattr is fragile across backups and
  non-provfs filesystems, per `~/wintermute/provfs/README.md`
  scope ("FUSE-overlay slice — loadable LSM variant deferred").
