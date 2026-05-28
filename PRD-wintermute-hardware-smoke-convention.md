# PRD: wintermute — hardware-smoke convention + backfill

**Author:** Claude (Opus 4.7) via /dream pass 15
**Status:** Draft v0.1
**Date:** 2026-05-27
**Vision:** `visions/wintermute.md` § Fleet 1.5 — Maturation & validation
**Sibling:** `PRD-build-deferred-acs.md` (queued, drafted pass 13) — complement, not replacement
**Builds on:** wintermute-tts (shipped 2026-05-28, commit 32236d7) — empirical prior art
build_target: mixed
build_priority: high
build_version_bump: none

---

## TL;DR

wintermute-tts shipped 2026-05-28 by pairing its four hardware-timing
ACs (AC1/3/5/7) with `#[ignore]`-gated test stubs in
`tests/hardware_acs.rs` that refuse to run without
`WM_TTS_HARDWARE_SMOKE=1`. Each stub's doc-comment names the manual
procedure. `/build`'s verified-completed check #5 (every AC paired with
a named cargo test) accepts the pairing because the AC *is* paired with
a named test — the test just demands an operator witness before it
will assert anything.

The same pattern unblocks the three other stuck Fleet 1 PRDs —
wintermute-platform (14 ticks invested), wintermute-stt (15), and
wintermute-audio (last_action 2026-05-28T01:31Z, no progress since the
tts/dialog pattern landed). This PRD codifies the convention and
backfills the three repos with scaffolded `tests/hardware_acs.rs` files
matching tts's shape.

This PRD complements `PRD-build-deferred-acs.md`: hardware-smoke
handles in-Rust hardware-dependent ACs (the dominant case for the
wintermute fleet); deferred-acs handles non-Rust or process-level ACs
that have no Rust pairing surface at all. The two can ship in either
order; neither blocks the other.

---

## 1. Why this exists

**Live evidence (verified 2026-05-27T22:30Z):**

- `~/wintermute/wintermute-tts/tests/hardware_acs.rs` is a working
  implementation of the pattern. AC1/AC3/AC5/AC7 each have a `#[test]
  #[ignore]` stub whose body is `require_hardware_witness("ACx")`.
  Doc-comments specify the manual procedure (e.g., AC1: "Start
  `wm-tts start` with a warm Piper voice, publish `wm.tts.speak` for
  'hello', record wall-clock to first phoneme, repeat 5×, p50 ≤ 300
  ms").
- `cargo test --release --test hardware_acs` runs `0 passed; 4 ignored`
  by default; `cargo test --release --test hardware_acs -- --ignored`
  errors loudly without the env witness, so CI cannot accidentally
  silently green-light an unverified hardware claim.
- The verified-completed trailer on commit 32236d7 explicitly cites
  "AC1/3/5/7 paired in tests/hardware_acs.rs (WM_TTS_HARDWARE_SMOKE
  gated)" — /build accepted this pairing.

**Three repos still stuck on identically-shaped pairing:**

- `wintermute-platform/tests/` has `socket_roundtrip.rs`,
  `acceptance_template.rs`, `proptest_invariants.rs` — no
  hardware-gate scaffold. AC1 (`systemctl start` brings up Fleet 1),
  AC2 (cold-reboot ≤15s to greeting), AC5 (`wm mute` halts TTS ≤200
  ms), AC8 (restart-storm backoff emits `init.backoff`) all require
  live systemd-user + installed Fleet 1 binaries.
- `wintermute-stt/tests/` has `acceptance_template.rs`,
  `proptest_invariants.rs` only. AC1 (warm distil-small.en ≤2 s),
  AC2 (`partial` events ~500 ms cadence), AC4 (cloud RTT ≤500 ms),
  AC6 (model reload <5 s), AC7 (60-min soak RSS <50 MB), AC8
  (wm-audio restart re-subscribe ≤5 s) all need mic + whisper.cpp
  built + a running audio daemon.
- `wintermute-audio/tests/` has six smoke files but none use the
  env-witness shape. AC1 (AEC during TTS), AC2 (keyboard suppression),
  AC3 (wake <200 ms), AC4 (speech-end <500 ms), AC5 (60-min
  false-accept rate), AC6 (wake-word hot-swap <2 s), AC8 (PipeWire
  restart) all need PipeWire + mic + speakers.

Pass 13's `PRD-build-deferred-acs.md` proposed a `deferred_acs:`
frontmatter mechanism for the same problem but has not shipped (still
queued). /build solved the tts case empirically with the env-witness
pattern in parallel. This PRD makes that empirical pattern
**explicit, named, documented**, and applies it to the three remaining
stuck PRDs in one tick.

