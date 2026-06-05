# Vision: constellation — wintermute grows beyond one laptop

**Authored by:** /dream (Claude Opus 4.8), with jsy
**Created:** 2026-06-04
**Status:** active
**Seed:** jsy — *"I need you to expand across multiple computers. This laptop is
too resource constrained... I have a 32GB AMD desktop with a medium Radeon GPU.
...best way to install wintermute linux on this and other computers, and for you
to communicate across all of them. I will primarily use voice control so that
should start immediately on boot. Make them identical in appearance — i3, the
terminal window shapes, all taskbar tools — everything about you. The goal is to
enable you to coordinate, collaborate and distribute workloads to maximize
development throughput. We can also setup a cloud service... /dream of growing
beyond this laptop."* Grounded in deep external research (four parallel research
agents, 2026-06-04; citations throughout the fleet PRDs).

## TL;DR

wintermute today is one Intel laptop — 8 cores, 15GB RAM, an Intel iGPU with no
discrete GPU — and it shows: the local brain takes 20-30s per voice turn, builds
are slow, and there is nowhere to offload. constellation turns one laptop into a
**fleet**: every machine (this laptop, a 32GB AMD+Radeon desktop, future
machines, and an always-on cloud node) is provisioned to be **identical** — same
i3, same terminal geometry, same taskbar, same tools, voice control live on boot
— and **connected** so the wintermute agents on each can see each other, share
one coordination bus, serve a fast GPU brain to the weak machines, and
**distribute build/inference work to whoever has capacity**. The laptop stops
being the bottleneck; the desktop's Radeon serves a ~2-4s brain instead of ~25s;
the cloud node is the always-on hub; and development throughput becomes the sum of
the fleet, not the floor of its weakest member.

## Why now (Phase 1 research, 2026-06-04)

- **The laptop is genuinely the constraint.** Confirmed live: 8-core Intel,
  15GB RAM, Intel UHD iGPU (no discrete GPU). The voice memory already records
  20-30s/turn local brain and the decision to route voice to cloud purely for
  latency. A 32GB AMD desktop with a Radeon changes the economics.
- **The bus is single-host by construction.** agorabus runs *only* over a Unix
  domain socket (`~/.cache/agorabus/sock`) — its own README says "co-located
  sessions." To coordinate across machines the bus needs a network transport.
  Research validated **NATS** (subject pub/sub maps directly onto `wm.*`, plus
  request/reply, queue groups, JetStream durable work-queues + KV) over MQTT/
  Redis — but found NATS has **no Unix-socket listener**, so the integration is a
  small **agorabus↔NATS bridge** sidecar that keeps every existing local UDS
  client unchanged. That bridge is the keystone new component.
- **Identical appearance is a solved problem with the right tools.** Research
  recommends **Arch + Ansible + chezmoi**, NOT NixOS: the custom `linux-wintermute`
  kernel PKGBUILD (which already exists) is exactly where Nix adds work (compile-
  on-rebuild) rather than removing it, and `~/wintermute/dotfiles/` +
  `wintermute-desktop` already hold the i3/desktop config to template. chezmoi is
  the only dotfile manager that does the per-host templating "identical config,
  different GPU/monitor" requires.
