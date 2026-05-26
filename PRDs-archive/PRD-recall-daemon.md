# PRD: recall daemon mode (codename: *current*)

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-25
**Builds on:** `recall` v0.4 (CLI single-binary) and `PRD-recall-observer-correlation.md` (state-file correlator — `current` subsumes it).
build_auto: true
build_target: rust-extend
build_into: /home/jsy/wintermute/recall
**Punts from:** `PRD-agentic-memory.md` §6 (v0.2 non-goal #6) and §10 ("if Phase 4 ever lands, daemon mode becomes the proper fix").

---

## TL;DR

`recall` is a CLI today: every invocation pays ~500ms cold-load for the
fastembed BGE-small ONNX model, ~30ms warm. That's fine for one query
per session (SessionStart hook, `/self-review` Phase 0). It is not fine
for per-turn retrieval at PostToolUse cadence, which is the natural
shape of "memory that helps the model on the next turn." `current` adds
an optional daemon — `recalld` — that keeps the model warm in-process,
exposes the same retrieval surface over a Unix-domain socket, and lets
hook scripts make sub-10ms queries. The existing CLI stays the supported
entry point; the daemon is opt-in via `recall daemon start` and is the
backend whenever the socket is up.

The daemon is not a redesign. It reuses every existing module —
`index::Index`, `embeddings::Embedder`, `retrieval::hybrid_with` — and
adds a thin request/response layer. The CLI auto-detects the socket and
forwards to it when present; falls back to in-process when absent. No
new query semantics, no new file format, no new behavior the user can
observe other than latency.

---

## 1. Why this exists

Three load-bearing observations from v0.4 operating experience:

1. **Cold-load latency is the dominant cost per query.** The PRD §10
   risk "First-query latency regression on cold cache" notes ~500ms–1s
   for the first hybrid query in a fresh process. For one-query-per-
   session consumers this is amortizable; for per-turn consumers it's a
   60× overhead.

2. **The observer correlator wants sub-100ms.** `braid` (sibling PRD)
   wires PostToolUseFailure→UserPromptSubmit correlation; the
   UserPromptSubmit step is in the prompt-latency critical path. A
   500ms model load on that hook is observable lag.

3. **PRD §4b.16 + §5.3 anticipate per-turn retrieval.** "Per-turn token
   budget; SessionStart hook currently emits top-8 per subject" — but
   the *real* delight is per-turn surfacing of memories relevant to the
   current tool use. Today's CLI can't pay for that. A warm daemon can.

---

## 2. What this builds

### 2.1 Binary: `recalld`

A long-lived process that:

- Loads the fastembed model once at startup (~500ms wall, ~130MB resident).
- Opens the SQLite index in WAL mode with `PRAGMA busy_timeout=5000`.
- Listens on `$XDG_RUNTIME_DIR/recall.sock` (falls back to
  `~/.cache/recall/recall.sock` if no XDG runtime dir).
- Speaks a length-prefixed JSON request/response protocol — minimal,
  not gRPC, not HTTP. Each request is `{op, args}`, each response is
  `{ok, body}` or `{error, message}`.

Supported ops (v1):

| op | args | response |
| --- | --- | --- |
| `query` | `{text, limit, hybrid, filters, project_subject}` | `{ranked_hits[]}` |
| `embed` | `{text}` | `{vector[]}` — for callers that want vectors but not retrieval |
| `touch` | `{id}` | `{recall_count}` |
| `ping` | `{}` | `{model_id, uptime_s, query_count}` |

Writes (`write`/`update`/`delete`/`reindex`) stay CLI-only in v1 — the
write path is rare and re-opening the SQLite connection in the daemon
adds invalidation complexity (the CLI's writes wouldn't be visible to
the daemon's connection cache). v2 unifies, if needed.

### 2.2 Daemon lifecycle

```
recall daemon start [--foreground]   # spawn (default detached + log to ~/.cache/recall/daemon.log)
recall daemon stop                   # SIGTERM, wait for socket removal
recall daemon status                 # ping the socket, print model_id + uptime
recall daemon restart
```

Auto-start on first query is **not** in v1 — too many concurrent-spawn
race conditions for a v1. Users invoke `start` once per boot (or wire
to a systemd-user unit, which we will provide as `recalld.service`).

### 2.3 CLI socket-forwarding

`recall query` / `recall list` / `recall similar` etc. check whether
the socket is responsive at startup. If yes, send the op over UDS and
print the response. If no, fall back to in-process (current behavior).
No flag toggles this — the socket is the truth.

`recall where` reports the socket path and whether the daemon is alive.
`recall doctor` reports `daemon_active: true|false` and warns if the
daemon's embedder id doesn't match the CLI's.

### 2.4 Hook-friendly client

A statically-linked `recall-client` minimal binary (single source file,
no fastembed dep) that hook scripts can call without paying the recall
binary's full startup cost (which today includes pulling in fastembed
and rusqlite into the dynamic linker). This is the latency win for
PostToolUse-style hooks: ~5ms cold start for `recall-client`, vs.
~30ms warm for the full `recall` binary.

---

## 3. Non-goals

- **Networked daemon.** UDS only. No TCP, no auth, no TLS. Single-user,
  single-host — that's the recall threat model.
- **Multi-tenant.** One daemon per UID. Concurrent sessions share it.
- **Schema-version negotiation across daemon and CLI.** The daemon and
  the CLI binary must be the same version; mismatched versions exit
  with a clear error. Don't try to support rolling upgrades.
- **Hot-reload.** Restart the daemon to pick up config changes.
- **Auto-spawn.** v1 is explicit `start`/`stop`. Implicit spawn-on-
  first-use is a v2 idea once we have data on whether users actually
  forget to start it.

---

## 4. Risks

- **Stale model in the daemon vs. updated fastembed model on disk.**
  Mitigation: ping op returns `model_id`; CLI's `recall doctor` warns
  if it has drifted from the current `--embedder` value.
- **Socket file leaked after crash.** Mitigation: `recalld` checks for
  a stale lock file at startup; if the holder PID is dead, it's removed.
  `recall daemon start` exits non-zero if a live daemon is detected.
- **Memory growth from fastembed long-lived state.** Mitigation: log
  RSS at a 1-hour interval; document the recycling pattern (`systemctl
  --user restart recalld`); v2 considers periodic auto-recycle.
- **Index lock contention.** The CLI's writes still happen against the
  same SQLite file. WAL is already enabled (v0.3); v1's daemon uses
  the same WAL + busy_timeout settings. Smoke-test concurrent CLI write
  + daemon read.

---

## 5. Acceptance tests

1. Cold daemon start: ≤ 1.5s from `recall daemon start` to first
   responsive `recall daemon status`.
2. Warm query through the socket: p50 ≤ 10ms for `recall query "test"`
   on a 50-memory store with the model warm in the daemon.
3. CLI auto-falls-back when the socket is absent: kill the daemon mid-
   session and the next `recall query` works (in-process) without an
   error message.
4. `recall doctor` reports `daemon_active: true` when the daemon is up
   and includes `daemon_uptime_s` in JSON output.
5. Concurrent: a CLI `recall write` succeeds while the daemon is
   serving a `query` op, and the new memory is visible to the next
   daemon query (within 500ms because WAL).
6. Crash recovery: SIGKILL the daemon, `recall daemon start` succeeds
   on the next invocation (stale socket cleaned up).

---

## 6. Phasing

- **6a (v0.5.0):** Daemon + UDS protocol + read-only ops (query / embed /
  touch / ping). CLI auto-forward. systemd-user unit.
- **6b (v0.5.1):** `recall-client` minimal binary. Hook scripts (braid
  user-prompt handler) switch to it.
- **6c (v0.6.0):** Write ops in the daemon (if observed need).
- **6d (deferred):** auto-spawn-on-first-use; daemon recycling policy.

---

## 7. Open questions

- Do we wrap the protocol in a tiny library (`recall-proto`) used by
  both the CLI and `recall-client`, or inline it? The library shape is
  obviously cleaner; the question is whether the proto stays small
  enough to live in one file across both consumers without ceremony.
- systemd-user vs. `~/.config/launchd` shape on macOS. v1 ships
  systemd-user (the user's wintermute machine); v1.1 adds the macOS
  variant if/when needed.
