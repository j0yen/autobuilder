# PRD: provq — file → session provenance query CLI

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-25
**Vision:** [visions/continuity.md](visions/continuity.md)
build_auto: false
build_target: rust-cli
**Boot-gated:** AC4–AC7 gate on the wintermute kernel's provfs LSM
stamping live xattrs. AC1–AC3 pass against the existing
`~/wintermute/provfs/` FUSE-overlay implementation, which already
writes the same `user.prov.*` xattrs.

---

## TL;DR

The provfs LSM stamps `user.prov.session`, `user.prov.ts`, and
`user.prov.tool` xattrs on every closed-after-write file (skipping
`/proc`, `/tmp`, `.git`, `target`, `node_modules`). `getfattr -d
<file>` answers "who wrote this," but `getfattr` is verbose, has no
session-id filter, doesn't decode the `comm:` fallback gracefully,
and has no recursive shape. `provq` is the polished read side: one
binary, two subcommands. `provq show <path>` decodes the xattrs into
human-readable JSON or table. `provq scan <dir> --since 1h
--session <id>` walks a tree and filters by predicate, useful for
"every file my last `/build` session touched."

---

## 1. Why this exists

1. **Raw `getfattr` is read-only ergonomics.** It dumps base64 for
   binary xattrs, has no session-id awareness, and recursion via
   `getfattr -R` is per-file output with no filtering. The provfs
   README itself ends with "single `getfattr -d <path>` call" — that
   is the entry point, but it is not the destination.

2. **Two real query shapes need different tools.**
   - "What session wrote `~/wintermute/recall/Cargo.toml`?" → point
     query. `provq show`.
   - "Show me everything my last `/build` session touched in the
     last hour." → predicate sweep. `provq scan`.

3. **The data exists in two places already, with the same schema.**
   The provfs FUSE overlay (`~/wintermute/provfs/`) and the
   in-kernel provfs LSM (`~/wintermute/provfs/lsm/`) both stamp the
   same xattr keys per the dream skill Phase 1.5 brief. `provq` is a
   pure read-side tool — it doesn't care which one wrote the
   xattrs.

---

## 2. What this builds

### 2.1 Binary: `provq`

```
provq show <path> [--format json|table|raw]
provq scan <dir> [--since <duration>] [--session <id>] [--tool <name>]
                 [--format json|table|paths] [-r/--recursive] [-z/--null]
```

### 2.2 `provq show`

Reads `user.prov.session`, `user.prov.ts`, `user.prov.tool` (and any
other `user.prov.*` keys present) from a single file. Decodes:

- `user.prov.session` — if it parses as a UUID-shaped 128-bit
  agentns session_id, print as hex. If it matches the
  `comm:<name>:pid:<n>:uid:<n>` fallback format, parse into a
  structured object. Otherwise print as raw bytes with a hex
  fallback.
- `user.prov.ts` — interpret as nanoseconds-since-boot (or
  whatever the LSM writes; consult kernel header). Render as ISO
  timestamp using `/proc/stat` boot epoch + the ns offset.
- `user.prov.tool` — string, no decode needed.

Formats:
- `json` (default for scripting): one object per file with stable
  field names.
- `table`: human-readable; fixed-width labels.
- `raw`: tab-separated key/value, suitable for `awk`.

Exit codes: 0 on read; 1 on file missing; 2 on file present but no
provfs xattrs; 3 on xattr read denied (permissions).

### 2.3 `provq scan`

Walks a directory tree (respecting the same skip-prefixes as the
LSM unless `--include-skipped`), reads xattrs on each file, applies
filters, and prints matches.

- `--since <duration>` — e.g., `1h`, `5m`, `7d`. Matches files
  whose `user.prov.ts` is within the window. Wall-clock; no
  monotonic confusion.
- `--session <id>` — filter to a specific session_id. Accepts the
  hex form or the `comm:…` fallback.
- `--tool <name>` — filter on `user.prov.tool` substring.
- `-r/--recursive` (default true; `--no-recursive` for one-level).
- `-z/--null` — null-terminated paths for piping into `xargs -0`.
- `--format paths` — print just the paths, one per line. Useful for
  `provq scan ~/wintermute --since 1h --format paths | xargs grep
  TODO`.

### 2.4 What `provq` does NOT do

