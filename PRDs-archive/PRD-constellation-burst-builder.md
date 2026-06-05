# PRD: constellation-burst-builder — one dedicated cloud box absorbs the heavy compiles

Status: Draft v0.1
build_target: rust-cli
deferred_acs: [2, 4, 5]
# AC2 (real-host ansible provision), AC4 (warm remote sccache build), and AC5
# (remote SSH exec) require a live cloud builder + SSH target + sccache endpoint,
# none of which exist on this CPU-only laptop. Each ships a mock test under
# tests/mocks/ac<N>.rs (wired via tests/mocks.rs) exercising the same public API
# surface — playbook generation, sccache-stats parsing, exit-code propagation +
# cost-log recording — with in-process fakes. The PRD's own wording for these
# ACs is "demonstrated OR reproducibly documented"; the mocks are that
# reproducible documentation until a real Hetzner box is stood up.
Vision: visions/constellation.md
Refines: PRD-constellation-cloud-build.md (carves out a standalone, mesh-free
  "phase 0" path) and PRD-constellation-dispatch.md (this needs neither the
  JetStream work-queue nor the capability registry to be live)

## TL;DR

`wm-burst` is a small Rust CLI that points this laptop's `cargo` at **one
always-on, cheap, dedicated cloud box** (a Hetzner server-auction Ryzen 9 9950X,
32 threads / 64–128 GB, ~€50–70/mo) and a **shared sccache object cache**, so cold
builds, autobuilder determinism runs, and wake-word/ML CPU jobs stop pinning the
local cores. Unlike `constellation-cloud-build`, it requires **no NATS mesh, no
dispatch coordinator, no capability registry** — it is `ssh` + `sccache` + a config
file. It is the thing the user can stand up *today* and the on-ramp the full fleet
graduates from later. Crucially it refuses to run a remote build when the remote
toolchain doesn't match `rust-toolchain.toml`, so the 1.85/1.88 drift that has
bitten cold builds before can't silently corrupt a burst.

## Why this exists

jsy, 2026-06-04: *"maybe we should burst heavy rust compile and CPU jobs to the
cloud."* The motivating realization in this session: if heavy/batch work bursts to
the cloud, the local machine no longer has to be a 16-core, 128 GB, RTX-5090
monster — it only has to run the latency-bound voice loop + editor + Claude Code.
That reframes both the next hardware purchase **and** the day-to-day pain points
already on record:

- Wake-word **retrain OOMs** at 11.2 GB on this no-swap box (memory:
  `self_recall_baseline_gate_red`, the 2026-06-03 OOM-kill of
  `wake-retrain-realvoice.service`). A 64 GB cloud box makes that job fit instead
  of buying 128 GB locally for something run a few times a week.
- Autobuilder **cold-build/determinism receipts** (memory:
  `self_autobuilder_receipt_order`) run `cargo clean` and rebuild from scratch —
  exactly the workload worth offloading; the laptop stays free for the model and
  the voice stack.
- The full `constellation-cloud-build` PRD solves this *for the fleet* (sccache-dist
  servers, JetStream routing, the 5700U `no_build` carve-out). But all of that is
  gated on the mesh + dispatch layers being live. There needs to be a **standalone
  rung** that delivers the offload with zero fleet infrastructure — and that an
  individual machine (the one the user might buy) can use on its own.

## What this builds

A new CLI crate `~/wintermute/wm-burst` (published as `j0yen/wm-burst`):

