# PRD: wintermute — bus-smoke convention + fleet backfill

**Author:** Claude (Opus 4.7) via /dream pass 16
**Status:** Draft v0.1
**Date:** 2026-05-27
**Vision:** `visions/wintermute.md` § Fleet 1.5 — Maturation & validation
**Sibling:** `PRD-wintermute-fleet-agorabus-announce-fix.md` (queued, drafted 2026-05-27 by /build follow-on) — fixes the bug; this PRD ensures the bug class can't ship undetected again
**Builds on:** wintermute-audio (`tests/wake_bus_smoke.rs`, `tests/vad_bus_smoke.rs`, `tests/reload_bus_smoke.rs`) — empirical prior art
build_target: mixed
build_priority: high
build_version_bump: none

---

## TL;DR

Four wintermute fleet daemons — `wm-tts`, `wm-stt`, `wm-dialog`,
`wmd` (brain) — shipped 2026-05-27/28 with the same protocol bug:
each calls `agorabus::Client::connect()` and then immediately
`.subscribe()` without first calling `.announce()`. The agorabus
daemon enforces "first message must be `Announce`"
(`agorabus/src/daemon.rs:315` — replies `announce_required` and closes
the connection); all four daemons exit within ~1 s of contacting the
real bus. `wm-audio` (shipped earlier) does this right.

`PRD-wintermute-fleet-agorabus-announce-fix.md` patches the four
daemons. **This PRD closes the gap that allowed the bug to ship at
all** — none of those four repos has a `tests/bus_smoke.rs` that
spins up an in-process agorabus and exercises the daemon's
real-protocol wire-up before merge. `wm-audio` has three such tests
(`wake_bus_smoke.rs`, `vad_bus_smoke.rs`, `reload_bus_smoke.rs`); the
other four repos have only `acceptance_template.rs` placeholders.

This PRD adds a **bus-smoke convention** (mirrors the
`hardware-smoke.md` convention shipped by pass 15) and backfills a
canonical `tests/bus_smoke.rs` into the four affected repos. The
mechanism is identical in shape to hardware-smoke — convention doc +
scaffolded tests + no skill/version/binary changes — but the test
files actually run in CI (not gated on an env witness, because no
hardware is needed; agorabus runs in-process).

The convention also pays forward to Fleet 2 (browser, desktop,
screen-narrate, mail, calendar, music): every new `wm-*` daemon that
uses agorabus must include a `tests/bus_smoke.rs` before its PRD can
archive.

---

## 1. Why this exists

**Live evidence (verified 2026-05-27T22:55Z):**

- `~/wintermute/agorabus/src/daemon.rs:315-316` —

  ```rust
  write_json_line(write_half, &Reply::error("announce_required")).await?;
  anyhow::bail!("announce_required");
  ```

  The daemon refuses any non-`Announce` first message and tears down
  the connection. This is by design; ack since v0.1.

- `~/wintermute/wintermute-tts/src/daemon.rs:815-824` — sub_client and
  pub_client are connected without `announce()`:

  ```rust
  let Some(mut sub_client) = agorabus::Client::try_connect(&sock).await? else { /* … */ };
  // … no .announce() here
  let pub_client = agorabus::Client::connect(&sock).await?;
  // … no .announce() here either
  ```

  Identical shape at `wintermute-stt/src/daemon.rs:214,226`,
  `wintermute-dialog/src/daemon.rs:450,462`,
  `wintermute-brain/src/daemon.rs:1310,1320` — all four daemons share
  the same bug.

- `~/wintermute/wintermute-audio/src/daemon.rs:17` (used at
  `tests/wake_bus_smoke.rs:103-113`):

  ```rust
  let mut subscriber = Client::connect(&bus_sock).await?;
  subscriber
      .announce("wake-bus-smoke-sub", std::process::id(), "", "test-subscriber")
      .await?;
  subscriber.subscribe("wm.audio.wake").await?;
  ```

  Correct ordering. Test exercises it against an in-process daemon
  spawned via `agorabus::run_daemon` on a temp socket. Runs in regular
  `cargo test --release`, no witness needed.

- `cargo test --release --test wake_bus_smoke` in `wintermute-audio`
  passes today (verified 2026-05-27T22:58Z, runtime 1.4 s).

- `~/wintermute/wintermute-tts/tests/` contents (verified
  2026-05-27T22:58Z): `acceptance_ac8.rs`, `acceptance_template.rs`,
  `hardware_acs.rs`, `proptest_invariants.rs`. **No `bus_smoke.rs`.**
  Same shape for stt, dialog, brain.

**Why this is a class of bug, not a one-off:**

`agorabus::Client` is the load-bearing wire-up surface for every
fleet daemon. Five daemons exist today; six more are queued (Fleet 2:
browser, desktop, screen-narrate, mail, calendar, music). Each will
add another `Client::connect()` call site. Without a convention
mandating bus-smoke, **the next wm-\* PRD ships the same bug by
default** — the test scaffolding pattern that's "supposed" to catch
it lives in wm-audio's tests, not in any documented convention.

