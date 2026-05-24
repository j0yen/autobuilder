# PRD: AgentNS — a Linux Namespace for Agent Sessions (codename: *agentns*)

**Author:** Claude (Opus 4.7), for me
**Status:** Draft v0.1 — vendor-fork PRD, not upstream
**Date:** 2026-05-22
**Forks:** `kernel/nsproxy.c`, `include/linux/sched.h`, `fs/proc/base.c`, plus a new `kernel/agent_namespaces.c`
**Realism:** ~0% upstream-mergeable. Wintermute-kernel-only.

---

## TL;DR

Today's `ctrace` (eBPF + sudo) reconstructs "which syscalls belong to which Claude session" by following the PID tree from the root claude binary. It works, but the abstraction is wrong: the kernel does not know what an agent session is, so every consumer (ctrace, fsstory, episode, transcript) re-implements the same parent-chain walk. Linux already has seven namespaces (mount, pid, net, uts, ipc, user, cgroup) plus time; this PRD adds an eighth — `CLONE_NEWAGENT`. Every process inside an agent namespace carries an opaque 128-bit `agent_session_id` and a per-NS counter set (syscalls, bytes_written, openat_count, connect_count). Children inherit the id automatically; the id is exposed at `/proc/$PID/agent_session`. The "leaked tracer outlives its session" bug from `/self-review` becomes structurally impossible: the namespace dies with the session, and so does every process inside it. Vendor-patch on the wintermute kernel; document the diff so future kernel bumps can rebase.

---

## 1. Why this exists

Concrete kernel-shaped observations from today's work:

1. **ctrace follows PID trees in userspace.** That's tail-wagging-the-dog: the kernel already knows which task spawned which, but ctrace has to re-derive "is this PID a descendant of my root_pid" on every event. Move the labeling into the kernel and ctrace becomes trivial.
2. **The "leaked tracer" failure mode.** `/self-review` Phase A has explicit logic to detect and reap a ctrace that outlived its Claude session by >24h. That bug exists because the tracer is associated with a session *by convention*, not by kernel-enforced lifetime. With AgentNS, the session is a kernel object; reaping is automatic.
3. **Multi-agent concurrency is illegible to the OS.** I can spawn sub-agents via the Agent tool. If two of my sub-agents and the user's own commands all touch the same file in the same second, `lsof`/`ps`/`strace` give me no way to say "these three writes came from these three agent sessions." Per-NS labeling fixes that.
4. **Per-session resource accounting is missing.** `getrusage` is per-process or per-task; I can't say "Claude session XYZ used 12 GB of RAM and made 4283 syscalls today." Cgroups can do this, but cgroups are heavy and the user has to set them up — they're a manual contract. AgentNS is automatic.
5. **The "is this process me?" question has no answer.** When `fsstory` wants to attribute a write to a Claude session, it does a userspace lookup: "is this PID in the descendant set of the running claude binary?" A field on the task_struct would replace all that.

---

## 2. Who this is for

Me, and every tool that today reinvents PID-tree-walking. The user benefits because reliability improves — the leaked-tracer bug class disappears.

---

## 3. What I'd use it for (concretely)

| Today's pain                                            | With AgentNS                                                        |
| ------------------------------------------------------- | ------------------------------------------------------------------- |
| ctrace follows the PID tree to label events             | Every event in `perf` already carries the agent_session_id          |
| fsstory does multi-source joins to attribute writes     | `xattr user.agent_session` is stamped at write-time (via a one-line LSM hook on top of the NS) |
| /self-review reaps stale tracers                        | NS destruction handles it; no reaper needed                         |
| getrusage on the claude PID misses sub-agents           | `/proc/$PID/ns/agent/counters` gives session-wide counters          |
| Two parallel Claude sessions write to the same file     | Each write carries its own session_id; no ambiguity                 |
| Phase 0 of /self-review wants "what did the last session of *this skill* do?" | Sessions can carry an `intent_tag` (e.g. `self-review`); query the kernel for "last 5 NS-destruction events with intent_tag=self-review" |

---

## 4. Functional requirements

### 4.1 New namespace type

In `include/uapi/linux/sched.h`:

```c
#define CLONE_NEWAGENT  0x40000000  /* TBD; pick an unused bit */
```

Equivalent `unshare(CLONE_NEWAGENT)` and `clone3({.flags = CLONE_NEWAGENT})` semantics. The `nsproxy` struct gains an `agent_ns` pointer. Default root NS for every existing task is a global `init_agent_ns` with `agent_session_id = 0` (meaning "not in an agent session").

### 4.2 Per-task state

In `task_struct`:

```c
struct agent_namespace *agent_ns;       /* never NULL after fork; inherits parent */
u128 agent_session_id;                  /* duplicated for fast-path access */
const char *intent_tag;                 /* optional, short, set via prctl */
```

### 4.3 prctl interface

```c
prctl(PR_SET_AGENT_INTENT_TAG, "self-review");
prctl(PR_GET_AGENT_SESSION_ID, &id_out);
prctl(PR_SET_AGENT_BUDGET_LIMITS, &limits);
```

The session id is read-only after NS creation. The intent tag is writable but rate-limited (1 set/sec) to discourage live re-labeling games.

### 4.4 Per-NS counters

`kernel/agent_namespaces.c` keeps per-NS atomic counters for:

```
total_syscalls
openat_count           (sum of openat/open/openat2)
write_bytes            (sum of write/pwrite/writev byte counts)
connect_count          (TCP+UDP)
unlink_count
fork_count             (clones with non-zero result)
elapsed_ns             (NS wall-time since creation)
```

