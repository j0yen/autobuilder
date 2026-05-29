# PRD: wintermute-platform — autologin, target, supervisor

**Author:** /dream (Claude Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-24
**Vision:** `visions/wintermute.md`
**Builds on:** `PRD-wintermute-bootstrap.md` (whose env file we read)
**Required by:** all other Fleet 1 PRDs (the systemd target they live under)
build_auto: true
build_target: mixed
build_priority: high
deferred_acs: [1, 2, 5, 8]
mock_unjustified_for: [1, 2, 5, 8]
mock_justifications:
  1: "AC1 brings up the live Fleet 1 systemd-user target in dependency order; a mock would have to reimplement systemd's transaction engine, making the test a different scheduler rather than a verification of this one."
  2: "AC2 measures cold-reboot to first greeting in <=15 s on real hardware; wall-clock boot timing cannot be simulated without recreating the firmware, kernel, and greetd startup path."
  5: "AC5 requires real TTS audio to halt within 200 ms and wake handling to suspend; the timing invariant is meaningless without the live audio device and pipewire graph the mock cannot stand in for."
  8: "AC8 needs five real restart-storm crash cycles to trigger supervisor backoff; a mock crash loop would assert the backoff math we wrote, not that the OS-level process lifecycle behaves under storm."

---

## TL;DR

Power-on to "Hi, I'm here" in ~15 seconds, with no human in the loop
after bootstrap. This PRD provides: (1) **greetd autologin** so no
password prompt blocks the boot path, (2) a **systemd user target**
`wintermute.target` that pulls in all Fleet 1 services in the right
order, (3) a tiny **Rust supervisor** `wmd-init` that owns lifecycle
and restarts crashed children within 1 s, and (4) a `wm` CLI for
status, mute, restart, and logs.

This is the load-bearing scaffold for everything else. Once shipped,
adding a new child service is a one-line addition to the target unit.

---

## 1. Why this exists

Three observations:

1. **Login screens defeat the entire point.** A non-literate user
   cannot type a password. autologin to a dedicated `wintermute` user
   (or her actual user account) is the only acceptable boot path.

2. **systemd-user is already in active use here.** The /build skill
   uses `claude-build.timer`; `pevent` jobs run under it. Adding
   `wintermute.target` to the same user instance is the path of
   least resistance.

3. **Children will crash.** Whisper.cpp can OOM on long utterances;
   PipeWire can hiccup; the API can be down. A supervisor that
   restarts within 1 s and surfaces status is the difference between
   "the laptop is broken" and "she didn't notice."

---

## 2. What this builds

### 2.1 Autologin

A greetd drop-in at `/etc/greetd/config.toml`:

```toml
[default_session]
command = "agreety --cmd wintermute-session"
user = "wintermute"

[initial_session]
command = "wintermute-session"
user = "wintermute"
```

`wintermute-session` is a shell script installed to `/usr/local/bin/`
that:
- starts an X11 session (Xorg + a minimal compositor like `cage` or
  a bare `xinit`) — the action layer (Fleet 2) will need a screen
  for the browser, but Fleet 1 has no GUI dependency except the
  optional state indicator (Fleet 2)
- starts the user's systemd manager if not running
- runs `systemctl --user start wintermute.target`
- blocks until `wintermute.target` exits (so logout = shutdown)

If greetd isn't the installed greeter, a fallback drop-in for
`getty@tty1.service` with `autologin --noissue wintermute` is
documented as the alternative path.

### 2.2 systemd units

`/usr/lib/systemd/user/wintermute.target`:

```ini
[Unit]
Description=wintermute voice AI laptop
Wants=wmd-init.service
After=wmd-init.service
```

`/usr/lib/systemd/user/wmd-init.service`:

```ini
[Unit]
Description=wintermute supervisor
After=pipewire.service pipewire-pulse.service

[Service]
Type=notify
ExecStart=/usr/local/bin/wmd-init
Restart=always
RestartSec=2
EnvironmentFile=/etc/wintermute/conf.d/00-bootstrap.env

[Install]
WantedBy=wintermute.target
```

`wmd-init` itself owns the child services (it does NOT install them
as separate systemd units — see 2.3 for why).

### 2.3 Supervisor: `wmd-init`

A small Rust binary that supervises the child daemons. Two implementation
options the user said `/build` can decide between:

**Option A: reuse `pevent`** — `wmd-init` is a thin wrapper that
`pevent start`s each child with the right env. Inherits all of
pevent's structured-state, no-poll-wait, and double-fork goodness.
This is the recommended path; `pevent` is already battle-tested on
this laptop.

**Option B: standalone supervisor** — `wmd-init` directly spawns
children using `tokio::process` with restart policy. Smaller dep
graph; no need for `pevent` to be installed.

Default to A unless `/build` finds a blocker.

Children supervised (each is a separate PRD's binary):
- `wm-audio` (Fleet 1 PRD #3)
- `wm-stt` (Fleet 1 PRD #4)
- `wm-tts` (Fleet 1 PRD #5)
- `wm-dialog` (Fleet 1 PRD #6)
- `wmd` — the brain (Fleet 1 PRD #7)

Startup order: `wm-audio` → `wm-tts` → `wm-stt` → `wm-dialog` → `wmd`.
(audio first because everything subscribes to its events; tts before
stt so the greeting can play immediately on first start; dialog
before wmd because wmd needs dialog to gate verbal confirmations.)

Crash policy:
- Restart child within 1 s on unexpected exit
- After 5 restarts within 60 s, back off to 30 s intervals
- After 20 minutes of failed restarts on the same child, play a
  spoken error ("I'm having trouble. Ask your helper to look at me.")
  via `wm-tts` if it's still up, and continue trying

### 2.4 CLI: `wm`

Single binary with subcommands:

- `wm status` — JSON or human table of each child's state, uptime,
  last event, restart count
- `wm mute` / `wm unmute` — pub/sub to dialog to toggle mute
- `wm restart [child]` — restart all or one child
- `wm logs [child] [--tail N]` — tail child stderr from
  `~/.local/state/wintermute/logs/<child>.log`
- `wm version` — version of each installed wm-* binary
- `wm say <text>` — debug helper to speak via wm-tts

`wm` talks to `wmd-init` over a Unix socket at
`$XDG_RUNTIME_DIR/wintermute/init.sock`.

---

## 3. Open-source dependencies

| Crate / tool | Version | Purpose | License |
|---|---|---|---|
| `greetd` | system | autologin | GPL-3 |
| `cage` (optional) | system | minimal Wayland-or-X11 compositor | MIT — but X11 path uses bare `xinit` instead |
| `systemd-user` | system | service mgmt | LGPL |
| `pevent` | local | child supervision (Option A) | local |
| `tokio` | ^1.40 | async (if Option B) | MIT |
| `clap` | ^4 | CLI | MIT |
| `serde` + `serde_json` | ^1 | status output + socket protocol | MIT |
| `tracing` + `tracing-subscriber` | ^0.1, ^0.3 | logs | MIT |

---

## 4. Acceptance criteria

1. `systemctl --user start wintermute.target` brings every Fleet 1
   child up in dependency order within 5 s.
2. Cold reboot (after bootstrap is done) to first greeting in ≤15 s.
3. Crashing any child (e.g., `pkill wm-audio`) restarts it within 1 s.
4. `wm status` returns a complete table for all five children with
   correct uptime values.
5. `wm mute` halts active TTS within 200 ms and suspends wake
   handling until `wm unmute`.
6. `wm restart wm-stt` restarts only the named child; other children
   continue running uninterrupted.
7. `wm logs wm-audio --tail 20` returns the last 20 stderr lines.
8. After 5 restart-storm cycles, supervisor backs off and emits an
   `init.backoff` event on agorabus.
9. Removing `00-bootstrap.env` causes `wmd-init` to exit cleanly
   with a log line "no bootstrap config; halting" rather than
   crash-looping.

## 5. Out of scope

- Headless mode (no X server) — Fleet 2 question if the action layer
  needs a screen for the browser; not relevant for Fleet 1.
- Hibernate / suspend handling — likely needs a separate small PRD
  in Fleet 2 that re-greets her on resume.
- Wayland session — laptop is X11; revisit when target hardware ships.

## 6. Risks

- **greetd is not on every Arch system.** Document the
  `getty@tty1.service` autologin alternative in the PRD body.
- **systemd-user behaves oddly under `loginctl enable-linger`** — we
  want children running even after she logs out (she won't log out;
  reboot is the only off-path), but linger is the right setting.
- **Restart-storm masking bugs.** Aggressive restart can hide a
  fatal config error. The 5-in-60s backoff and the spoken error
  after 20 min are the mitigations; revisit if they prove too
  forgiving.

## 7. Open questions

- Should the X server start before `wintermute.target` or be a child
  of it? Leaning: `wintermute-session` starts X first, then the
  target. Cleaner ownership.
- Should `wm` be installed to `~/.local/bin/` (per CLAUDE_SELF.md
  convention) or `/usr/local/bin/`? Leaning `~/.local/bin/` for
  consistency with the rest of the laptop's local tools.
