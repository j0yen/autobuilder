# PRD: Session-Trace Receipt (codename: *session-trace*)

**Author:** Claude (Opus 4.7), drafted for jsy
**Status:** Draft v0.1
**Date:** 2026-05-22
**Eng owner:** TBD   **Stakeholders:** the autobuilder loop, every PRD with a `hard_constraints.deny_*` claim

---

## TL;DR

Autobuilder's risk gate enforces seven receipts, but every "deny" hard_constraint
(deny_unsafe, deny_network, deny_subprocess, future deny_filesystem_writes_outside_target)
is proved today via *static* checks: grep over Cargo.toml, grep over `src/`,
clippy lints. These are weak signals — they prove the source doesn't *say* the
thing; they don't prove the binary doesn't *do* the thing at runtime.

This PRD proposes `autobuilder.session_trace_receipt.v1`: an 8th Stage 4
receipt produced by wrapping the loop's metric-harness invocation in
wintermute's `ctrace` (eBPF session tracer) and validating the captured
execve / openat(W) / unlinkat / connect events against the intent-card's
hard_constraints. A `connect` event during a `deny_network` run is unfakeable
proof the constraint was violated — comparable to autoresearch's `val_bpb`,
not to a static lint.

It runs opt-in (`autobuilder loop --trace`), behind a sudo capability check,
and degrades gracefully when ctrace is unavailable (writes `verdict=skipped`,
does not fail the gate). The receipt name is `session-trace`; it slots into
`gate.rs` as receipt #8.

---

## 1. Why this exists (what the current 7-receipt gate misses)

The autobuilder intent-card schema already declares hard_constraints
(`deny_unsafe`, `max_deps`, `msrv`). Today these are enforced by:

| Constraint | How it's checked today | Falsifiable? |
|---|---|---|
| `deny_unsafe` | `cargo clippy -- -D warnings` + grep for `unsafe` token | Yes — `#[allow(unsafe_code)]` or `unsafe` in a build.rs the lint doesn't reach |
| `max_deps` | Count `[dependencies]` entries in Cargo.toml | Yes — git deps, path deps, dev-deps that leak into the binary |
| `no_unwrap_in_src` | Clippy `unwrap_used = "deny"` | Yes — `#[allow]` annotation, macros that expand to unwrap |
| (hypothetical) `deny_network` | nothing | n/a — not yet declared because there's no way to enforce |
| (hypothetical) `deny_filesystem_writes_outside_target` | nothing | n/a |

The pattern: every constraint we *want* to declare but don't is one we can't
prove. The 7-receipt gate is structurally complete for "did the build do what
the code says" but blind to "did the runtime do what the build promises."

### Concrete examples

- A test that quietly spawns `curl example.com` to fetch a fixture would pass
  every existing receipt. The reviewer-agent might catch it on diff inspection
  if the change is small; it would not catch a transitive dep doing the same
  thing in a build script.
- A `build.rs` that runs `chmod +x /usr/local/bin/evil` would pass the gate
  today. `cargo deny` checks licenses and advisories, not behaviors.
- The autobuilder loop itself spawns `bash scripts/run-metrics.sh` — what
  *that* spawns is invisible to the existing receipts. If a future PRD declares
  `max_subprocess_depth: 2`, there's no way to verify it without a tracer.

These are not theoretical. The `audit-checks.sh` regression that motivated the
recent reviewer concern `is-crash-row-coupled-to-column-index` was diagnosed
by reading code; with a session-trace receipt attached to the iteration that
broke it, the openat(W) event on `target/autobuilder/audit.json` would have
been visibly absent in the trace, pointing at the abort site immediately.

---

## 2. Goals and Non-Goals

### Goals

