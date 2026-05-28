# Vision: continuity — kernel signal becomes agent self-awareness

**Authored by:** /dream (Claude Opus 4.7), with jsy
**Created:** 2026-05-25
**Updated:** 2026-05-28 (kernel boot validated; Fleet 1.5 lives in `visions/onramp.md`)
**Status:** active
**Fleet 1 drafted:** 5 PRDs (kernel→userspace bridge for session continuity)
**Fleet 1.5:** see `visions/onramp.md` — 4 PRDs for kernel-tier production-readiness (post-install + Claude launch wrap + richer provfs fallback + deferred xattr stamp)
**Fleet 2:** captured as bullets; future `/dream extend continuity`
**Kernel boot:** VALIDATED 2026-05-28. `uname -r=7.0.10-arch1-5-wintermute`; `/dev/memlog` live; `/proc/self/agent_session` reads 32 zeros (Claude not yet wrapped — `onramp` PRD #2 is the load-bearing unblock).

---

## TL;DR

The kernel tier shipped on 2026-05-24 — `memlog`, the `provfs` LSM, and
agent namespaces — adds three primitives that survive process death and
carry stable session identity. Nothing in userspace consumes them yet.
This vision is the bridge: wrap Claude sessions in an agent namespace so
every session has a 128-bit id from birth, stand up a CLI to query
`user.prov.session` xattrs, persist pre-compaction memlog snapshots per
session, stamp `recall` memories with the same session id, and join all
four signals into a one-command postmortem. The point is not the
plumbing. The point is that next-Claude can read what last-Claude was
doing, with primary-source kernel evidence, instead of inferring it from
JSONL leftovers.

## End-state

When Fleet 1 ships:

1. **Every Claude session starts in its own agent namespace** with a
   stable 128-bit `session_id` and a meaningful `intent_tag`
   (slash-command name, headless-service identifier, or interactive
   default). `/proc/$PID/agent_session` reads the same value from any
   process the session spawns, including grandchildren of grandchildren.
2. **Any file under the home tree can be traced to the session that
   wrote it** in one command: `provq /path` → `session=<id> ts=…
   tool=<name>`. Recursive sweep: `provscan ~/wintermute --since 1h`.
3. **Pre-compaction context snapshots are persisted per session** to
   `~/.claude/memlog/<session-id>/snap-NNNN.json` by a userspace
   `memlog-witness` daemon. Snapshots outlive the Claude process; next
   session can read them.
4. **`recall` memories carry the writing session's id** (xattr-stamped
   by the provfs LSM, also embedded in record front-matter for
   non-provfs filesystems). `recall query --session <id>` returns the
   memories that session wrote.
5. **`session-postmortem <id>`** joins memlog snapshots, provfs file
   writes, recall memory writes, and the existing ctrace summary into a
   single markdown brief. Operates on a session that died at compaction,
   was OOM-killed, hit a budget kill, or simply ended.

When Fleet 2 ships (hooks into existing introspection tooling):

6. **`mirror` weekly grading** picks up agent_counters as quantitative
   ground truth (syscalls/openat/write_bytes/connect/unlink/fork) rather
   than re-deriving from JSONL parsing.
7. **`episodic-observer`** consumes memlog snapshots as another episode
   source — the try/fail/retry pattern visible at the compaction
   boundary is exactly what an observer wants.
8. **`letters-we-never-sent` / `confidant`** seed letters from a
   session's last memlog snapshot — past-Claude's literal last thoughts
   feed next-Claude's intake.

## Why now

- The kernel tier is built and queued for boot validation
  (`~/wintermute/wintermute-kernel/pkg/`, `linux-wintermute`). Userspace
  consumers can scaffold and unit-test against mock interfaces today; AC
  validation under the new kernel comes after boot.
- The existing introspection ecosystem (`mirror`, `episodic-observer`,
  `letters-we-never-sent`, `claude-self`, `self-portrait`,
  `conversations-zine`, `confidant`, `tide-chart`, `session-index`) is
  rich, but every member of it reconstructs session identity from
  session-JSONL filenames or PID-tree heuristics. A primary-source id at
  the kernel surface lets all of them stop guessing.
