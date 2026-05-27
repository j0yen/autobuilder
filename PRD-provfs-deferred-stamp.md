# PRD: provfs — defer xattr stamping off the `file_release` hot path

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-26
**Pairs with:** [PRDs-archive/PRD-provenance-fs.md](PRDs-archive/PRD-provenance-fs.md) (the v0.1 LSM that this hardens)
build_target: kernel-extend
build_into: ~/wintermute/provfs/lsm
**Boot-gated:** AC1–AC3 compile-time; AC4–AC8 require booting the rebuilt `linux-wintermute` kernel with `lsm=…,provfs` on the cmdline.

---

## TL;DR

`provfs_stamp` currently runs synchronously inside the `file_release`
LSM hook. That hook fires from `fput`, including from `exit_files()`
during task teardown. v0.1 plus the 2026-05-26 emergency fix
(`d_path → d_absolute_path`, NULL guards) keeps the hook from
NULL-derefing, but it leaves three things wrong with the design:

1. `kmalloc(PATH_MAX, GFP_KERNEL)` + path walk + two `__vfs_setxattr_noperm`
   calls happen on every writable close. That's hot-path work in a hook
   that should be a few instructions.
2. The hook still runs under task teardown — any future helper that
   touches `current->mm`, `current->fs`, or `current->files` re-introduces
   the class of bug we just fixed. The defensive guards in v0.1.1 are
   load-bearing; they shouldn't have to be.
3. Phase 1 of the original PRD (read `$CLAUDE_TOOL` from `current->mm`
   via `access_remote_vm()`) is *un-shippable* from the current hook
   context — `mm` may already be gone. Deferring stamping makes Phase 1
   trivially safe.

This PRD moves the actual xattr write to a workqueue. The hook captures
the minimum state needed (resolved path string, session string, ts) and
enqueues a `provfs_stamp_work`. A bounded worker drains the queue and
writes the xattrs. On overflow we drop and bump a counter — provenance
is best-effort.

---

## 1. Why this exists

### 1.1 The Oops we just patched is a symptom, not a root cause

Boot -1 (2026-05-26 10:17 PDT) oopsed in `d_path+0xa2 → provfs_stamp+0x129`
inside `glxtest` (PID 814) during firefox startup. The fix shipped
the same day (`d_path → d_absolute_path`) prevents that specific NULL
deref. But the hook still runs in process-teardown context. Any future
patch that reaches further into `task_struct` will re-discover the
same fragility. The structural answer is "don't do real work in the
hook."

### 1.2 Latency on every writable close

`fput` is on the close path for every writable file in the system —
`Cargo.toml` saves, browser cache writes, journald rotations, every
single `cargo build` artifact, every git pack. Each one pays
`kmalloc(PATH_MAX) + d_absolute_path + 2 × __vfs_setxattr_noperm`.
The xattr writes alone are journaled filesystem operations. For an
LSM that's meant to be observability, not enforcement, the cost
should be amortized off the syscall return path.

### 1.3 Phase 1 ergonomics need a deferred context

Reading `$CLAUDE_TOOL` from the writer's environment requires
`access_remote_vm(current->mm, …)`. In `file_release` from
`exit_files()`, `mm` has already been torn down by `exit_mm()`. The
deferred worker, running on a kthread with no relationship to the
original task, will instead need to capture the env at hook time as
part of the work payload. That's tractable; the current shape isn't.

---

## 2. Who this is for

Me, indirectly. Every consumer of `user.prov.*` xattrs (`provq`,
`fsstory`, `/self-review` Phase A) keeps working unchanged — same
keys, same values, same on-disk shape. The win is structural: the LSM
stops crashing the kernel and stops adding measurable latency to
unrelated workloads.

---

## 3. What this builds

### 3.1 New file: `provfs_work.c` (sibling of `provfs_lsm.c`)

