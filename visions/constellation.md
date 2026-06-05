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
**fleet**: every machine (this laptop, a 32GB **Ryzen 7 5700U** desktop/APU,
future machines, and an always-on cloud node) is provisioned to be **identical**
— same i3, same terminal geometry, same taskbar, same tools, voice control live on
boot — and **connected** so the wintermute agents on each can see each other,
share one coordination bus, and **route work to where it belongs**.

**The resource-allocation spine (corrected 2026-06-04 after the hardware was
identified):** the "desktop" is a Ryzen 7 5700U — a Zen 2 APU with a Vega 8 iGPU,
no discrete GPU, no dedicated VRAM, ~51 GB/s shared DDR4. It does ~8-10 tok/s on a
7-8B model (CPU *or* iGPU — generation is bandwidth-bound, the iGPU doesn't help),
not the 35-50 tok/s a discrete GPU would. It cannot be a "fast" brain, and it
cannot run a heavy build *and* serve a model at once (not enough cores/RAM for
both). So the fleet splits the load: **the local box is dedicated to a local LLM**
(privacy/offline/cheap default tier + command routing), **heavy build/CI/ML jobs
are pushed to the cloud** (burst CPU/GPU pods), and **the latency-critical voice
brain stays the Anthropic API**. The laptop stops being the bottleneck; the
desktop becomes a dedicated always-available local-model node; the cloud is both
the always-on hub and the build workhorse; throughput becomes the fleet's sum.

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
- **The "desktop" is an APU, not a GPU box (corrected 2026-06-04).** It is a Ryzen
  7 5700U (Zen 2 / Lucienne, Vega 8 iGPU, gfx90c, no discrete GPU, ~51 GB/s shared
  DDR4). Research verdict: **llama.cpp Vulkan** runs on the Vega iGPU but token
  generation is bandwidth-bound, so iGPU offload gives ~2× prompt-prefill and
  **~zero generation speedup** over CPU — both ~8-10 tok/s on a 7-8B Q4 (vs 35-50
  on a real dGPU). ROCm on gfx90c needs `HSA_OVERRIDE_GFX_VERSION=9.0.0` and is
  unreliable — use Vulkan/CPU. So the local box is a **dedicated local-LLM node**
  (it can run **qwen2.5-8B Q4** at ~8-10 tok/s *only if build activity is stopped*
  — the user's own call: not enough cores/RAM for build+model at once), serving a
  privacy/offline/cheap-default `local-llm` tier + command routing — NOT the fast
  latency brain. Per-host role isolation: this node serves the model and does NOT
  run heavy builds.
- **Heavy builds belong in the cloud.** Because the local box is dedicated to the
  model, Rust/CI/ML build jobs are pushed to **burst cloud pods** (CPU for
  compilation, GPU on-demand for ML) via the dispatch layer + sccache-dist build
  servers in the cloud — freeing local cores for inference.
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
- **constellation-brain-gpu** — *SUPERSEDED 2026-06-04 by constellation-brain-local*
  (assumed a discrete Radeon; the hardware is a 5700U APU). Kept for history; archive.
- **constellation-brain-local** *(supersedes brain-gpu)* — llama.cpp Vulkan/CPU
  `llama-server` on the dedicated 5700U node serving qwen2.5-8B as a `local-llm`
  tier (privacy/offline/default + routing), resource-isolated from builds; honest
  ~8-10 tok/s, NOT the fast latency brain.
- **constellation-cloud** — the always-on cheap cloud node: NATS hub + mesh exit
  + offline-fallback brain, provisioned by the same Ansible.
- **constellation-cloud-build** *(refines dispatch)* — make the cloud the build/CI
  workhorse: cloud sccache-dist build servers + burst CPU/GPU pods take the heavy
  Rust/ML jobs, and dispatch routes builds AWAY from the LLM-dedicated local node.
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
- ~~The exact Radeon model is unknown~~ **RESOLVED 2026-06-04: it is a Ryzen 7
  5700U APU (Vega 8 iGPU, no discrete GPU).** This downgrades the local-brain
  expectation to ~8-10 tok/s and drives the build-out/LLM-in split above. The
  open follow-on: is the dedicated `local-llm` tier worth the 6-8s/reply for
  privacy/offline use, or should the local box instead be a CPU build node and the
  brain stay fully cloud? (User's stated preference 2026-06-04: dedicate local box
  to the model, push builds to cloud — so local-llm it is, builds out.)
- **Secrets bootstrapping** — every host needs one root secret (age key / SSH key
  / Vault password) delivered out-of-band before it can decrypt the rest. What's
  the delivery channel (USB, manual paste, the cloud node's tunnel)?
- **Voice on every node?** The desktop and cloud node may not want a live mic.
  Voice-on-boot should be a per-host role flag (the laptop/companion devices are
  voice nodes; the desktop is a compute node, optionally voice).