- The pain is observed. Today's 2026-05-24 self-review run-1 journal:
  "Missing summary for claude-20260523T020109.ndjson (root_pid 60874
  dead, SessionEnd hook never fired — reboot killed it mid-session)."
  A memlog snapshot would have preserved that session's intent;
  per-session file persistence would have made recovery deterministic.

## Architecture

```
┌────────────────────────────────────────────────────────────┐
│  POSTMORTEM    session-postmortem <id>                     │
│                joins all four signals                      │
├────────────────────────────────────────────────────────────┤
│  PERSISTENCE   memlog-witness daemon (per-session files)   │
│                recall-session-stamp (memory frontmatter)   │
├────────────────────────────────────────────────────────────┤
│  QUERY         provq / provscan (file → session)           │
│                recall query --session <id>                 │
├────────────────────────────────────────────────────────────┤
│  IDENTITY      agentns-claude (launcher)                   │
│                kernel: CLOAGENT_NEW + 128-bit session_id   │
├────────────────────────────────────────────────────────────┤
│  KERNEL        memlog · provfs LSM · agent namespaces      │
│                (linux-wintermute, awaiting boot validation)│
└────────────────────────────────────────────────────────────┘
```

## Fleet 1 — Foundation (drafted 2026-05-25)

All five PRDs carry `build_auto: false` (the /dream default — user opts
in per PRD).

| # | PRD | Target | Output | Notes |
|---|---|---|---|---|
| 1 | `PRD-agentns-claude.md` | rust-cli | `agentns-claude` | wrap Claude Code launch in unshare(CLONE_NEWAGENT); foundation for the rest |
| 2 | `PRD-provq.md` | rust-cli | `provq`, `provscan` | xattr-based file→session attribution; works against both the provfs FUSE overlay and the in-kernel LSM |
| 3 | `PRD-memlog-witness.md` | rust-extend | (extends `~/wintermute/memlog/`) | userspace daemon: subscribe to `/dev/memlog`, persist per-session snapshots |
| 4 | `PRD-recall-session-stamp.md` | rust-extend | (extends `~/wintermute/recall/`) | stamp memory writes with agentns session_id; `recall query --session` |
| 5 | `PRD-session-postmortem.md` | rust-cli | `session-postmortem` | join memlog + provfs + recall + ctrace per session |

**Sequencing:**
- #1 (`agentns-claude`) is the entry gate. Without a stable session_id
  at session start, #4 and #5 degrade to PID-tree fallbacks.
- #2 and #3 can develop in parallel — both are kernel-surface readers
  with no cross-dep. #3 has a real dep on the kernel boot (it reads
  `/dev/memlog`) but the daemon scaffolding can land before boot.
- #4 depends on #1 (needs a session_id to stamp).
- #5 depends on all four (joins their signals). Land last.

**Boot gating:**
- #1, #2, #4 can iterate against a mock kernel interface
  (`AGENTNS_SESSION_ID_OVERRIDE` env var or `/tmp/fake-agent-session`
  file) so the work isn't blocked on the reboot. Live ACs gate on boot.
- #3 cannot meaningfully test without `/dev/memlog`. Scaffolding +
  parser tests can land pre-boot; integration ACs gate on boot.
- #5 can run against pre-boot mocks of all four signals.

**Cross-PRD coordination:**
- #4 (`recall-session-stamp`) interacts with `recall-daemon` (in
  flight). The stamp should land in a version that doesn't collide with
  recall-daemon's v0.5.x or recall-outcome-feedback's v0.5.1–v0.5.3
  rebased range — target `v0.6.0` to be safe and explicit.
- #2 (`provq`) lives in its own new repo, not inside
  `~/wintermute/provfs/` — the provfs repo is the LSM/FUSE
  implementation; provq is a downstream consumer. Keep the layering
  clean.
- #5 (`session-postmortem`) is a new repo, not a skill — too much logic
  for the skill format, and it should be invokable from a skill (a thin
  `/postmortem <id>` skill that calls `session-postmortem` is a Fleet 2
  bullet).

## Reusable foundation already on this laptop

- **Kernel tier (`~/wintermute/{memlog,provfs,agentns}/`,
  `wintermute-kernel/pkg/`)** — primary-source signal. Builds clean
  against linux 7.0.10. Boot validation pending.
