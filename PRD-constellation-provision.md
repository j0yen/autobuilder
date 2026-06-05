# PRD: constellation-provision — turn any machine into an identical wintermute node that boots into voice

Status: Draft v0.1
build_target: mixed
Vision: visions/constellation.md

## TL;DR

Today wintermute lives on one hand-built laptop; there is no repeatable way to
stand up a second machine that is the same. This PRD builds the provisioning
pipeline: an **Ansible control plane** that installs the custom kernel (from a
local pacman repo), the full toolset, and the desktop, driven from a **golden
archiso** for day-0 bare metal — and wires **greetd autologin → i3 →
`wintermute.target`** so the machine boots straight into live voice control. One
ISO + one playbook run = an identical wintermute node.

## Why this exists

Phase 1 research settled the platform debate decisively for this situation:

- **Stay on Arch; drive it with Ansible — NOT NixOS.** The custom `linux-wintermute`
  kernel already exists as a PKGBUILD (`~/wintermute/wintermute-kernel/pkg/`,
  bakes in the LSM/agentns patches). NixOS *can* express a custom kernel but
  makes it *more* painful (compile-on-every-rebuild unless you stand up a binary
  cache), and the migration is a multi-month language ramp. The PKGBUILD already
  produces a cached `.pkg.tar.zst` — the Arch-native move is to `repo-add` it into
  a **local pacman repo** and install it everywhere via the idempotent
  `community.general.pacman` module. (Known sharp edge: AUR-via-Ansible with `yay`
  can fail `target not found`; the local-repo path sidesteps it.)
- **archiso golden ISO for day-0.** The `releng` profile + a `packages.x86_64`
  list + the local repo in `airootfs/etc/pacman.conf` produces a bootable installer
  carrying the exact base. (`customize_airootfs.sh` is deprecated — use pacman
  hooks.) An ISO is a point-in-time snapshot, so it seeds install; Ansible
  maintains day-2+.
- **Boot-to-voice is a known wiring with one footgun.** greetd `initial_session`
  gives passwordless autologin into a session command; i3 then starts the voice
  stack. The footgun: **i3 does not activate `graphical-session.target` by
  default** (i3 issue #5186), so a naive `WantedBy=graphical-session.target` user
  unit never starts. Fix explicitly (i3-as-user-service, or i3 config does
  `systemctl --user import-environment DISPLAY XAUTHORITY && systemctl --user
  start wintermute.target`). This matches the existing daemon model: project
  memory records `wm-audio`/`wm-stt` user units and `agorabus.service` already
  `WantedBy=wintermute.target`.
- The existing `wintermute-bootstrap/install.sh` is only ~57 lines of package
  install — there is no enrollment/provisioning layer above it. This PRD is that
  layer.

## What this builds

A new repo `~/wintermute/constellation/` (provisioning home for the fleet), with:

- **`ansible/`** — playbooks + roles:
  - `role: base` — pacman config incl. the **local wintermute repo**, base
    packages, the `linux-wintermute` kernel installed from that repo (not rebuilt
    per host), bootloader, user, groups (`render`/`video` for GPU, `memlog`).
  - `role: desktop` — i3, terminal emulator, status bar, and the full taskbar
    toolset (the "all the tools" set), enumerated in one place so every node
    matches.
  - `role: voice` — the wm-* daemons + `wintermute.target`, **gated by a per-host
    `voice_node: true|false`** (the desktop/cloud may be compute-only; vision OQ).
  - `host_vars/` + `group_vars/` — per-host divergence (GPU: `amd|intel`, monitors,
    role flags) feeding the same playbooks.
  - Optional **pinned mirror snapshot** (Arch Linux Archive date) so two nodes
    provisioned weeks apart converge to the same package versions.
- **`isobuild/`** — an archiso `releng`-derived profile: `packages.x86_64`, the
  local repo in `pacman.conf`, an `archinstall` answer file, and a build script
  producing `wintermute-<date>.iso`. The same image seeds the cloud node base.
- **`localrepo/`** — scripts to build `linux-wintermute` once and `repo-add` it
  into a served pacman repo (file:// or http over the mesh) so every host pulls
  the prebuilt kernel.
- **`boot/`** — the boot-to-voice wiring: `/etc/greetd/config.toml`
  (`initial_session` autologin into a startx/i3 wrapper), the i3→
  `graphical-session.target` bridge, and `wintermute.target` + per-daemon unit
  `WantedBy`/`PartOf` (only enabled on `voice_node` hosts).
- Idempotency: every role re-runnable; a second `ansible-playbook` run on a
  converged host makes no changes.

Non-goals: the dotfile *appearance* layer (constellation-appearance owns chezmoi),
the mesh (constellation-mesh), the bus/brain/cloud/dispatch. This PRD is "base
system + kernel + desktop packages + boot-to-voice," reproducibly.

## Acceptance criteria

1. The `constellation` repo exists with an `ansible/` tree that lints
   (`ansible-lint`) and whose `base` role installs the `linux-wintermute` kernel
   from a **local pacman repo** (not a per-host rebuild) — verified in a container/
   VM or with `--check` against a target.
2. Re-running the full playbook on an already-provisioned host reports **zero
   changed tasks** (idempotency).
3. Per-host divergence works: a host marked `gpu: amd` installs amdgpu/Vulkan
   packages and a host marked `gpu: intel` installs i915/Intel packages, from the
   same playbook + differing `host_vars` (tested with two host_vars files).
4. The `isobuild/` profile builds a bootable `wintermute-<date>.iso` carrying the
   local repo and the enumerated package set (build script completes; ISO boots in
   a VM to the installer).
5. Boot-to-voice: on a `voice_node` host, greetd `initial_session` autologins into
   i3, i3 activates `graphical-session.target` (issue-#5186 bridge present), and
   `wintermute.target` + the wm-* daemons come up `active` — demonstrated in a VM
   or documented reproducible test; a non-`voice_node` host does NOT start the
   voice stack.
6. The toolset (i3 + terminal + status bar + the full taskbar tool list) is
   enumerated in exactly one authoritative place in the repo, so "all the tools"
   is a single list every node installs.
7. Secrets are never committed in plaintext: any API/mesh key the playbook places
   comes from an encrypted store (Ansible Vault / age), and the repo contains only
   ciphertext (grep-asserted).
