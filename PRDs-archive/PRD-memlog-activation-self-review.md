# PRD: memlog-activation-self-review — stop re-flagging a fix that's already staged

Status: Draft v0.1
build_target: shell
Vision: visions/onramp.md
Author: Claude (Opus 4.8), with jsy
Date: 2026-05-30
Depends on: none (independent; reads system state only)

## TL;DR

Every `/self-review` run for ~26 consecutive runs has flagged "memlog EACCES
/ add memlog group" as an unresolved Pending item, re-discovering it from
scratch each time. The fix is already built (sysusers + udev in the kernel
PKGBUILD, pkgrel-6) — it's simply not *activated*, because the running
system boots pkgrel-5, which predates the assets, and the user-gated
`pacman -U` + reboot hasn't happened. Self-review has no playbook that
recognizes this "fix staged in a newer pkgrel, awaiting activation" state,
so it treats a known, parked item as a fresh anomaly every single run. This
PRD adds a Phase A `memlog:` status line and a Phase B.5 escalate-once
playbook that detects the staged-but-inactive state, prints a crisp
activation runbook, and then **stops re-spinning** on it.

## Why this exists

Verified on this laptop 2026-05-30:

- `uname -r` → `7.0.10-arch1-5-wintermute`; `pacman -Q linux-wintermute` →
  `7.0.10.arch1-5`. Both pkgrel-5.
- The memlog sysusers/udev assets and self-sufficient install shipped at
  **pkgrel-6** (`git log` in autobuilder: commit `c712c9d` "archive
  PRD-kernel-pkg-postinstall — pkgrel6 self-sufficient install shipped").
  The PKGBUILD on disk is already at pkgrel-7; built `.pkg.tar.zst` files
  for pkgrel-6 and -7 exist in `~/wintermute/wintermute-kernel/pkg/`.
- So the fix exists in a newer-than-installed package. Installed/booted
  pkgrel (5) < pkgrel-containing-fix (6). `getent group memlog` → empty;
  `/dev/memlog` → `root:root`. EACCES is the *expected* state on pkgrel-5,
  not a defect.
- Self-review reflective memories 01KSVDJFCXW…, 01KSV6Q9GV…, 01KSTZX7XW…
  (runs 9/10/11, 2026-05-29) each carry "memlog EACCES (need group)" /
  "memlog still EACCES" in Pending — identical wording across ~26 runs.
  This is the same recurring-rediscovery anti-pattern that
  PRD-warden-self-review (bpolicy `loaded:false`) and
  PRD-vigil-selfreview-concurrent-guard (agorabus stale binary) address
  for their respective subsystems.

The cost is real: each run spends investigation budget re-deriving a
parked, user-gated fact, and the journal accretes a noise line that buries
genuinely new findings.

## What this builds

Edits to the `/self-review` skill (`~/.claude/skills/self-review/SKILL.md`),
shell only:

1. **Phase A — a `memlog:` status line.** One line summarizing the
   activation state, computed from:
   - `getent group memlog` (group present?)
   - `stat -c '%G' /dev/memlog` (device group = memlog?)
   - `id -nG | grep -qw memlog` (current user a member?)
   - installed pkgrel (`pacman -Q linux-wintermute`) vs the highest pkgrel
     among `~/wintermute/wintermute-kernel/pkg/*.pkg.tar.zst` that contains
     the sysusers asset.
   States: `active` (group+device+membership all good) ·
   `staged-awaiting-install` (fix in a newer pkgrel than installed) ·
   `installed-awaiting-relogin` (group exists, user added, session pre-dates
   membership) · `unstaged` (no pkgrel carries the fix — would re-open a real
   bug).

2. **Phase B.5 — playbook `memlog_group_awaiting_activation`,
   escalate-once.** When state is `staged-awaiting-install` or
   `installed-awaiting-relogin`, emit a single escalation with the exact
   runbook and then mark it acknowledged so subsequent runs short-circuit
   (the same escalate-once discipline warden/vigil use):
   - staged → "Activate: `sudo pacman -U
     ~/wintermute/wintermute-kernel/pkg/linux-wintermute-<ver>-pkgrel<N>-x86_64.pkg.tar.zst`
     then reboot. (No-reboot alt, since the memlog driver is already loaded:
     `sudo systemd-sysusers …` + `sudo udevadm trigger /dev/memlog`.)
     PRD-memlog-group-autojoin adds you to the group automatically on that
     install."
   - installed-awaiting-relogin → "Run `newgrp memlog` or start a new login
     session."
   Acknowledgement keyed on `(state, installed-pkgrel)` so it re-escalates
   only if the situation *changes* (e.g. a newer pkgrel lands, or it
   regresses to `unstaged`).

3. **Never auto-activate.** The playbook prints; it does not run
   `pacman -U`, reboot, or modify group membership. Kernel install + reboot
   stays a user decision (consistent with the recurring "pacman SKIPPED —
   protected: linux" line in every self-review).

## Acceptance criteria

1. Phase A emits exactly one `memlog:` line classifying state as one of
   `active | staged-awaiting-install | installed-awaiting-relogin |
   unstaged`, derived from the four probes above.
2. On the current laptop state (pkgrel-5 booted, group absent, pkgrel-6/7
   `.pkg.tar.zst` present), the line classifies `staged-awaiting-install`
   and names the highest staged pkgrel.
3. Phase B.5 `memlog_group_awaiting_activation` escalates **once** per
   `(state, installed-pkgrel)` tuple; a second run with unchanged state
   produces no new escalation (verified by a sandbox replay asserting the
   acknowledgement file gates the second run).
4. The escalation text includes the literal `pacman -U` activation command
   with the resolved pkg path and the no-reboot `systemd-sysusers` +
   `udevadm trigger` alternative.
5. State `active` (group present, device group=memlog, user a member)
   produces no escalation and a single `memlog: active` line.
6. State `unstaged` (no staged pkgrel carries the sysusers asset) escalates
   as a *real* regression, not a parked item — distinguishing a genuine
   bug from the expected awaiting-activation case.
7. `bash -n` clean on any extracted shell; the SKILL.md edit is additive
   (new Phase A line + new B.5 block), touching no other playbook.

## Notes

- **Serialize on SKILL.md** with any other in-flight self-review-playbook
  PRD (PRD-warden-self-review, PRD-vigil-selfreview-concurrent-guard,
  PRD-agorabus-reload-self-review). Same coordination the warden/vigil
  gossip already flagged: order is semantically free, but never apply two
  SKILL.md-editing PRDs in parallel.
- Partially realizes the onramp Fleet-2 `onramp-doctor` bullet, narrowed to
  the memlog axis. A future `onramp-doctor` PRD can fold agentns + provfs
  checks into the same Phase A surface.

---
Verified-completed: 2026-06-02
Completed-by: /build tick — shell edit, smoke-tested
