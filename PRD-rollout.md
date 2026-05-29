# PRD: rollout — safe rolling restart for the live fleet

Status: Draft v0.1
build_target: rust-cli
Vision: visions/vigil.md

## TL;DR

`binstale` tells you a daemon is running stale code. Bringing it current
is today a five-step hand-rolled dance — `cargo build --release` →
reinstall → `kill <pid>` → relaunch → `git push` — that the run-18
self-review deliberately *didn't* do autonomously because a careless
restart drops the live 8-peer `wm-*` voice fleet mid-conversation.
`rollout` makes that dance one command: it consumes `binstale scan`,
rebuilds/reinstalls/restarts stale daemons **one at a time**, polls
agorabus `peers` to confirm each one re-registers before moving on, and
defaults to `--dry-run` so it shows the plan before touching anything.

## Why this exists

Directly from the run-18 self-review §Pending
(`~/brain/journal/2026-05-28.md`):

> To roll out the multi-prefix-subscribe fix: `(cd ~/wintermute/agorabus
> && cargo build --release)` → reinstall to `~/.local/bin/agorabus` →
> restart daemon (`kill 2138939`; relaunch) → `git push` … This drops +
> re-registers all 12 peers incl the wm-* voice fleet, so pick a window
> (or let the next /build tick that owns this PRD finish the rollout).
> Deferred deliberately — same escalate-don't-restart call that paid off
> at run 16.

Two facts make this a tool, not a script:
1. The sequence is identical every time and error-prone by hand (run 16
   resolved it via an out-of-band rebuild+restart; run 18 re-staled
   because a commit landed *without* the rebuild+restart). A tool closes
   that gap.
2. The reason it's deferred is **safety**, not difficulty — dropping the
   voice fleet mid-turn is the real cost. A tool can encode the safety
   (serialize, confirm re-registration, window-guard) that a hand-typed
   `kill` cannot.

`pevent list` is empty — the daemons are *not* supervised, so there's no
existing restart authority to lean on. `rollout` is that authority.

## What this builds

New repo `~/wintermute/rollout/`, published as `j0yen/rollout`.

### Inputs

- Reads `binstale scan --format json` (shells out to `binstale`, or reads
  a piped JSON file via `--from -`). Operates only on non-`fresh`
  verdicts.
- A launch-recipe config at `~/.config/rollout/fleet.toml`: per daemon,
  `{ repo, build_cmd (default "cargo build --release"), install_cmd,
  launch_cmd, healthcheck (default "agorabus peers | jq ...") }`. The
  config is **required** — `rollout` refuses to restart a daemon it has
  no recipe for (no guessing how to relaunch a process). See vision open
  question on launch-recipe provenance.

### Behavior

- `rollout plan` (alias: default when no subcommand) — print the ordered
  list of stale daemons and the exact commands that *would* run. No
  mutation. This is the default posture.
- `rollout apply` — execute, **strictly serialized** (never two daemons
  in flight). Per daemon: build → install → record pre-restart peer set
  → SIGTERM the old pid → wait for exit (bounded, then SIGKILL fallback)
  → run launch_cmd → poll the healthcheck until the daemon re-registers
  on agorabus or a timeout elapses → emit a one-line result. Stop the
  whole run on the first daemon that fails to come back (don't cascade).
- `--only <name>` — restrict to one daemon (e.g. `--only agorabus`).
- `--window <duration>` — coarse safety guard: refuse to restart any
  daemon whose name matches the voice set (`wm-dialog|stt|tts`) unless
  the bus has shown no `wm.dialog.turn.*` activity for `<duration>`
  (best-effort via a short agorabus subscribe sample). This is the
  interim guard; the precise turn-in-flight guard is Fleet 2
  (`rollout-window-guard`, depends on continuity-of-conversation's
  session-boundary events).
- Never pushes git. Rollout restarts *running* processes from
  *already-committed* source; pushing is a separate human/skill concern
  (the run-18 note lists `git push` as the operator's step, and per
  user instruction /build owns commit+push). `rollout` stays
  restart-only to keep its blast radius legible.

### Shape

- `src/main.rs` — clap (`plan`, `apply`).
- `src/fleet.rs` — load + validate `fleet.toml`; refuse unknown daemons.
- `src/scan.rs` — invoke/parse `binstale` JSON.
- `src/restart.rs` — the serialized build→install→SIGTERM→relaunch→verify
  loop; bounded waits; SIGKILL fallback.
- `src/health.rs` — agorabus `peers` poll / re-registration check.
- Deps: `clap`, `serde`/`toml`/`serde_json`, `nix` (signals), no tokio
  required (sequential, blocking is fine).

## Acceptance criteria

1. `rollout plan` against a `binstale` JSON containing one `deleted-exe`
   daemon prints that daemon and the exact build/install/launch commands
   from its `fleet.toml` recipe, and mutates nothing (verify: no process
   killed, no file written; assert via wchg/ctrace in test).
2. `rollout apply --only <fixture-daemon>` against a controlled fixture
   daemon (a sleeper with a recipe) rebuilds, SIGTERMs the old pid, waits
   for exit, relaunches, and confirms the new pid via healthcheck — and
   the new pid differs from the old. (Integration test with a tmp
   fixture daemon + fake healthcheck.)
3. `rollout apply` processes stale daemons **strictly one at a time**:
   at no point are two recipes mid-execution (assert via timestamped
   per-daemon start/end log; no interleave).
4. A daemon with no entry in `fleet.toml` is **refused** with a clear
   error and is never killed; `rollout` exits non-zero listing the
   unknown daemons.
5. On a daemon that fails to re-register within the healthcheck timeout,
   `rollout apply` stops the run (does not proceed to the next daemon),
   reports the failure with the old/new pids, and exits non-zero.
6. SIGTERM is sent first; if the process does not exit within a bounded
   grace period, SIGKILL is sent and the event is logged. (Test with a
   SIGTERM-ignoring fixture.)
7. `--window <dur>` refuses to restart a voice-set daemon when recent
   `wm.dialog.turn.*` activity is observed on the bus; allows it when the
   bus is quiet for the window. (Test against a fixture publisher.)
8. `rollout plan` is the default subcommand (running `rollout` with no
   args = `rollout plan`); `apply` is the only mutating path and is never
   reached without the explicit subcommand.
9. Crate builds clean; `cargo clippy` clean; README documents the
   `fleet.toml` schema, the serialized-with-verify guarantee, the
   `--window` interim guard + its Fleet-2 successor, and that rollout
   never pushes git.
10. `rollout --help` / `rollout apply --help` / `rollout plan --help`
    document every flag; `rollout --version` returns `rollout 0.1.0`.
