# PRD: constellation-cloud — the always-on hub that keeps the fleet coherent

Status: Draft v0.1
build_target: mixed
build_into: /home/jsy/wintermute/constellation
Vision: visions/constellation.md

## TL;DR

Personal machines sleep, roam, and power off; the fleet needs one node that is
always on to host the NATS hub, anchor the mesh, and provide an offline-fallback
brain. This PRD provisions that node cheaply — a ~€8/mo Hetzner ARM box (Oracle
free tier as hot spare) — running the JetStream hub, the mesh exit, and a small
local fallback brain, all from the same Ansible control plane. The latency brain
stays the Anthropic API, because at personal volume that beats any rentable GPU by
20-40×.

## Why this exists

Phase 1 research made the cloud economics unambiguous:

- **Do NOT self-host the latency brain.** A personal voice turn is ~1K tokens; even
  200 turns/day ≈ 6M tokens/month ≈ **$3-11/mo on the Anthropic API** (Haiku 4.5,
  prompt-cached) — already wired (`WM_ANTHROPIC_API_KEY` live, the ladder already
  goes local→haiku→sonnet→opus per project memory). The **cheapest 24/7 GPU is
  ~$200/mo** for a *worse* brain. Break-even is ~15-20M tokens/*day* sustained.
  So the cloud node is a cheap coordinator, not a GPU box.
- **Cheap always-on coordinator:** **Hetzner CAX21** (4 ARM / 8GB, ~€8/mo) is the
  recommended primary — headroom for the NATS hub + mesh exit + a 3B fallback
  model. **Oracle Always-Free A1** (4 ARM / 24GB, $0) is a strong *hot spare* but
  unreliable as sole safety-critical coordinator (ARM capacity scarcity + idle
  reclaim). Given project memory's "recalld liveness is safety-critical" stance,
  pay the €8 for reliability and keep Oracle as a free standby.
- **GPU is burst-only:** RunPod (RTX 4090 $0.69/hr, A40 48GB $0.44/hr) or Vast.ai
  spun up *per job* and killed — for constellation-dispatch's build/ML jobs, never
  left running. (Fly.io GPUs are retired after Aug 2026 — excluded.)
- This node is the **NATS hub** (constellation-bus defines the hub config) and the
  **mesh anchor / exit** (constellation-mesh), so it's where those layers
  physically land.

## What this builds

A `constellation/cloud/` Ansible role + provisioning scripts:

- **Provision** a Hetzner CAX21 (primary) and/or Oracle A1 (spare) from the same
  `constellation` Ansible (a `cloud` host group), reusing the base role (minus the
  desktop/voice roles — it's headless, `voice_node: false`).
- **NATS hub** — install + run `nats-server` with JetStream (**domain `hub`**),
  the leafnode listener `:7422` (TLS + creds), and the durable assets
  constellation-dispatch will use (`WM_WORK` stream, `WM_NODES` KV) created/owned
  here. (Config authored in constellation-bus; this PRD runs it on the cloud host.)
- **Mesh anchor/exit** — enroll as a stable Tailscale node (`tag:cloud`), advertise
  as exit/relay so NAT'd nodes always have a reachable peer (constellation-mesh
  policy applied).
- **Offline-fallback brain** — a small `qwen2.5:3b`-class local model (ollama or
  llama.cpp CPU) on the node as the *degraded* brain for when the Anthropic API is
  unreachable — consistent with the ladder's safe-posture floor; NOT the primary
  brain (the API is).
- **Failover doc + spare** — a documented, scripted promotion of the Oracle spare
  to primary hub (re-point leaf URLs via the MagicDNS name) if Hetzner is down.
- **Cost guardrail** — the role provisions only the cheap always-on node; any GPU
  pod is explicitly out-of-band/burst (constellation-dispatch), so the standing
  bill stays ~€8 + API usage.

Non-goals: the bus bridge/config internals (constellation-bus), job dispatch logic
(constellation-dispatch), GPU pods (burst, dispatch-owned). This PRD is "stand up
and run the always-on hub node."

## Acceptance criteria

1. The same `constellation` Ansible provisions a headless cloud node (Hetzner
   CAX21 primary profile) with the base role, `voice_node: false`, no desktop —
   verified via `--check`/VM or a real provision.
2. `nats-server` runs on the cloud node with JetStream **domain `hub`** and a
   leafnode listener on `:7422` (TLS + creds), reachable by fleet leaves over the
   mesh by MagicDNS name (asserted by a leaf connecting and seeing the hub).
3. The durable JetStream assets (`WM_WORK` work-queue stream, `WM_NODES` KV bucket)
   exist on the hub for constellation-dispatch to use (created idempotently by the
   role).
4. The node is enrolled as a stable `tag:cloud` mesh peer / exit and is reachable
   by every other node by its MagicDNS name.
5. An offline-fallback small brain runs on the node and answers a turn when invoked
   directly; the architecture documents that this is the *degraded* path and the
   **Anthropic API is the primary latency brain** (not self-hosted) — with the
   cost rationale recorded.
6. A documented, scripted failover promotes the Oracle Always-Free spare to hub
   (leaf URLs re-point by MagicDNS name) — demonstrated or reproducibly documented.
7. The standing monthly footprint is only the cheap always-on node (no persistent
   GPU); GPU usage is explicitly burst-only and out of this PRD's provisioned
   footprint (asserted by the role provisioning no GPU resource).
