# PRD: constellation-brain-gpu — the Radeon serves a fast brain to the fleet

Status: Draft v0.1
build_target: mixed
build_into: /home/jsy/wintermute/wintermute-brain
Vision: visions/constellation.md

## TL;DR

The laptop's local brain takes 20-30s per voice turn because it has no GPU. The
desktop has a 32GB box with a Radeon. This PRD stands up **llama.cpp with the
Vulkan backend** serving an OpenAI-compatible `llama-server` on the desktop, and
adds a **`local-gpu` tier** to wintermute-brain's existing ladder pointing at it —
so the laptop's turns are served in ~2-4s by the desktop's GPU instead of ~25s
locally, with the cloud API still above as fallback.

## Why this exists

Phase 1 research picked the stack and quantified the win:

- **Vulkan, not ROCm.** On consumer Radeon, llama.cpp's **Vulkan (RADV)** backend
  is more reliable (no ROCm version churn, works across RDNA2/3/4) *and* is as
  fast or faster on **token-generation** — the metric that dominates short voice
  turns (ROCm wins prompt-processing, which matters for long-context RAG, not
  voice). There's even an upstream issue of ROCm tg *slower* than Vulkan on a 7900
  XTX. So Vulkan is not a compromise here, it's the right default.
- **The numbers:** Qwen2.5-7B / Qwen3-8B at Q4_K_M on a medium Radeon →
  ~35-50 tok/s generation, sub-2s TTFT; a full voice turn drops from ~25s to
  ~2-4s. The laptop's existing brain ladder (project memory: local-3b → haiku →
  sonnet → opus, with local-8b skipped because qwen3:8b pins this CPU box) gains a
  fast local rung that doesn't depend on cloud credit or latency.
- **The exact Radeon model is unknown** and changes the path (RDNA2 gfx1030 needs
  `HSA_OVERRIDE_GFX_VERSION=10.3.0` for the ROCm path; RDNA3 mostly just works) —
  but the **Vulkan path avoids ROCm entirely**, so detection mostly picks the model
  and confirms VRAM (which gates 7B vs 14B). The PRD must **detect** the GPU
  (`vulkaninfo`/`lspci`), not assume.
- **Serving to the fleet** is just `llama-server --host 0.0.0.0 --port 8080 -ngl
  999 --parallel 4 --api-key <token>` reachable over the mesh at
  `http://desktop.<tailnet>:8080/v1` — model resident for process lifetime (no
  cold-reload latency), under systemd.

## What this builds

Two coupled pieces:

- **Desktop serving (config/systemd, in `constellation/brain-gpu/`):**
  - a GPU-detect script (`vulkaninfo --summary` / `lspci | grep VGA`) that
    confirms a Vulkan-capable Radeon and reports VRAM, selecting a model
    (7-8B for ≥8GB, optional 14B for ≥12GB), all Q4_K_M.
  - install of `vulkan-radeon vulkan-icd-loader mesa vulkan-tools` + a
    `llama.cpp-vulkan` build (Ansible role, AMD-gated `host_vars`).
  - a **`llama-server.service`** systemd unit: `llama-server -m <model>.gguf
    --host 0.0.0.0 --port 8080 -ngl 999 -c 16384 --parallel 4 --flash-attn auto
    --api-key <token from encrypted store>`, restart-on-failure, model resident.
  - bound to the **mesh** only (Tailscale ACL from constellation-mesh restricts
    `:8080` to fleet tags) — never the public internet.
- **Brain ladder integration (rust-extend into `wintermute-brain`):**
  - a new **`local-gpu` tier** in the ladder (between `local-3b` and the cloud
    tiers) whose endpoint is `http://<desktop-magicdns>:8080/v1`, configurable via
    env (e.g. `WM_BRAIN_GPU_ENDPOINT`) consistent with the existing
    `WM_BRAIN_SKIP_TIERS`/`WM_BRAIN_MAX_TIER` runtime knobs.
  - **graceful absence**: if the GPU endpoint is unreachable (desktop asleep/off),
    the ladder skips `local-gpu` and falls through to cloud exactly as today — the
    fleet brain is an *acceleration*, never a hard dependency (mirrors the
    recall-down safe-posture discipline in project memory).
  - canonical tier naming + `wmd swap-model local-gpu` / `wmd route` support so the
    tier is selectable and observable like the others.

Non-goals: the mesh (constellation-mesh), the bus (constellation-bus), generic job
dispatch (constellation-dispatch), training/fine-tuning. This PRD is "fast GPU
brain, served to the fleet, wired into the ladder."

## Acceptance criteria

1. A GPU-detect script confirms a Vulkan-capable Radeon and reports its VRAM,
   selecting a 7-8B model for ≥8GB (and a 14B option for ≥12GB), all Q4_K_M — and
   exits with a clear message (not a crash) on a non-Vulkan / Intel-only host.
2. The Ansible role installs the Vulkan stack + `llama.cpp-vulkan` on an
   `gpu: amd` host only, and installs a `llama-server.service` that comes up
   `active` serving `/v1/chat/completions` on `:8080` with the model resident.
3. `llama-server` is reachable from another fleet node over the mesh by MagicDNS
   name and an `--api-key`; it is NOT reachable from outside the mesh (ACL
   asserted).
4. A served 7-8B turn returns first token in under ~2s and sustains the expected
   tok/s range on the GPU host (measured; numbers documented as card-dependent) —
   demonstrably faster than the laptop's CPU local tier.
5. wintermute-brain gains a `local-gpu` tier in the ladder pointing at the
   configurable GPU endpoint; a voice turn routed to it is served by the desktop
   and reports `tier=local-gpu` (verified like the existing tier checks in project
   memory).
6. When the GPU endpoint is unreachable, the ladder **skips** `local-gpu` and falls
   through to the next tier with no turn failure (proven by a test pointing the
   endpoint at a dead address).
7. The GPU API key and endpoint are sourced from config/encrypted store, not
   hardcoded; `wmd swap-model local-gpu` and the route/observability surfaces treat
   the new tier like the others.
