# Vision: onramp — from kernel-booted to kernel-consumed

**Authored by:** /dream (Claude Opus 4.7), with jsy
**Created:** 2026-05-27
**Status:** active
**Fleet 1 drafted:** 3 PRDs (post-install + Claude launch wrap + richer provfs fallback)
**Fleet 2:** captured as bullets; future `/dream extend onramp`

---

## TL;DR

`linux-wintermute 7.0.10-arch1-5` booted. `provfs` is in
`/sys/kernel/security/lsm`, `/dev/memlog` is a live char device, and
`/proc/self/ns/agent` exists. The kernel half of the
[continuity vision][continuity] has landed in production. But three
empirically-observed gaps stand between "substrate live" and "tools
consume it":

1. **No `memlog` group on the system.** `/dev/memlog` is
   `root:root 0660`. `getent group memlog` returns nothing. The PKGBUILD
   ships the kernel image but no `install=` hook, no sysusers.d entry,
   no udev rule. Userspace consumers can't even open the device.
2. **Nothing wraps the Claude launch in `unshare(CLONE_NEWAGENT)`.**
   `cat /proc/self/agent_session` reads 32 zeros from every
   process in every Claude session. `~/.claude/scripts/agorabus-session-start.sh`
   sets a session id via `pgrep+awk` from inside the (already-started)
   process — too late to enter a new namespace. PRD-agentns-claude
   builds the wrapper binary; nothing yet routes the actual `claude`
   launch through it.
3. **`provfs` fallback values mis-attribute the writer.**
   Live xattrs today: `~/wintermute/recall/Cargo.toml` →
   `comm:awk:pid:76630`; `~/.local/bin/recall` → `comm:install`.
   Both files were materialized by /build pipelines; the `comm`
   captured was a transient intermediate (sed/awk in the autobuilder
   chain, `install` in the binary copy). The fallback path will remain
   load-bearing for system daemons, hooks, and cron jobs that can't
   enter agentns, so enrichment matters even after gap #2 closes.

These are the three things between the user and the
[continuity Fleet 1][continuity] PRDs actually doing useful work.

## End-state

When Fleet 1 ships:

1. **`pacman -S linux-wintermute` is self-sufficient.** The package
   creates the `memlog` group (sysusers.d), installs a udev rule that
   chowns `/dev/memlog` to `root:memlog 0640`, and adds `jsy` to the
   group on first install. After a reboot a user in the group can
   `memlog show` without sudo.
2. **Every Claude session starts inside an agent namespace.** The
   interactive `claude` invocation (via shell alias or wrapper), the
   /self-review systemd-user service, the /build and /dream timers,
   and the headless services all route through `agentns-claude --intent
   <tag> -- claude …`. From inside any of them, `/proc/self/agent_session`
   reads a stable 128-bit id, propagated to every child.
