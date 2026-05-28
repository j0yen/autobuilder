# PRD: agentns-claude — wrap Claude sessions in an agent namespace

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-25
**Vision:** [visions/continuity.md](visions/continuity.md)
build_auto: false
build_target: rust-cli
deferred_acs: [5, 6, 7, 8]
**Boot-gated:** live ACs gate on booting into `linux-wintermute`. Mock
interface (`AGENTNS_SESSION_ID_OVERRIDE` env or `/tmp/agentns-mock`
file) lets the binary iterate pre-boot.

---

## TL;DR

The wintermute kernel exposes a 128-bit `session_id` per agent
namespace via `/proc/$PID/agent_session`, but the kernel only assigns
one when a process actually calls `unshare(CLONE_NEWAGENT)`. Today
nothing wraps `claude` (Claude Code) in that unshare, so every
Claude session reads "not in agent ns" and downstream consumers have
no stable identity to attribute work to. `agentns-claude` is a thin
launcher: `agentns-claude --intent /build -- claude` does the
unshare, sets a meaningful `intent_tag`, sets `prctl` budget limits
if configured, and execs Claude Code. From the inside of the
session, every `/proc/$PID/agent_session` read returns the same
value — including grandchildren.

This is the foundation PRD for the `continuity` vision. PRDs #4
(`recall-session-stamp`) and #5 (`session-postmortem`) depend on a
session having an id from birth; #2 (`provq`) and #3
(`memlog-witness`) read whatever id the kernel reports and benefit
when it's stable.

---

## 1. Why this exists

Three load-bearing observations:

1. **The kernel surface is there but unused.** From `CLAUDE_SELF.md`
   changelog 2026-05-24: "agentns Phase 3+4 (LSM stamping + budget
   enforcement) baked into linux-wintermute." From the dream skill's
   Phase 1.5 brief: `/proc/$PID/agent_session` is stable but only
   populated when a process is in the namespace. No userspace tool
   creates the namespace at session start today.

2. **Every introspection tool re-derives session identity.** `mirror`,
   `episodic-observer`, `session-index`, `ctrace`, the self-review
   skill — each parses session-JSONL filenames or walks the PID tree
   to attribute work to a session. From 2026-05-24 run-1 journal:
   "Missing summary for claude-20260523T020109.ndjson (root_pid 60874
   dead, SessionEnd hook never fired)." A kernel-stamped id would
   have survived the death.

3. **Budget enforcement needs a namespace.** The kernel's
   `PR_SET_AGENT_BUDGET_LIMITS` (per CLAUDE_SELF Phase 1.5 brief)
   sends SIGTERM/SIGKILL on overage. The runaway-session scenarios in
   the self-review journal (PID 886 at RSS 515MB / 9.2GB io_write
   across 16h) are exactly the shape this protects against, but the
   prctl only applies inside an agent namespace. No launcher → no
   protection.

---

## 2. What this builds

### 2.1 Binary: `agentns-claude`

A Rust CLI that:

```
agentns-claude --intent <tag> [--budget <spec>] [--no-unshare] -- <cmd> [args...]
```

- `--intent <tag>` — required. Free-form string written to
  `prctl(PR_SET_AGENT_INTENT_TAG)`. Conventions: `/build`,
  `/dream`, `/self-review`, `interactive`, `headless`,
  `headless:<service-name>`.
- `--budget <spec>` — optional. Comma-separated key=value pairs:
  `wall=3600s,syscalls=1e7,write_bytes=10G,fork=1000`. Maps to
  `PR_SET_AGENT_BUDGET_LIMITS`. SIGTERM on soft limit, SIGKILL on
  hard. Conservative defaults documented but no built-in defaults
  applied (explicit > implicit).
- `--no-unshare` — for non-wintermute kernels. Skips the unshare,
  emits a warning, synthesizes a session_id from
  `(uid, boot_time_ns, monotonic_now_ns)` so downstream tools still
  get something stable per-session.
- `-- <cmd>` — the wrapped command and its argv. Typically `claude`,
  but the launcher is generic — `agentns-claude --intent /build --
  bash` works.

### 2.2 Wrap, don't replace

`agentns-claude` does the minimum and execs. It is not a supervisor;
it is not a session manager; it is not a hook host. It exists so
that anything spawned underneath inherits the namespace and reads a
stable id. `pevent`, the self-review service unit, and any
interactive shell alias for `claude` opt in by prepending
`agentns-claude --intent <tag> --`.

### 2.3 Hook integration (downstream, not in this PRD)

Once `agentns-claude` ships, a separate small change to the
SessionStart hook writes the session_id into
`~/.claude/agentns-session-id` for hook consumers that can't read
`/proc/self/agent_session` themselves. That change is a one-line
hook patch and belongs in the wintermute dotfiles, not in this PRD's
scope.

### 2.4 Mock mode for pre-boot iteration

When `$AGENTNS_SESSION_ID_OVERRIDE` is set in the environment,
`agentns-claude` skips the unshare and uses that value as the
session_id. When `/tmp/agentns-mock` exists, it reads the
session_id from that file. Both paths emit `[agentns-claude] MOCK
MODE: session_id=…` to stderr so it's never silently mistaken for
a real namespace. Mock mode unblocks PRD #4 and #5 development
pre-boot.

