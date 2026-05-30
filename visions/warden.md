# Vision: warden — the guardrail that was built but never armed

**Authored by:** /dream (Claude Opus 4.8), with jsy
**Created:** 2026-05-29
**Status:** active
**Fleet 1 drafted:** 4 PRDs (home repo · declarative policy · safe-load lifecycle · self-review wiring)
**Fleet 2:** captured as bullets; future `/dream extend warden`

---

## TL;DR

`bpolicy` is the eighth local tool — the eBPF-LSM `file_open` write
enforcer, the one the toolkit memory describes as *"if the user wants
hard guardrails on you, bpolicy."* It works: a real BPF object at
`~/.local/src/bpolicy/bpolicy.bpf.o`, a Python control CLI at
`~/.local/bin/bpolicy`, fork-tracked per-PID enforcement. And it is the
only one of the eight tools that has **no home repo, no PRD, no vision,
and — on this boot — has never been loaded.** `bpolicy status` returns
`{"loaded": false}`, and `/self-review` re-flags exactly that string
every run as a standing blind spot ("bpolicy not loaded — no
enforcement"). The guardrail exists and is inert.

There are three reasons it stays inert, and each is a PRD:

1. **It has no source-controlled home.** Every other tool that matters
   lives at `~/wintermute/<slug>/` (per the wintermute-home rule); the
   kernel-tier primitives (`memlog`, `provfs`, `agentns`) each have a
   repo. `bpolicy` is an orphan: a Python script in `~/.local/bin` and
   C source in `~/.local/src/bpolicy`, versioned nowhere, with no test
   harness. You can't safely evolve a thing you can't diff.

2. **Its allow-list is hardcoded in the BPF object.** The enforcer
   denies `FMODE_WRITE` opens outside `/tmp`, `/dev/{null,tty,std*,pts}`,
   and `/proc/self/` — and that set is baked into `bpolicy.bpf.c`. There
   is no way to say "this agent may also write `~/wintermute/foo` and
   nowhere else" without recompiling the BPF. So the one model the tool
   supports — jail a PID tree to an allow-list — is unusable for any
   real agent, because every real agent writes `~/.claude` and
   `~/wintermute` constantly. Hardcoded allow-list ⇒ load it and Claude
   dies on its first write.

3. **Loading it is a cliff with no railing.** A `file_open` LSM hook
   that denies writes is, by construction, the kind of thing that can
   brick the laptop: arm a too-tight policy on your own session and you
   can no longer write the file that would fix it. There is no
   dry-run/audit mode (count denies without blocking), no deadman timer
   (auto-unload if not renewed), no blast-radius story. A careful user
   *correctly* never loads it. The safety scaffolding that would make
   loading a reasonable thing to do does not exist.

`warden` is the vision where the guardrail becomes armable on purpose:
a versioned home, a declarative policy you can read before you trust it,
a load lifecycle that cannot strand you, and a self-review line that
escalates the inert state **once** instead of re-noticing it forever.

The point is not to jail Claude. The point is that when the user reaches
for the hard guardrail the toolkit promises, it is a tool and not a
landmine.

## End-state

When Fleet 1 ships:

1. **`bpolicy` has a home at `~/wintermute/bpolicy/`** with the BPF
   source vendored, a build script, and a test harness. The control
   plane is a single binary with the same six subcommands
   (`load`/`unload`/`enforce`/`release`/`status`/`log`) and the same
   JSON output shape, so `CLAUDE_SELF.md`, the toolkit memory, and every
   skill that shells `bpolicy status` keep working unchanged.

2. **The allow-list is declarative.** A policy file
   (`~/.config/bpolicy/policy.toml`) names profiles, each a set of
   writable path prefixes layered on top of the always-allowed defaults.
   `bpolicy load --profile workspace` populates a BPF allow-list map
   from the policy; the `.bpf.c` reads the map by longest-prefix instead
   of comparing against compiled-in constants. You can read the policy
   before you trust it, and change it without a recompile.

3. **Loading cannot strand you.** `bpolicy load --audit` arms in
   log-only mode — every would-be denial is counted and logged, nothing
   is blocked — so you can watch what a profile *would* do against a
   live workload before enforcing. `bpolicy load --ttl 15m` arms a
   deadman: if `bpolicy renew` is not called before the TTL, the policy
   auto-unloads. A bad policy is self-healing on a clock.

4. **`/self-review` reports the guardrail's state and escalates it
   once.** Phase A prints a `warden:` health line (`loaded`,
   `audit|enforce`, protected-PID count, deny count). A Phase B.5
   playbook turns the recurring `{"loaded": false}` observation into a
   single durable escalation (a docket finding / one journal note),
   not a fresh flag every run.

When Fleet 2 ships (bullets below): session enrollment via agentns,
a deny-list (protect-these-paths-from-all) inverse mode, and reaping of
the empty-file artifacts the `file_open`-after-create quirk leaves.

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  OBSERVE     /self-review Phase A `warden:` line + B.5 escalate-  │
│              once playbook  (PRD-warden-self-review · shell)      │
├──────────────────────────────────────────────────────────────────┤
│  SAFE LOAD   --audit (log-only) · --ttl + renew (deadman)         │
│              (PRD-warden-deadman · rust-extend → bpolicy)         │
├──────────────────────────────────────────────────────────────────┤
│  POLICY      ~/.config/bpolicy/policy.toml → BPF allowlist map    │
│              longest-prefix match in bpolicy.bpf.c                │
│              (PRD-warden-policy · rust-extend → bpolicy)          │
├──────────────────────────────────────────────────────────────────┤
│  HOME        ~/wintermute/bpolicy/  Rust control CLI + vendored   │
│              bpolicy.bpf.c/.o + build.sh + tests                  │
│              (PRD-warden-home · rust-cli, NEW repo)               │
├──────────────────────────────────────────────────────────────────┤
│  KERNEL      BPF-LSM file_open hook (BOOTED · CONFIG_BPF_LSM=y)   │
│              pinned at /sys/fs/bpf/bpolicy/ · bpftool load path   │
└──────────────────────────────────────────────────────────────────┘
```

This is the [onramp][onramp] pattern — *from built to consumed* — but
for the enforcement primitive rather than the observation primitives
(`memlog`, `agentns`, `provfs`). `onramp` makes the kernel's signals
readable; `warden` makes the kernel's one *guardrail* armable. Siblings,
not overlaps: no `onramp` PRD touches `bpolicy`, and no `warden` PRD
touches the memlog group, the agentns wrap, or the provfs fallback.

## Order

1. **PRD-warden-home** — no dependencies; ships first. Until `bpolicy`
   has a repo with a build + test harness, the policy and deadman PRDs
   have nowhere to land and nothing to regression-test against. This PRD
   is also the back-compat anchor: it pins the CLI surface and JSON
   shape that the toolkit memory + `CLAUDE_SELF.md` already document.

2. **PRD-warden-policy** — `rust-extend` into `~/wintermute/bpolicy`.
   Depends on warden-home. Adds the policy file, the allow-list BPF map,
   and the longest-prefix match in `bpolicy.bpf.c`. This is the one that
   touches the BPF C; it is an unprivileged runtime BPF-LSM object, not
   a kernel-package change (see Phase 1.5 note in dream), so it does not
   need `apply-agentns.py` or a PKGBUILD bump.

3. **PRD-warden-deadman** — `rust-extend` into `~/wintermute/bpolicy`.
   Depends on warden-home; semantically pairs with warden-policy (the
   deadman is what makes a *too-tight policy* survivable). Adds `--audit`
   (a BPF config-map flag read by the hook) and `--ttl`/`renew`
   (a userspace timer that unloads). **Serialize against warden-policy
   — same `build_into`, never build in parallel.**

4. **PRD-warden-self-review** — `shell`. Depends only on warden-home's
   stable `status` JSON. Independent of policy/deadman; can ship as soon
   as home lands. Adds the Phase A line + the escalate-once playbook.

Order is "home first (everyone needs it), then the two repo-extends
serialized in either order, then the observation wiring."

## Fleet 2 — future `/dream extend warden`

Bullets only; draft after Fleet 1 ships ≥2 of 4.

- **`warden-enroll-session`** — at SessionStart, enroll the agentns
  session-root PID (so the whole descendant tree is governed) under a
  profile derived from the session's `intent_tag`. Hard-gated on
  [onramp][onramp] PRD #2 (`claude-agentns-wrap`) landing first —
  enrollment by session only makes sense once `/proc/self/agent_session`
  is non-zero. Until then enrollment is by root-PID, which the fork
  tracker already handles.
- **`warden-deny-mode`** — the inverse model: protect a small set of
  *specific* paths (`~/.claude/settings.json`, `/etc/wintermute/`,
  `.git/` internals) from writes by **all** PIDs, not jail one PID tree
  to an allow-list. A different BPF map + hook branch; useful for "these
  files are sacred" independent of who's running. Spec before drafting:
  confirm the `file_open` hook can see the target path cheaply enough
  for an all-PID check.
- **`warden-empty-file-reap`** — the known quirk: `file_open` fires
  after inode creation, so a denied write leaves a 0-byte file (rc≠0,
  size 0, no data leaked). A `bpolicy log`-driven reaper that removes
  the empties it can attribute to a denial. Small, depends on the deny
  log being structured (warden-home should emit structured deny events).
- **`warden-doctor`** — a `bpolicy doctor` that checks: object compiles,
  loads + unloads cleanly in a throwaway, a known-bad write is denied
  and a known-good write is allowed under a test profile, and the
  deadman fires. Embeddable in `/self-review` Phase A as a deeper check
  than the status line. (Mirror of onramp's `onramp-doctor` bullet, for
  the enforcement tool.)

## Open questions

- **Keep the binary named `bpolicy`, or rename to `warden`?** Leaning
  **keep `bpolicy`** — it is established in `CLAUDE_SELF.md`, the toolkit
  memory, and at least one skill (`drift` cites `bpolicy status`).
  Renaming breaks documented surface for cosmetics. The *vision* is
  `warden`; the *tool* stays `bpolicy`. The repo is `~/wintermute/bpolicy/`.
- **Rust rewrite of the control plane, or repo-ify the Python as-is?**
  Leaning **Rust rewrite** — the autobuilder pipeline is Rust-native and
  gives a real test harness; the control plane is thin (subprocess
  `bpftool` + map updates + JSON), a clean CLI-shaped target. The
  Python stays in-repo as `reference/bpolicy.py` for diffing. Hard
  requirement: byte-identical `status` JSON shape and the same six
  subcommands (back-compat AC in warden-home).
- **Does the allow-list map belong to the policy or the profile?**
  i.e. one map reloaded per `load --profile`, or N pre-populated maps?
  Leaning **one map, repopulated on load** — simpler, and load is rare.
- **Where does `audit` mode store its counts?** A per-prefix deny
  counter (which rule would have fired) is far more useful than a single
  scalar for tuning a profile, but costs a second BPF map. Leaning
  **per-prefix counter** in warden-deadman; revisit if map pressure
  shows up.
- **Should `--ttl` default to on?** A user who types `bpolicy load`
  bare is the exact user who most needs the deadman. Leaning **yes —
  default `--ttl 30m`, opt out with `--ttl 0`** for a permanent arm.
  Flagged for the user; deadman PRD drafts it defaulted-on.

## Provenance

- **Seeded by:** `/dream` invocation 2026-05-29 21:33 PDT (bare, no
  topic). Phase 0/1 listening surfaced the recurring self-review blind
  spots; recall ideation hits pointed at the kernel/enforcement tier
  being "built but inert."
- **Research (all run this pass):**
  - `bpolicy status` → `{"loaded": false}` (never armed this boot)
  - `which bpolicy` → `~/.local/bin/bpolicy`; `file` → Python script,
    5360 bytes, mtime 2026-05-21
  - `ls ~/wintermute/bpolicy` → **no such repo** (every other tool tier
    has one; `memlog`/`provfs`/`agentns` repos all present)
  - `ls PRD-*bpolicy* visions/*bpolicy*` → no matches; `grep -rl bpolicy
    visions/` → only `drift.md`, and only as a `--format json` surface
    note, never as a subject
  - read `~/.local/bin/bpolicy`: confirmed six subcommands, per-PID
    enforce/release, JSON status, BPF object at
    `~/.local/src/bpolicy/bpolicy.bpf.o`, allow-list **hardcoded** in
    the comment + implied by the C ("`/tmp`, `/dev/{null,tty,std*,pts}`,
    `/proc/self/`"), known empty-file quirk documented in the toolkit
    memory
  - toolkit memory `feedback_local_tools.md`: bpolicy is tool #8,
    "if the user wants hard guardrails on you, `bpolicy`"; source at
    `~/.local/src/bpolicy/{bpolicy.bpf.c,vmlinux.h,bpolicy.bpf.o}`
  - self-review journals 2026-05-29 runs 1/2: **"bpolicy not loaded
    (`{"loaded":false}`) — no enforcement; loading needs sudo + a
    user-owned policy file"** appears verbatim in the Pending section of
    consecutive runs — the recurring blind spot warden-self-review kills
  - `uname -r` → `7.0.10-arch1-5-wintermute` (BPF-LSM available)
- **Sibling vision:** [onramp][onramp] — same built→consumed shape, the
  observation half; warden is the enforcement half. Verified disjoint
  (no shared PRD, no shared primitive).
- **User decisions pending:** the open questions above, especially
  whether to ever arm enforcement on an interactive session vs. only on
  headless/sandboxed ones.

[onramp]: onramp.md