3. **`provfs` fallback identifies the originating tool, not the
   transient intermediate.** When `agent_session` is zero (system
   daemons, init scripts, processes outside agentns), the xattr value
   is composed from: parent comm chain up to 3 levels, `$CLAUDE_TOOL`
   and `$AGORABUS_SID` env vars when readable, and cwd. Format is
   parseable and bounded (xattr ≤ 256 bytes). Composition reuses
   PRD-provfs-deferred-stamp's hook-time capture buffer so it's free.

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  CONSUMERS    [continuity Fleet 1: provq, memlog-witness,        │
│                recall-session-stamp, session-postmortem]         │
│               (drafted; usefully running gates on this vision)   │
├──────────────────────────────────────────────────────────────────┤
│  LAUNCH       claude-agentns-wrap  (alias + systemd-user units)  │
│               → every session enters agentns from birth          │
├──────────────────────────────────────────────────────────────────┤
│  ATTRIBUTION  provfs-comm-richer  (kernel patch: enriched        │
│               fallback when agent_session is zero)               │
├──────────────────────────────────────────────────────────────────┤
│  INSTALL      kernel-pkg-postinstall  (sysusers + udev + group)  │
│               → /dev/memlog readable by jsy; group exists        │
├──────────────────────────────────────────────────────────────────┤
│  KERNEL       linux-wintermute-7.0.10-arch1-5  (BOOTED)          │
│               provfs LSM live · /dev/memlog live · agentns in    │
└──────────────────────────────────────────────────────────────────┘
```

## Order

1. **PRD-kernel-pkg-postinstall** — no dependencies, smallest, ships
   first. Cleans up the install pathway for everyone (current and
   future installs). After ship the user can `memlog show` without
   sudo and `memlog-witness` becomes installable as a regular daemon.
2. **PRD-claude-agentns-wrap** — depends on PRD-agentns-claude.md
   ([continuity Fleet 1][continuity]) being built and installed. Should
   land soon after that PRD ships. Until it lands the rest of
   continuity Fleet 1 keeps falling back to the `comm:` form because
   `agent_session` stays zero.
3. **PRD-provfs-comm-richer** — depends on PRD-provfs-deferred-stamp
   ([provfs kernel-extend][provfs-deferred]) being a soft prerequisite:
   they share the hook-time capture buffer. If deferred-stamp ships
   first, comm-richer is purely additive (new fields in the buffer +
   richer formatting). If they race, the comm-richer PRD ships its own
   minimal capture buffer.

These three are mutually independent in terms of acceptance — each is
tested standalone. The order above is "smallest unblock first, then
the one with the most consumers waiting, then the cleanup."

## Fleet 2a — the memlog consumer spine (drafted 2026-05-30)

The `kernel-pkg-postinstall` branch of Fleet 1 **shipped** (archived commit
`c712c9d`, "pkgrel6 self-sufficient install shipped"): the PKGBUILD now
carries `linux-wintermute-memlog.sysusers` (`g memlog -`) + udev rule +
`install=` scriptlet. But the laptop still boots **pkgrel-5**, which
predates the assets — so `getent group memlog` is empty and `/dev/memlog`
is `root:root`, unchanged. The fix is *staged, not active*. And even once
activated, the group is created empty (no user) and nothing writes to the
device. Three PRDs close the gap from "install machinery exists" to "the
circular log actually fills and is readable":

1. **PRD-memlog-group-autojoin** — `post_install`/`post_upgrade` auto-adds
   the invoking user (`SUDO_USER`/`logname`) to `memlog`, removing the
   manual `usermod` step the install scriptlet currently punts to the user.
   Independent, smallest. Ships in pkgrel-8 (repack, no kernel rebuild).
2. **PRD-memlog-activation-self-review** — teaches `/self-review` to
   recognize the `staged-awaiting-install` state and escalate-once with an
   activation runbook, instead of re-flagging "memlog EACCES" as a fresh
   anomaly every run (~26 runs and counting). Independent; shell;
   serialize-on-SKILL.md with the other self-review-playbook PRDs.
3. **PRD-memlog-precompact-witness** — installs the `memlog` reader and
   wires a PreCompact hook that appends about-to-be-discarded context to
   `/dev/memlog` (today's only PreCompact hook is a sound effect). This is
   the producer/consumer the architecture's top "CONSUMERS" layer gates on.
   Hard-ordered after group activation for the write path; fails open until
   then so it's safe to install early.

Order: `group-autojoin` (+ user-gated install/reboot) → `precompact-witness`
write path; `activation-self-review` is independent and ships anytime.

**Two open questions resolved by this pass:** the `memlog` group GID stays
**dynamic** (`g memlog -`) — no cross-host record sharing planned. And the
membership gap is closed at **package post_install** (durable, one-time),
not per-session — the per-session pevent/cgroup auto-add stays the separate
`memlog-readable-by-default` bullet below.

## Fleet 2 — future `/dream extend onramp`

Bullets only; draft after Fleet 1 ships ≥2 of 3 components.

- **`agentns-launcher-hardening`** — `agentns-claude` learns
  `--inherit-from <pid>` so a SessionStart hook can re-enter an
  existing session's namespace via setns(); useful for cases where
  the user starts `claude` without the alias and the hook needs to
  bring the session into agentns post-hoc. (Spec: requires kernel
  `CONFIG_AGENT_NS_SETNS=y`; check before drafting.)
- **`memlog-readable-by-default`** — once `memlog` group is durable,
  expand it: a `pevent`/cgroup helper auto-adds Claude session PIDs to
  the group at start, so a fresh process inherits readability without
  newgrp.
- **`provfs-attribution-test-suite`** — small Rust binary that runs a
  controlled workload (touch a file via `bash → sed → cat` chain) and
  asserts the xattr fallback names the *originating* tool, not the
  innermost child. Regression guard for comm-richer.
- **`onramp-doctor`** — `doctor` subcommand that runs all three checks
  (group exists, /dev/memlog readable, current session in agentns, last
  written file has a usable provfs xattr) and prints a single
  pass/fail summary. Embedable in `/self-review` Phase A.
- **`kernel-pkg-postinstall-tests`** — Arch package install/upgrade/
  downgrade integration test using a chroot. Catches PKGBUILD
  regressions before they hit a real install.

## Open questions

- **Should `kernel-pkg-postinstall` create the `memlog` group with a
  fixed GID or a dynamic one?** Fixed is reproducible across hosts;
  dynamic is conventional for system groups. Leaning dynamic; if a
  consumer cares about GID stability they can pin it themselves.
- **Where does `claude-agentns-wrap` belong?** Three candidates:
  (a) edit `~/.zshrc` to add a `claude()` function, (b) install
  `~/.local/bin/claude` shim that execs into `agentns-claude --
  /usr/local/bin/claude`, (c) edit the user's systemd-user unit files
  in place. Leaning (a)+(c): function for interactive, edited units
  for services. Don't shadow the real binary via (b).
- **For `provfs-comm-richer`, what's the byte budget?** xattrs have
  no hard limit on most filesystems but consumers may parse them as
  C strings. Leaning ≤256 bytes total, truncate from the right
  (preserve the originating tool, drop intermediate chain links).
- **Does `claude-agentns-wrap` change `agorabus-session-start.sh`?**
  The hook today derives a sid from pgrep+awk. If the session is in
  agentns, the kernel-id should win. Leaning: hook reads
  `/proc/self/agent_session`; if non-zero, use it as the sid; else
  keep the pgrep fallback for graceful degradation.

## Provenance

- **Seeded by:** `/dream` invocation 2026-05-27 (bare, no topic; the
  11th /dream pass in the arc). The cadence rest-pace heuristic's
  "new kernel substrate landing with at least one consumer" trigger
  fired.
- **Research:**
  - `uname -r` confirms `7.0.10-arch1-5-wintermute` booted
  - `cat /sys/kernel/security/lsm` confirms `provfs` in active LSM list
  - `ls -la /dev/memlog` confirms char device, perms `crw-rw---- 660`
  - `cat /proc/self/agent_session` returns 32 zeros (32-char hex)
  - `getent group memlog` returns empty
  - `getfattr -d ~/wintermute/recall/Cargo.toml` returns
    `comm:awk:pid:76630:uid:1000`
  - `getfattr -d ~/.local/bin/recall` returns
    `comm:install:pid:95273:uid:1000`
  - `grep -nE 'install=|sysusers|udev' ~/wintermute/wintermute-kernel/pkg/PKGBUILD`
    returns nothing
  - `~/.claude/scripts/agorabus-session-start.sh` head 60 confirms no
    unshare; sid synthesis via pgrep+awk
  - 2026-05-26 journal runs 13/14/15 — three consecutive flags of
    "agentns userspace wrapping STILL missing"
- **Not yet validated:** the three drafted PRDs scaffold against the
  live kernel; each names the empirical commands above as its
  ground-truth checks.
- **User decisions pending:** opt-in per PRD (build_auto:false on
  all three); open-questions decisions above.

[continuity]: continuity.md
[provfs-deferred]: ../PRD-provfs-deferred-stamp.md
