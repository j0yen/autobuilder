# PRD: vigil-build-restart-wiring — /build's install step restarts the daemon it shipped

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-30
**Vision:** visions/vigil.md (Fleet 4)
**build_target:** shell
**Depends on:** PRD-vigil-install-restart
**Codename:** *ship-means-running* — a daemon PRD isn't done until the daemon runs the new code.

## TL;DR

/build ships a `rust-extend` PRD, builds the binary, installs it to
`~/.local/bin/` — and stops. If that binary backs a long-lived daemon
(`agorabus`, `recalld`, `wmd`, the voice fleet), the running daemon keeps
executing yesterday's bytes until *something else* (a self-review tick, a
lucky later /build, a reboot) bounces it. That "something else" is the
seven-run stale-binary saga. This PRD adds one convention to /build:
**if a PRD's `build_into` resolves to a binary that an active
systemd-user unit `ExecStart`s, the install step routes through
`rollout install` (PRD-vigil-install-restart) — which installs *and*
restarts the unit — instead of a bare `install -m755`.** Prevention at
the source, so the staleness never accrues for self-review to find.

## Why this exists

This is the upstream half of the run-9/10/11 finding. The downstream
half — making the bounce non-destructive and giving non-agorabus
daemons a one-step install+restart — is Fleet 3 + PRD-vigil-install-
restart. But a tool nobody calls fixes nothing. The three reflective
memories all point at the *install path* as the place to intervene:

- **Run 10** (`01KSV6Q9...`, verbatim): "RECURRING ROOT CAUSE: /build
  installs new agorabus binary **without restarting the daemon**;
  consider wiring `systemctl --user restart agorabus.service` into
  /build's agorabus-install path."
- **Run 9** (`01KSTZX7...`): the only reason the saga *ever* resolves is
  when "a /build tick rebuilt + restarted **together**" — accidental
  coupling. Making it deliberate is this PRD.
- The blast radius is fleet-wide, not agorabus-only: `recalld.service`,
  `wmd.service`, `wm-{audio,dialog,stt,tts}.service` each `ExecStart` an
  installed binary (verified live 2026-05-30) and each is reachable by a
  daemon-backed `rust-extend` PRD (e.g. brain/recall extends reinstall
  `recalld`; the wintermute audio fleet reinstalls `wm-*`). Every one of
  them inherits the gap the moment /build ships an extend without a
  restart.

The earlier hand-wave — "just add `systemctl --user restart
agorabus.service`" — is wrong as a general fix: a bare restart of the
bus is *destructive* to live subscribers (vigil Open Q "Restart vs
reload"), and each daemon has a different correct restart path. Routing
through `rollout install` is what makes the convention safe: it picks
`agorabus reload --build` for the bus and window-guarded `systemctl`
for the rest, with the window-guard that protects a voice daemon
mid-conversation.

## What this builds

A convention + the wiring that enforces it in /build's "wire it into the
system" step.

- **The rule (documented in build-skill SKILL.md):** after building a
  `rust-extend` PRD whose `build_into` produces a binary, resolve that
  binary's install dest. If an active `~/.config/systemd/user/*.service`
  unit `ExecStart`s that dest, the install MUST run as
  `rollout install <artifact> --dest <dest>` (which installs and
  restarts the unit through the safe path). If no unit backs the dest,
  the existing `install -m755` is unchanged. Non-daemon binaries
  (CLIs, libs) are entirely unaffected.
- **The detection helper:** a small shell function (in build-skill's
  install path / scripts) `wm_unit_for_dest <dest>` that scans the user
  units' `ExecStart=` lines (with `%h`/`%t` expansion) and echoes the
  matching unit name or empty. This is the same map PRD-vigil-install-
  restart builds in Rust; here it's a cheap pre-check so /build only
  reaches for `rollout install` when a daemon is actually involved.
- **Idempotence + fallback:** if `rollout` is not yet installed
  (Fleet 1/4 not shipped on a given machine), the wiring logs a warning
  and falls back to `install -m755` + a Pending note "daemon <unit>
  installed but not restarted — `rollout install` unavailable," so the
  convention degrades to the *current* behaviour rather than failing a
  build. No build is ever blocked by this.
- **Gossip + apply trail:** the install step appends the chosen path
  (`rollout-install` vs `install-m755-fallback`) and the resulting unit
  verdict to /build's existing per-tick log, so a later self-review sees
  *why* a daemon is (or isn't) current.

**Scope boundary:** this PRD does not touch self-review's *reaction*
playbook (that's PRD-vigil-selfreview-concurrent-guard). It only changes
the *forward* path: how /build installs daemon-backed binaries. It does
not change how non-daemon binaries install.

## Acceptance criteria

1. build-skill SKILL.md documents the convention: a `rust-extend` ship
   whose `build_into` binary backs an active systemd-user unit installs
   via `rollout install <artifact> --dest <dest>`, not bare
   `install -m755`.
2. A `wm_unit_for_dest <dest>` helper resolves a dest to its backing
   user-unit name (with `%h`/`%t` expansion) and echoes empty for a dest
   no unit `ExecStart`s; covered by a shell test with a temp fixture
   unit.
3. When the resolved unit exists and `rollout install` is available, the
   install step invokes `rollout install <artifact> --dest <dest>` and
   captures its verdict into /build's per-tick log.
4. When the dest backs **no** unit, the install path is the existing
   `install -m755` unchanged (verify: a CLI-only PRD installs exactly as
   before — no `rollout` invocation, no behavioural diff).
5. When `rollout` is **not installed**, the step falls back to
   `install -m755` and writes a Pending/gossip note naming the unit that
   was installed-but-not-restarted; the build still succeeds (exit 0).
6. The convention is a no-op for non-daemon-backed builds: `rust-cli`,
   `rust-lib`, `hooks`, `config`, and `shell` targets see no change.
7. **[user-verify]** A real daemon-backed extend ship (e.g. a recalld or
   agorabus `rust-extend` PRD) ticked through /build leaves the daemon
   running the freshly-installed inode — confirmed by `agorabus doctor`
   exit 0 (bus) or `/proc/<pid>/exe` resolving to the dest (others) —
   with no self-review stale-binary finding on the next tick.
