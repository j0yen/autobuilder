# PRD: provfs-comm-richer — enrich the fallback xattr when `agent_session` is zero

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-27
**Vision:** [visions/onramp.md](visions/onramp.md)
**Pairs with:** [PRD-provfs-deferred-stamp.md](PRD-provfs-deferred-stamp.md) (shared hook-time capture buffer)
build_auto: false
build_target: kernel-extend
build_into: /home/jsy/wintermute/provfs/lsm

---

## TL;DR

`provfs`'s `file_release` hook stamps `user.prov.session` with the
agentns 128-bit session id when one is present, otherwise falls back
to `comm:<comm>:pid:<pid>:uid:<uid>` using the writer's last `task->comm`.
The fallback is structurally wrong: `task->comm` at hook time is the
process *closing* the fd, which is often a transient utility like
`awk`, `sed`, or `install`, not the originating tool that intended to
write the file.

Observed live xattrs 2026-05-27 on files written by /build:

| File                            | Stamped session                       | Actual writer       |
|---------------------------------|---------------------------------------|---------------------|
| `~/wintermute/recall/Cargo.toml` | `comm:awk:pid:76630:uid:1000`         | autobuilder pipeline |
| `~/.local/bin/recall`           | `comm:install:pid:95273:uid:1000`     | extend-handler install |

Both stamps are technically true but uselessly granular — they name
the innermost child of the pipeline rather than the meaningful actor.
A `provq` user asking "who wrote Cargo.toml" wants to hear "/build" or
"autobuilder", not "awk".

This PRD enriches the fallback path: when `agent_session` is zero, the
xattr value composes parent comm chain (up to 3 levels up via
`task->real_parent` walk), readable env vars (`$CLAUDE_TOOL`,
`$AGORABUS_SID`), and writer cwd. Total xattr value capped at 256
bytes; truncation preserves the *outermost* actor.

The agentns-id-present path is unchanged. Consumers of
`user.prov.session` keep working — the value format gains structure
but stays a string.

---

## 1. Why this exists

### 1.1 The empirical fallback is wrong

The xattrs in §TL;DR are not synthetic; they're what `getfattr` returns
right now on real files from this morning's /build runs. A user asking
"who wrote this Cargo.toml?" gets `comm:awk:…`. The actual answer is
"the autobuilder running inside the /build skill of the Claude session
that started at 09:02Z." None of that information survives.

### 1.2 The fallback remains load-bearing even after agentns wrapping

PRD-claude-agentns-wrap closes the Claude-session case. But the
fallback still fires for:

- **System daemons** — systemd, journald, udev, cron — not in any
  agent namespace.
- **Hooks invoked by the host shell** — e.g. zsh's own preexec/precmd.
- **Processes that escape the namespace** — `nsenter`, `setns`,
  setuid-root binaries that re-enter init userns.
