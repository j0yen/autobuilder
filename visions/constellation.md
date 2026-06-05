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

wintermute today is one Intel laptop — 4 cores, 15GB RAM, an Intel iGPU with no
discrete GPU — and it shows: the local brain takes 20-30s per voice turn, builds
are slow, and there is nowhere to offload. constellation turns one laptop into a
**fleet** of three physical machines + an always-on cloud node, each provisioned
**identical** — same i3, same terminal geometry, same taskbar, same tools, voice
control live on boot — and **connected** so the wintermute agents on each see each
other, share one coordination bus, and **route work to where it belongs**.

**The fleet (scoped 2026-06-04 by jsy's decisions: cloud-first brain; the AMD
5700U is REMOVED — it's jsy's work machine; the GTX 1080 tower is DEFERRED — not
immediately needed). Near-term fleet = laptop + cloud:**

| Machine | Spec | Role | Status |
|---|---|---|---|
| **Laptop** | i7-10610U (4c/8t), 15GB, Intel iGPU, no dGPU | **Voice node**; brain = cloud | ACTIVE |
| **Cloud** | Hetzner CAX21 / Oracle free | **Always-on hub** (NATS + mesh exit) **+ the primary brain (Anthropic API, ~0.7s)** + build pods | ACTIVE |
| ~~5700U box~~ | Ryzen 7 5700U, 32GB, Vega 8 APU | ~~build workhorse~~ — **REMOVED** (jsy's work machine) | DROPPED |
| **i7 tower + GTX 1080** | 5th-gen i7, 32GB, GTX 1080 8GB | (future) local-gpu brain + fleet GPU for batch ML | **DEFERRED** |

**The resource-allocation spine (current):** the **brain is cloud-first** — the
Anthropic API (Haiku/Sonnet, ~0.7s) is faster *and* smarter than any model these
machines can host, and cheap at personal volume (~$3-11/mo); it is the primary
voice brain. The **laptop** is a thin voice node. The **cloud node** is the
always-on hub (NATS + mesh) and hosts the API brain + burst build pods. Heavy
builds run on **cloud build pods** (the 5700U that would have been the local
builder is gone). **Deferred for later:** when the GTX 1080 tower is picked up, it
adds a `local-gpu` tier (8B Q4, ~2-3s) *below* cloud in the ladder — a
private/offline brain + the fleet's free GPU for batch ML (homeward embeddings,
training) — but it is NOT needed for the near-term fleet to work.

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
- **constellation-brain-gpu** — *SUPERSEDED* (assumed a discrete Radeon). Archive.
- **constellation-brain-local** — *ABANDONED 2026-06-04* — it ran qwen2.5-8B on the
  5700U, which has been removed from the fleet. Do not build. Archive.
- **constellation-brain-cuda** — *DEFERRED 2026-06-04* (the tower is not immediately
  needed). When picked up: nvidia-dkms on `linux-wintermute` + llama.cpp/ollama CUDA
  on the GTX 1080 serving 8B Q4 as a `local-gpu` tier **below cloud** (cloud-first);
  a private/offline brain + the fleet's batch-ML GPU. Kept drafted for later.
- **constellation-cloud** — the always-on cheap cloud node: NATS hub + mesh exit
  + **the primary Anthropic-API brain** + burst build pods, provisioned by Ansible.
  *(Near-term this is the most valuable node — it's the brain + hub.)*
- **constellation-cloud-build** *(refines dispatch)* — **builds run on cloud pods**
  (sccache-dist + burst); the local-builder role is gone with the 5700U. Builds
  route to cloud (and the laptop when voice-idle), never to the tower.
- **constellation-dispatch** — JetStream work-queue + capability KV registry +
  sccache-dist distributed builds; near-term this spans laptop + cloud only.

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