Exposed at `/proc/$PID/ns/agent/counters` as JSON. Increment is done from inside the syscall paths via a per-NS hook (cheap; uses per-cpu atomics).

### 4.5 Lifecycle

- NS created on `unshare(CLONE_NEWAGENT)` or `clone3(CLONE_NEWAGENT)`. Caller gets a fresh `agent_session_id` (kernel-issued, monotonic + random salt).
- NS exists as long as any task references it.
- Last-task-exit fires a `RING_BUFFER_AGENT_NS_DESTROY` event with the final counters. eBPF programs can subscribe to this for end-of-session telemetry without a userspace reaper.
- A boot-time tunable `agent_ns_max_lifetime` (default: 86400s = 24h) reaps any NS older than this regardless of task presence. Belt and suspenders against the leaked-tracer class.

### 4.6 /proc surface

```
/proc/$PID/ns/agent              symlink to "agent:[42]"
/proc/$PID/agent_session         128-bit hex id
/proc/$PID/agent_intent          intent_tag string
/proc/$PID/agent_counters        JSON: per-NS counter snapshot
```

`ls -la /proc/$PID/ns/agent` shows the same inode for two processes in the same NS — the standard namespace UX.

### 4.7 eBPF integration

Two new tracepoints:

```
agent_ns:agent_session_start    fields: session_id, parent_session_id, intent_tag
agent_ns:agent_session_end      fields: session_id, counters, elapsed_ns
```

`ctrace` is rewritten to subscribe to these instead of polling the PID tree. The `--root_pid` arg goes away.

### 4.8 LSM hook (optional but obvious)

A wintermute-local LSM module `agent_lsm.c` hooks `inode_setattr`, `file_open(O_CREAT)`, and `socket_connect`. On call, it reads the current task's `agent_session_id` and either:
- Stamps an xattr on the inode (`user.agent_session=<hex>`) — see [PRD-provenance-fs.md](PRD-provenance-fs.md).
- Records the event to a per-NS ring buffer.

The LSM is independent of AgentNS — could ship without it — but pairs naturally.

---

## 5. Architecture

```
kernel/
├── nsproxy.c            (modified — adds agent_ns to nsproxy)
├── fork.c               (modified — copies/creates agent_ns on clone)
├── agent_namespaces.c   (NEW — NS create/destroy, counters, prctl handlers)
├── pid_namespace.c      (modified — clarify interaction with PID NS)
include/linux/
├── sched.h              (modified — task_struct gets agent_ns/agent_session_id)
├── agent_namespaces.h   (NEW)
fs/proc/
├── base.c               (modified — adds /proc/$PID/agent_*)
security/
├── agent_lsm/           (NEW — optional LSM module)
```

Estimated diff size: ~800 LoC across kernel, ~200 for the LSM, ~300 for the eBPF tracepoint plumbing. Build-tested against the Arch `linux` package on this laptop.

---

## 6. Non-goals

1. **Upstreaming.** Not in scope. The patch is for wintermute's kernel; tracking upstream releases is a maintenance cost the user accepts.
2. **Mandatory enforcement.** If a tool doesn't unshare(CLONE_NEWAGENT), nothing breaks — it just stays in the global init agent NS with session_id=0. AgentNS is an opt-in label, not a security boundary.
3. **Replacing cgroups.** Cgroups are for resource control; AgentNS is for identity and observability. They coexist.
4. **Cross-host federation.** Single laptop.
5. **Process-level granularity.** AgentNS is *session*-level. A session spans many processes; that's the point.

---

## 7. Phasing

| Phase | Scope                                                                                |
| ----- | ------------------------------------------------------------------------------------ |
| 0     | task_struct + nsproxy + fork.c diff + /proc/$PID/agent_session. Minimum viable label.|
| 1     | Counters + prctl interface + ring-buffer destroy event.                              |
| 2     | eBPF tracepoints; ctrace v2 subscribes to them and drops its PID-tree walker.        |
| 3     | LSM stamping xattrs (folds into provenance-fs PRD).                                  |
| 4     | Budget-limit prctl enforcement (kill or signal the NS root on overage).              |

---

## 8. Risks

- **Kernel-rebase cost.** Every Arch `linux` update reapplies. *Mitigation:* the diff is small and stable; CI builds the patched kernel and the user opts in.
- **Memory ordering subtleties in the per-NS counters.** Per-cpu atomics with periodic aggregation should be enough for the scale we care about.
- **Interaction with user namespaces.** A `userns`-rootless process should still be able to `unshare(CLONE_NEWAGENT)`. v0.1 disallows this conservatively; v0.2 figures it out.
- **Tooling that doesn't expect a new NS.** Most `ps`/`top`/`lsof` don't enumerate namespaces explicitly; they should be fine. Specialized tools (`nsenter`, `lsns`) need the new NS added to their tables.

---

## 9. Open questions

1. Is "agent" the right name? Alternatives: `task`, `intent`, `context`. "Agent" reads natural but is overloaded.
2. Should there be a hierarchy of agent NSes (sub-agents nested inside parent sessions), or are they flat? *Probably hierarchical*, matching how the Agent tool spawns sub-agents today.
3. Should the session_id be globally unique forever or wrap? 128 bits + UUID-style means functionally never-wrap.
4. Should sysctls allow disabling AgentNS entirely at runtime, for users who don't want the overhead? Yes — `sysctl kernel.agent_ns.enabled=0` makes the entire subsystem a no-op.
5. The `intent_tag` is a free-form string. Should it be from an enum (skill-name|tool-name|agent-name)? Free-form is more honest but harder to query.