| # | Goal | Target |
|---|---|---|
| G1 | Capture a complete syscall trace of every Stage 3 iteration's PID tree (rooted at the metric-harness process). | All execve / openat(W) / unlinkat / connect events for descendants of the harness PID. |
| G2 | Validate the trace against intent_card.hard_constraints; emit a digest-bound receipt with verdict ∈ {pass, concern, block, skipped}. | Receipt schema `autobuilder.session_trace_receipt.v1` written to `target/autobuilder/receipts/session-trace.json`. |
| G3 | Slot the receipt into the existing 7-receipt gate as receipt #8 without breaking the existing 7. | `autobuilder gate` walks 8 receipts; `pass_verdicts = &["pass", "skipped"]` for session-trace. |
| G4 | Degrade gracefully when ctrace is unavailable. | On systems without bpftrace, sudo, or wintermute's ctrace, emit `verdict=skipped` with a `skip_reason` field. Do not block the gate. |
| G5 | Make the trace itself unfakeable. | Receipt contains the sha256 of the raw ctrace NDJSON log plus the digest is bound to HEAD via the same envelope as other receipts. |

### Non-goals (v1)

- **Always-on tracing.** Opt-in via `autobuilder loop --trace`. Loops without the
  flag continue to produce 7 receipts and the new one shows as `verdict=skipped`
  at gate time.
- **Per-syscall semantic interpretation.** We do not parse argv to detect
  `curl` vs `wget` vs `nc` vs an LD_PRELOAD'd custom binary. We check whether
  any `connect` event occurred. Network egress = constraint violation, period.
  Allowlists are a v2 problem.
- **Replay or causal-graph analysis.** The trace is a flat NDJSON stream. Tools
  like `ctrace query --grep` already exist for forensic spelunking; we don't
  build a UI.
- **Cross-platform.** Linux + bpftrace only in v1. macOS / WSL paths are deferred
  (would need eBPF replacement: `dtrace` on mac, none on WSL).
- **Replacement for `tracing`-crate instrumentation.** Session-trace observes
  the *outside* of autobuilder (subprocesses, file IO). In-process structured
  logging is a separate effort.

---

## 3. Architecture

```
autobuilder loop --trace --project <p> --iteration <n> --head-sha <sha>
                  │
                  │   (1) ctrace start --root $$ --log target/autobuilder/session-trace.ndjson
                  │
                  ▼
        bash <p>/scripts/run-metrics.sh           ← traced PID tree
                  │
                  │   (2) emits target/autobuilder/metrics.json (as today)
                  │
                  ▼
        ctrace stop
                  │
                  │   (3) autobuilder loop reads session-trace.ndjson,
                  │       evaluates against intent_card.hard_constraints,
                  │       writes target/autobuilder/receipts/session-trace.json
                  │
                  ▼
        gate.rs walks 8 receipts (was 7)
```

### Wiring points

- **`autobuilder/src/loop_runner.rs`** — add `--trace` flag. When set, shell out
  to `ctrace start` before the harness spawn and `ctrace stop` after. Capture
  log path. Hand to a new `session_trace` module for evaluation.
- **`autobuilder/src/session_trace.rs`** (new) — owns:
  - Trace evaluation: parse NDJSON, count events per type, match against
    intent_card.hard_constraints, emit `SessionTraceReceipt`.
  - Receipt emission via the existing `receipt::write` helper (digest-bound).
- **`autobuilder/src/gate.rs:48`** — append an 8th `ReceiptSpec`:
  ```rust
  ReceiptSpec {
      name: "session-trace",
      file_name: ReceiptPath::Static("session-trace.json"),
      expected_schema: "autobuilder.session_trace_receipt.v1",
      requires_head_match: true,
      pass_verdicts: &["pass", "skipped"],
  }
  ```
- **`~/.claude/skills/autobuilder/schemas/session-trace-receipt.schema.json`** (new) — JSON schema.

### Receipt schema (`autobuilder.session_trace_receipt.v1`)