- **Voice-on-boot is a known wiring.** greetd `initial_session` autologin → i3 →
  `systemctl --user start wintermute.target`, with the documented i3→
  `graphical-session.target` bridge fix (i3 issue #5186). Matches the existing
  `wm-audio`/`wm-stt`/`agorabus.service` + `wintermute.target` daemon model.
- **A Radeon brain is transformative and cheap to stand up.** Research: **llama.cpp
  Vulkan** (not ROCm — more reliable on consumer Radeon *and* faster on token-
  generation, which dominates short voice turns) serving Qwen2.5-7B/Qwen3-8B
  Q4_K_M at ~35-50 tok/s, sub-2s TTFT — a ~10× win, dropping a turn from ~25s to
  ~2-4s. Slots in as a new `local-gpu` tier between the laptop's local-3b and
  cloud haiku in the existing brain ladder.
- **The cloud node should be cheap-coordinator + API-brain, not a GPU.** Research
  is decisive: at personal voice volume the Anthropic API costs ~$3-11/mo and
  beats any rentable GPU by 20-40×; self-hosting a big model 24/7 is $200-940/mo
  for a *worse* brain. So: a ~€8/mo Hetzner (or free Oracle) always-on node hosts
  the NATS hub + mesh exit + a small offline-fallback brain; the latency brain
  stays the Anthropic API (already wired, `WM_ANTHROPIC_API_KEY` live); GPU pods
  burst on-demand only for build/ML jobs.

## End-state

When constellation is fulfilled:

1. **Any new machine becomes an identical wintermute node** from a golden ISO +
   one Ansible run — same kernel, same i3, same terminals, same taskbar, same
   tools — and **boots straight into live voice control**.
2. **All nodes share one mesh and one bus.** A `wm.*` event published on any
   machine is visible fleet-wide; every local UDS client keeps working unchanged.
3. **The weak machines borrow the strong one's brain.** The laptop's voice turns
   are served by the desktop's Radeon (~2-4s) or the cloud API, transparently, via
   the existing brain ladder.
4. **Work flows to capacity.** Builds, tests, and ML/embedding jobs are dispatched
   to whichever node has the cores/VRAM/headroom; Rust compilation is shared
   across nodes via a distributed cache.
5. **An always-on cloud node** is the hub that keeps the fleet coherent even when
   personal machines sleep, at a few dollars a month.
6. **Development throughput is the fleet's sum** — the laptop is no longer the
   ceiling.

## Components (one bullet per PRD)

- **constellation-provision** — Ansible control plane + local pacman repo for the
  custom kernel + golden archiso + greetd→i3→`wintermute.target` boot-to-voice.
- **constellation-appearance** — chezmoi-templated dotfiles for pixel-identical
  i3 / terminal geometry / taskbar / tools across hosts, with per-host templating.
- **constellation-mesh** — Tailscale (MagicDNS, ACLs, exit node) joining every
  node into one private network with stable names.
- **constellation-bus** — the agorabus↔NATS bridge daemon + NATS hub/leaf config
  + JetStream, carrying `wm.*` fleet-wide while local UDS clients stay unchanged.
- **constellation-brain-gpu** — llama.cpp Vulkan `llama-server` on the Radeon +
  a `local-gpu` tier in wintermute-brain serving a fast brain to the fleet.
- **constellation-cloud** — the always-on cheap cloud node: NATS hub + mesh exit
  + offline-fallback brain, provisioned by the same Ansible.
- **constellation-dispatch** — JetStream work-queue + capability KV registry +
  sccache-dist distributed builds to maximize throughput across nodes.

## Order

```
constellation-provision ─► constellation-appearance
        │
        └─► constellation-mesh ─► constellation-bus ─► constellation-cloud
                                          ├─► constellation-brain-gpu
                                          └─► constellation-dispatch
```

provision stands up identical nodes; appearance perfects their look; mesh
connects them; bus is the coordination keystone; cloud, brain-gpu, and dispatch
each build on the bus.

## Open questions

- **Bit-for-bit vs convergent identical.** Arch+Ansible+chezmoi gives
  *functionally* identical, convergent machines, not cryptographically bit-
  identical (only NixOS or a pinned-mirror snapshot does that). Is "looks and
  behaves identical" enough, or is literal bit-identity a hard requirement? (Only
  the latter flips the recommendation to NixOS — at a multi-month migration cost.)
- **Mesh control plane sovereignty.** Tailscale (easiest) uses a third-party
  control plane; **Headscale** self-hosted on the cloud node keeps the same UX
  while owning the control. Which matters more — ops simplicity or sovereignty?
- **The exact Radeon model is unknown** and changes the inference story (RDNA2
  gfx1030 needs the `HSA_OVERRIDE` dance; RDNA3 mostly "just works"). The brain-gpu
  PRD must *detect* the GPU (`vulkaninfo`/`lspci`) and pick the path, not assume.
- **Secrets bootstrapping** — every host needs one root secret (age key / SSH key
  / Vault password) delivered out-of-band before it can decrypt the rest. What's
  the delivery channel (USB, manual paste, the cloud node's tunnel)?
- **Voice on every node?** The desktop and cloud node may not want a live mic.
  Voice-on-boot should be a per-host role flag (the laptop/companion devices are
  voice nodes; the desktop is a compute node, optionally voice).
