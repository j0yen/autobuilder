# PRD: agorabus-doctor-selfstale — the bus answers "am I current?"

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/agorabus
Vision: visions/vigil.md

## TL;DR

`binstale` detects daemon staleness from the *outside* (scanning
`/proc`). The agorabus bus daemon is the most-restaged process on this
laptop and the recurring star of the staleness journal, yet it has no way
to report its own currency. This PRD adds `agorabus doctor`: a subcommand
that asks the *running* bus whether its executing binary still matches
the installed binary on disk, using the same `(deleted)`/inode/provfs
signals applied to the daemon's own pid. The bus introspects itself.

## Why this exists

- agorabus is the canonical staleness case: run-18 journal flags pid
  2138939 running a `(deleted)` binary; Phase 1 of this vision confirmed
  it live (`/proc/2138939/exe → …/agorabus (deleted)`).
- The agorabus CLI today (`src/main.rs` `enum Command`) is purely
  client/daemon: `Daemon`, `Announce`, `Peers`, `Publish`, `Subscribe`,
  `Heartbeat`, `Claim`, `Intent`. There is **no** health/doctor surface
  — confirmed by reading the enum during Phase 1. A `doctor` subcommand
  is a natural, low-risk addition that makes the bus self-describing
  about its own freshness.
- Self-introspection is strictly cheaper than an external scan for the
  common "is the bus stale?" question and removes the dependency on
  `binstale` being installed for that one (most important) daemon. It
  complements, not duplicates, binstale: binstale scans the *fleet*;
  `agorabus doctor` answers for *itself*.

## What this builds

Extends `~/wintermute/agorabus/` (rust-extend; preserves all existing
behavior, adds one subcommand):

- New `Command::Doctor` variant in `src/main.rs` and a `src/doctor.rs`
  module. `agorabus doctor`:
  1. Finds the **running daemon** pid. Preferred: query the bus's own
     announce/peer record for the daemon's pid (the daemon announces
     itself), or read a pidfile if the daemon writes one; fallback:
     resolve via `/proc` scan for the `agorabus daemon` cmdline. (The bus
     is single-instance per socket.)
  2. Resolves `/proc/<daemon-pid>/exe`, detects the ` (deleted)` suffix,
     compares the running inode against the installed-path inode, and
     reads the installed binary's provfs `user.prov.ts`.
  3. Prints a verdict: `current` | `stale: deleted-exe` |
     `stale: inode-drift` | `stale: prov-newer`, with the evidence.
  4. `--format json|text` (default text). Exit code 0 = current, 1 =
     stale, 2 = could not determine (no running daemon / unreadable
     `/proc`).
- Fail-open consistency with the rest of the CLI: when no daemon is
  running, `doctor` exits 2 with a clear message (not a panic), matching
  the existing "fail-open with no daemon" convention documented in
  `src/main.rs`.
- Reuses the project's existing deps; adds `xattr` only if not already
  present. No new async surface — `doctor` is a short-lived client like
  `peers`.
- CHANGELOG.md entry + version bump per the repo's existing convention
  (it tracks v0.2.0/v0.3.0/v0.4.0 sections).

## Coordination note

The live bus daemon (pid 2138939) is currently stale and its rollout was
**deliberately escalated, not auto-restarted** (run-18). Building +
installing this PRD will itself reinstall the agorabus binary; the
build/publish flow must **not** kill the running daemon as a side effect.
Restarting the live bus is `rollout`'s job under a chosen window, or the
operator's — see gossip and visions/vigil.md open questions. This PRD
adds a *read-only introspection subcommand*; it does not restart anything.

## Acceptance criteria

1. `agorabus doctor` is a new subcommand; `agorabus --help` lists it and
   `agorabus doctor --help` documents `--format` and the exit-code
   contract.
2. With a running bus daemon whose binary is unchanged since launch,
   `agorabus doctor` prints `current` and exits 0.
3. With a running bus daemon whose `/proc/<pid>/exe` ends in ` (deleted)`
   (binary reinstalled underneath it — the live pid-2138939 condition),
   `agorabus doctor` prints `stale: deleted-exe` and exits 1.
   (Today-testable against the live stale daemon if it is still running
   at build time; otherwise reproduce with a fixture.)
4. With no daemon running, `agorabus doctor` exits 2 with a clear
   message and does not panic (fail-open consistent with other
   subcommands).
5. `--format json` emits `{daemon_pid, exe_path, exe_inode,
   ondisk_inode, prov_ts, verdict}` as valid JSON; `--format text` (the
   default) is human-readable.
6. The daemon-pid discovery works without requiring `binstale` to be
   installed (agorabus doctor is self-contained).
7. All existing agorabus subcommands and tests are unchanged and still
   pass; clippy clean; the new `Doctor` variant does not alter the
   behavior of `Daemon`/`Peers`/`Publish`/`Subscribe`/`Heartbeat`/
   `Claim`/`Intent`.
8. CHANGELOG.md gains an entry for the new subcommand and Cargo.toml
   version is bumped per the repo's convention.
