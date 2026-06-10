# autobuilder-bincov-receipt

Detects `[[bin]]` crates with no integration test that drives the binary as a subprocess; emits a `bincov.v1` receipt JSON.

## TL;DR

Autobuilder's Stage-4 gate runs `cargo test --release` — but for a crate that ships a `[[bin]]`, lib-only tests never execute binary dispatch arms. This CLI detects that gap: given a crate directory, it checks whether any `tests/` file drives the binary via `std::process::Command` or `assert_cmd`, and emits a machine-readable receipt. A `concern` verdict is a signal the gate (or `/build`) can surface or hard-block on with `--strict`.

## Usage

```
autobuilder-bincov-receipt <crate-dir> [--format json|human] [--strict]
```

### Example output

```json
{
  "receipt": "bincov.v1",
  "crate": "mqo-mcp-server",
  "has_bin": true,
  "bin_names": ["mqo-mcp-server"],
  "has_integration_test": false,
  "integration_test_files": [],
  "verdict": "concern",
  "note": "Crate ships a [[bin]] but no tests/ file drives it via std::process::Command; binary dispatch arms are unreachable by lib-only `cargo test`."
}
```

### Verdict rules

| Situation | verdict | exit code |
|---|---|---|
| No `[[bin]]`, no `src/main.rs` | `pass` | 0 |
| `[[bin]]` + integration test found | `pass` | 0 |
| `[[bin]]` + no integration test | `concern` | 0 (default), 3 (with `--strict`) |

## Acceptance criteria

1. **AC1** — `[[bin]]` + no `std::process::Command` test → `has_bin:true`, `has_integration_test:false`, `verdict:"concern"`
2. **AC2** — `[[bin]]` + `tests/integration_cli.rs` using `Command::new(env!("CARGO_BIN_EXE_foo"))` → `verdict:"pass"`, file named in `integration_test_files`
3. **AC3** — Pure-lib crate (no `[[bin]]`, no `src/main.rs`) → `has_bin:false`, `verdict:"pass"`
4. **AC4** — Single-bin convention (`src/main.rs`, no explicit `[[bin]]`) → `has_bin:true`
5. **AC5** — `--strict` exits 3 on `concern`, 0 on `pass`
6. **AC6** — `assert_cmd::Command` import counts as an integration test
7. **AC7** — JSON output is schema-stable and deterministic across runs

## Install

```bash
cargo install --path .
```

Or build locally:

```bash
cargo build --release
# binary at target/release/autobuilder-bincov-receipt
```

## Dependencies

- `clap` — CLI argument parsing
- `serde` + `serde_json` — receipt serialization
- `toml` — Cargo.toml parsing

No network access. No cargo invocation (static file inspection only).
