# PRD: constellation-cloud-build — heavy builds run in the cloud, local stays free for the model

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/agorabus-nats-bridge
Vision: visions/constellation.md
Refines: PRD-constellation-dispatch.md (corrects build-server placement: cloud +
  laptop, NOT the LLM-dedicated 5700U) and PRD-constellation-cloud.md (adds the
  burst build-pod role)

## TL;DR

Because the 5700U is dedicated to serving the local LLM (constellation-brain-local),
the heavy Rust/CI/ML compilation it would otherwise do must go somewhere else. This
PRD makes **the cloud the build workhorse**: an always-available sccache-dist build
server on the cheap cloud node plus **burst CPU/GPU pods** spun up for big jobs, and
a dispatch routing rule that sends build jobs to the cloud (and optionally the
laptop) but **never to the LLM-dedicated local node**. The local box keeps all its
cores+RAM for qwen2.5-8B; throughput for builds comes from elastic cloud capacity.

## Why this exists

jsy, 2026-06-04: *"I think we should run the build jobs in the cloud so we can use
the localhost with a local LLM. not enough RAM nor CPU for both."* and *"We may be
able to run the qwen2.5 8B if we stop all build activity here."* This is a direct
resource-isolation decision that reshapes the dispatch layer:

- The original constellation-dispatch put sccache-dist build servers on "desktop +
  cloud." Given the desktop (5700U) is now the dedicated model node, that placement
  is wrong — builds there would starve the model (and vice versa). **Build servers
  belong on the cloud** (and optionally the laptop when it's not doing voice).
- Phase 1 research already established the cloud build economics: a cheap always-on
  coordinator (Hetzner CAX21, ~€8/mo) can host an sccache-dist build server, and
  **burst GPU/CPU pods** (RunPod RTX 4090 $0.69/hr, A40 48GB $0.44/hr, or a big CPU
  pod) handle the heavy fan-out on demand and are killed after — elastic, pay-per-
  use, no standing GPU bill.
- The `~/wintermute` workload is Rust-heavy, so sccache + sccache-dist (compose with
  the JetStream work-queue, don't replace it) is the right distributed-compile tool;
  bubblewrap ≥0.3.0 + kernel ≥4.6 required (this box qualifies).

## What this builds

Extends `agorabus-nats-bridge` (the dispatch home) + the cloud Ansible role:

- **Cloud build servers** — an Ansible role installing **sccache-dist build
  servers** on the cloud node (always-available baseline) and a parameterized
  **burst-pod** provisioner (RunPod/Vast API or a cloud CPU instance) that stands up
  an ephemeral high-core/GPU build server, registers it with sccache-dist, runs the
  job, and tears it down. Shared sccache cache in object storage so cache hits are
  fleet-wide.
- **Routing rule (the key correction)** — the dispatch coordinator
  (constellation-dispatch's capability registry) is configured so that
  `wm.work.build.*` / `wm.work.test.*` jobs are placed on **cloud build servers**
  (and the laptop when `voice_idle`), and **explicitly never** on the host whose
  capability record carries the `role: local-llm` / `no_build: true` flag. The
  5700U advertises `no_build: true`, so it is never selected as a build target.
- **`wm-work build` corrected** — fans a `~/wintermute` build out to the cloud
  sccache-dist server(s), bursting a pod when the queue depth or job size crosses a
  threshold, and reporting where it ran. Local `cargo` on the laptop transparently
  offloads `rustc`/`cc` to the cloud builders via the `rustc-wrapper` config.
- **Cost guardrails** — burst pods are created only on demand and torn down on
  completion/idle-timeout; every pod lifecycle (create/run/destroy + cost estimate)
  is logged so spend is visible and a pod can never be silently left running. A
  configurable monthly burst-budget cap with a warning.
- **Local-stays-free guarantee** — a test/asserted invariant that no build job is
  ever dispatched to the `local-llm` node, so the model never contends with a build.

Non-goals: the work-queue/registry mechanics (constellation-dispatch owns them — this
refines placement), the local model node (constellation-brain-local), the bus/mesh.

## Acceptance criteria

1. An Ansible role installs an sccache-dist build server on the cloud node that
   accepts offloaded compilation from the laptop (a `~/wintermute` crate builds with
   the cloud as the remote builder and a shared-cache hit) — demonstrated or
   reproducibly documented.
2. A burst-pod provisioner stands up an ephemeral high-core/GPU build server,
   registers it for the job, and **tears it down** on completion/idle-timeout; the
   full lifecycle + a cost estimate is logged (no silently-running pod).
3. The dispatch routing places `wm.work.build.*`/`test.*` jobs on cloud build
   servers (and the laptop when idle) and **never** on a node advertising
   `no_build: true` / `role: local-llm` — proven by a test submitting a build with
   the 5700U present and asserting it is not selected.
4. The 5700U's capability record carries `no_build: true`, and a fleet-wide
   invariant test confirms no build job lands there while it serves the model.
5. `wm-work build <crate>` offloads to the cloud builder(s), bursts a pod when a
   size/queue-depth threshold is crossed, reports where it ran, and the laptop's
   `cargo` transparently offloads via `rustc-wrapper`.
6. A configurable monthly burst-budget cap is enforced and warned on; exceeding it
   blocks new pods (no surprise bill) and is logged.
7. With builds routed to cloud and the local node serving qwen2.5-8B, a
   simultaneous "build + voice turn" scenario shows the build running in the cloud
   while the local-llm tier answers — neither starving the other (integration test
   or documented reproducible demo).
