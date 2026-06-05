# PRD: constellation-bus — the agorabus↔NATS bridge that makes the bus fleet-wide

Status: Draft v0.1
build_target: rust-cli
Vision: visions/constellation.md

## TL;DR

agorabus — the pub/sub + presence layer every wintermute daemon already speaks —
runs only over a local Unix socket; co-located processes are the only ones that
see each other. This PRD is the keystone of the whole vision: a small **bridge
daemon** that mirrors `wm.*` events between the local agorabus UDS and a **NATS**
mesh (hub on the cloud node, leaf per machine), so a wake on the laptop, a job
result on the desktop, and a heartbeat from the cloud are all visible fleet-wide —
**while every existing local UDS client keeps working unchanged.**

## Why this exists

Phase 1 research validated the transport choice and surfaced the one correction
that defines this PRD's shape:

- **NATS is the right cross-host bus** — its subject pub/sub maps directly onto the
  existing `wm.*` topics, and it uniquely also gives request/reply, queue groups,
  and JetStream durable work-queues + KV (which constellation-dispatch needs), with
  a NAT-friendly **leaf-node** topology MQTT/Redis can't match (leaf nodes serve
  local consumers first and queue cloudward traffic during disconnects).
- **Correction (load-bearing): NATS has no Unix-domain-socket listener** (confirmed
  open/unimplemented 2025-2026). So agorabus **cannot** become a NATS leaf over its
  UDS, and you cannot point NATS at `~/.cache/agorabus/sock`. The only way to keep
  local clients unchanged is a **bridge sidecar** that speaks the agorabus UDS
  protocol on one side and a NATS TCP client on the other.
- **Second footgun: JetStream is blocked across leaf boundaries by default** and is
  keyed by **JetStream domain** — the hub needs a domain (e.g. `hub`), each leaf
  its own, and clients address hub streams via the domain-qualified `$JS.hub.API.>`
  prefix or they silently can't see them.
- agorabus's own README confirms the single-host constraint ("co-located
  sessions... over a Unix-domain socket"); the topics are already `wm.*`, so the
  bridge mapping is near-identity. This is a thin, well-bounded Rust daemon — a
  natural `~/wintermute/agorabus-nats-bridge/` (the vision's keystone component).

## What this builds

A new repo `~/wintermute/agorabus-nats-bridge/` shipping the `wm-busbridge`
daemon, plus the NATS topology config:

- **Bridge daemon (`wm-busbridge`)** — one sidecar per machine:
  - an **agorabus UDS client** subscribing to the local bus (and able to publish
    back into it), so it sees/injects local `wm.*` events without any local client
    changing.
  - a **NATS client** to the local leaf at `127.0.0.1:4222`.
  - **identity mapping**: local `wm.<topic>` → NATS subject `wm.<topic>` and back.
  - **selective forwarding**: only `wm.fleet.>` (and an allowlist) crosses to NATS;
    local-only chatter (e.g. `wm.audio.speech.chunk` PCM, `wm.local.>`) stays on
    the UDS — fleet bandwidth and privacy. (Directly informed by the dialog
    chunk-flood lesson — never fan high-volume PCM across the network.)
  - **loop guard**: tag bridged messages (header/subject prefix) so an event that
    arrived *from* NATS is not re-published *out* to NATS (the one correctness
    pitfall).
  - **presence → fleet**: publish this node's agorabus presence onto a fleet
    subject so `agorabus peers` can optionally include remote nodes.
  - SIGPIPE-safe `main()` (`sigpipe::reset()` first line — toolkit convention).
- **NATS topology config** (in the repo, applied by Ansible):
  - **hub** config (runs on the cloud node, constellation-cloud installs it):
    JetStream enabled, **domain `hub`**, leafnode listener `:7422` (TLS + per-node
    creds).
  - **leaf** config (each machine): local client listener `:4222`, own JS domain,
    dials the hub at `nats://cloud.<tailnet>:7422` (MagicDNS name from
    constellation-mesh), TLS + creds.
  - per-node **credentials** drawn from the encrypted store; publish/subscribe
    permissions scope what each node may emit/consume.
- A `wm-busbridge selftest` that publishes a tagged event locally and confirms it
  appears on NATS exactly once (no loop) and round-trips back into a second local
  subscriber.

Non-goals: the mesh (constellation-mesh provides MagicDNS + reachability), the
cloud hub *host* (constellation-cloud runs the hub), work dispatch
(constellation-dispatch builds on JetStream here). This PRD is the bridge + bus
plumbing.

## Acceptance criteria

1. `wm-busbridge` subscribes to the local agorabus UDS and a local NATS leaf, and
   mirrors an allowlisted `wm.fleet.*` event published locally onto the matching
   NATS subject — verified against an embedded/test NATS server.
2. **Local UDS clients are unchanged**: an existing agorabus client (e.g.
   `agorabus peers` / a `wm.*` subscriber) works identically with the bridge
   running, requiring no code or socket change (asserted).
3. **Loop guard**: an event injected from NATS into the local bus is NOT
   re-published back to NATS (a message traverses the bridge at most once per
   direction) — proven by a selftest counting exactly-once delivery.
4. **Selective forwarding**: a high-volume / local-only topic (e.g.
   `wm.audio.speech.chunk` or `wm.local.*`) is NOT forwarded to NATS; only the
   allowlisted fleet topics cross (asserted).
5. The NATS topology config sets a **JetStream domain** on the hub and leaves, and
   a leaf client can reach a hub JetStream stream via the domain-qualified API
   prefix (documented + asserted against a hub+leaf test setup).
6. Per-node NATS credentials come from the encrypted store; the repo contains no
   plaintext creds (grep-asserted), and a node's pub/sub permissions are scoped by
   its credential.
7. `wm-busbridge selftest` passes end-to-end (local publish → NATS once → back into
   a second local subscriber) and the daemon does not panic on a closed pipe
   (SIGPIPE reset present).
