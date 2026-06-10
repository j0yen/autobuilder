# autobuilder-harness-portability-audit

Scans shell scripts for Linux-only idioms and reports macOS-equivalent suggestions. Draft-only — never edits scripts.

## TL;DR

The autobuilder harness scripts assumed Linux. This CLI surfaces every Linux-only idiom (bare `nproc`, `/proc/` paths, `flock`, GNU `date -d`, `readlink -f`, `sed -i` without suffix, `stat -c`) with its file, line, and a macOS-equivalent suggestion. A `--strict` flag exits 4 when any unguarded finding exists, enabling CI enforcement.

## Usage

```
autobuilder-harness-portability-audit <scripts-dir> [--format json|human] [--strict]
```

## Rules

| id | pattern | macOS suggestion |
|----|---------|------------------|
| `nproc` | bare `nproc` not followed by a fallback | `nproc 2>/dev/null \|\| sysctl -n hw.logicalcpu \|\| echo 4` |
| `proc-fs` | `/proc/` path | use ps/sysctl |
| `flock` | `flock ` invocation | mkdir-based lock |
| `gnu-date` | `date -d ` or `date --date` | BSD `date -j -f` |
| `readlink-f` | `readlink -f` | python3 realpath |
| `sed-i-empty` | `sed -i ` without backup-suffix | BSD `sed -i ''` |
| `stat-c` | `stat -c` | BSD `stat -f` |

## Output

```json
{
  "report": "portability.v1",
  "scripts_dir": "scripts",
  "findings": [
    { "rule": "nproc", "file": "run-mutants.sh", "line": 124,
      "text": "...", "already_guarded": true, "suggestion": "..." }
  ],
  "summary": { "files_scanned": 9, "findings": 1, "unguarded": 0 }
}
```

`already_guarded: true` when the line already contains its own fallback. Only `unguarded` findings count toward `--strict` exit 4.

## Acceptance Criteria

- **AC1**: Bare `nproc` → `already_guarded: false`, counted in `unguarded`
- **AC2**: `nproc` with `sysctl -n hw.logicalcpu` fallback → `already_guarded: true`, not in `unguarded`
- **AC3**: `/proc/`, `flock`, `date -d`, `readlink -f`, `sed -i` (no suffix), `stat -c` each trigger their rule
- **AC4**: Every finding has file, 1-based line, matched text, non-empty suggestion
- **AC5**: `--strict` exits 4 when unguarded ≥ 1, exits 0 when all guarded or none
- **AC6**: Clean dir → `findings: []`, exit 0
- **AC7**: Output deterministic — findings sorted by (file, line)

## Install

```bash
cargo install --path .
# or build directly:
cargo build --release
./target/release/autobuilder-harness-portability-audit scripts/ --format human
```
