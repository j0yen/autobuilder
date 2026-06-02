# PRD: binstale-source-cmp — the `behind-head` verdict

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/binstale
Vision: visions/vigil.md

## TL;DR

`binstale` (Fleet 1 PRD #1) catches binaries whose *file* was replaced
or unlinked underneath a running process. It does **not** catch the most
common case from the journal: the binary still exists and matches its
inode, but a fix was committed to source *after* the binary was built,
so the running process is behind HEAD without any file-level signal.
This PRD extends `binstale` with a `behind-head` verdict: map a daemon to
its source repo, read the newest commit timestamp touching `src/`, and
flag a running binary whose build/install provenance predates it.

## Why this exists

The run-18 self-review (`~/brain/journal/2026-05-28.md`, §Carried
forward) describes exactly the gap binstale-core misses:

> commit `cf98f2d` (v0.4.0 multi-prefix-subscribe) landed 19:56 PDT and
> rewrote `src/daemon.rs` (+33 lines). Running daemon (pid 2138939)
> binary was built 14:55 → predates the fix. Genuine staleness.

At the moment the fix was committed (19:56) but before the 20:52
reinstall, the running daemon's binary was **not** `deleted-exe` and
**not** `inode-drift` — it was a perfectly valid on-disk file that simply
predated a source commit. The only way to detect that window is to
compare the binary's provenance timestamp against the source repo's HEAD.

Confirmed during Phase 1:
- `git -C ~/wintermute/agorabus log -1 --format=%ct -- src/` yields the
  commit time of `cf98f2d` (v0.4.0, touched `src/daemon.rs`).
- The installed binary's provfs `user.prov.ts="1780026726"` (20:52) is
  *newer* than that commit — so today it's current; but for the ~56
  minutes between 19:56 and 20:52 a `behind-head` verdict would have
  flagged it, and that is the window self-review keeps catching by hand.

## What this builds

Extends `~/wintermute/binstale/`:

- A new module `src/source.rs` and a daemon→repo map (config file at
  `~/.config/binstale/repos.toml`, with a built-in default mapping the
  known fleet: `agorabus → ~/wintermute/agorabus`, `recalld → ~/wintermute/recall`,
  `wm-* → ~/wintermute/<...>`). Entries the user can override/extend.
- For a target with a known repo, compute `source_head_ts =
  git log -1 --format=%ct -- <repo>/src` (via `std::process::Command`
  invoking `git`, or `git2` crate — implementer's choice; `git` shell-out
  is acceptable and simpler). Compare against the binary's effective
  build time: provfs `user.prov.ts` if present, else binary mtime.
- New verdict **`behind-head`**: binary effective-build-ts <
  `source_head_ts`. Ranks below `deleted-exe`/`inode-drift` (those are
  also "stale" but for a different reason); a process can be both, in
  which case the file-level verdict wins in the single-verdict field and
  `behind-head` is additionally recorded in `evidence`.
- `binstale check`/`scan` JSON gains `source_repo`, `source_head_ts`,
  `source_head_commit` (short sha) fields (null when no repo mapping).
- New flag `--no-source` to skip the git comparison (keeps the Fleet-1
  read-only-`/proc`-only behavior for environments without the repos).

## Acceptance criteria

1. With a repo mapping where `git log -1 --format=%ct -- src/` is newer
   than the running binary's provenance ts, `binstale check <pid>`
   reports `behind-head` (or records it in `evidence` if a file-level
   verdict outranks it). (Test: fixture git repo + a stale binary copy.)
2. When the binary's provenance ts is newer than the source HEAD commit
   ts, the verdict is `fresh` (no false positive). (Test: build, then
   ensure no later src commit.)
3. The daemon→repo mapping is read from `~/.config/binstale/repos.toml`
   when present and merged over the built-in default; a user entry
   overrides the default for the same daemon name.
4. `--no-source` skips all git invocation and reproduces Fleet-1
   verdicts exactly (no `source_*` fields populated, no `git` subprocess
   spawned — verifiable by tracing or by running with `git` off PATH).
5. JSON output gains `source_repo`, `source_head_ts`,
   `source_head_commit`; all are `null` for a process with no repo
   mapping, and the tool does not error on unmapped processes.
6. When `git` is unavailable or the mapped repo path does not exist,
   `binstale` degrades gracefully (logs a warning to stderr, leaves
   `source_*` null, does not crash, exit code unaffected by the git
   failure alone).
7. Existing Fleet-1 acceptance tests (verdict taxonomy, exit codes,
   provfs fallback) still pass unchanged.
8. README + `--help` document the `behind-head` verdict, the repos.toml
   format, and the run-18 agorabus 19:56→20:52 window as the worked
   example.