```c
/* Bounded queue of pending stamps.
 *
 * Each entry holds the fully-resolved path string (already filtered
 * against the skip list), the rendered session value, the timestamp,
 * and pinned references to the dentry + mnt_idmap so the target
 * inode survives until the worker runs.
 */
struct provfs_stamp_work {
    struct work_struct       work;
    struct dentry           *dentry;   /* dget() in hook, dput() in worker */
    struct mnt_idmap        *idmap;
    char                     session[PROV_IDENT_MAX];
    char                     ts[PROV_TS_MAX];
};

static struct workqueue_struct *provfs_wq;
static atomic_t provfs_queue_depth;
static atomic64_t provfs_queue_dropped;
static int provfs_queue_max = 1024;   /* sysctl-tunable */

void provfs_enqueue_stamp(struct dentry *dentry, struct mnt_idmap *idmap,
                          const char *session, const char *ts);
```

### 3.2 Modified file: `provfs_lsm.c`

`provfs_stamp` is split:

- **Hook-side** (still called from `file_release`): does the cheap
  work — dentry/inode/path-skip guards, path resolution via
  `d_absolute_path`, session rendering via `provfs_build_session`, ts
  rendering. If the file passes the skip filter, calls
  `provfs_enqueue_stamp(…)` and returns.
- **Worker-side** (`provfs_stamp_worker`): runs from `provfs_wq`,
  performs the two `__vfs_setxattr_noperm` calls, `dput()`s the
  dentry, frees the work struct.

`provfs_enqueue_stamp`:

1. If `atomic_read(&provfs_queue_depth) >= provfs_queue_max`,
   `atomic64_inc(&provfs_queue_dropped)` and return.
2. `kzalloc(GFP_ATOMIC)` the work struct (so we don't sleep in the
   hook). On alloc failure, increment dropped counter and return.
3. `dget(dentry)`, copy session+ts, set idmap.
4. `INIT_WORK(&w->work, provfs_stamp_worker)`.
5. `queue_work(provfs_wq, &w->work)`. `atomic_inc(&provfs_queue_depth)`.

`provfs_stamp_worker`:

1. `__vfs_setxattr_noperm(idmap, dentry, PROV_SESSION_KEY, session, …)`
2. `__vfs_setxattr_noperm(idmap, dentry, PROV_TS_KEY, ts, …)`
3. `dput(dentry)`. `atomic_dec(&provfs_queue_depth)`. `kfree(w)`.

### 3.3 sysctl surface (new)

Under `/proc/sys/kernel/provfs/`:

| key              | type | default | meaning |
|------------------|------|---------|---------|
| `queue_max`      | int  | 1024    | max in-flight stamp work items |
| `queue_depth`    | int  | (ro)    | current depth (snapshot) |
| `queue_dropped`  | u64  | (ro)    | cumulative drops since boot |

Implemented via `register_sysctl("kernel/provfs", provfs_sysctl_table)`.
Mind the post-6.11 sentinel-free convention — *no* trailing `{}`
(this is the exact bug that broke memlog; cf. 2026-05-26 fix).

### 3.4 Module init/exit changes

`provfs_init`:
- After `security_add_hooks`, allocate the workqueue:
  `provfs_wq = alloc_workqueue("provfs_stamp", WQ_UNBOUND | WQ_MEM_RECLAIM, 0);`
- Register the sysctl table.

`provfs_exit` (or equivalent unload path; LSMs don't unload, but for
symmetry and future-module-conversion):
- `destroy_workqueue(provfs_wq)` (drains pending work synchronously).
- Unregister sysctl.

### 3.5 What's intentionally NOT in scope

- **Per-CPU queues.** A single unbound wq is fine for v0.2. Move to
  per-CPU only if `queue_dropped` is non-zero in normal use.
- **Coalescing rapid re-writes of the same file.** Could be a
  follow-up PRD; v0.2 stamps every closed-after-write file.
- **Reading `$CLAUDE_TOOL` from `current->mm`.** That's Phase 1 of
  the original PRD; this PRD only *enables* it by ensuring the worker
  doesn't depend on the original task. Phase 1 is its own PRD.

---

## 4. Acceptance criteria