- **Pre-wrap-install transitional state** — every file written *before*
  PRD-claude-agentns-wrap ships keeps a stale comm-fallback xattr
  forever (provfs doesn't restamp).

So enrichment matters; it's not a "throwaway path."

### 1.3 The hook-time capture buffer is shared with deferred-stamp

PRD-provfs-deferred-stamp's §1.3 motivates a hook-time capture buffer
that carries enough state to do the actual stamp from a workqueue
worker, including the writer's env. This PRD's enrichment naturally
lives in the same buffer:

```c
struct provfs_stamp_work {
  char path[PATH_MAX];
  char session_str[64];     // existing: 32-hex agentns id or empty
  char comm_chain[128];     // NEW: "tool>parent>gparent" max 3 levels
  char env_signal[64];      // NEW: CLAUDE_TOOL=… or AGORABUS_SID=…
  char cwd[PATH_MAX];       // NEW: writer's cwd at close time
  u64 ts_ns;
  // …
};
```

If deferred-stamp ships first, this PRD is additive: same struct,
more fields, richer formatting. If they ship in either order they
compose cleanly.

### 1.4 Concrete user need today

`/self-review` and the `provfs_attribution` playbook (when it exists)
need to answer: "for files written in the last hour, group by writer
session." Comm-as-writer crushes everything into a long tail of
intermediates. A correct attribution lets the playbook see "12,842
writes from /home/jsy/wintermute were from the autobuilder" instead of
"4,200 awk writes + 3,100 sed writes + 2,000 install writes + ..."
that the user has to manually re-aggregate.

---

## 2. What this builds

### 2.1 Hook-time capture (changes scoped to `file_release` hook)

When the agentns session id is zero, capture (best-effort, all may
fail silently):

- **Comm chain** — walk `current->real_parent` up to 3 levels;
  format `comm0>comm1>comm2`. Stop at level 1 if the parent is
  `init`/`systemd`/`kthreadd`.
- **Env signal** — `access_remote_vm(current->mm, …)` to read the
  process env. Search for `CLAUDE_TOOL=`, `AGORABUS_SID=`, `CLAUDE_SESSION_ID=`.
  First match wins; truncate value to 48 bytes. Skipped if `current->mm`
  is gone (which, per deferred-stamp PRD §1.3, is exactly when this
  needs to be deferred — the capture buffer is the right place).
- **CWD** — `get_fs_pwd(current->fs)` + `d_absolute_path` (same
  function the deferred-stamp PRD adopts).

All three are advisory; missing fields are simply omitted from the
xattr value.

### 2.2 Xattr value format

Today (agentns absent):
```
user.prov.session=comm:awk:pid:76630:uid:1000
```

Proposed (agentns absent):
```
user.prov.session=comm-chain:bash>autobuilder.sh>awk;env:CLAUDE_TOOL=/build;cwd:/home/jsy/wintermute/recall;pid:76630;uid:1000
```

Fields are `key:value` pairs separated by `;`. Order is fixed
(`comm-chain` first, then `env`, then `cwd`, then `pid`, `uid`). Any
field may be absent. `pid` and `uid` are always present (cheap, never
fail). Total value capped at 256 bytes; truncation drops fields from
the right (preserves outermost-actor signal at the front).

The agentns-present format is unchanged:
```
user.prov.session=<32-hex>
```

### 2.3 What this does NOT change

- Does not change `user.prov.ts`. Timestamp xattr stays as-is.
- Does not change the agentns-id path. Only the fallback.
- Does not add new xattr keys. Same `user.prov.session` key, richer
  value.
- Does not enforce env-var reading (the kernel module never failed
  loudly when `access_remote_vm` fails today and won't here either).
- Does not retrofit existing files. Existing stale xattrs from
  pre-deploy are not rewritten.

### 2.4 Consumer-side adapter (`provq` parser update)

Lightweight follow-up bundled into this PRD: when `provq` (continuity
Fleet 1) parses `user.prov.session`, it should switch on prefix:
`comm-chain:` → rich form, parse fields; bare 32-hex → agentns form;
old `comm:` (legacy stale) → legacy form. Backwards-compatible. The
parser change ships in the `provq` repo, not this kernel patch, but
is acceptance-tested here (a tiny test that round-trips a
known-shape xattr).

---

## 3. Acceptance criteria

1. **Comm chain captured on fallback path.** With agentns id zero,
   write a file via `bash -c 'echo x | awk "{print}" > /tmp/provtest'`,
   read the xattr, see `comm-chain:bash>awk` in the value (no `sh`
   shim or `>` redirect noise; just the actual chain).
2. **Env var captured when available.** Set `CLAUDE_TOOL=/build` in
   the writer's env; observed xattr contains `env:CLAUDE_TOOL=/build`.
3. **CWD captured.** From cwd `/home/jsy/wintermute/recall`, write a
   file; xattr contains `cwd:/home/jsy/wintermute/recall`.
4. **Field-absent graceful degradation.** A write from a process with
   no `CLAUDE_TOOL`/`AGORABUS_SID` env (e.g. a stock daemon) produces
   a xattr without the `env:` field, still with `comm-chain:`, `cwd:`,
   `pid:`, `uid:`.
5. **256-byte cap honored.** A pathological writer with a very long
   `cwd` produces a xattr ≤256 bytes; truncation drops from the
   right; `comm-chain:` is always preserved when present.
6. **agentns-present path unchanged.** With agentns id non-zero,
   xattr value is the bare 32-hex string, same as today's behavior.
   No regression for the wrapped-session case.
7. **No hot-path regression.** `perf stat -e cycles -e instructions`
   on a `cargo build` of a small Rust crate shows ≤5% overhead vs
   pre-PRD provfs (a small cost is acceptable; the comm-walk +
   env-read add work). Verify via deferred-stamp's workqueue
   amortization if deferred-stamp ships first.
8. **No new oopses under stress.** Run a sustained `make -j$(nproc)`
   on a medium kernel build for 5 minutes; `dmesg | grep -i provfs`
   shows no NULL derefs, no use-after-free, no warnings.
9. **`provq` round-trip test.** A `provq /tmp/provtest` invocation
   prints the parsed enriched form (e.g. `comm-chain=bash>awk
   cwd=/home/jsy/… …`). Old-format xattrs from pre-deploy files
   still parse (legacy `comm:` branch).
10. **AC1–9 verified live by jsy** on the rebuilt + rebooted
    `linux-wintermute` kernel. Mechanical-only checks (AC1–6 against
    a synthetic workload) are not sufficient; AC7/8 require real
    workloads, and the kernel patch can't ship without them.

---

## 4. Out of scope

- Rewriting existing stale xattrs from before this PRD. Files keep
  their old `comm:` form; future writes get the new form.
- Adding more env vars to the search list. `CLAUDE_TOOL`,
  `AGORABUS_SID`, `CLAUDE_SESSION_ID` are the three with downstream
  consumers today. Extensions can come in a future PRD.
- Changing the xattr *key* from `user.prov.session`. Format-only.
- A user-space rewrite tool (`provfs-restamp`) that walks the fs and
  refreshes xattrs from external evidence (e.g. ctrace summaries).
  Future PRD if needed.
- Replacing `provfs` with eBPF. Out of scope of this vision; the LSM
  is the substrate.

---

## 5. Bootstrap notes

- Kernel patch lives in `~/wintermute/provfs/lsm/`; same repo as
  `provfs.c` today.
- Use the inline-edits pattern (`apply-agentns.py` style) for any
  PKGBUILD-side change needed to pick up the new code — anchored
  idempotent insertions, no raw `.patch` files.
- Build target `kernel-extend` means the resulting commits land in
  `~/wintermute/provfs/lsm/` and `~/wintermute/wintermute-kernel/pkg/`
  (PKGBUILD references new source).
- This PRD pkgrel-bumps the kernel `6 → 7` (composes with
  PRD-kernel-pkg-postinstall's 5 → 6); they should ship in either
  order, both are pkgrel-only.
- Live AC verification requires reboot into the rebuilt kernel.
  Implementer should coordinate the reboot via /self-review or
  user-prompt.
- The `provq` parser change (§2.4) lives in
  `~/wintermute/provq/` (or wherever PRD-provq.md lands the repo);
  cross-repo PR worth coordinating.

[continuity]: visions/continuity.md
