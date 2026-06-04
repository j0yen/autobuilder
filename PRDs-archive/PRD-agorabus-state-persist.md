# PRD: agorabus-state-persist — claims and intents survive a bounce

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/agorabus
Vision: visions/vigil.md

## TL;DR

The agorabus daemon keeps its claims table and sticky intents in memory
only. A restart — exactly the operation vigil wants to make routine —
silently drops every active chord-claim lock and every intent string. A
file-lock another session is holding through agorabus vanishes the moment
the bus is rolled. This PRD journals the durable slice of bus state
(claims + sticky intents) to `~/.cache/agorabus/state.json` on mutation
and on drain, and rehydrates it on start, so a reload does not quietly
revoke locks.

## Why this exists

- **The code already admits the gap.** `daemon.rs:72` (read in Phase 1,
  2026-05-29) comments on the `claims` map: *"In-memory only; dropped on
  daemon restart per PRD-chord-claim §State persistence."* The
  persistence that comment defers has never been built — confirmed by
  grepping `persist` across `src/` (only that comment matches).
- **A bounce shouldn't revoke a lock.** chord-claim exists so concurrent
  sessions don't clobber shared files (recall DB, settings.json — the
  founding agorabus use case, README "Why"). If vigil rolls the bus while
  a `/build` session holds a claim, that session believes it still owns
  the path while the new daemon has no record of it — precisely the
  clobber chord-claim was built to prevent. Persistence closes that
  window.
- **Sticky intent is meant to persist.** `protocol.rs:29` documents
  intent as *"Sticky: set once, persists until cleared"* — but only
  within a daemon lifetime. A bounce clears it, contradicting the stated
  semantics. Journaling intent restores the documented behavior across a
  reload.
- Supports vigil's non-destructive-reload end-state: a reload should
  preserve coordination state, not reset it.

## What this builds

Extends `~/wintermute/agorabus/` (rust-extend; adds persistence to the
existing `BusState`). Current version 0.4.0.

- A `state.json` at `~/.cache/agorabus/state.json` (path overridable via
  `--state-file`), written `0600`, holding the serializable slice of
  `BusState`: the `claims` map (`canonical_path → ClaimRecord` incl.
  `ttl_unix_secs`) and the sticky intents per session_id. Live socket
  connections and ephemeral peer-connection ids are **not** persisted
  (they are meaningless across a restart; peers re-announce via
  PRD-agorabus-client-reconnect).
- Write strategy: atomic write-and-rename on each mutation that changes
  the durable slice (claim acquire/release/expire, intent set/clear) plus
  a final flush during drain (composes with PRD-agorabus-drain-notice but
  does not depend on it — a plain SIGTERM still flushes). Debounced so a
  burst of mutations coalesces (default 250ms, `--state-flush-ms`).
- Rehydrate on startup: if `state.json` exists and parses, load claims
  and intents into `BusState::new()`, then immediately
  `prune_expired_claims(now)` (the existing method, `daemon.rs:~86`) so
  claims whose TTL elapsed during downtime are dropped on load — a stale
  lock is never resurrected.
- Corruption tolerance: a missing or unparseable `state.json` is treated
  as empty state (log a warning, start clean) — the bus must always
  start, never refuse to boot on a bad journal.

No protocol change. No change to claim/intent *semantics*, only their
durability. Reuses the existing `serde` derive already on the records.

## Acceptance criteria

1. **AC1 — claims survive restart.** A claim acquired on a path is still
   present (same `ttl_unix_secs`) in `agorabus peers`/claim-query output
   after the daemon is killed and relaunched against the same state-file.
   Integration test: `tests/acceptance_claim_persists.rs`.
2. **AC2 — expired claims are not resurrected.** A claim whose
   `ttl_unix_secs` lies in the past at rehydrate time is absent after
   restart (load → `prune_expired_claims`). Test sets a short TTL, waits
   past it across a restart, asserts the claim is gone.
3. **AC3 — sticky intent survives restart.** An intent set via
   `agorabus intent set` for a session_id is reported after a daemon
   bounce, matching the `protocol.rs:29` "persists until cleared"
   contract. (The intent is restored even though the peer connection is
   new post-reconnect.)
4. **AC4 — atomic, mode-0600 journal.** `state.json` is created with mode
   0600 and written via a temp-file rename (no partial-write window);
   test inspects the file mode and asserts no `.tmp` residue after a
   flush.
5. **AC5 — corrupt journal starts clean.** With a deliberately truncated
   `state.json`, the daemon starts successfully with empty claims/intents
   and logs a warning (asserted in daemon.log); it does not exit nonzero.
6. **AC6 — no regression.** The existing agorabus acceptance suite passes
   unchanged; with no `state.json` present the daemon behaves exactly as
   0.4.0 (writes one on first mutation).
