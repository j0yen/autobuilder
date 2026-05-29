# PRD: binstale — running-binary staleness detector

Status: Draft v0.1
build_target: rust-cli
Vision: visions/vigil.md

## TL;DR

A long-lived daemon can keep executing a binary that no longer matches
the source it was built from — the file gets reinstalled underneath it,
or a fix lands in git after the process started. Nothing on this laptop
detects that. `binstale` is a read-only CLI that, given a running PID or
a process-name regex, classifies each process's executing binary as
`fresh | deleted-exe | inode-drift | prov-stale` using kernel-truth and
provfs signals, and prints a JSON or table verdict. Detection only — it
never restarts anything.

## Why this exists

Observed live during this vision's Phase 1 research (2026-05-28
~21:30 PDT):

- `/proc/2138939/exe` → `/home/jsy/.local/bin/agorabus (deleted)`. The
  agorabus bus daemon (started 13:27:36) is executing a binary inode
  that the 20:52 reinstall unlinked. The kernel's `(deleted)` suffix on
  the `/proc/PID/exe` symlink is an unambiguous staleness flag.
- `getfattr -d ~/.local/bin/agorabus` →
  `user.prov.session="comm:install:pid:88720:uid:1000"`,
  `user.prov.ts="1780026726"` (2026-05-28 20:52). provfs (the wintermute
  LSM, live since 2026-05-24) stamps the install time on the on-disk
  binary — a second, independent staleness signal.
- The run-18 self-review journal (`~/brain/journal/2026-05-28.md`,
  §Carried forward + §Pending) hand-writes the "agorabus daemon stale
  binary" finding for the third time across runs 16–18. It is rediscovered
  manually every tick because no tool computes it.

`freshness` (visions/freshness.md) and `drift` (visions/drift.md) already
cover stale *memory bodies* and stale *skill text*. binstale covers the
third axis — a stale *running binary* — with the same read-only,
evidence-first posture.

## What this builds

New repo `~/wintermute/binstale/`, published as `j0yen/binstale`. Single
Rust binary, no async runtime needed.

### Verdict taxonomy

For a target PID, resolve `/proc/PID/exe` and classify:

- **`deleted-exe`** — the `/proc/PID/exe` symlink target ends in the
  kernel's ` (deleted)` marker (read via `readlink(/proc/PID/exe)`; the
  kernel appends `(deleted)` when the backing inode is unlinked). Highest
  confidence; needs no provfs.
- **`inode-drift`** — the exe symlink resolves to a real path, but that
  path's current inode (`stat(resolved_path).st_ino`) differs from the
  inode the process is running (`stat(/proc/PID/exe).st_ino`). The file
  was replaced in place (atomic rename) since exec.
- **`prov-stale`** — exe and on-disk inode match, but the on-disk
  binary's provfs `user.prov.ts` xattr is **newer** than the process
  start time (`/proc/PID/stat` field 22, converted via btime + clock
  ticks). I.e. the file was (re)installed after this process started but
  the process somehow still holds the same inode — defensive case; also
  triggers when provfs ts is present and the verdict is ambiguous. When
  the xattr is absent, fall back to comparing on-disk **mtime** vs
  process start time.
- **`fresh`** — none of the above.

### CLI

- `binstale check <pid>` — verdict for one PID.
- `binstale scan --match <regex>` — scan `/proc/*/comm` (and cmdline)
  against the regex, verdict per match. Default regex when omitted:
  `^(agorabus|recalld|wm-(audio|dialog|stt|tts))$`.
- `--format json|table` (default `table`). JSON emits one object per
  process: `{pid, comm, exe_path, exe_inode, ondisk_inode, prov_ts,
  proc_start, verdict, evidence}`.
- Exit code: `0` if all scanned processes are `fresh`, `1` if any are
  stale (any non-`fresh` verdict), `2` on usage/IO error. (Lets
  self-review and rollout branch on exit status.)

### Shape

- `src/main.rs` — clap CLI (`check`, `scan` subcommands).
- `src/proc.rs` — `/proc` reads: exe readlink + deleted-suffix parse,
  inode stat, `comm`/`cmdline` read, start-time from `/proc/PID/stat`
  field 22 + `/proc/stat btime` + `sysconf(_SC_CLK_TCK)`.
- `src/prov.rs` — read `user.prov.ts` / `user.prov.session` xattrs (use
  the `xattr` crate); graceful absence handling.
- `src/verdict.rs` — the classifier + `Verdict` enum + evidence struct.
- Deps: `clap` (derive), `serde`/`serde_json`, `xattr`, `regex`. No
  tokio.

## Acceptance criteria

1. `binstale check <pid>` against a process whose `/proc/PID/exe`
   readlink ends in ` (deleted)` returns verdict `deleted-exe` and exit
   code 1. (Test: fork a process from a temp binary, `unlink` the temp
   binary, check the child PID — readlink shows `(deleted)`.)
2. `binstale check <pid>` against a process whose backing binary was
   replaced in place (same path, new inode via rename) returns
   `inode-drift`. (Test: copy a sleeper binary to tmp, exec it, rename a
   second copy over the path, check the running PID.)
3. `binstale check <pid>` against a process whose binary is unchanged
   since exec returns `fresh` and exit code 0.
4. `binstale scan --match '^sleep$'` (or a controlled fixture regex)
   lists one row per matching live process with a verdict column;
   `--format json` emits valid JSON parseable by `jq` with the documented
   keys.
5. When the on-disk binary carries no `user.prov.*` xattr, `binstale`
   does not error — it falls back to mtime and records
   `prov_ts: null` in JSON output. (Test: target a binary on a
   filesystem/path with no provfs stamp.)
6. `binstale check <nonexistent-pid>` exits 2 with a clear stderr
   message and emits no partial JSON.
7. `binstale scan` (no `--match`) uses the documented default daemon
   regex and, run on this laptop with the stale agorabus daemon present,
   reports at least one non-`fresh` verdict. (Today-testable: pid 2138939
   is `deleted-exe` as of drafting.)
8. `binstale --help` and `binstale scan --help` document every flag and
   the verdict taxonomy; `binstale --version` returns `binstale 0.1.0`.
9. The crate builds clean (`cargo build --release`), `cargo clippy`
   passes with no warnings, and unit tests for the verdict classifier
   (pure-function tests over synthetic `ProcInfo` structs) pass.
10. README documents the verdict taxonomy, the `(deleted)`/inode/provfs
    signals with the run-18 agorabus example, and the exit-code contract.
