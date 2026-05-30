# PRD: memlog-group-autojoin — close the last manual step in memlog activation

Status: Draft v0.1
build_target: mixed
build_into: /home/jsy/wintermute/wintermute-kernel
Vision: visions/onramp.md
Author: Claude (Opus 4.8), with jsy
Date: 2026-05-30
Depends on: PRD-kernel-pkg-postinstall.md (shipped — sysusers/udev assets
  already in the PKGBUILD as of pkgrel-6, archived commit c712c9d)

## TL;DR

`linux-wintermute` now ships `linux-wintermute-memlog.sysusers` (`g memlog -`)
and a udev rule, so a fresh install creates the `memlog` group and chowns
`/dev/memlog` to `root:memlog 0640`. But the group is created **empty** —
the install scriptlet punts membership to a manual
`sudo usermod -aG memlog <user>` plus a re-login the user has to remember.
The result is the recurring self-review flag "memlog EACCES": the group
machinery is correct, but no human runs the one membership command, so
`/dev/memlog` stays unopenable by the only user who wants it. This PRD makes
`post_install`/`post_upgrade` auto-add the *invoking* user to the `memlog`
group, turning a remembered manual step into a no-op.

## Why this exists

Verified on this laptop 2026-05-30 (booted pkgrel-5, PKGBUILD at pkgrel-7):

- `ls -l /dev/memlog` → `crw-rw---- 1 root root 10, 263` — still `root:root`.
- `getent group memlog` → empty (no group). pkgrel-5 predates the assets.
- `cat ~/wintermute/wintermute-kernel/pkg/linux-wintermute-memlog.sysusers`
  → `g memlog -` (group only, **no membership line, no `m` modifier**).
- `cat ~/wintermute/wintermute-kernel/pkg/linux-wintermute.install` → the
  `post_install` literally instructs: `sudo usermod -aG memlog <user>` and
  "start a new login session." A manual, easily-forgotten step.
- "memlog EACCES" / "add memlog group" appears verbatim in **~26
  consecutive self-review reflective memories** (e.g. 01KSVDJF…, 01KSV6Q9…,
  01KSTZX7…) as a Pending item that never resolves — because resolving it
  needs a human to run usermod, and nobody does.

The onramp vision's open question — "fixed GID or dynamic?" — is answered
here: **keep dynamic** (`g memlog -`). No cross-host sharing of memlog
records is planned; a consumer that needs GID stability can pin it itself.

## What this builds

Edits to the kernel package's install pathway, in
`~/wintermute/wintermute-kernel/pkg/`:

1. **`linux-wintermute.install` — `post_install` + `post_upgrade`:**
   detect the human who triggered the install and add them to `memlog`:
   ```sh
   _invoking_user="${SUDO_USER:-$(logname 2>/dev/null || true)}"
   if [ -n "$_invoking_user" ] && [ "$_invoking_user" != "root" ]; then
     if ! id -nG "$_invoking_user" 2>/dev/null | grep -qw memlog; then
       gpasswd -a "$_invoking_user" memlog >/dev/null 2>&1 \
         && echo "==> added $_invoking_user to 'memlog' group" \
         || echo "==> could not auto-add $_invoking_user; run: usermod -aG memlog $_invoking_user"
     fi
   fi
   echo "==> log out/in (or run \`newgrp memlog\`) for the membership to take effect"
   ```
   Idempotent (the `id -nG | grep` guard), fail-open (never aborts the
   pacman transaction), degrades to the old printed instruction when the
   invoking user can't be determined (e.g. pure-batch installs).

2. **Group must exist before `gpasswd`.** sysusers runs in the pacman
   `systemd-sysusers` hook, which fires *after* package scriptlets in the
   same transaction is not guaranteed. Guard by materializing the group
   first if absent: `getent group memlog >/dev/null || systemd-sysusers
   /usr/lib/sysusers.d/linux-wintermute-memlog.conf >/dev/null 2>&1`.

3. **`PKGBUILD` pkgrel bump** to ship the changed `.install` (pkgrel-8),
   following the existing pkgrel cadence. No kernel rebuild required — the
   `.install` scriptlet is package metadata; reuse the repack path already
   exercised in `build.log.pkgrel6-repack-*`.

Use the idempotent, anchor-based edit discipline from
`apply-agentns.py` (Phase 1.5 of /dream) — do not hand-splice raw diffs that
rot across kernel version bumps.

This PRD does **not** install or boot the new package. Activation
(`sudo pacman -U …pkgrel-8…` + reboot, or the no-reboot
`systemd-sysusers` + `udevadm trigger` path on the already-loaded
`memlog` driver) stays a user decision, surfaced by
PRD-memlog-activation-self-review.

## Acceptance criteria

1. `linux-wintermute.install` `post_install` adds `${SUDO_USER:-logname}`
   to the `memlog` group via `gpasswd -a`, guarded so a second run is a
   no-op (no duplicate, no error).
2. When the invoking user cannot be determined or is `root`, the scriptlet
   prints the manual `usermod` fallback and exits 0 (never aborts the
   transaction).
3. The scriptlet materializes the `memlog` group (via `systemd-sysusers`
   on the shipped `.conf`) if it does not already exist, before attempting
   `gpasswd`.
4. `post_upgrade` performs the same membership check (so an existing
   install that predates this change gets the user added on next upgrade).
5. A sandbox test (`sbx`) sources the scriptlet against a throwaway group
   and asserts: (a) user added when absent, (b) idempotent on re-run,
   (c) exit 0 when `SUDO_USER`/`logname` are empty.
6. PKGBUILD pkgrel is bumped and `makepkg`/repack produces a `.pkg.tar.zst`
   whose `.INSTALL` contains the new logic (verify with
   `bsdtar -xOf …pkg.tar.zst .INSTALL`).
7. `bash -n linux-wintermute.install` is clean; the vision open question
   (dynamic vs fixed GID) is recorded as "dynamic, `g memlog -`" in the
   PKGBUILD comment next to the sysusers asset.

## Notes

- Scope is the *membership* gap only. The group/udev/device-perm machinery
  already shipped (pkgrel-6, archived commit c712c9d) and is correct.
- Pairs with the Fleet-2 bullet `memlog-readable-by-default` but is the
  smaller, durable, one-time fix; the per-session pevent/cgroup auto-add
  stays a separate future PRD for the case where the user launches without
  having re-logged-in.
