# PRD — tool-manifest

Status: Draft v0.1
build_target: rust-cli
build_into: ~/wintermute/tool-manifest/
Vision: visions/drift.md
build_priority: medium

## TL;DR

A small Rust CLI that probes installed binaries via `--help` and
writes a structured JSON manifest of their flag and subcommand
surface. The manifest is the source of truth that `skill-doctor`
(and future Fleet 2 tools) reads from when checking whether a
skill's invocation matches reality.

## Why this exists

Today, no canonical source describes which tools are installed in
`~/.local/bin/`, what flags they accept, or which subcommands
they expose. Skills hardcode this knowledge in prose; the
knowledge drifts; the journal logs the drift; nobody resolves it.
(See `visions/drift.md` for the four currently-live drift
instances.)

To detect future drift mechanically, a checker needs ground truth.
That ground truth is what each binary's own `--help` text reports.
This PRD captures that into a manifest so consumers don't
re-probe the binary on every check.

Why a separate tool rather than inline in `skill-doctor`:
- The manifest is useful to other consumers (bootstrap symlink
  checks, the `wm-publish` wrapper's pre-flight validation, a
  future `update-config` integrity check).
- Caching `--help` invocations across runs amortizes probing cost.
- Separation of concerns: probing is one job; checking is another.

## What this builds

A Rust CLI crate at `~/wintermute/tool-manifest/`:

```
~/wintermute/tool-manifest/
├── Cargo.toml
├── src/
│   ├── main.rs         # CLI entrypoint
│   ├── lib.rs          # public API for consumers
│   ├── probe.rs        # subprocess `<tool> --help` + parse
│   ├── manifest.rs     # JSON schema + load/store
│   └── parse.rs        # heuristic --help parser
└── tests/
    └── probe.rs        # round-trip tests against a fixture tool
```

### Manifest shape

`~/.claude/tool-manifest/manifest.json`:

```json
{
  "schema_version": 1,
  "generated_at": "2026-05-28T01:35:00Z",
  "bin_dirs": ["/home/jsy/.local/bin"],
  "tools": {
    "pevent": {
      "path": "/home/jsy/.local/bin/pevent",
      "version": "0.3.1",
      "version_only": false,
      "flags": ["-h", "--help"],
      "subcommands": {
        "gc": {
          "flags": ["-h", "--help", "--older-than"],
          "version_only": false
        },
        "list": { "flags": ["-h", "--help", "--json"] }
      }
    },
    "bpolicy": {
      "path": "/home/jsy/.local/bin/bpolicy",
      "version": "0.2.0",
      "flags": ["-h", "--help"],
      "subcommands": {
        "status": { "flags": ["-h", "--help"] }
      }
    }
  }
}
```

- `version_only: true` for tools that don't accept `--help`. The
  manifest still records the binary path so consumers can detect
  presence; flag-validation is suppressed.

### CLI surface

- `tool-manifest sync` — walks `~/.local/bin/` (or configured
  `--bin-dir`), probes each binary, writes manifest. Idempotent.
  Records exit time + duration. Skips files that aren't
  executable.
- `tool-manifest show <tool>` — prints manifest entry for a tool
  (json by default; `--format text` for human).
- `tool-manifest query <tool> [<sub>] <flag>` — exit 0 if the
  manifest says the flag is supported, 1 if not, 2 if the tool
  isn't in the manifest. Designed for shell-script consumers
  (e.g., `tool-manifest query pevent gc --older-than && echo ok`).
- `tool-manifest list` — prints all tool names (newline-separated)
  so other tools can iterate.

### Probing strategy

1. For each binary, run `<bin> --help` with a small timeout (5s)
   and a hard cap on output (e.g., 64KB). Capture stdout+stderr;
   most argparse tools write usage to one or the other.
2. Parse the `--help` text heuristically:
   - Flag detection: any `--word` or `-X` at the start of an
     option-block line. (argparse and clap both emit this shape.)
   - Subcommand detection: a block headed `subcommands:` or
     `commands:` followed by indented names; or — if the binary's
     `--help` lists `{sub1,sub2,...}` in the usage line — split
     that.
3. For each detected subcommand, recurse: `<bin> <sub> --help`
   and parse the same way (one level deep only for Fleet 1).
4. Version detection: `<bin> --version` if accepted; fall back to
   parsing a `Version: X.Y.Z` line from `--help` if present;
   otherwise null.
5. Tools that error on `--help` are marked `version_only: true`
   and skipped for flag-parse.

### What's out of scope

- Recursive subcommand probing beyond one level.
- Manpage parsing as a fallback (separate PRD if Fleet 1 finds
  it's needed).
- Auto-running on a timer or hook (consumers invoke `sync`
  explicitly; future Fleet 2 may wire it in).
- Cross-referencing against PATH globally (only configured
  `--bin-dir`s; default `~/.local/bin/`).

## Acceptance criteria

1. `cargo build --release` green on a fresh clone; `cargo test
   --release --lib` green.
2. `tool-manifest sync` against `~/.local/bin/` writes
   `~/.claude/tool-manifest/manifest.json` containing entries for
   `pevent`, `bpolicy`, `ctrace`, `wchg`, `recall`, `claude-self`
   at minimum, each with a non-empty `flags` array.
3. `tool-manifest query pevent gc --older-than` exits 0
   (supported); `tool-manifest query pevent gc --dry-run` exits 1
   (not supported).
4. `tool-manifest query bpolicy status --format` exits 1.
5. `tool-manifest show pevent --format json` emits valid JSON
   matching the schema in §"Manifest shape" above.
6. Probing handles binaries that timeout or hang on `--help`: a
   5-second timeout is enforced; the manifest entry records
   `probe_status: timeout` rather than blocking the sync.
7. `tool-manifest sync` is idempotent — running it twice in a
   row produces the same manifest (modulo `generated_at`
   timestamp).
8. `tool-manifest --version` reports a version (matching
   Cargo.toml).
9. The crate is published to `github.com/j0yen/tool-manifest`
   under MIT+Apache-2.0 dual license, with a README citing the
   drift vision and a Usage section.
10. `~/.local/bin/tool-manifest` is installed via
    `bootstrap/install.sh` (the crate gets a row in
    `~/wintermute/REPOS.md` and the bootstrap script picks it
    up).

## Notes for /build

- Standard `/autobuilder` rust-cli flow. Stages 1–6 as usual.
- Subprocess probing needs care: don't spawn the probed binary
  with inherited stdin/stdout — use `Command::new(...).stdin(
  Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped())`
  and a tokio (or std::thread + signal) timeout.
- Parse heuristics will produce some false positives/negatives.
  Tolerable for Fleet 1; downstream consumer (`skill-doctor`)
  treats manifest hits as authoritative but its proposals are
  user-reviewed.
- For initial probing, optimize for the common case of clap-
  generated and argparse-generated `--help`. Other shapes can be
  added later.
- No collision with existing crates; `tool-manifest` is a clean
  slug. New repo, new directory.

## Dependencies

None. Can ship in parallel with `drift-fix-self-review-dream`.
`skill-doctor` depends on this PRD shipping.
