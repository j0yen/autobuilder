# PRD: constellation-brain-local — the 5700U as a dedicated local-LLM node

Status: Draft v0.1
build_target: mixed
build_into: /home/jsy/wintermute/wintermute-brain
Vision: visions/constellation.md
Supersedes: PRD-constellation-brain-gpu.md (which assumed a discrete Radeon GPU;
  the hardware is a Ryzen 7 5700U APU with no discrete GPU — archive brain-gpu)

## TL;DR

The fleet's second machine is a Ryzen 7 5700U — a Zen 2 APU with a Vega 8 iGPU,
no discrete GPU, and 32GB of shared DDR4. It cannot be a "fast GPU brain" (token
generation is memory-bandwidth-bound to ~8-10 tok/s on a 7-8B model whether on CPU
or iGPU), and it cannot run a heavy build and serve a model at the same time. This
PRD dedicates it to a **local LLM**: `llama.cpp` (Vulkan or CPU) serving
**qwen2.5-8B Q4** as a `local-llm` tier in the brain ladder — a privacy/offline/
cheap-default brain and command router — while heavy builds are kept off this node
(constellation-cloud-build sends them elsewhere). It is honestly *not* the
latency brain; the Anthropic API stays that.

## Why this exists

The hardware was identified (jsy, 2026-06-04: "its a Ryzen 7 5700U") and verified
by Phase 1 research, which corrected the earlier discrete-GPU assumption:

- **5700U = Zen 2 "Lucienne", 8c/16t, Radeon Vega 8 iGPU (gfx90c), NO discrete
  GPU, NO dedicated VRAM**, dual-channel DDR4-3200 (~51 GB/s) shared CPU+iGPU.
- **iGPU offload barely helps generation.** Measured on Vega-class APUs: llama.cpp
  Vulkan gives ~2× on prompt-prefill but **~zero gain on token generation** vs CPU
  (both hit the same ~51 GB/s ceiling) — ~8-10 tok/s on a 7-8B Q4. A real dGPU does
  35-50 tok/s (hundreds of GB/s VRAM). ROCm on gfx90c needs
  `HSA_OVERRIDE_GFX_VERSION=9.0.0` and is unreliable (a measured 5700U ROCm run was
  *slower*, 6.84 tg/s) — so use **Vulkan or plain CPU**, not ROCm.
- **Build + model don't fit together.** jsy: "not enough RAM nor CPU for both" and
  "we may be able to run the qwen2.5 8B if we stop all build activity here." So the
  node is *dedicated* to the model; builds are offloaded (constellation-cloud-build).
- This still earns its place: a dedicated qwen2.5-8B node is a real `local-llm` tier
  for privacy, offline operation, and zero-marginal-cost default/routing turns —
  complementing, not replacing, the cloud latency brain (project memory:
  `WM_ANTHROPIC_API_KEY` live, ladder local→haiku→sonnet→opus). It honestly sits
  *below* cloud on latency but *above* it on privacy/availability/cost.

## What this builds

Two coupled pieces (mirrors the superseded brain-gpu shape, corrected for the APU):

- **Dedicated serving (config/systemd, in `constellation/brain-local/`):**
  - a backend-detect script: try **Vulkan (RADV) on the Vega 8** (`vulkaninfo`),
    fall back to **CPU** if Vulkan offload doesn't beat CPU on this box (benchmark
    both, pick the faster for *generation*) — explicitly NOT ROCm.
  - install `vulkan-radeon vulkan-icd-loader mesa` (Vulkan path) + a `llama.cpp`
    build; Ansible role gated to this host's role flag.
  - a **`llama-server.service`** serving **qwen2.5-8B-Instruct Q4_K_M** (with a
    3B-class model — qwen2.5-3b — as a faster routing/secondary option) on
    `:8080`, `--host 0.0.0.0`, model resident, `--api-key` from the encrypted
    store, restart-on-failure, bound to the mesh only (Tailscale ACL).
  - **resource isolation**: a systemd slice / cgroup reservation so the model
    process keeps its cores+RAM, and a documented guarantee that build jobs do NOT
    run on this host (enforced by constellation-cloud-build routing + this node not
    registering as a build worker).
- **Brain ladder integration (rust-extend into `wintermute-brain`):**
  - a new **`local-llm` tier** (replacing the never-shipped `local-gpu` naming) at
    `http://<desktop-magicdns>:8080/v1`, configurable via env
    (`WM_BRAIN_LOCAL_LLM_ENDPOINT`), consistent with the existing
    `WM_BRAIN_SKIP_TIERS`/`WM_BRAIN_MAX_TIER` knobs.
  - **honest placement**: `local-llm` is the default/privacy/offline tier and the
    command-router's local option, but the latency-sensitive voice path may still
    prefer cloud (the existing `route prefer cloud-only` stays valid); the ladder
    treats `local-llm` as available-but-slow, not the fast brain.
  - **graceful absence**: if the endpoint is unreachable (box off, or busy), the
    ladder skips `local-llm` and falls through, no turn failure (same safe-posture
    discipline already in the ladder).
  - `wmd swap-model local-llm` + route/observability support, like the other tiers.

Non-goals: the mesh/bus/cloud/dispatch; making this fast (it can't be — that's the
honest point). This PRD delivers a *dedicated, resource-isolated local model node*
wired into the ladder with truthful expectations.

## Acceptance criteria

1. A backend-detect script benchmarks Vulkan-iGPU vs CPU *generation* on this host
   and selects the faster for serving (explicitly never ROCm), exiting cleanly with
   a clear message on a non-Vulkan host.
2. `llama-server` serves **qwen2.5-8B Q4_K_M** (and a 3B option) on `:8080` with
   the model resident, reachable from another fleet node by MagicDNS name + api-key,
   and NOT reachable outside the mesh (ACL asserted).
3. Measured generation throughput on the 5700U is recorded honestly (expected
   ~8-10 tok/s for 8B, ~18-28 for 3B) — the PRD/readme states this is a
   privacy/offline tier, NOT a sub-2s latency brain (no inflated claim).
4. The model process is resource-isolated (systemd slice/cgroup) and this host does
   **not** register as a build worker — a build job submitted to the fleet is never
   placed here (verified against the dispatch routing in constellation-cloud-build).
5. wintermute-brain gains a `local-llm` tier at the configurable endpoint; a turn
   routed to it is served by the desktop and reports `tier=local-llm`.
6. When the endpoint is unreachable or the node is busy, the ladder skips `local-llm`
   and falls through to the next tier with no turn failure (tested against a dead
   endpoint).
7. The endpoint + api-key come from config/encrypted store (not hardcoded);
   `wmd swap-model local-llm` and the route/observability surfaces treat the tier
   like the others.
