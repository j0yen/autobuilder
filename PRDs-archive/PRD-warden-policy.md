# PRD: warden-policy — a declarative allow-list you can read before you trust it

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/warden.md (Fleet 1)
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/bpolicy
**build_version_bump:** minor
**Depends on:** PRD-warden-home (Fleet 1)
**Codename:** *un-baked* — the allow-list moves from the compiler to a file.

## TL;DR

`bpolicy`'s allow-list is hardcoded in `bpolicy.bpf.c`: an enrolled PID
may write only under `/tmp`, `/dev/{null,tty,std*,pts}`, and
`/proc/self/`. That set is baked into the BPF object, so the only thing
the enforcer can express is "jail this PID to those few paths" — useless
for any real agent, which writes `~/.claude` and `~/wintermute`
constantly. This PRD makes the allow-list declarative: a
`~/.config/bpolicy/policy.toml` with named profiles, each adding
writable path prefixes on top of the always-allowed defaults; a new BPF
allow-list map the hook consults by **longest-prefix match**; and
`bpolicy load --profile <name>` to populate the map from the policy.
After this, "jail this agent to its own workspace and nowhere else"
becomes a one-line profile instead of a recompile.

## Why this exists

- Read of `~/.local/bin/bpolicy` (2026-05-29) confirms the allow-list is
  not a parameter anywhere in the control plane — it lives entirely in
  the compiled `.bpf.o`. The docstring and the toolkit memory both state
  the fixed set (`/tmp`, `/dev/{null,tty,std*,pts}`, `/proc/self/`).
- The self-review journals' Pending line — *"loading needs sudo + a
  user-owned policy file"* (2026-05-29 runs 1/2) — names the missing
  piece exactly: there is no policy file. The user instinct is already
  "a policy should be a file I write," and the tool has no such file.
- Without a declarative allow-list, the warden end-state ("jail an agent
  to its workspace") and Fleet 2's session-enrollment are both
  impossible. This is the load-bearing unblock for everything that makes
  the enforcer *usable* rather than merely *loadable*.
- This is a runtime BPF-LSM object, **not** a kernel-package change. Per
  the dream Phase 1.5 note, it does not touch the `linux-wintermute`
  PKGBUILD, `apply-agentns.py`, or any Kconfig — `bpolicy.bpf.c` is
  compiled and loaded unprivileged-with-sudo via `bpftool`, independent
  of the booted kernel image.

## What this builds

**Policy file** `~/.config/bpolicy/policy.toml`:
```toml
# always-allowed defaults (/tmp, /dev/{null,tty,std*,pts}, /proc/self/)
# are implicit and cannot be removed by a profile.

[profile.workspace]
description = "agent jailed to its wintermute + claude workspace"
allow = [
  "/home/jsy/wintermute",
  "/home/jsy/.claude",
  "/home/jsy/.cache",
  "/home/jsy/.config/bpolicy",   # so it can renew its own deadman
]

[profile.tight]
description = "tmp + dev only — the compiled default, named"
allow = []
```

**BPF change** (`bpf/bpolicy.bpf.c`, vendored in warden-home):
- Add an `allowlist` BPF hash/LPM map: keys are path prefixes (bounded,
  e.g. `BPF_MAP_TYPE_LPM_TRIE` keyed on the path string up to `PATH_MAX`
  cap, or a fixed-N hash of prefix segments if LPM-on-string proves
  awkward — implementation picks the cheaper correct one and documents
  the choice).
- In the `file_open` hook, when a PID is in `protected_pids` and the
  open carries `FMODE_WRITE`, resolve the target path and check it
  against (a) the compiled always-allowed defaults, then (b) the
  `allowlist` map by longest prefix. Allow on either hit; deny otherwise.
- The always-allowed defaults stay compiled in — a profile can only
  **add**, never remove them, so a profile can't accidentally lock out
  `/dev/null` or `/tmp`.

**Control plane** (`rust-extend` into `~/wintermute/bpolicy`):
- `bpolicy load --profile <name>` — parse `policy.toml`, validate the
  named profile, populate the `allowlist` map after `loadall`, then
  attach. `--profile` defaults to `tight` (compiled behavior) so a bare
  `--profile`-less `load` is unchanged from warden-home.
- `bpolicy policy show [--profile <name>]` — print the resolved
  allow-list (defaults + profile prefixes) as JSON; lets the user read
  exactly what would be enforced before arming.
- `bpolicy policy check <path> [--profile <name>]` — answer "would a
  write to this path be allowed?" without loading anything. Pure
  userspace evaluation of the same longest-prefix logic, for testing a
  profile.
- `bpolicy status` gains a `"profile": "<name>"` field (back-compat: the
  field is additive; absent ⇒ `tight`/compiled). The existing keys are
  untouched so warden-home's golden test still passes with the field
  added to the documented shape.

## Acceptance criteria

1. A `policy.toml` with a `workspace` profile parses; an unknown profile
   name to `--profile` errors with a clear message listing known
   profiles. Tested.
2. `bpolicy policy show --profile workspace` prints JSON containing both
   the compiled defaults **and** the profile's `allow` prefixes; the
   defaults are present even when the profile lists none.
3. `bpolicy policy check <path> --profile workspace` returns allowed for
   `/home/jsy/wintermute/x/y`, `/tmp/z`, `/dev/null`; denied for
   `/home/jsy/Documents/secret` and `/etc/passwd`. A table-driven test
   covers ≥8 paths across allow/deny.
4. The longest-prefix logic is correct: a profile allowing
   `/home/jsy/wintermute` but a tighter compiled deny is impossible
   (defaults only add); overlapping prefixes resolve to the longest
   match. Unit-tested in the userspace evaluator that mirrors the BPF
   logic.
5. `bpf/build.sh` compiles the modified `bpolicy.bpf.c` with the new
   `allowlist` map; `bpftool` accepts the object (or deferred-AC with
   reason if no clang/bpftool in build env, as in warden-home AC5).
6. `bpolicy load --profile workspace` (in a VM/privileged smoke, or
   mocked at the `bpftool map update` boundary) populates the allowlist
   map with the profile's prefixes; `status` reports `"profile":
   "workspace"`. The mock asserts the exact map-update calls.
7. Back-compat: `bpolicy load` with no `--profile` behaves identically
   to warden-home (compiled defaults only); warden-home's status golden
   test still passes with the additive `profile` field.
8. `cargo clippy -D warnings` + `cargo test` green; the userspace
   longest-prefix evaluator and the BPF hook logic share a documented
   spec so they cannot silently diverge (a comment in both citing the
   other).

## Notes

- **Still does not arm enforcement on any real session.** This PRD makes
  a *good* policy expressible and inspectable; warden-deadman makes
  arming one *survivable*; only the user decides to actually
  `enforce --pid` a live session.
- The BPF path-resolution in `file_open` is the subtle part: resolving a
  full path in-hook can be costly. The implementation must bound the
  work (cap prefix depth / path length) and document the cost; if
  full-path resolution proves too expensive, fall back to checking the
  dentry's path against prefixes at a fixed maximum depth and note the
  limitation. Correctness-over-cleverness: a profile that can't be
  enforced efficiently should be rejected at `load`, not silently
  partial.
- Serialize with **PRD-warden-deadman** — both `rust-extend` the same
  `build_into`; never build in parallel (dirty-tree collision +
  conflicting `bpolicy.bpf.c` edits). Order between them is semantically
  free.