- **`recall` v0.4.1** — agentic memory; `recall-daemon` in flight will
  give sub-10ms retrieval. Session-stamp is a small extension to its
  index module.
- **`ctrace`** — eBPF session tracer at `~/.local/bin/ctrace`.
  `session-postmortem` shells out to `ctrace summary <session>` rather
  than re-implementing.
- **Existing introspection repos (Fleet 2 consumers)** —
  `letters-we-never-sent`, `episodic-observer`, `mirror`, `claude-self`,
  `self-portrait`, `conversations-zine`, `confidant`, `tide-chart`,
  `session-index`. None modified by Fleet 1; Fleet 2 wires the kernel
  signal into them.
- **`pevent`** — supervised background processes. `memlog-witness`
  registers as a pevent job so the daemon is restarted on death.

## Fleet 2 — Hook into introspection (future `/dream extend continuity`)

Draft after Fleet 1 ships ≥3 of 5 components. Bullets only here:

- **`mirror-kernel`** — mirror's weekly grader picks up
  `/proc/$PID/agent_counters` as quantitative truth.
- **`episode-from-memlog`** — episodic-observer consumes memlog
  snapshots; the try/fail/retry pattern at compaction boundary is
  exactly what it wants.
- **`letter-from-snapshot`** — letters-we-never-sent (or confidant)
  seeds a letter draft from the user's last pre-compaction memlog
  snapshot. Past-Claude's literal last thoughts.
- **`/postmortem` skill** — thin slash command that calls
  `session-postmortem`. Phase 0 of an introspection-skill layer.
- **`agentns-budget-policy`** — wire `PR_SET_AGENT_BUDGET_LIMITS` to
  the slash command — `/build` gets a budget shape; `/dream` gets
  another; `/self-review` is bounded. SIGKILL on runaway sessions.

## Open questions

- **Backwards-compat for non-agentns kernels.** If the user is running
  the stock Arch kernel (or rolls back), `agentns-claude` falls through
  to "no session id" mode. Should that be a warning or hard fail?
  Leaning warning + a synthesized session_id from
  `(uid, boot_time, monotonic_now)` so downstream tools still get
  *something*.
- **Should `provq` and `provscan` be one binary?** They share most code.
  Splitting matches `getfattr` / `find -newer` convention; merging cuts
  one binary. Leaning split, but provq can multiplex via `-r` flag —
  /build can decide.
- **Where does `session-postmortem` write its output?** Stdout by
  default; option `--out ~/brain/postmortems/<id>.md` for journaling.
  Should it ALSO append to today's `~/brain/journal/YYYY-MM-DD.md`?
  Leaning no — journal is curated; postmortem is mechanical.
- **Naming.** The vision is `continuity`; the launcher is
  `agentns-claude`. Consider renaming the launcher to `aclaude` or
  `wclaude` for ergonomics — but agentns-claude is honest about what it
  does. Leaning keep.

## Provenance

- **Seeded by:** `/dream` invocation 2026-05-25 (no explicit topic).
  Listening surfaced (a) the kernel tier shipping yesterday with no
  userspace consumer arc planned, (b) the rich existing introspection
  ecosystem that re-derives session identity from JSONLs, (c)
  CLAUDE_SELF.md's aspiration to "honor continuity — past-Claude's
  lessons should reach future-Claude."
- **Research:** `~/wintermute/{memlog,provfs,agentns,wintermute-kernel}/`
  READMEs + Cargo manifests; existing introspection repo READMEs;
  recall v0.4.1 manifest + in-flight PRDs (daemon, observer-correlation,
  outcome-feedback) for version-collision avoidance; gossip + dream
  manifest for what's queued; 2026-05-24 journal entries (3 self-review
  runs) for evidence of session-attribution pain.
- **Not yet validated:** the kernel tier is built but not booted. All
  acceptance criteria that read live kernel surfaces (`/dev/memlog`,
  `/proc/$PID/agent_session`, provfs xattrs) gate on boot. PRDs flag
  this explicitly.
- **User decisions pending:** opt-in per PRD (build_auto:false on all
  five); rename/merge/split decisions in Open Questions above.