- **`wm-burst init`** — writes/edits `~/.config/wm-burst/config.toml`: the remote
  host (ssh alias), the sccache object-store endpoint + bucket (S3-compatible:
  Hetzner Object Storage, Backblaze B2, or the dedicated box's own MinIO), a monthly
  burst-budget cap, and an optional burst-pod provider (RunPod/Vast/Hetzner Cloud
  API) for jobs bigger than the standing box.
- **`wm-burst provision`** — an Ansible playbook (idempotent) that takes a fresh
  Hetzner dedicated box to a ready builder: installs the pinned rustc toolchains
  (1.85 **and** 1.88 to match this repo's split), `sccache`, the build deps, sets up
  the shared-cache credentials, and registers the ssh alias. Re-runnable; converges.
- **`wm-burst doctor`** — verifies the remote is reachable, the remote `rustc
  --version` set **matches** the project's `rust-toolchain.toml` (hard fail on
  drift, with the exact mismatch printed), the sccache bucket is writable, and
  reports the shared-cache hit rate. This is the guardrail against the cross-machine
  toolchain drift that has produced spurious compile failures before.
- **`wm-burst build [-- <cargo args>]`** — runs the build with `RUSTC_WRAPPER=sccache`
  against the shared cache and the remote builder (sccache distributed mode, or a
  documented remote-`cargo`-over-ssh fallback when sccache-dist is not wired),
  streams output locally, and prints **where it ran + cache hit/miss counts + an
  elapsed-vs-local estimate** at the end. `wm-burst exec -- <cmd>` does the same for
  a non-cargo CPU job (e.g. the wake-train script) on the remote box.
- **`wm-burst pod up|down`** (optional tier) — for a job larger than the standing
  box, stands up an ephemeral high-core/GPU pod via the configured provider, runs
  the job, and **tears it down** on completion/idle-timeout. Every pod lifecycle
  (create/run/destroy + cost estimate) is appended to a cost log; a pod can never be
  silently left running, and exceeding the monthly cap blocks new pods.
- **`wm-burst status`** — shows the standing box's load, the cache hit rate, this
  month's burst spend vs cap, and the last N jobs (where each ran, duration, cost).

Non-goals: the NATS work-queue / capability registry / fleet routing
(constellation-dispatch + constellation-cloud-build own those — this is the
no-mesh rung beneath them); the local model node (constellation-brain-local); the
voice stack, which by design **never** bursts (real-time, must stay local).

## Acceptance criteria

1. `wm-burst init` writes a valid `config.toml` (remote host, sccache endpoint +
   bucket, monthly budget cap, optional pod provider) and `wm-burst init --show`
   round-trips it; a missing/invalid config produces a clear, actionable error.
2. `wm-burst provision` is an idempotent Ansible playbook that converges a fresh
   host to a ready builder (pinned 1.85 + 1.88 toolchains, sccache, shared-cache
   creds) — demonstrated against a real host **or** reproducibly documented with the
   playbook + a dry-run/check-mode pass that succeeds.
3. `wm-burst doctor` **hard-fails** when the remote `rustc` set does not match the
   project `rust-toolchain.toml`, printing the exact local-vs-remote mismatch, and
   passes (reporting reachability + cache-writability + hit rate) when they match —
   proven by a unit/integration test that feeds a mismatched toolchain pair and
   asserts a non-zero exit with the diagnostic.
4. `wm-burst build` compiles a `~/wintermute` crate with `RUSTC_WRAPPER=sccache`
   against the shared cache and reports where it ran + cache hit/miss counts; a
   second build of the same crate shows a materially higher cache-hit ratio (warm
   cache) — demonstrated or reproducibly documented.
5. `wm-burst exec -- <cmd>` runs a non-cargo CPU job on the remote box and streams
   its output + exit code locally (e.g. the wake-train script runs remotely and its
   exit status is faithfully propagated).
6. The pod tier creates an ephemeral builder, runs a job, and **tears it down** on
   completion/idle-timeout; the full lifecycle + a cost estimate is appended to the
   cost log, and a configurable monthly burst-budget cap is enforced — exceeding it
   blocks new pods (no surprise bill) and is logged. Provable with a mocked provider
   in test (no real spend required to pass).
7. `wm-burst status` reports the standing box load, cache hit rate, month-to-date
   burst spend vs cap, and the last N jobs (where each ran, duration, cost).
8. CLI hygiene: `sigpipe::reset()` is the first line of `main()` (memory:
   `self_sigpipe_panic_toolkit` — `println!`-based local CLIs coredump on
   `wm-burst status | head`); `--help`/`--version` work; MSRV 1.85, no let-chains.

## Open questions (for /build or jsy)

- Shared-cache backend: Hetzner Object Storage vs Backblaze B2 vs self-hosted MinIO
  on the dedicated box. Cheapest correct default is MinIO on the box you already pay
  for; pick at `init` time.
- sccache distributed (`sccache-dist`) vs the simpler remote-`cargo`-over-ssh path
  for v0.1. Distributed is the end state (and what cloud-build assumes); ssh-remote
  may be the faster first rung. AC4 allows either.