| # | gate | how to verify |
|---|------|---------------|
| AC1 | compile-clean | `cd ~/wintermute/wintermute-kernel/pkg && makepkg -e --skippgpcheck --noconfirm` exits 0 |
| AC2 | no sparse/checkpatch regressions in `provfs_work.c` and the modified `provfs_lsm.c` | `make C=1 security/provfs/` produces no new warnings |
| AC3 | sysctl table valid | `dmesg` after boot shows no `sysctl table check failed` for `kernel/provfs/*` |
| AC4 | LSM still stamps | `touch /tmp/probe.txt; … wait …; getfattr -d /tmp/probe.txt` (use a non-skipped path like `~/test/probe.txt`) shows `user.prov.session` and `user.prov.ts` set |
| AC5 | hook latency dropped | `bpftrace -e 'kprobe:provfs_file_release { @ = hist(nsecs); }'` median ≤ 1µs over 10k writable closes (vs. >10µs in v0.1) |
| AC6 | teardown safety | run `tests/stamp_teardown.sh` (forks 1000 children that each write+close+exit immediately); after run, no `BUG:` or `Oops:` in `dmesg`; counters consistent |
| AC7 | overflow drop | `tests/stamp_flood.sh` (10k concurrent writes), `cat /proc/sys/kernel/provfs/queue_dropped` shows non-zero, no oops, no permanent leak (queue_depth returns to 0 within 5s of flood end) |
| AC8 | sysctl tune | `echo 64 | sudo tee /proc/sys/kernel/provfs/queue_max`; rerun AC7; drops increase relative to default |

AC1–AC3 are compile/boot-time; AC4–AC8 require a clean boot under
`linux-wintermute` with `lsm=…,provfs`. The 2026-05-26 boot regressions
(memlog sysctl failure, provfs `d_path` Oops) MUST be cleared first —
this PRD assumes both fixes are landed.

---

## 5. Files touched

```
~/wintermute/provfs/lsm/
  provfs_lsm.c        (split provfs_stamp into hook + enqueue)
  provfs_work.c       (new — workqueue + worker + enqueue)
  Makefile            (add provfs_work.o)
  tests/
    stamp_teardown.sh (new — AC6)
    stamp_flood.sh    (new — AC7/AC8)

~/wintermute/wintermute-kernel/pkg/
  PKGBUILD            (bump pkgrel)
  apply-agentns.py    (no change — provfs is dropped in via _apply_provfs_lsm
                       which copies whole files; the new provfs_work.c will
                       automatically be picked up if added to the install list)

(also touched, one line each in PKGBUILD's _apply_provfs_lsm:)
  install -Dm644 "$_PROVFS_LSM/provfs_work.c" security/provfs/provfs_work.c
```

---

## 6. Risk + rollback

**Risk**: workqueue allocation in hook (`GFP_ATOMIC`) under memory
pressure may fail, dropping stamps. Mitigation: dropped-counter is
visible; if it ever moves on a healthy machine, that's a signal to
tune `queue_max` up or move to per-CPU.

**Risk**: `dget()` in hook + `dput()` in worker could pin a soon-to-be-
unlinked dentry for the work duration. That's fine for our use case
(xattr write to an unlinked inode is a no-op that returns -ENOENT,
which we silently ignore via `(void)__vfs_setxattr_noperm`).

**Rollback**: revert to v0.1.1 (the d_absolute_path hardening). No
on-disk format change, no userland API change, no consumer impact.
The xattrs already written remain valid.

---

## 7. Open questions

1. **Should the worker batch?** Two xattr writes per file is two
   journal commits. A coalescing path that aggregates by inode within
   a short window could materially reduce write amplification. Defer
   to v0.3 unless AC5 measurements suggest the worker itself is the
   bottleneck.

2. **`WQ_MEM_RECLAIM` flag correct?** This lets the wq make progress
   during memory pressure (good for an observability tool that
   shouldn't itself deadlock on OOM). The provenance work doesn't
   directly free memory, so `WQ_MEM_RECLAIM` is mildly off-label. Pick
   it anyway — the alternative is the wq stalling exactly when we'd
   most want to know "who wrote this."

3. **Phase 2 history ring** (the original PRD's deferred work) becomes
   easier under this design — `provfs_stamp_worker` is the natural
   place to also append to an inode-scoped history. Out of scope here,
   noted as a future PRD anchor.
