# PRD: wintermute-companion — boot-on-power, no keyboard, no surprises

**Author:** /dream (Claude Opus 4.7), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/companion.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-platform
**build_version_bump:** minor
**Depends on:** PRD-wintermute-dialog-turn-fsm, PRD-wintermute-companion-degrade
**Codename:** *kiosk* — turn a developer's laptop into a device that boots into wintermute and stays there.
**deferred_acs:** [2, 3]
**deferred_ac_reasons:** {"2": "requires freshly-installed Arch box + stopwatch + live fleet — validated manually per install.sh --kiosk dry-run path; structural coverage in kiosk_plan_is_non_interactive + kiosk_plan_has_seven_canonical_steps", "3": "requires physical power-cycle + reboot-ff; covered structurally by recovery_service_unit_targets_correct_user_and_target and recovery_service_path_is_system_level tests"}

## TL;DR

The wintermute-platform crate already ships `wintermute.target` and `wmd-init` (per the platform PRD shipped 2026-05-27). What it doesn't do: configure boot-on-power, autologin into the target, disable the desktop greeter, recover from power loss, and present zero seams to a user who doesn't have a keyboard. This PRD extends platform to ship a kiosk-mode install path that, on a fresh device, takes you from "plug in power" to "wintermute saying its boot phrase" with no human interaction beyond pressing the power button.

## 1. Why this exists

- **Mother's home has no IT person.** If she has to touch a keyboard, the deployment failed.
- **The platform is half-built.** wintermute.target exists at `/usr/lib/systemd/user/wintermute.target`. greetd has an example config at `/etc/greetd/config.toml.example`. Neither is wired.
- **install-path drift bit four PRDs today.** The wmd-init binary lives at `/usr/local/bin/wintermute-session` (system) while service units are user-level. The mismatch breaks ExecStart paths intermittently. Boot-resilience is the natural place to make path conventions explicit.
- **Power-loss recovery matters.** Mother will trip the cord; mother's grandkids will trip the cord. Device must come back without ceremony.

## 2. What this builds

### 2.1 Kiosk install flag

`install.sh --kiosk` does the boot-resilience steps:

1. Enable autologin via greetd: copy `/etc/greetd/config.toml.example` to `/etc/greetd/config.toml`, set user=`wintermute`.
2. Enable `wintermute.target` system-wide on the wintermute user: `systemctl --user --machine=wintermute@ enable wintermute.target` (or the appropriate user-systemd mechanism for autologin chains).
3. Set `loginctl enable-linger wintermute` so user-systemd survives logout.
4. Install a tmpfiles.d rule for `/run/wintermute/`.
5. Disable any installed desktop environment auto-launch.
6. Verify systemd-resolved or NetworkManager has the wifi pre-configured (via the bootstrap caregiver-setup flow that wintermute-bootstrap already runs on first boot).
7. Drop a `/usr/lib/systemd/system/wintermute-boot-recovery.service` that, on boot, waits 30s and if `wintermute.target` is not active, calls `systemctl --user start wintermute.target`.

### 2.2 Path convention fix

All wm-* binaries install to `/usr/local/bin/` (system-wide, not per-user). Service ExecStart paths reference `/usr/local/bin/wm-X`. The `~/.local/bin/` install (current convention) becomes a developer-convenience fallback, not the deployment path. Document the change in CHANGELOG and update the `--kiosk` flag to enforce.

### 2.3 First-boot greeting

On first activation of `wintermute.target`, wmd-init publishes a `wm.boot.first` envelope. wm-dialog's FSM (when it sees this) speaks a configurable boot phrase via wm-tts: default "Wintermute is ready." On subsequent boots, wm-dialog publishes `wm.boot.recovered` and the daemon stays silent unless explicitly summoned.

### 2.4 Power-loss recovery

Already partially handled by systemd's `Restart=on-failure`. This PRD adds the boot-recovery service (above) for the case where the target itself failed to activate at boot (e.g., audio sink not ready when wm-audio tried).

## 3. Acceptance tests

1. **AC1 — `cargo test --release --lib` ≥ current+5** (kiosk install flag, path convention, first-boot vs recovered boot, recovery service trigger, greeter config parse).
2. **AC2 — fresh-machine install ends in a running fleet.** On a freshly-installed Arch box: run `install.sh --kiosk`, reboot. After login (autologin or user), all wm-* daemons are active and agorabus peers shows ≥8 wm-* sessions within 60s of boot.
3. **AC3 — power-loss recovery.** Send `systemctl reboot -ff` to the test machine; on next boot, the fleet self-recovers without manual intervention. Same 60s gate.
4. **AC4 — no keyboard input required.** From `install.sh --kiosk` through to a working voice loop, count keyboard events: zero. (The install itself uses the bootstrap caregiver-setup mDNS web form; deployment is one-shot.)
5. **AC5 — first-boot vs recovered boot speech.** First activation: wm.boot.first → TTS "Wintermute is ready." Second activation: wm.boot.recovered → silent.
6. **AC6 — path convention.** `which wm-tts wm-stt wm-audio wm-dialog wmd wmd-init wm-bootstrap` returns `/usr/local/bin/...` for all six on a kiosk install. systemctl unit ExecStart paths match.
7. **AC7 — recovery service triggers when target fails.** Force-fail: `systemctl --user stop wintermute.target` then wait 30s; verify the recovery service restarts it.
8. **AC8 — `cargo deny check bans licenses sources` clean.**
9. **AC9 — uninstall path.** `install.sh --uninstall-kiosk` reverts greetd config, removes recovery service, leaves wm-* binaries intact.

## 4. Non-goals

1. **First-time WiFi setup.** That's wintermute-bootstrap's caregiver-setup flow; this PRD assumes it ran.
2. **OS hardening.** Standard Arch install; no AppArmor profiles, no SELinux. Sibling PRD for security.
3. **OTA updates.** Future PRD.
4. **Remote management.** No SSH key install, no Tailscale bring-up. Future PRD for fleet management.
5. **Desktop environment.** This is a kiosk, not a workstation. If a developer wants a DE, they don't pass `--kiosk`.

## 5. Open questions

- Should `wintermute-boot-recovery.service` be system-level (the natural place) or user-level (where the rest of the fleet lives)? System-level is more reliable but mixes scopes. PRD defaults to system-level for the recovery service only.
- What boot phrase? "Wintermute is ready" is utilitarian. Future PRD on personality could revisit; this one ships a configurable default.

## 6. Files this PRD likely touches

- Modified: `install.sh` (--kiosk flag with all the steps above)
- New: `pkg/systemd/wintermute-boot-recovery.service`
- New: `pkg/systemd/wintermute-boot-recovery.timer` (optional, for periodic check)
- New: `pkg/tmpfiles/wintermute.conf`
- Modified: `pkg/greetd/config.toml` (the example, now wired)
- Modified: src/init.rs (publish wm.boot.first vs wm.boot.recovered based on /var/lib/wintermute/installed marker)
- Modified: `README.md`, `CHANGELOG.md`, install convention docs
