# PRD: constellation-brain-cuda — the GTX 1080 tower is the fast local brain

Status: Draft v0.1
build_target: mixed
build_into: /home/jsy/wintermute/wintermute-brain
Vision: visions/constellation.md
Supersedes: PRD-constellation-brain-gpu.md (assumed a discrete Radeon — the GPU is
  an NVIDIA GTX 1080, so the stack is CUDA, not Vulkan/ROCm; archive brain-gpu).
Repositions: PRD-constellation-brain-local.md (5700U qwen2.5-8B) from primary local
  brain to optional offline/privacy secondary — the tower is now the fast brain.

## TL;DR

The fleet has a real discrete GPU after all: a **GTX 1080 (8GB GDDR5X, ~320 GB/s)**
in a full-size 5th-gen i7 tower. That makes a *fast* local brain real for the first
time — an 8B Q4 model fits entirely in 8GB VRAM and serves at ~25-35 tok/s
(~2-3s/reply, sub-second first token), local and private. This PRD installs the
NVIDIA + CUDA stack against the custom kernel, runs `llama.cpp`/`ollama` CUDA as an
OpenAI-compatible server on the tower, and wires a `local-gpu` tier into the brain
ladder. Because the **GPU** runs the model, the tower's CPU stays free.

## Why this exists

The hardware was pinned down across this conversation (2026-06-04):

- The "desktop with a medium Radeon" was first a Ryzen 7 5700U APU (no discrete
  GPU, ~8-10 tok/s — too slow; `constellation-brain-local`), but the fleet actually
  has a **separate full-size tower (5th-gen i7, 32GB) that can take a GTX 1080**.
- **Measured baseline:** qwen3:8b on the laptop CPU (i7-10610U, 4c) = **4.34 tok/s**
  (verified live this session) → ~14s/reply, the unusable path that forced voice to
  cloud. The 5700U doubles that to ~8-10 tok/s (still ~6-8s).
- **The GTX 1080 is ~6× the APU's memory bandwidth** (320 vs 51 GB/s), and LLM
  generation is bandwidth-bound, so it lands at ~25-35 tok/s on a 7-8B Q4 (typical
  llama.cpp Pascal) → **~2-3s/reply** — finally competitive with cloud for a local,
  private brain. An 8B Q4 (~5GB) fits comfortably in 8GB VRAM; a 14B (~8.5GB) would
  spill and crawl, so stay at 7-8B.
- **Pascal caveats (honest):** GTX 1080 = Pascal (2016), compute 6.1, no tensor
  cores, weak native FP16 — but llama.cpp's CUDA Q4_K kernels handle Pascal well
  (it's a popular budget inference card). Current CUDA still supports sm_61. Use
  **llama.cpp/ollama CUDA**, NOT Vulkan/ROCm.
- **The GPU frees the CPU:** unlike the 5700U (where the model ate the cores), here
  inference is on the GPU, so the tower's CPU can do other work — though the 5700U
  remains the better *builder*, so heavy builds still route there
  (constellation-cloud-build), not the tower while it serves the brain.

## What this builds

Two coupled pieces (the CUDA analogue of the superseded Vulkan brain-gpu):

- **Tower serving (config/systemd, in `constellation/brain-cuda/`):**
  - an Ansible role (gated to the `gpu: nvidia` host) installing the **NVIDIA
    proprietary driver as `nvidia-dkms`** so it rebuilds against the
    `linux-wintermute` custom kernel on kernel bumps, plus `cuda`, and a
    **CUDA-built `llama.cpp`** (or `ollama` with CUDA) — with a clear preflight that
    checks the kernel headers / DKMS build succeeds.
  - a GPU-detect/preflight: confirm `nvidia-smi` sees the GTX 1080 and reports
    8GB; confirm an 8B Q4 model selection (and refuse/῾warn on a 14B that won't fit).
  - a **`llama-server.service`** (or ollama) serving an **8B Q4_K_M** model
    (qwen2.5-8B / qwen3-8b) fully GPU-resident (`-ngl 999`), `--host 0.0.0.0
    --port 8080 --api-key <token from encrypted store>`, model resident, restart-on-
    failure, bound to the **mesh only** (Tailscale ACL restricts `:8080` to fleet
    tags).
  - power: **CONFIRMED 2026-06-04 — the GTX 1080 is already plugged in and running
    in the tower**, so the PSU/8-pin gate is closed. The role still does a light
    `nvidia-smi` power-draw sanity check, but this is no longer an open hardware
    risk. The only remaining provisioning wrinkle is the nvidia-dkms build against
    `linux-wintermute` (AC1).
- **Brain ladder integration (rust-extend into `wintermute-brain`):**
  - a **`local-gpu` tier** at `http://<tower-magicdns>:8080/v1`, configurable via
    `WM_BRAIN_GPU_ENDPOINT`, placed in the ladder **between local-3b and cloud
    haiku** — and, given ~2-3s latency, optionally as the default voice brain when
    the tower is up (a `prefer local-gpu` option), with cloud as the faster
    fallback for hard/again-faster turns.
  - **graceful absence:** if the tower is asleep/unreachable, the ladder skips
    `local-gpu` and falls through to cloud with no turn failure (existing
    safe-posture discipline; the same skip the ladder already does for tiers).
  - `wmd swap-model local-gpu`, `wmd route`, and the observability surfaces treat
    the tier like the others (reports `tier=local-gpu`).

Non-goals: the mesh/bus/cloud/dispatch; the 5700U build node (cloud-build); the
5700U secondary LLM (brain-local, demoted). This PRD is "fast CUDA brain on the
1080 tower, wired into the ladder."

## Acceptance criteria

1. An Ansible role on the `gpu: nvidia` host installs `nvidia-dkms` that builds
   successfully against `linux-wintermute` (DKMS status OK after install), plus CUDA
   and a CUDA-enabled llama.cpp/ollama; a preflight confirms `nvidia-smi` sees the
   GTX 1080 with 8GB and exits with a clear message (not a crash) if the GPU/driver
   is absent.
2. `llama-server`/ollama serves an **8B Q4_K_M** model fully GPU-resident on `:8080`,
   reachable from another fleet node by MagicDNS name + api-key, and NOT reachable
   outside the mesh (ACL asserted). A 14B selection is refused/warned (won't fit 8GB).
3. Measured generation throughput on the GTX 1080 is recorded (expected ~25-35
   tok/s for 8B Q4) and a served voice turn returns first token in under ~1s and a
   short reply in ~2-3s — demonstrably faster than the laptop (4.34 tok/s measured)
   and the 5700U (~8-10 tok/s).
4. wintermute-brain gains a `local-gpu` tier at the configurable endpoint, placed
   below cloud in the ladder (or as preferred-when-up); a turn routed to it is
   served by the tower and reports `tier=local-gpu`.
5. When the tower is unreachable, the ladder skips `local-gpu` and falls through to
   the next tier with no turn failure (tested against a dead endpoint).
6. The model never contends with builds: heavy build jobs route to the 5700U/cloud
   (constellation-cloud-build), and the tower advertises itself as the brain node;
   a build submitted to the fleet is not placed on the tower while it serves the
   brain (verified against dispatch routing).
7. Endpoint + api-key come from config/encrypted store (not hardcoded);
   `wmd swap-model local-gpu` and the route/observability surfaces treat the tier
   like the others. (PSU/8-pin power confirmed 2026-06-04 — card is plugged in and
   running; no open hardware gate. Only nvidia-dkms-vs-custom-kernel remains, AC1.)
