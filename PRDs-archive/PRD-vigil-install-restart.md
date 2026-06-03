# PRD: vigil-install-restart — install a fresh binary, restart the daemon it backs

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-30
**Vision:** visions/vigil.md (Fleet 4)
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/rollout
**build_version_bump:** minor
**deferred_acs:** [7, 11]
**deferred_ac_reasons:** {"7": "voice-set window guard requires a live agorabus bus with active dialog turns; cannot be exercised in a unit test without a running daemon stack", "11": "[user-verify] requires a real recalld build and live systemd socket rebind; explicitly marked user-verify in the PRD"}
**Depends on:** PRD-rollout (Fleet 1), PRD-agorabus-reload (Fleet 3)
**Codename:** *no-stranded-daemon* — copying the bytes is half the job.

## TL;DR

Installing a freshly-built daemon binary and restarting the daemon that
runs it are two separate acts on this laptop, and almost every tool does
only the first. `install -m755 target/release/X ~/.local/bin/X` replaces
the file; the long-lived daemon keeps executing the old (now `(deleted)`)
inode until something bounces it. `agorabus` got a bespoke one-step fix
(`agorabus reload --build`), but `recalld`, `wmd`, and the voice fleet
(`wm-audio|dialog|stt|tts`) did not — each has a systemd-user unit
pointing at an installed binary and no `reload` subcommand. This PRD
adds `rollout install <binary> --dest <path>`: copy the binary, look up
which live systemd-user unit `ExecStart`s that dest, and restart it
through the best available path — `agorabus reload --build` for the bus,
window-guarded `systemctl --user restart` for the rest — emitting a
structured verdict. Detection-only `binstale` and the orchestrator
`rollout apply` already exist in this repo's vision; `rollout install`
is the *write-on-purpose* sibling: you just built a thing, install it
correctly, which means the daemon runs it.

## Why this exists

The "running daemon executing a stale binary" anomaly has re-opened
across **seven+ consecutive self-review runs** and is the single
longest-standing carried-forward item in the journal. The root cause is
not detection (vigil Fleet 1 solved that; `agorabus doctor` now reports
`current|stale_deleted_exe|stale_inode_drift`) nor a destructive bounce
(Fleet 3 solved that for the bus). It is that **install and restart are
decoupled at the source.** Evidence, all from 2026-05-29 reflective
memories written by self-review:

- **Run 10** (reflective memory `01KSV6Q9...`, verbatim): "RECURRING
  ROOT CAUSE: /build installs new agorabus binary without restarting the
  daemon; consider wiring `systemctl --user restart agorabus.service`
  into /build's agorabus-install path." The fix verdict that run was
  `stale_deleted_exe` — "daemon pid 923014 running deleted inode
  24906090 while on-disk 24910130; a /build tick rebuilt+installed at
  16:51 but never restarted the systemd daemon."
- **Run 9** (reflective memory `01KSTZX7...`): the saga is "7 runs" long;
  resolution that run happened only because "a /build tick rebuilt +
  restarted together" — i.e. it self-heals only by luck, when the same
  tick that installs also happens to restart.
- The unit→binary map is real and fleet-wide (observed live
  2026-05-30): `agorabus.service → %h/.local/bin/agorabus daemon`,
  `recalld.service → %h/.local/bin/recalld`, `wmd.service →
  %h/.local/bin/wmd start`, `wm-audio.service → %h/.cargo/bin/wm-audio
  start`, `wm-dialog/stt/tts → %h/.local/bin/wm-*`. Seven daemon units,
  one shared failure mode, and only one (agorabus) has a self-heal.
