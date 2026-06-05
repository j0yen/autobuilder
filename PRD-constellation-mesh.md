# PRD: constellation-mesh — one private network with stable names

Status: Draft v0.1
build_target: shell
Vision: visions/constellation.md

## TL;DR

The nodes are on different networks — a laptop that roams behind NAT, a desktop at
home, a cloud node with a public IP. Before any bus or job traffic can flow, they
need one private, NAT-traversing network with stable names. This PRD enrolls every
node into a **Tailscale** mesh (WireGuard underneath), gives each a stable MagicDNS
name, restricts access with ACLs, and makes the cloud node the exit/relay — the
substrate every later layer rides on.

## Why this exists

Phase 1 research compared the mesh options for a 3-5 node personal fleet with one
cloud node and a roaming laptop:

- **Plain WireGuard "hits a wall" on NAT** — peers behind CGNAT can't connect
  without an out-of-band coordinator WireGuard deliberately omits. That's exactly
  the laptop case.
- **Tailscale** adds the coordinator + DERP relays (~99% NAT-traversal incl.
  CGNAT), **MagicDNS** (stable per-node hostnames), and ACLs — near-zero-config,
  free for personal use, WireGuard-fast when P2P.
- **The MagicDNS decision is load-bearing:** every later NATS leaf/hub URL and job
  endpoint should be a **MagicDNS name, never an IP**, so a roaming laptop
  reconnects with zero config change. This single choice makes the bus layer clean.
- Sovereignty option: **Headscale** (self-hosted Tailscale control server on the
  cloud node) keeps the client UX + MagicDNS while owning the control plane —
  flagged in the vision as an open question.

## What this builds

A `constellation/mesh/` setup (shell + config, integrated as an Ansible role so
provision can apply it):

- **Enrollment** — a scripted/role-driven `tailscale up` per node with an auth key
  from the encrypted secret store (pre-authorized, tagged by role:
  `tag:laptop`, `tag:desktop`, `tag:cloud`).
- **Stable names** — MagicDNS enabled; assert each node resolves the others by a
  stable name (`cloud`, `desktop`, `laptop` within the tailnet). A small
  `constellation mesh names` helper prints the canonical fleet name map that the
  bus/brain/dispatch layers consume.
- **ACLs** — a committed ACL policy: which tags may reach which ports (e.g. only
  fleet nodes may reach the NATS leaf `:4222` / hub `:7422`; the GPU brain
  `:8080` reachable only from fleet tags). Ciphertext-free (ACL is not secret;
  keys are).
- **Exit/relay** — the cloud node configured as a stable always-reachable peer
  (and optional exit node), since it hosts the hub.
- **Headscale variant (documented + scripted-optional)** — a flag to point
  enrollment at a self-hosted Headscale on the cloud node instead of Tailscale's
  control plane, for the sovereignty path.
- **Health check** — `constellation mesh status` verifies every expected node is
  present, reachable by MagicDNS name, and within ACL — usable by later layers as
  a precondition.

Non-goals: the bus itself (constellation-bus), job dispatch, brain serving. This
PRD delivers connectivity + names + access policy only.

## Acceptance criteria

1. An Ansible role / script enrolls a node into the mesh using a pre-authorized
   auth key drawn from the encrypted store (no plaintext key in the repo).
2. After enrollment, every node resolves every other node by a **stable MagicDNS
   name** (asserted: `ping`/`tailscale status` by name, not IP, succeeds across
   the fleet — demonstrated with at least two nodes or a documented reproducible
   test).
3. A roaming node (network change simulated) reconnects and is reachable by the
   same MagicDNS name with no config change.
4. The committed ACL policy restricts the bus/brain ports to fleet tags only; a
   node/tag outside the policy is denied (asserted against the policy file's
   semantics).
5. `constellation mesh names` prints the canonical fleet name map (role → MagicDNS
   name) that downstream layers consume; `constellation mesh status` reports
   present/absent/unreachable per expected node and exits non-zero if the fleet is
   incomplete.
6. The Headscale (self-hosted control) path is at least documented and selectable
   by a flag, so the sovereignty option is real, not hypothetical.
7. No mesh private key is ever written to the repo; only auth keys (encrypted) and
   the non-secret ACL policy are version-controlled.