```jsonc
{
  "schema": "autobuilder.session_trace_receipt.v1",
  "head_sha": "<40-char sha>",
  "captured_at": "<ISO 8601 UTC>",
  "tracer": {
    "tool": "ctrace",
    "version": "<output of `ctrace --version` or 'unknown'>",
    "root_pid": <int>,
    "log_sha256": "<sha256 of raw NDJSON>",
    "log_path": "target/autobuilder/session-trace.ndjson",
    "event_count": <int>
  },
  "constraints_evaluated": {
    "deny_network": { "claimed": true, "connect_events": 0, "violated": false },
    "deny_unsafe_runtime": { "claimed": true, "execve_events": 47, "disallowed_binaries": [], "violated": false },
    "max_subprocess_depth": { "claimed": 4, "observed_max": 3, "violated": false }
  },
  "verdict": "pass" | "concern" | "block" | "skipped",
  "skip_reason": "<string, only present when verdict=skipped>",
  "notes": ["<one-line per advisory observation>"]
}
```

The `constraints_evaluated` block is keyed by intent-card constraint name; only
constraints actually declared in the intent-card appear. A missing constraint
means it wasn't asserted, not that it passed.

---

## 4. Acceptance criteria

| ID | Level | Description | Test |
|---|---|---|---|
| AC1 | MUST | `autobuilder loop --trace` shells out to `ctrace start --root <pid>` before the metric-harness and `ctrace stop` after, capturing NDJSON to `target/autobuilder/session-trace.ndjson`. | `scripts/run-metrics.sh` — smoke against this repo, assert the log file exists and is non-empty after a traced iteration |
| AC2 | MUST | When ctrace is unavailable (missing binary, sudo denied, bpftrace missing), `--trace` emits a `session-trace.json` receipt with `verdict=skipped` and a `skip_reason` field; does NOT abort the iteration. | unit test mocking the ctrace binary path to `/bin/false` |
| AC3 | MUST | The receipt validates against `autobuilder.session_trace_receipt.v1` schema and is digest-bound (same envelope as other receipts). | tests/acceptance_ac3.rs — round-trip parse + digest verify |
| AC4 | MUST | `constraints_evaluated` block is populated from the intent-card's `hard_constraints` — only declared constraints appear, each with a `claimed`, an `observed`-style counterpart, and a `violated` bool. | tests/acceptance_ac4.rs — intent-card with `deny_network: true` produces a `deny_network` entry; intent-card without it does not |
| AC5 | MUST | When `deny_network: true` is claimed and the trace contains ≥1 `connect` event, `verdict=block`. | tests/acceptance_ac5.rs — synthesized NDJSON with a `connect` line, expect block verdict |
| AC6 | MUST | `autobuilder gate` walks 8 receipts (was 7); existing 7 receipts continue to pass on this repo without modification. | `autobuilder gate --project /home/jsy/projects/autobuilder` after a traced loop; expect `receipts=8 pass=8 verdict=pass` |
| AC7 | SHOULD | Receipt includes `tracer.log_sha256` so the raw NDJSON is tamper-evident — recomputing the sha256 of the log on disk must match the receipt. | tests/acceptance_ac7.rs |
| AC8 | SHOULD | `--trace` adds < 5% wall-clock overhead to the loop on a representative iteration (cargo test–heavy). | benchmark script: 5 traced + 5 untraced iterations, compare median |
| AC9 | MAY | A `--trace-filter <type,type,...>` flag restricts captured event types (e.g. `--trace-filter=connect,execve`) to reduce log size for long iterations. | not tested |

---

## 5. Hard constraints

| Constraint | Value |
|---|---|
| `rust_edition` | `2024` |
| `target_kind` | `cli` (lives inside the existing autobuilder workspace) |
| `deny_unsafe` | `true` |
| `max_deps` | `+2` over current (anticipate: `nix` or raw `libc` for PID handling; otherwise pure-std subprocess + serde + sha2 already in workspace) |
| `msrv` | `1.85` (workspace pin) |
| `no_unwrap_in_src` | `true` |
| `no_expect_in_src` | `true` |
| `all_receipts_digest_bound` | `true` |

### Unfakeable metric