- `recalld` liveness is **safety-critical** (per project memory
  `project_brain_local_first_ladder` — recalld is the local-first
  brain's memory tier). A stale recalld is a worse outcome than a stale
  bus, and it has no `reload` subcommand at all.

`rollout` already owns the per-daemon launch-recipe map
(`~/.config/rollout/fleet.toml`, PRD-rollout §config) and the
window-guard discipline (one daemon at a time, never mid-conversation).
`rollout install` reuses both — it is the smallest correct home for the
install→restart coupling, and it means a *single* tool (not seven
bespoke `reload` subcommands) closes the gap for the whole fleet.

## What this builds

A new `Command::Install` in `~/wintermute/rollout/` (new module
`src/install.rs`), wired into the existing clap dispatch.

**UX:**

```
rollout install <binary-path> --dest <install-path> [--restart-window <secs>] [--dry-run] [--format json|table]
```

- `<binary-path>`: the freshly-built artifact (e.g.
  `target/release/recalld`).
- `--dest`: where it installs (e.g. `~/.local/bin/recalld`). The dest
  is the key into the reverse unit-map.

**Behaviour:**

1. **Install** the binary to `--dest` with mode `0755` via a
   copy-to-temp-then-rename (atomic replace; never truncate-in-place,
   which would race a reading daemon). Under `--dry-run`, report what
   *would* be installed and skip the write.
2. **Reverse unit-map lookup.** Build the dest→unit map by scanning
   `~/.config/systemd/user/*.service` for `ExecStart=` lines and
   resolving each `%h`/`%t` specifier and the leading argv[0] to an
   absolute path; match against the canonicalised `--dest`. The map is
   derived from the units themselves (not a hand-maintained list) so it
   stays correct as the fleet grows. Reuse `rollout`'s existing
   `fleet.toml` recipe where one is present; fall back to the unit's own
   `ExecStart` otherwise.
3. **Restart path selection.** If the matched unit is `agorabus.service`
   and `agorabus reload --help` succeeds, restart via `agorabus reload
   --build --format json` (non-destructive, Fleet 3) and fold its
   verdict in. Otherwise restart via `systemctl --user restart <unit>`,
   honouring `rollout`'s window-guard (refuse a voice daemon while a
   dialog turn is in flight, per the existing guard; `--restart-window`
   sets the coarse time guard until the session-boundary signal exists).
4. **Verify.** After restart, confirm the unit is `active` and — for
   agorabus — that `agorabus doctor` exits 0. For other daemons, confirm
   `/proc/<new-pid>/exe` resolves to `--dest` (not `(deleted)`), i.e.
   the new process is genuinely running the just-installed inode.
5. **Verdict.** Emit `{binary, dest, installed: bool, unit:
   <name>|null, restart_path: "agorabus-reload"|"systemctl"|"none",
   restarted: bool, verify: "current"|"stale"|"unit-inactive"|"skipped",
   dry_run: bool}`. If `--dest` backs no unit, `unit: null`,
   `restart_path: "none"` — a plain successful install, not an error.

**Non-goals:** rebuilding (the caller builds; `rollout install` takes a
built artifact — except the agorabus path, where `reload --build`
rebuilds as one atomic step it already owns). It does not scan the fleet
(`binstale`/`rollout apply` do). It does not invent launch recipes for
units it can't map — it reports `unit: null` and installs anyway,
leaving the restart to a human or a `fleet.toml` entry.

**Deps:** no new crates beyond `rollout`'s existing set (clap, serde,
serde_json). Shells out to `systemctl`/`agorabus` exactly as the Fleet-1
`rollout apply` path already does.

## Acceptance criteria

1. `rollout install <bin> --dest <path>` installs the binary to `path`
   with mode `0755` via atomic temp-then-rename, and a reading process
   holding the old inode is never truncated mid-read (verify: old
   `/proc/<pid>/exe` shows `(deleted)`, never a zero-length file).
2. The dest→unit map is derived by scanning
   `~/.config/systemd/user/*.service` `ExecStart=` lines with `%h`/`%t`
   specifier expansion; a test fixture unit pointing at a temp dest is
   matched correctly, and a dest backing no unit yields `unit: null`.
3. Installing to `~/.local/bin/agorabus` selects the
   `agorabus reload --build` restart path when that subcommand is
   available, and folds agorabus's reload verdict into the output.
4. Installing to a non-agorabus daemon dest (e.g. a test unit, or
   `recalld`) selects the `systemctl --user restart <unit>` path.
5. After a non-agorabus restart, AC-verify confirms
   `/proc/<new-pid>/exe` canonicalises to `--dest` (the new process runs
   the just-installed inode, not a `(deleted)` one).
6. `--dry-run` performs the unit lookup and prints the verdict it
   *would* produce, but writes no file and restarts nothing (verify: dest
   mtime unchanged, unit `MainPID` unchanged).
7. The window-guard is honoured: restarting a voice daemon
   (`wm-dialog|stt|tts`) is refused/deferred within `--restart-window`
   of activity, matching `rollout apply`'s existing guard semantics
   (shared code path, not a re-implementation).
8. `--format json` emits the full verdict object with all fields from
   §What-this-builds step 5; `--format table` renders the same.
9. A dest that backs no unit installs successfully and returns
   `{installed: true, unit: null, restart_path: "none"}` with exit 0 —
   a plain install is not an error.
10. The full repo gate stays green: `cargo build --release`, `cargo
    test` (including the new install tests), `cargo clippy -D warnings`,
    `cargo deny check bans licenses sources`.
11. **[user-verify]** A real round-trip: build a fresh `recalld`, run
    `rollout install target/release/recalld --dest ~/.local/bin/recalld`,
    and confirm `recalld.service` is restarted onto the new inode with
    the socket re-bound — closing the exact gap the run-10 memory named,
    for a daemon that has no `reload` of its own.