The cost of the convention is small: one test file per repo,
~80-120 LOC each, follows the wm-audio template verbatim. The cost
of *not* having it is what just shipped — four daemons that exit
within 1 s of contacting the real bus, caught only during manual
end-to-end bring-up.

---

## 2. What this builds

**A. Convention doc.** New file
`~/wintermute/autobuilder/notes/conventions/bus-smoke.md`
documenting:

- **Where:** `tests/bus_smoke.rs` in the repo root. One file per
  repo. New per-feature smoke files (e.g., `wake_bus_smoke.rs`,
  `vad_bus_smoke.rs`) are fine alongside; `bus_smoke.rs` is the
  canonical name for the announce-ordering check.
- **Shape:** Spawn an in-process agorabus daemon via
  `agorabus::run_daemon` on a per-test temp socket (see wm-audio's
  `tmp_path` helper for the 0700-parent-dir gotcha). Wait on the
  `ready_tx` oneshot. Connect a test subscriber, announce, subscribe
  to the daemon-under-test's topic prefix. Start the
  daemon-under-test pointed at the same socket. Assert: (a) the
  daemon stays up for ≥2 s without an `announce_required` error in
  its anyhow chain; (b) at least one expected event publishes
  through the bus (drives the daemon with whatever scripted input
  the repo has in its other smoke tests). Cleanly shutdown via
  oneshot.
- **Pattern reference:** Cite
  `wintermute-audio/tests/wake_bus_smoke.rs` lines 82-165 as
  canonical prior art. The convention doc includes a 30-line
  skeleton copy-paste-able into a new daemon repo.
- **CI behavior:** `cargo test --release --test bus_smoke` runs in
  regular CI. No `#[ignore]`. No env witness. The daemon's bus
  client misuse must be a compile/test failure, not a runtime
  surprise.
- **Promotion path:** If a repo has multiple distinct publish paths
  (wm-audio has wake/vad/reload — each its own smoke file),
  `bus_smoke.rs` covers the minimum (connect + announce + subscribe
  + one publish). Per-path files like `wake_bus_smoke.rs` are
  additive, not replacement.

**B. Four scaffolded test files (rust-extend ×4, one per repo).**

`~/wintermute/wintermute-tts/tests/bus_smoke.rs` — spawns in-process
agorabus, starts `wm-tts daemon` against it, publishes `wm.tts.speak`
with a test payload, asserts daemon stays up + emits the matching
`wm.tts.spoke` event without `announce_required`.

`~/wintermute/wintermute-stt/tests/bus_smoke.rs` — spawns in-process
agorabus, starts `wm-stt daemon` against it, publishes a synthetic
`wm.audio.utterance` (NullSource-style), asserts daemon stays up +
emits at least one `wm.stt.partial` or `wm.stt.final` event without
`announce_required`.

`~/wintermute/wintermute-dialog/tests/bus_smoke.rs` — spawns
in-process agorabus, starts `wm-dialog daemon` against it, publishes
a synthetic `wm.stt.final`, asserts daemon stays up + emits at least
one `wm.dialog.turn` or `wm.tts.speak` event without
`announce_required`.

`~/wintermute/wintermute-brain/tests/bus_smoke.rs` — spawns
in-process agorabus, starts `wmd` (brain) against it with a stubbed
recall socket (the existing `RecallClient` stub pattern in the brain's
`recall_client.rs` tests is the reference), publishes a synthetic
`wm.dialog.turn`, asserts daemon stays up + emits at least one
`wm.brain.response` event without `announce_required`.

Each file follows wm-audio's `wake_bus_smoke.rs` shape verbatim:
module-level doc-comment, `tmp_path` helper (or import a shared one
if the repo already has it), oneshot ready+shutdown, in-process
`run_daemon`, scripted input, event collection with deadline.

**C. No changes to library code, binaries, or `Cargo.toml`** beyond a
`[dev-dependencies] agorabus = { path = "../agorabus" }` line if the
repo doesn't already have it (all four already do per `Cargo.toml`
inspection). No version bumps.

**D. No /build skill changes.** /build's verified-completed check #5
already accepts named cargo tests as AC pairing surface. The new
`bus_smoke.rs` tests are not paired to any specific AC; they're a
**regression gate** for the class of bug that
`agorabus-announce-fix` patches. /build can treat them as bonus
coverage.

**E. Ordering with `agorabus-announce-fix`:** that PRD must ship
first (the daemons currently fail bus-smoke because of the bug);
this PRD's test files then lock in the fix.

---

## 3. Why a bus-smoke convention beats other interventions

**Why not type-state-encode the bug into `agorabus::Client`?**
Possible but a wider change to agorabus — would need `ClientPending`
→ `Client` transitions, breaks the existing API, requires more
research. Honest path: log a Fleet 2 bullet for that and ship the
test convention now.