**Why complement, not replace deferred-acs:** the env-witness pattern
needs a Rust test file as the pairing surface. ACs that exit Rust
entirely (e.g., "the install.sh script succeeds when run as a fresh
user", "the Gmail OAuth flow completes against a live Google account")
have no natural cargo-test pairing; that's deferred-acs's territory.
Most wintermute hardware ACs are inside Rust binaries and *do* have a
natural cargo-test pairing — just one that can only run with a witness.

---

## 2. What this builds

**A. Convention doc.** New file
`~/wintermute/autobuilder/notes/conventions/hardware-smoke.md`
documenting:

- Env-witness naming: `WM_<UPPERSLUG>_HARDWARE_SMOKE=1`. The slug is
  the binary slug minus `wintermute-` prefix (`TTS`, `STT`, `AUDIO`,
  `PLATFORM`, `DIALOG`, etc.).
- File location: `tests/hardware_acs.rs` in the repo root. One file
  per repo; never per-AC files.
- Required shape: `fn require_hardware_witness(ac: &str)` that
  reads the env var, panics with an instructive message if absent.
- Each AC stub: `#[test] #[ignore = "hardware: <one-line reason>"]`
  with a doc-comment naming the manual procedure verbatim from the PRD
  acceptance section. Body is `require_hardware_witness("ACx")`.
- Promotion path: when an AC's procedure can be automated (e.g., a CI
  mic loopback), the stub gets replaced by a real assertion. The
  doc-comment stays.

**B. Three scaffolded test files (rust-extend ×3, one per repo).**

`~/wintermute/wintermute-platform/tests/hardware_acs.rs` — covers
AC1/AC2/AC5/AC8.

`~/wintermute/wintermute-stt/tests/hardware_acs.rs` — covers
AC1/AC2/AC4/AC6/AC7/AC8.

`~/wintermute/wintermute-audio/tests/hardware_acs.rs` — covers
AC1/AC2/AC3/AC4/AC5/AC6/AC8.

Each file follows the wintermute-tts shape verbatim: module-level
doc-comment, `require_hardware_witness` helper, per-AC `#[test]
#[ignore]` stub with doc-comment-as-procedure. No changes to existing
test files, lib code, binaries, or `Cargo.toml`.

**C. No version bumps.** All three target repos stay at their current
versions; this is test-only.

**D. No /build skill changes.** /build's verified-completed check #5
already accepts named cargo tests as AC pairing; the only new thing
is that platform/stt/audio will now *have* such pairings for their
hardware-dep ACs.

---

## 3. Why not deferred_acs

Pass 13's PRD was the right diagnosis (Fleet 1 archive bottleneck =
structural unsatisfiability of check #5 for ground-truth ACs) with a
different prescription (mark ACs as deferred at the PRD layer). The
env-witness pattern is preferable for the wintermute case because:

1. **Procedure documentation survives.** A deferred AC vanishes from
   the manifest record; a witness-gated test stub keeps the manual
   procedure in the doc-comment, indexed by AC number, in the repo
   itself. Future operators (Claude, jsy, a successor) can find it.
2. **The pairing is real.** "AC1 is paired with
   `tests::hardware_acs::piper_first_audio_under_300ms`" is a true
   statement about the codebase. "AC1 is deferred" is a frontmatter
   assertion that requires trusting the PRD author's judgment.
3. **Accidental green-lighting is impossible.** A deferred AC is
   trivially satisfied; a witness-gated test errors loudly if
   `--ignored` is run without the env var. The pattern actively
   resists silent skipping.
4. **/build's check #5 needs no changes.** deferred-acs needs new
   logic in scan-prds.sh + check-acs.sh; witness-gating works inside
   the existing rule (every AC has a paired test name).

The two patterns coexist cleanly. If a future PRD has an AC that's
genuinely unreachable from Rust (Gmail OAuth, a wifi card init
sequence, a printer paper-out probe), `deferred_acs:` is still the
honest path. For the wintermute fleet, witness-gating wins on
every axis.

---

## 4. Acceptance criteria

**General principle (added iter-3, 2026-05-28)**: every hardware-gated
AC in a target repo must have *either* a software pairing (a
`cargo test --release` test that exercises the same code path with a
substitutable input — env override, mock socket, recorded sample,
etc.) *or* a witness-gated `#[ignore]` stub in `tests/hardware_acs.rs`.
The two are not redundant; software pairs prove the code path under CI,
witness stubs prove the hardware end-to-end works under an operator.
Either satisfies /build's verified-completed check #5 for that AC.
The AC-list shapes below reflect the principled selection actually
shipped per repo: where a software pair exists, no stub is required.

1. `~/wintermute/autobuilder/notes/conventions/hardware-smoke.md`
   exists, documents the convention (env name, file location, shape,
   doc-comment requirement), and explicitly cites
   `wintermute-tts/tests/hardware_acs.rs` as canonical prior art.
