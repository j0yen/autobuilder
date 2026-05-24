# PRD: Provenance Filesystem (codename: *provfs*)

**Author:** Claude (Opus 4.7), for me
**Status:** Draft v0.1 — LSM module + optional FUSE overlay
**Date:** 2026-05-22
**Forks:** Linux LSM framework + xattr conventions. No kernel core changes; ships as a loadable LSM.
**Pairs with:** [PRD-agent-namespace.md](PRD-agent-namespace.md) (uses AgentNS's session_id if available; falls back to PID/comm heuristics otherwise).

---

## TL;DR

`fsstory` (the attribution-timeline PRD) joins ctrace ndjson, session JSONLs, and pacman logs to answer "who wrote this file." That's a heroic reconstruction from external signal. The right answer is to stamp the provenance into the file's own metadata at write-time. Linux already has extended attributes (xattrs) on ext4/btrfs/xfs/zfs — every file can carry up to ~64KB of `key=value` blobs. `provfs` is a Linux Security Module that hooks `inode_setattr` and `file_open(O_CREAT|O_WRONLY|O_RDWR)`, reads the calling task's `agent_session_id` (and `intent_tag`, and a `$CLAUDE_TOOL` env hint), and writes them as `user.prov.session`, `user.prov.tool`, `user.prov.turn` xattrs on the resulting file. `getfattr -d <path>` then tells you, instantly, who wrote that file and through which tool — no joins, no PID-tree walks, no ctrace correlation. `fsstory who-wrote` becomes a single `getfattr` call.

---

## 1. Why this exists

Today, the answer to "who wrote `~/.claude/settings.json` last?" is a query that involves:

1. `stat` for mtime
2. ctrace ndjson grep for the matching ts/path/PID
3. PID→session mapping (which is itself a heuristic walk)
4. Session JSONL grep for the tool call at that turn

That's four data sources for what is fundamentally a single fact. Files should carry their own provenance. The kernel already supports this via xattrs; nothing has been built to populate them automatically.

The LSM angle is important: a pure-userspace tool like `fsstory` runs *after* the write, can miss events, and depends on ctrace having been running. A kernel-side LSM hook *cannot* miss writes — every write that succeeds goes through the LSM gate.

---

## 2. Who this is for

Me. Every tool that today reconstructs file attribution (`fsstory`, `/self-review` Phase A) becomes simpler. Forensic questions ("did pacman overwrite my edited config?") become one-shot.

---

## 3. What I'd use it for (concretely)

| Today                                                              | With provfs                                                        |
| ------------------------------------------------------------------ | ------------------------------------------------------------------ |
| "Who wrote this file?" → ctrace+jsonl join                          | `getfattr -d $file` → `user.prov.session=01KS...`, `user.prov.tool=Edit`, `user.prov.turn=42` |
| "Did the user touch this since I last did?"                         | Read `user.prov.session` and compare to my current session         |
| `/self-review` Phase A "files changed by actor"                     | `for f in $(wchg since ~/.claude | jq .files[].name); getfattr -n user.prov.session $f; done` |
| "Is this `cargo build`-generated, or did I edit it?"                | `user.prov.tool=cargo` (set by the agentns-wrapping `cargo` invocation) vs `user.prov.tool=Edit` (set by me directly) |
| Stale-config detection                                              | `user.prov.session` is from a session id older than 30d → known-stale |

---

## 4. Functional requirements

### 4.1 LSM hooks

`security/provfs/provfs_lsm.c`:

```c
static int provfs_inode_init_security(struct inode *inode, struct inode *dir,
                                      const struct qstr *qstr,
                                      const char **name, void **value, size_t *len);

static int provfs_file_open(struct file *file);

static int provfs_inode_setattr(struct user_namespace *mnt_userns,
                                struct dentry *dentry, struct iattr *attr);
```

On every hook fire, read the calling task's:

1. `agent_session_id` from `current->agent_ns` (if AgentNS module is loaded)
2. `intent_tag` from `current->intent_tag`
3. `$CLAUDE_TOOL` from `current->mm->env_start` (env-var read, lazy, fallback if no AgentNS)
4. `$CLAUDE_TURN` likewise
5. `comm` and `pid` as last-resort fallbacks

Write to xattrs:

```
user.prov.session  = "01KS..."           (128-bit hex from AgentNS, or "comm:claude:pid:1202" fallback)
user.prov.tool     = "Edit"               (from $CLAUDE_TOOL or comm)
user.prov.turn     = "42"                 (optional)
user.prov.ts       = "2026-05-22T17:46:31Z"
user.prov.intent   = "self-review"        (from intent_tag)
```

Xattrs are 5 short strings, well under the 64KB-per-file limit.

### 4.2 Append semantics, not replace

By default, writing a new value to `user.prov.session` *replaces* the prior. That loses history. v0.1 instead maintains a small ring of the last N=5 sessions that touched the file:

```
user.prov.history = "01KS9...,01KS8...,01KS7..."  (CSV, MRU first, capped at 5)
```

Bigger histories belong in `fsstory`/external; the xattr is the recent-tip.

### 4.3 Opt-out path

Some files should never carry provenance (`.git/objects/...`, lockfiles, `node_modules/...`). A boot-time tunable list of path-prefixes is skipped:

```
sysctl provfs.skip_prefixes = "/proc/,/sys/,/dev/,/run/,/.git/,/node_modules/"
```

Or a per-mount option `mount -o provfs=off /some/mount`.

### 4.4 Reader CLI sugar

A tiny `~/.local/bin/prov` wrapper:

```
prov show <path>         # pretty-prints xattrs
prov who <path>          # one-line: "session 01KS... via Edit at 17:46 (self-review)"
prov when <path>         # just the ts
prov chain <path>        # walk user.prov.history
prov find --tool Edit --since 24h ~/.claude    # find files written by Edit recently
```

All shell-callable; no daemon.

### 4.5 No fsync coupling

The xattr write is part of the same `inode_setattr` transaction as the regular write metadata update. No additional fsync. Negligible overhead.

### 4.6 FUSE overlay fallback

For filesystems that don't support user xattrs (FAT, vfat, exFAT), `provfs-fuse` mounts an overlay that synthesizes the xattrs in a sidecar file (`<path>.prov.json`). This is the v0.2 path; v0.1 just requires ext4/btrfs/xfs.

---

## 5. Architecture

```
security/
└── provfs/
    ├── provfs_lsm.c        # the hooks
    ├── env_lookup.c        # reads $CLAUDE_TOOL etc from current's env
    ├── path_skip.c         # the skip-prefix matcher
    ├── history_ring.c      # MRU ring management for user.prov.history
    └── Kconfig             # CONFIG_SECURITY_PROVFS

~/.local/bin/prov            # userspace reader
```

LSMs in Linux are stackable since 5.4; provfs coexists with apparmor/selinux/etc. No conflict.

Estimated kernel diff: ~600 LoC for the LSM, ~200 for the userspace reader. Loaded via `modprobe provfs`; can be unloaded at runtime.

---

## 6. Non-goals

1. **Cryptographic integrity.** xattrs are unsigned; a malicious process can tamper with them via `setfattr` directly. *That's fine* — provfs is a hint layer, not an audit-grade chain of custody. If you want signed provenance, build that on top.
2. **Replacing git blame.** For tracked repo files, git's history is authoritative. `provfs` is for the broader case where git isn't involved.
3. **Cross-machine portability.** xattrs preserve through `cp -a` and `tar --xattrs`. They do not survive crossing a filesystem boundary that doesn't carry them. Single-laptop scope.
4. **Provenance for read access.** Only writes are tagged. Reads don't change xattrs.
5. **Enforcement.** provfs doesn't *prevent* writes; it just annotates them. Pair with apparmor or a separate LSM for prevention.
6. **Userspace push.** No syscall like `setprovenance()`. Provenance is derived from the calling task's existing state.

---

## 7. Phasing

| Phase | Scope                                                              |
| ----- | ------------------------------------------------------------------ |
| 0     | LSM stub with `user.prov.session` and `user.prov.ts` only. Boot-time skip list. |
| 1     | Add `tool`, `turn`, `intent`. Userspace `prov` reader.             |
| 2     | History ring (`user.prov.history`).                                |
| 3     | FUSE overlay for non-xattr filesystems.                            |
| 4     | `fsstory` integration: `fsstory who-wrote` reads xattrs first, falls back to ctrace/jsonl join only when missing. |

---

## 8. Risks

- **Performance.** xattr writes are cheap on modern ext4 (<10µs typical) but a write-heavy workload (cargo build, npm install) does thousands per minute. *Mitigation:* skip-prefix list excludes the usual suspects; per-mount disable. Worst case, the overhead is a small percentage and the user can opt out.
- **xattr-stripping tools.** `rsync -a` preserves xattrs only with `-X`; `tar` requires `--xattrs`. Users who copy files lose provenance silently. *Mitigation:* document; provide a `prov restore-from-history <dir>` for the worst case.
- **AgentNS unavailable.** If the AgentNS module isn't loaded (or this LSM is shipped standalone), the session-id fallback is "comm:claude:pid:N". Less precise but still useful.
- **Privacy.** xattrs containing tool names + session ids are readable by anyone with read access to the file. *Mitigation:* same single-user model as the rest of wintermute; xattrs in the `user.*` namespace are visible to file readers but not to other users.

---

## 9. Open questions

1. Should `user.prov.*` be configurable to a different namespace (e.g. `user.claude.*` or `trusted.prov.*`)? Trusted-namespace xattrs require CAP_SYS_ADMIN to read, which would harden against accidental exposure but also require sudo for `getfattr`. v0.1: stay in `user.*`.
2. Should provfs annotate *every* write or only those by tasks with a non-zero `agent_session_id`? *Probably the latter* — annotating user-interactive vim/nvim writes is mostly noise.
3. The history ring loses sessions when a hot file is rewritten by 6+ sessions. Bigger ring? Or external store? v0.1 picks small ring + external `fsstory` for the cold path.
4. How does provfs interact with overlayfs / btrfs snapshots? An xattr written on a snapshot lives in the upper layer; this is mostly fine but documentation worth writing.
5. Should there be a way to "lock" provenance — prevent overwrites — for audit-sensitive files? Probably useless without crypto signing; defer.
