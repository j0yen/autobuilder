# PRD: kernel-pkg-postinstall — make `linux-wintermute` self-sufficient on install

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-27
**Vision:** [visions/onramp.md](visions/onramp.md)
build_auto: false
build_target: kernel-extend
build_into: /home/jsy/wintermute/wintermute-kernel/pkg
build_version_bump: pkgrel

---

## TL;DR

`linux-wintermute 7.0.10-arch1-5` boots and the kernel exposes
`/dev/memlog`, but the PKGBUILD ships zero userspace wiring: no group,
no udev rule, no sysusers entry, no install script. Right now
`/dev/memlog` is `root:root 0660` and `getent group memlog` returns
nothing. A user without sudo cannot read the device that the kernel
just spent megabytes baking in.

This PRD adds the missing post-install layer to the existing
PKGBUILD: a `linux-wintermute.install` hook, a sysusers.d file for the
`memlog` group, and a udev rule that chowns `/dev/memlog` to
`root:memlog 0640`. After `sudo pacman -U linux-wintermute*.zst` and a
reboot, a user in the `memlog` group can `cat /dev/memlog` (subject to
the kernel's own RBAC) without sudo.

The PKGBUILD pkgrel bumps `5 → 6`. The kernel image itself is
unchanged; only package metadata + install assets grow.

---

## 1. Why this exists

### 1.1 The current install is incomplete

Observed 2026-05-27:

```
$ uname -r
7.0.10-arch1-5-wintermute
$ ls -la /dev/memlog
crw-rw---- 1 root root 10, 263 May 27 01:01 /dev/memlog
$ getent group memlog
(empty)
$ grep -nE 'install=|sysusers|udev' ~/wintermute/wintermute-kernel/pkg/PKGBUILD
(no matches)
```

The kernel module set the device perms to `0660` expecting a `memlog`
group. The group doesn't exist, so `chown` never resolves to anything
useful, so the device is `root:root`, so the device is sudo-only. The
gap is the PKGBUILD's responsibility, not the kernel's.

### 1.2 Self-review has flagged this three runs running

`~/brain/journal/2026-05-26.md`:
- run 13: "memlog group membership still missing"
- run 14: "memlog group membership — id -nG lacks memlog. Fix: sudo
  usermod -aG memlog jsy + new login"
- run 15: "memlog group membership — still missing — blocks
  memlog_ring_saturated playbook"

The "fix" each run proposes (`sudo usermod -aG memlog jsy`) is wrong
in isolation — the group doesn't exist yet. The actual fix is in the
package, not the user's shell session.

### 1.3 Consumers are blocked transitively

PRD-memlog-witness.md ([continuity Fleet 1][continuity]) is supposed to
be a long-running userspace daemon that drains `/dev/memlog` into
per-session files under `~/.claude/memlog/<session-id>/`. Today that
daemon, if installed, would need to run as root or via sudo to open
the device, which is a poor security shape for a per-user observability
tool. Fixing the install path unblocks shipping memlog-witness as a
regular systemd-user service.

---

## 2. What this builds

### 2.1 New files added to the PKGBUILD `source=` array

- **`linux-wintermute.install`** — `.install` script with
  `post_install()`, `post_upgrade()`, `post_remove()` hooks. Idempotent
  in all three. Surfaces a clear "reboot + relogin to pick up
  `memlog` group" message after install.
- **`linux-wintermute-memlog.sysusers`** — installed to
  `/usr/lib/sysusers.d/linux-wintermute-memlog.conf`. Single line:
  `g memlog -` (dynamic GID).
- **`linux-wintermute-memlog.rules`** — installed to
  `/usr/lib/udev/rules.d/70-linux-wintermute-memlog.rules`. Single
  line: `KERNEL=="memlog", GROUP="memlog", MODE="0640"`.

### 2.2 PKGBUILD edits

Add three things:

```bash
install=linux-wintermute.install

source+=(
  linux-wintermute.install
  linux-wintermute-memlog.sysusers
  linux-wintermute-memlog.rules
)

# In package_linux-wintermute() — install the userspace assets
install -Dm644 "${srcdir}/linux-wintermute-memlog.sysusers" \
  "${pkgdir}/usr/lib/sysusers.d/linux-wintermute-memlog.conf"
install -Dm644 "${srcdir}/linux-wintermute-memlog.rules" \
  "${pkgdir}/usr/lib/udev/rules.d/70-linux-wintermute-memlog.rules"
```

Plus matching `sha256sums` lines for the three new sources.

### 2.3 Install hook shape

```bash
# linux-wintermute.install
post_install() {
  echo "==> linux-wintermute: created /usr/lib/sysusers.d/linux-wintermute-memlog.conf"
  echo "    Run \`systemd-sysusers\` to materialize the 'memlog' group now,"
  echo "    or it will be created on the next boot."
  echo
  echo "==> To consume /dev/memlog as a non-root user:"
  echo "      sudo usermod -aG memlog <user>"
  echo "    and start a new login session."
  echo
  echo "==> Reboot to load the new kernel image."
}

post_upgrade() {
  # Idempotent — sysusers/udev re-installation is handled by pacman.
  echo "==> linux-wintermute: package upgraded; reboot to load new kernel."
}

post_remove() {
  echo "==> linux-wintermute: removed."
  echo "    The 'memlog' group is left in place (used by no other package"
  echo "    in this PKGBUILD; remove manually with: sudo groupdel memlog)."
}
```

### 2.4 What this does NOT do

- Does not change the kernel image, modules, or boot path. `pkgrel`
  bumps; `pkgver` stays.
- Does not auto-add `jsy` to the `memlog` group. That's a deliberate
  policy choice the user makes (`sudo usermod -aG memlog jsy`); the
  install hook documents it.
- Does not install `agentns-claude`, `memlog-witness`, or `provq`.
  Those are separate PRDs in separate repos. This PRD is the kernel
  package's own responsibility for its own device.
- Does not handle udev rule conflicts. If a user has their own
  `/etc/udev/rules.d/` override for `/dev/memlog`, theirs wins
  (higher precedence than `/usr/lib`).

---

## 3. Acceptance criteria

1. **`makepkg -f` succeeds** with the edited PKGBUILD and the three
   new source files present. Resulting package
   `linux-wintermute-7.0.10.arch1-6-x86_64.pkg.tar.zst` is produced.
2. **Installed assets are at the expected paths:**
   `pacman -Ql linux-wintermute` includes both
   `/usr/lib/sysusers.d/linux-wintermute-memlog.conf` and
   `/usr/lib/udev/rules.d/70-linux-wintermute-memlog.rules`.
3. **The install message fires:** running `sudo pacman -U` on the new
   package shows the `post_install` (or `post_upgrade`) banner.
4. **`systemd-sysusers` materializes the group:** after
   `sudo systemd-sysusers`, `getent group memlog` returns a line.
5. **udev rule applies:** after `sudo udevadm control --reload-rules &&
   sudo udevadm trigger /dev/memlog` (or a reboot),
   `stat -c '%U:%G %a' /dev/memlog` returns `root:memlog 640`.
6. **User in group can read:** after `sudo usermod -aG memlog jsy` and
   a fresh login (`newgrp memlog` or relogin), `cat /dev/memlog` (or
   the existing `memlog show` CLI) exits 0 without sudo.
7. **Idempotent reinstall:** `sudo pacman -U` of the same package
   twice in a row leaves group/udev state unchanged; install hook
   doesn't error.
8. **Downgrade clean:** `sudo pacman -U linux-wintermute*pkgrel-5*.zst`
   over the pkgrel-6 install does not leave dangling sysusers/udev
   files (pacman handles file removal); the group is left in place
   (no harm, matches `post_remove` policy).
9. **Stock `linux` package untouched:** the parallel-install property
   from pkgbase docs holds; `pacman -Qo /boot/vmlinuz-linux` still
   shows it owned by `linux`, not `linux-wintermute`.
10. **AC1–9 verified live by jsy** after install + reboot, with
    receipts in `~/brain/journal/YYYY-MM-DD.md` or the kernel package
    repo's CHANGELOG. Mechanical AC11 isn't enough — the device-perms
    check needs a real reboot.

---

## 4. Out of scope

- Adding `provfs` udev rules (it's an LSM, not a device).
- Adding `agentns` setup helpers (separate PRD —
  PRD-claude-agentns-wrap.md handles launch wrapping).
- Migrating the udev rule from `/usr/lib/udev/rules.d/` to a more
  specific location. The 70- prefix is conventional for "after the
  default 50/60 rules"; no other package is expected to touch
  `/dev/memlog`.
- Documentation update to `~/wintermute/wintermute-kernel/README.md`.
  That's a small follow-up worth doing in the same commit, but not
  acceptance-tested.
- Auto-restarting `memlog-witness` after kernel upgrade. The witness
  daemon, if it exists, is responsible for re-attaching after device
  reappearance.

---

## 5. Bootstrap notes

- The kernel package repo is at `~/wintermute/wintermute-kernel/pkg/`.
  `git remote -v` should show `j0yen/wintermute-kernel` (verify before
  push).
- The three new source files live alongside the PKGBUILD; they are NOT
  fetched from upstream.
- pkgrel bumps `5 → 6`. After build, the resulting `.pkg.tar.zst` joins
  the existing parallel-install set in `pkg/` for the user to install.
- The install hook is tested under `sudo pacman -U` (local file
  install). It's not necessary to push to any repo first.
- Build expected wall time: same as a normal kernel build (~30 min on
  this host) — the kernel rebuild dominates; the new assets are
  microscopic.

[continuity]: visions/continuity.md