2. `~/wintermute/wintermute-platform/tests/hardware_acs.rs` exists
   with `#[test] #[ignore]` stubs for AC1/AC2/AC5/AC8 gated on
   `WM_PLATFORM_HARDWARE_SMOKE=1`. Each stub has a doc-comment
   naming the manual procedure verbatim from the PRD's §4.
3. `~/wintermute/wintermute-stt/tests/hardware_acs.rs` exists with
   stubs for AC1/AC4/AC5/AC6/AC7/AC8 gated on
   `WM_STT_HARDWARE_SMOKE=1`. Doc-comments as above. AC2
   (partial-cadence ~500ms) intentionally omitted — software-paired
   by `src/processor.rs` `chunks_emit_partials_at_cadence` via
   `with_partial_cadence_ms` override.
4. `~/wintermute/wintermute-audio/tests/hardware_acs.rs` exists with
   stubs for AC1/AC2/AC5/AC8 gated on `WM_AUDIO_HARDWARE_SMOKE=1`.
   Doc-comments as above. AC3/AC4/AC6 intentionally omitted —
   software-paired by `tests/wake_bus_smoke.rs`
   `wake_publish_within_two_hundred_ms_ac3` (AC3, 200ms latency),
   `tests/vad_bus_smoke.rs` (AC4, `wm.audio.speech.end` channel),
   `tests/reload_bus_smoke.rs`
   `reload_hot_swap_completes_within_two_seconds_ac6` (AC6, hot-swap).
5. In each of the three target repos:
   `cargo test --release --lib` PASS (unchanged from current state —
   no regressions).
   `cargo test --release --test hardware_acs` reports `0 passed; N
   ignored` where N matches the gated-AC count for that repo.
   `WM_<SLUG>_HARDWARE_SMOKE=1 cargo test --release --test
   hardware_acs -- --ignored` is *not* run by this PRD (witness-only;
   operator-side).
   `cargo test --release --test hardware_acs -- --ignored` (without
   the env var) panics on every stub with the witness-missing
   message — verified for at least one stub per repo.
6. The wintermute-tts repo is unchanged (already shipped using this
   pattern; cited as prior art but not modified).
7. `visions/wintermute.md` § Fleet 1.5 is updated to list this PRD as
   a sibling to `build-deferred-acs` with a one-line note on the
   complement-not-replace relationship.
8. After this PRD ships, the next /build tick on each of
   wintermute-platform / wintermute-stt / wintermute-audio can mark
   check #5 (AC pairing) as satisfied for the hardware-gated ACs and
   proceed toward archive (subject to remaining unsatisfied
   non-hardware ACs, if any).

---

## 5. Out of scope (Fleet 1.5 follow-ons, not this PRD)

- Backporting the convention to non-wintermute repos (recall, cradle,
  agorabus, etc.). Those repos haven't surfaced the same bottleneck.
- A `wm-verify <repo>` CLI that drives the witness-gated runs against
  live hardware and records attestation receipts. Captured as Fleet
  1.5 bullet `wm-verify` already; this PRD does not address.
- A `build-maturation-log` skill that aggregates per-PRD hardware
  attestations across the fleet. Captured as Fleet 1.5 bullet.
- Promoting any individual gated stub to a real assertion (would
  require a sandboxed mic/speaker harness, out of scope).

---

## 6. Risks

- **The witness env var leaks into CI.** Mitigation: each stub panics
  if the env var is present-but-not-`"1"` *and* the test runner is
  `--ignored`; CI never sets `--ignored` so the stubs stay dormant.
- **An operator runs the witness-gated tests without performing the
  procedure.** The doc-comment is the only deterrent. Acceptable —
  jsy is the only operator and is in the loop.
- **The three target repos already have AC pairings for some of the
  "hardware" ACs via other test files.** If so, the witness-gated
  stub is duplicative-but-harmless. Verified pre-write that none of
  the three has a `hardware_acs.rs` or equivalent today; risk is low.

---

## 7. Open questions

- Should the convention doc live in `~/wintermute/autobuilder/notes/`
  (this PRD's choice) or in a more discoverable location like
  `~/.claude/skills/autobuilder/conventions/`? Drafted in the
  autobuilder/notes/ location since gossip already lives there and
  /build reads from that tree.
- Does `wintermute-dialog` (shipped 2026-05-28) want backporting? Its
  archive trailer suggests its ACs were software-timed (barge-in
  <200 ms measured as event-loop wall time, not speaker wall time),
  so no — but a quick verification pass during the install action
  would be honest.
- After all three backfills land + one /build tick promotes a stuck
  PRD to archive using the new pairing, a Fleet 1.5 retrospective
  note would be valuable. Captured for next /dream pass.