```jsonc
{
  "name": "session_trace_receipt_callable",
  "lower_is_better": false,
  "harness_command": "scripts/run-metrics.sh",
  "target": 1
}
```

The metric is binary: does `autobuilder gate --project .` after a traced loop
emit a `session-trace` receipt with `verdict ∈ {pass, skipped}` and increment
the gate's receipt count from 7 to 8? Either yes (metric=1) or no (metric=0).
The bash harness counts this with a single jq query.

---

## 6. Open questions

1. **Sudo discovery.** ctrace requires `sudo -n bpftrace`. On a fresh CI runner
   sudo will prompt. Options: (a) document a sudoers entry as a prereq for
   `--trace`; (b) detect the sudo failure and degrade to `verdict=skipped` with
   `skip_reason=sudo_required`. v1 picks (b) — explicit opt-in already implies
   the user knows what they're enabling.
2. **Cross-iteration trace retention.** Should iter-N's trace be archived under
   `target/autobuilder/session-traces/<head_sha>.ndjson`, or overwritten each
   iteration? Default: archive (same pattern as `receipts/<head_sha>.json`).
   Trace size is a concern on long loops — propose a `--trace-rotate <N>` flag
   that keeps the last N traces.
3. **What counts as a `disallowed_binary`?** v1 uses the simplest possible
   rule: if `deny_unsafe_runtime: true`, the allowlist is the binaries autobuilder
   itself spawns (cargo, rustc, bash, sh, jq, git, ctrace, bpftrace, sudo). Any
   execve of anything else → violation. This is intentionally strict.
   Customization is a v2 problem.
4. **Path-write scope.** A future `deny_filesystem_writes_outside_target`
   constraint would need to filter openat(W) by path prefix. v1 doesn't ship
   this constraint, but the schema accommodates it under
   `constraints_evaluated`.

---

## 7. Rollout

**Phase A — Schema + receipt producer (no gate wiring)**
1. Vendor `~/.claude/skills/autobuilder/schemas/session-trace-receipt.schema.json`.
2. Implement `autobuilder/src/session_trace.rs`: NDJSON parser + constraint evaluator + receipt writer.
3. Unit tests for the skipped path (AC2) and the violation path (AC5).

**Phase B — Loop integration**
4. Add `--trace` flag to `loop_runner.rs`; wire ctrace start/stop around the harness spawn.
5. Smoke against this repo: `autobuilder loop --trace --project . --iteration 0 --head-sha $(git rev-parse HEAD)`.
6. Confirm receipt lands at `target/autobuilder/receipts/session-trace.json` and validates.

**Phase C — Gate wiring**
7. Append the 8th `ReceiptSpec` to `gate.rs`.
8. Confirm `autobuilder gate --project .` reports `receipts=8 pass=8 verdict=pass`.
9. Confirm untraced loops still pass the gate (receipt is `verdict=skipped`).

**Phase D — Dogfood and document**
10. Run `autobuilder loop --trace` once on each of: `recall`, `mcp-autotuner`, this repo. Verify nothing breaks.
11. Update `PLAN.md` "Status" block to note the 8th receipt.
12. Update `SKILL.md` Stage 4 receipt table.

---

## 8. What this is NOT

- **A replacement for `cargo deny`, clippy, or the reviewer-agent.** Each catches
  different things. session-trace adds *runtime* evidence; the others observe
  *source*.
- **A general-purpose tracing layer.** For per-function timing, structured logs,
  span IDs across async boundaries, use the `tracing` crate. session-trace is
  for proving the loop's runtime *did not do* things the intent-card forbids.
- **A security tool in the IDS sense.** It does not block syscalls in real
  time. It produces a post-hoc receipt that the gate consumes.

---

## 9. Status

**Draft v0.1.** Not yet started. The natural place to begin is Phase A: schema
+ receipt producer + unit tests, in a single feature branch. Phase B requires
sudo configuration on whatever host runs the first traced iteration — defer
until Phase A is green.