- It does not write xattrs. (The provfs LSM and FUSE overlay do.)
- It does not maintain a database — every query reads filesystem
  state directly. The provfs xattrs ARE the database.
- It does not join across sessions or aggregate counters. (PRD #5,
  `session-postmortem`, does that join.)

---

## 3. Non-goals (v0.1)

- xattr inheritance traversal (e.g., "which session's session_id is
  set on the *parent directory*"). Files inherit nothing in this
  scheme; if needed, that's a future flag.
- Output to a database, sqlite, or recall. `provq scan --format
  json` is structured enough that downstream tools can ingest.

---

## 4. Acceptance criteria

1. **AC1 — Builds and installs.** `cargo build --release` →
   `target/release/provq`; `cargo install --path .` lands it in
   `~/.cargo/bin/`. `--version` prints the crate version.
2. **AC2 — `show` against FUSE-overlay.** Mount the existing provfs
   FUSE overlay (`~/wintermute/provfs/`), write a file inside it,
   `provq show <path>` prints session, ts, tool fields decoded.
   `--format json` is valid JSON parseable by `jq`.
3. **AC3 — `show` no-xattrs.** `provq show /tmp/clean-file` (no
   `user.prov.*` set) prints "no provfs xattrs" message to stderr,
   exits 2.
4. **AC4 [boot] — `show` against LSM.** Under `linux-wintermute`,
   `provq show ~/wintermute/recall/Cargo.toml` (just touched) shows
   a 128-bit hex session_id (the agentns one), not the `comm:`
   fallback. Round-trip with the `agentns-claude` launcher (PRD #1):
   the session_id in `provq show` matches the session_id reported
   by `agentns-claude --verbose`.
5. **AC5 [boot] — `scan --since`.** `provq scan ~/wintermute
   --since 1m` after writing three files prints those three paths.
   `provq scan ~/wintermute --since 1m --format paths` prints the
   same three paths, one per line, no headers.
6. **AC6 [boot] — `scan --session`.** `provq scan ~/wintermute
   --session <id> --format paths` prints exactly the files written
   by that session.
7. **AC7 [boot] — skip-prefixes.** `provq scan ~ --since 1d` does
   NOT recurse into `~/wintermute/recall/target/` (a skip-prefix);
   `--include-skipped` does.
8. **AC8 — `-z/--null` pipeline.** `provq scan ~ --since 1h --format
   paths -z | xargs -0 ls -la` works on files with spaces in
   names.
9. **AC9 — README + CHANGELOG.** Repo `README.md` documents both
   subcommands with examples, including the FUSE-overlay path that
   works pre-boot. `CHANGELOG.md` section for v0.1.0.

---

## 5. Shape

```
~/wintermute/provq/            new repo, j0yen/provq
├── Cargo.toml
├── README.md
├── CHANGELOG.md
├── src/
│   ├── main.rs       subcommand dispatch
│   ├── decode.rs     xattr → typed enum (session id, comm fallback, ts)
│   ├── show.rs       single-file path
│   └── scan.rs       walkdir + filter
└── tests/
    ├── show.rs       static fixture: write xattrs via xattr crate, read back
    └── scan.rs       fixture tree, --since / --session / --tool
```

Dependencies: `clap`, `xattr` (read-only), `walkdir`, `serde_json`,
`uuid`, `humantime` (--since parsing). No tokio; this is sync.

---

## 6. Open questions

- Should `provq scan` allow `--newer-than <file>` (à la
  `find -newer`)? Useful when "since I last ran `provq scan` an hour
  ago" is the natural shape. Easy add; gating until requested.
- `provq show` on a directory: currently exits 1 ("not a regular
  file"). Should directories carry their own provfs xattrs? Per the
  LSM spec they do not. Keep error.

---

## 7. Provenance

- Kernel/LSM behavior per `/dream` Phase 1.5 brief: provfs LSM
  stamps `user.prov.session` + `user.prov.ts`, skip-prefixes
  documented, fallback format `comm:<name>:pid:<n>:uid:<n>` when
  outside an agent namespace.
- FUSE-overlay implementation already at `~/wintermute/provfs/`
  (Rust crate; `src/main.rs`, `src/xattrs.rs`, `src/identity.rs`)
  proves the schema and is the pre-boot AC harness.
- Vision: visions/continuity.md, Fleet 1 PRD #2.