### 2.5 Output and exit code

By default, `agentns-claude` is silent on success — it execs and
inherits the wrapped command's exit code. With `--verbose`, it logs
the session_id, intent_tag, and budget settings to stderr before
exec.

---

## 3. Non-goals (v0.1)

- A session-manager UI. (`session-postmortem` is PRD #5.)
- Custom intent-tag schemas. The string is free-form; conventions
  are documented but not enforced.
- Auto-detection of slash commands to set intent. The caller passes
  `--intent`; the SessionStart hook (downstream) reads the session
  prompt and sets intent_tag at hook-time.
- Live network-namespace, cgroup, or seccomp shaping. Out of scope
  for this PRD — wintermute kernel does the inheritance work.

---

## 4. Acceptance criteria

Numbered, testable. Items marked **[boot]** require booting into
`linux-wintermute` and pass against the live kernel; others pass
against mock mode.

1. **AC1 — Builds and installs.** `cargo build --release` produces
   `target/release/agentns-claude`; `cargo install --path .` puts
   it in `~/.cargo/bin/`. Binary `--version` prints the crate
   version.
2. **AC2 — `--help` is honest.** Lists all flags above; documents
   mock mode; documents `--no-unshare` fallback for stock kernels.
3. **AC3 — Mock mode.** With `AGENTNS_SESSION_ID_OVERRIDE=deadbeef-…`
   set, `agentns-claude --intent test -- printenv AGENTNS_SESSION_ID`
   prints `deadbeef-…` (the launcher exports the override into the
   child env). With `/tmp/agentns-mock` containing a different id,
   the file wins over the env if both are set (file is more
   deliberate than ambient env).
4. **AC4 — Exec semantics.** `agentns-claude --intent test --
   echo hi` prints `hi` and exits 0. `agentns-claude --intent test
   -- false` exits 1. `agentns-claude --intent test -- nonexistent`
   exits non-zero with a clear error.
5. **AC5 [boot] — unshare succeeds.** Under `linux-wintermute`,
   `agentns-claude --intent test -- cat /proc/self/agent_session`
   prints a 128-bit hex id, and a second invocation prints a
   *different* id (each session gets a fresh one).
6. **AC6 [boot] — inheritance.** `agentns-claude --intent test --
   bash -c 'cat /proc/self/agent_session; sh -c "cat /proc/self/agent_session"'`
   prints the same id twice.
7. **AC7 [boot] — intent_tag.** `agentns-claude --intent /build --
   cat /proc/self/agent_intent_tag` (or whatever the kernel exposes
   per Phase 1.5 brief) prints `/build`.
8. **AC8 [boot] — budget enforcement.** `agentns-claude --intent test
   --budget wall=2s -- sleep 60` is terminated by the kernel at
   ~2s (SIGTERM or SIGKILL per spec); exit code reflects signal.
9. **AC9 — `--no-unshare` fallback.** On a stock kernel without
   `CLONE_NEWAGENT`, `--no-unshare` exits 0 and emits a stderr
   warning. Without `--no-unshare`, exits non-zero with a clear
   "kernel does not support agent namespaces" message.
10. **AC10 — README + CHANGELOG.** Repo `README.md` documents
    install, usage, and the mock-mode contract. `CHANGELOG.md`
    section for v0.1.0.

---

## 5. Shape

```
~/wintermute/agentns-claude/        new repo, j0yen/agentns-claude
├── Cargo.toml
├── README.md
├── CHANGELOG.md
├── src/
│   ├── main.rs        argv parsing, dispatch
│   ├── unshare.rs     CLONE_NEWAGENT syscall + prctl
│   ├── mock.rs        mock-mode shim
│   └── budget.rs      --budget spec parser → prctl args
└── tests/
    ├── mock_mode.rs   exec + env propagation
    └── nonkernel.rs   --no-unshare warn path
```

Dependencies: `clap` (argv), `nix` (libc bindings for unshare /
prctl), maybe `libc` direct for the agentns-specific prctls if `nix`
hasn't added them yet (it almost certainly hasn't). No async, no
tokio, no network — this is a sub-millisecond launcher.

---

## 6. Open questions

- Should the launcher set a non-zero `intent_tag` even without
  `--intent` (default to `argv[0]` or `"unknown"`)? Currently
  required; making it optional saves caller verbosity but allows
  silent dropping of attribution. Leaning required.
- `--budget` syntax: comma vs. space vs. JSON? Comma is shortest;
  JSON is composable in scripts. Leaning comma with a `--budget-file
  <path>` escape for the JSON case.

---

## 7. Provenance

- Kernel surfaces per `CLAUDE_SELF.md` Phase 1.5 of `/dream` skill
  (PR_SET_AGENT_INTENT_TAG, PR_SET_AGENT_BUDGET_LIMITS,
  `/proc/$PID/agent_session`).
- Pain point from self-review journal 2026-05-24 run-1:
  "Missing summary for claude-20260523T020109.ndjson (dead root_pid,
  SessionEnd hook never fired — reboot killed it mid-session)."
- Foundation for `continuity` vision (visions/continuity.md);
  unblocks PRDs #4 and #5 in that fleet.
