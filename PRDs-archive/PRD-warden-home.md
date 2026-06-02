# PRD: warden-home — give the write-enforcer a versioned home

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/warden.md (Fleet 1)
**build_target:** rust-cli
**build_into:** /home/jsy/wintermute/bpolicy
**build_version_bump:** n/a (new repo, v0.1.0)
**Depends on:** none
**Codename:** *no-orphan-guardrail* — you can't safely evolve a tool you can't diff.

## TL;DR

`bpolicy` is the eighth local tool and the only one with no
source-controlled home. It is a Python script at `~/.local/bin/bpolicy`
plus C source at `~/.local/src/bpolicy/{bpolicy.bpf.c,vmlinux.h,bpolicy.bpf.o}`,
versioned nowhere, with no test harness. This PRD creates
`~/wintermute/bpolicy/`: a Rust control-plane CLI that reproduces the
existing six subcommands and JSON output byte-for-byte, the BPF source
vendored into the repo, a `build.sh` that compiles `bpolicy.bpf.o`, and
a test harness. It is the back-compat anchor for the rest of the warden
fleet — it pins the CLI surface (`load`/`unload`/`enforce`/`release`/
`status`/`log`) and the `status` JSON shape that `CLAUDE_SELF.md`, the
toolkit memory, and the `drift` skill already depend on. No behavior
change; a home and a harness so policy + deadman + self-review have
something to extend and test against.

## Why this exists

- `ls ~/wintermute/bpolicy` returns **no such repo** (verified
  2026-05-29). Every other tool tier has one: `memlog`, `provfs`,
  `agentns` all live at `~/wintermute/<slug>/`. The wintermute-home rule
  (memory `feedback_wintermute_home`) says Rust CLIs/libs I build for
  myself belong there; `bpolicy` is the lone exception.
- The control plane is a 5360-byte Python script (mtime 2026-05-21) with
  no tests. The toolkit memory (`feedback_local_tools.md`) documents it
  as tool #8 — *"if the user wants hard guardrails on you, `bpolicy`"* —
  with source at `~/.local/src/bpolicy/`. None of it is under version
  control or regression-tested.
- The two follow-on PRDs (`warden-policy`, `warden-deadman`) are
  `rust-extend` into this repo. They have nowhere to land and nothing to
  test against until the home + harness exist.
- Back-compat matters: `drift.md` already cites `bpolicy status` output;
  any reimplementation that changes the JSON shape silently breaks a
  skill. This PRD makes "same surface, new home" an explicit, tested
  invariant.

## What this builds

**Repo:** `~/wintermute/bpolicy/` → `j0yen/bpolicy` (public per /build).

**Layout:**
```
bpolicy/
├── Cargo.toml            # bin "bpolicy"; deps: clap, serde, serde_json, anyhow
├── rust-toolchain.toml   # 1.85 (lib-crate convention)
├── src/
│   ├── main.rs           # clap subcommand dispatch
│   ├── bpf.rs            # bpftool subprocess wrappers (load/unload/map ops)
│   ├── status.rs        # status JSON assembly (back-compat shape)
│   └── pids.rs          # pid → little-endian u32 key bytes
├── bpf/
│   ├── bpolicy.bpf.c    # vendored from ~/.local/src/bpolicy/ (unchanged)
│   ├── vmlinux.h        # vendored
│   └── build.sh         # clang -O2 -g -target bpf → bpolicy.bpf.o
├── reference/
│   └── bpolicy.py       # the original Python, kept for diffing
├── tests/
│   ├── acceptance_help.rs
│   ├── acceptance_status_shape.rs
│   └── acceptance_pidkey.rs
└── README.md
```

**Control plane (Rust, replaces the Python):**
- `bpolicy load` — `bpftool prog loadall <obj> /sys/fs/bpf/bpolicy
  autoattach pinmaps`; idempotent (`{"already_loaded": true}` when the
  `file_open_check` pin exists). Uses `sudo -n` exactly as the Python
  does; the privileged ops are unchanged.
- `bpolicy unload` — `rm -rf /sys/fs/bpf/bpolicy`; idempotent.
- `bpolicy enforce --pid PID [--pid …]` / `release --pid …` — update/
  delete the `protected_pids` pinned map by LE-u32 key.
- `bpolicy status` — **byte-identical JSON** to the Python:
  `{"loaded": false}` when not loaded; otherwise
  `{"loaded": true, "protected_pids": [...], "stats": {"checked","allowed","denied","forked_in"}}`
  with the same `indent=2` formatting.
- `bpolicy log [-n N]` — tail `trace_pipe`, filter `bpolicy:` lines.

**BPF object:** vendored unchanged; `bpf/build.sh` reproduces
`bpolicy.bpf.o` from source so the repo is self-contained. The installed
binary continues to read the object from `~/.local/src/bpolicy/` by
default (so a fresh `install` doesn't strand the existing pinned path),
with `BPOLICY_OBJ` env override pointing at the repo copy.

**Install:** `install -m755 target/release/bpolicy ~/.local/bin/bpolicy`
replaces the Python with the Rust binary. Same path, same name; the
toolkit memory + `CLAUDE_SELF.md` stay accurate without edits.

## Acceptance criteria

1. `bpolicy --help` lists exactly six subcommands: `load`, `unload`,
   `enforce`, `release`, `status`, `log`. A test asserts each appears.
2. `bpolicy status` with no BPF loaded prints exactly `{"loaded": false}`
   (modulo whitespace the Python emits) — a golden test compares against
   the Python reference output captured into `tests/`.
3. When loaded (mocked: a fixture that fakes the `bpftool -j map dump`
   output), `status` produces the documented shape with `protected_pids`
   sorted ascending and `stats` keyed `checked/allowed/denied/forked_in`.
   Test feeds canned `bpftool` JSON and asserts the assembled object.
4. `enforce`/`release` translate a PID to the same little-endian u32
   key bytes the Python `pid_to_key_bytes` produces (e.g. PID 1000 →
   `["232","3","0","0"]`). A unit test asserts the mapping for several
   PIDs including 0, 1, 65535, 4194304.
5. `bpf/build.sh` compiles `bpf/bpolicy.bpf.c` to a `bpolicy.bpf.o` whose
   BTF/section layout `bpftool prog loadall` accepts (verified in CI by
   `bpftool gen` dry-check, or skipped-with-reason if no clang/bpftool
   in the build env — declare as a deferred AC if so).
6. `load` and `unload` are idempotent: a second `load` when already
   loaded prints `{"already_loaded": true}`; a second `unload` prints
   `{"already_unloaded": true}`. Tested against a mock that toggles the
   pin-exists check.
7. `reference/bpolicy.py` is present and `diff`-able; README documents
   the one-line behavioral contract ("same surface, new home") and the
   `BPOLICY_OBJ` override.
8. `cargo clippy -D warnings` and `cargo test` are green (MSRV 1.85, no
   let-chains). Privileged paths (`sudo -n bpftool …`) are isolated in
   `bpf.rs` behind a trait so tests inject a mock and never invoke sudo.

## Notes

- **Do not load the enforcer as part of this PRD.** Home + harness only.
  Arming is the user's call and is what warden-deadman makes safe.
- **No BPF behavior change.** The `.bpf.c` is vendored verbatim; the
  hardcoded allow-list stays hardcoded until warden-policy. This PRD is
  deliberately behavior-preserving so the diff is "Python → Rust, same
  output," nothing else.
- Per the wintermute-home rule, the repo lives at `~/wintermute/bpolicy/`,
  not `~/projects/`. Joe Yen identity for the wintermute commit; `j0yen`
  for the published repo (/build's publish step handles the split).
