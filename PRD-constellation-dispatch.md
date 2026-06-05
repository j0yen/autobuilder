# PRD: constellation-dispatch — work flows to whoever has capacity

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/agorabus-nats-bridge
Vision: visions/constellation.md

## TL;DR

This is the payoff the user named: "coordinate, collaborate and distribute
workloads to maximize development throughput." With the bus fleet-wide, this PRD
adds the dispatch layer: a **JetStream work-queue** where build/test/inference
jobs are claimed by whichever node has capacity (demand-pull), a **capability
registry** so the coordinator routes GPU jobs to the GPU node and heavy builds to
the many-core node, and **distributed Rust compilation** via sccache-dist so a
cargo build on the laptop runs on the desktop and cloud. The laptop stops being
the throughput ceiling.

## Why this exists

Phase 1 research mapped this to two complementary lanes plus a registry, all on
infrastructure constellation-bus already stands up:

- **Generic jobs → JetStream work-queue + pull consumers.** A `WM_WORK` stream
  (WorkQueuePolicy: each job delivered once, removed on ack) with subjects encoding
  routing (`wm.work.build.rust`, `wm.work.infer.embed`, `wm.work.test`). Each node
  runs a **pull consumer** and fetches at its own pace — "pull consumers should be
  used for worker queues where processing speed varies," which is exactly a fleet
  of uneven hardware: the laptop pulls less, the desktop pulls more, no central
  scheduler needed for basic balancing. JetStream gives at-least-once + redelivery,
  so a job survives a worker crash (vs a hand-rolled queue you'd bolt durability
  onto).
- **Capability advertisement → JetStream KV registry + heartbeat TTL**, not gossip
  (gossip's scale benefit is irrelevant at 3-5 nodes). Each node writes
  `node.<name>` into the `WM_NODES` KV every N seconds (`{cores, ram_gb, gpu,
  vram_gb, load1, free_ram, queue_depth, ts}`); a TTL/stale-`ts` self-expires a
  dead node. The coordinator watches the bucket and routes: GPU/embedding →
  `vram_gb` node with lowest load; Rust builds → most cores / lowest load. The
  bridge daemon (constellation-bus) already heartbeats presence — this extends it
  with capability.
- **Rust/C compilation → sccache(-dist), composed with the queue, not replaced.**
  NATS dispatches the *task* ("build crate X on the desktop"); on that node
  **sccache** (as `build.rustc-wrapper`) + **sccache-dist** parallelize the
  *compilation* and share a cache (object storage) across nodes. The
  `~/wintermute` workload is Rust-heavy, so this is the highest-leverage build win.
  bubblewrap ≥0.3.0 + kernel ≥4.6 required — this box (`7.0.10-arch1`) qualifies.

## What this builds

Extends `agorabus-nats-bridge` (the fleet daemon home) with a `constellation-dispatch`
crate + a `wm-work` CLI, plus an Ansible role for sccache:

- **Capability heartbeat** — the bridge daemon (or a sibling) samples local
  capacity (`procstat`-style: cores, RAM, load, GPU/VRAM if present, current queue
  depth) and writes `node.<name>` to the `WM_NODES` KV every N seconds with a TTL.
- **Worker** — a per-node pull consumer on `WM_WORK`, filtered to the job classes
  the node qualifies for (the GPU node binds `wm.work.infer.*`; every node binds
  `wm.work.build.*`/`test.*`). Claims a job, runs it, acks; on crash the job is
  redelivered.
- **Dispatch CLI (`wm-work`)** — `wm-work submit <class> <payload>` enqueues a job;
  `wm-work submit --to <node>` pins to a node; `wm-work status`/`wm-work nodes`
  reads `WM_NODES` to show the fleet's live capacity. A coordinator mode does
  capability-aware placement (read KV → choose subject/node) for jobs that need
  smarter-than-pull routing.
- **Distributed Rust builds** — an Ansible role configuring **sccache** as
  `rustc-wrapper` in `~/.cargo/config.toml` fleet-wide, **sccache-dist** build
  servers on the desktop + cloud (the laptop offloads), and a shared cache in
  object storage so cache hits are fleet-wide. A `wm-work build <crate>` convenience
  that fans a `~/wintermute` build out via the queue + sccache-dist.
- **Throughput guardrails** — never forward high-volume payloads through the bus
  (jobs carry references/paths, not large blobs — payloads stay small, artifacts
  move via shared storage/rsync over the mesh); log any job dropped/capped so
  silent truncation can't masquerade as completion.

Non-goals: the bus/NATS setup (constellation-bus), the hub host (constellation-cloud),
the GPU brain serving (constellation-brain-gpu). This PRD is the work-queue +
registry + distributed-build layer.

## Acceptance criteria

1. A `WM_WORK` JetStream work-queue (deliver-once, ack-removes) and per-node pull
   consumers exist; a submitted job is claimed by exactly one worker and removed on
   ack — verified against a hub+leaf test setup.
2. A worker crash before ack causes the job to be **redelivered** to another worker
   (at-least-once), proven by a test that kills a worker mid-job.
3. Each node writes a `node.<name>` capability record (`cores, ram_gb, gpu,
   vram_gb, load1, queue_depth, ts`) to the `WM_NODES` KV on a heartbeat, and a
   stale/crashed node's record self-expires via TTL.
4. `wm-work nodes` prints the live fleet capacity from `WM_NODES`; coordinator
   placement routes a GPU-class job (`wm.work.infer.*`) only to a node advertising
   `vram_gb > 0`, and a build job to the highest-core / lowest-load node (tested
   against a seeded KV).
5. sccache is configured as `rustc-wrapper` fleet-wide and sccache-dist build
   servers on the desktop/cloud accept offloaded compilation from the laptop; a
   `~/wintermute` crate builds with distributed compilation and a shared cache hit
   on a second node (demonstrated or reproducibly documented).
6. Job payloads on the bus are small (references/paths, not large blobs); a test
   asserts the dispatch path rejects/avoids oversized payloads, and large artifacts
   move via shared storage/rsync over the mesh, not the bus.
7. Any job dropped, capped, or unplaceable (no qualifying node) is logged
   explicitly (no silent loss), and `wm-work status` reflects it.