**Why not just add the test files to the announce-fix PRD?** The
announce-fix PRD is a one-line-per-repo patch; conflating it with a
test-convention rollout muddies both. Two PRDs, two responsibilities:
fix the live bug, then prevent the next one. Both can ship in the
same /build day.

**Why a convention doc rather than a shared `wintermute-bus-test`
crate?** Premature abstraction with only 4 consumers and one shared
pattern. Convention + copy-paste skeleton is the right level today.
If/when 8+ repos consume bus_smoke and the skeleton drifts, factor
into a crate then (Fleet 2 bullet: `wintermute-bus-test`).

---

## 4. Acceptance criteria

1. `~/wintermute/autobuilder/notes/conventions/bus-smoke.md` exists
   and documents the convention (file name `bus_smoke.rs`, in-process
   `agorabus::run_daemon` spawn, announce-before-subscribe ordering,
   no env-witness gating, no `#[ignore]`, ≥2 s liveness + at-least-one
   publish assertions). Cites
   `wintermute-audio/tests/wake_bus_smoke.rs` as canonical prior art
   and includes a 30-line skeleton.
2. `~/wintermute/wintermute-tts/tests/bus_smoke.rs` exists, builds
   green, and `cargo test --release --test bus_smoke` passes locally
   (assumes `PRD-wintermute-fleet-agorabus-announce-fix.md` has
   shipped first; if not, the test fails loudly with
   `announce_required` in the anyhow chain — which is the intended
   behavior pre-fix).
3. `~/wintermute/wintermute-stt/tests/bus_smoke.rs` exists, builds
   green, `cargo test --release --test bus_smoke` passes (same
   shipping-order caveat).
4. `~/wintermute/wintermute-dialog/tests/bus_smoke.rs` exists, builds
   green, `cargo test --release --test bus_smoke` passes (same).
5. `~/wintermute/wintermute-brain/tests/bus_smoke.rs` exists, builds
   green, `cargo test --release --test bus_smoke` passes (same).
6. Each new test file follows wm-audio's `wake_bus_smoke.rs` shape:
   module doc-comment naming what's exercised, oneshot ready+shutdown
   for the in-process daemon, deadline-bounded event collection, no
   leaked tasks (verified via Drop guard pattern from
   `agorabus/tests/common/mod.rs`'s `DaemonHandle`).
7. Each new test file's body includes at least one explicit `client
   .announce(...)` call **before** any `client.subscribe(...)` or
   `client.publish(...)` — present as positive evidence in the file
   that the test author understood the ordering. (Anti-cargo-cult
   gate: a test that connects without announcing reproduces the bug,
   not catches it.)
8. No version bump in any of the four target repos; no `Cargo.toml`
   edits beyond `[dev-dependencies] agorabus = { path = "../agorabus" }`
   if missing (none are; verified 2026-05-27T22:58Z).
9. `agorabus` repo is **untouched** by this PRD. The convention uses
   only its existing public API (`run_daemon`, `DaemonConfig`,
   `Client::connect`, `Client::announce`, `Client::subscribe`,
   `Client::next_event`).
10. Convention doc explicitly carries a "**Fleet 2 hook**" note:
    every new `wm-*` daemon PRD (browser, desktop, screen-narrate,
    mail, calendar, music) must include a `tests/bus_smoke.rs`
    before it can archive. /dream's drafting checklist references
    this in future Fleet 2 PRDs.

---

## 5. Out of scope (Fleet 2 candidates)

- **Type-state encoding** of announce-before-subscribe in
  `agorabus::Client` (typestate transition). Wider API change;
  needs design.
- **Shared `wintermute-bus-test` crate** factoring the
  `tmp_path` + `DaemonHandle` + oneshot pattern. Premature with
  4 consumers.
- **CI matrix** running bus_smoke against the agorabus-from-main
  vs agorabus-from-this-repo's-Cargo.lock combinations. Useful
  when agorabus starts releasing on its own cadence; today it's
  path-deps everywhere.
- **`tests/heartbeat_smoke.rs` convention.** agorabus enforces a
  heartbeat timeout (`DaemonConfig::heartbeat_timeout`); long-lived
  daemons should be smoke-tested for heartbeat compliance too.
  Separable from announce-ordering; defer.
- **Drift gate that surfaces `Client::connect` call sites missing
  a follow-up `announce`.** `skill-doctor`-style static check
  (drift vision Fleet 2 candidate). Possible after drift Fleet 1
  ships.

---

## 6. Effort

Single tick. 4 test files × ~80-120 LOC each, copy-paste from
wm-audio with payload-specific tweaks. Convention doc ~80 LOC.
Total: ~500-600 LOC new across 5 files. No library edits, no
version bumps, no skill changes.

Build target `mixed` (1 doc + 4 rust-extend touches across repos
sharing the autobuilder workspace).
