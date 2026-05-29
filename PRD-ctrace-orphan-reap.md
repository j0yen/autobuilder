# PRD: ctrace-orphan-reap — reconcile orphaned tracer state

Status: Draft v0.1
build_target: rust-cli
Vision: visions/scribe.md

## TL;DR

ctrace is a singleton tracer guarded by `tracer.pid` and a
`claude-owns.json` ownership marker. When the owning session dies
ungracefully (SIGKILL / cgroup teardown), the SessionEnd hook never runs:
the tracer can be left running with a stale owner, or the marker can point
at a dead session while the log goes un-stopped and un-summarized. Today
this surfaces as a drifting "ctrace orphans" count and a transient
`running:false` that "recovers on its own." `ctrace-orphan-reap` is a
read-by-default CLI that reconciles tracer state against live PIDs and,
with `--apply`, stops the orphan and renders its log.

## Why this exists

From this vision's Phase 1 research (2026-05-28):

- `~/.cache/ctrace/` holds `tracer.pid`, `claude-owns.json`, and
  `current.json` — singleton tracer state with no reconciler. The
  SessionEnd hook clears the marker only on graceful exit; an ungraceful
  exit leaves it stale.
- Recall `01KSRV7R4FE…` (self-review run 18): a transient `running:false`
  was observed mid-pass and "recovered on its own"; the
  `ctrace_tracer_down` playbook did not trigger. Recall `01KSK8SDM4J0…`
  (run 13): *"ctrace orphans dropped 7→1"* — orphan-tracer state drifts
  and is only ever observed, never deterministically reconciled.
- The same ungraceful-exit root cause behind un-summarized logs
  (memory `self_build_detached_cgroup_teardown`) leaves the *tracer*
  orphaned too; PRD-ctrace-session-end-resilient fixes the summary side,
  this fixes the process/state side.

This is the running-process analogue of vigil's staleness work, but
distinct: vigil detects a *stale binary* on a healthy process; this
detects a *leaked tracer* whose owner is gone. Read-only by default,
mutation opt-in — same posture as binstale.

## What this builds

New repo `~/wintermute/ctrace-orphan-reap/`, published as
`j0yen/ctrace-orphan-reap`. Single Rust binary, no async runtime.

### Verdict

Read `tracer.pid` + `claude-owns.json` + `current.json`, resolve the
owning session and the tracer PID, and classify:

- **`healthy`** — tracer PID alive and its owner session alive.
- **`orphaned-tracer`** — tracer PID alive but the owner session
  (root_pid in the marker) is dead → the tracer outlived its session.
- **`stale-marker`** — `claude-owns.json` names a dead owner and no
  tracer is running → marker should be cleared; the named log likely
  needs a backfill render.
- **`no-tracer`** — no `tracer.pid` / not running (clean idle state).

### Modes

- default / `--json` — print the verdict and the affected log path(s);
  mutate nothing. Exit 0; nonzero only on unreadable state.
- `--apply` — for `orphaned-tracer`: `ctrace stop` the orphan; for both
  `orphaned-tracer` and `stale-marker`: render the orphaned log (shell out
  to `scribe render` if present, else `summarize-ctrace-session.sh`) and
  clear the stale marker. Each action is logged.
- `--apply --dry-run` — print the exact actions `--apply` would take,
  mutate nothing.

### Safety

- Never kills a process that is not the recorded tracer PID, and never one
  whose owner is still alive. Refuses to act if `tracer.pid` and the live
  tracer disagree (race) — reports the conflict instead.
- Idempotent: a second `--apply` on a now-healthy state is a no-op.

## Acceptance criteria

1. Against a synthetic state dir where the tracer PID is alive but the
   owner PID is dead, the default verdict is `orphaned-tracer` and names
   the log; no process is signaled.
2. Against a state dir whose `claude-owns.json` names a dead owner with no
   running tracer, the verdict is `stale-marker` and names the log to
   backfill.
3. Against a healthy state (tracer + owner both alive), the verdict is
   `healthy` and exit is 0.
4. `--json` emits a single valid JSON object with the verdict, tracer PID,
   owner PID, and affected log path(s).
5. `--apply` on `orphaned-tracer` stops the recorded tracer PID, renders
   its log, clears the marker, and logs each action; a second `--apply` is
   a no-op (`healthy`).
6. `--apply --dry-run` prints the would-take actions and mutates nothing
   (no signal sent, no file changed) — verified with a wchg/scope guard.
7. The tool refuses to signal any PID that is not the recorded tracer, and
   refuses to act when `tracer.pid` disagrees with the live tracer,
   reporting the conflict and exiting nonzero.
8. `--help` documents the verdict taxonomy and all flags; exit 0.
