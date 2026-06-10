# autobuilder-ac-counter

Counts acceptance criteria correctly across all three test layouts used in the autobuilder fleet.

## TL;DR

The autobuilder Stage-3 metric harness (`run-metrics.sh:68`) uses
`grep -cE '^fn ac[0-9]+_'` to count ACs — too narrow: it misses the `new_ac[N]_…`
and `ext[N]_…` function families. The `mqo-mcp-server` crate ships all three families
(`ac1_…`, `new_ac1_…`, `ext1_…`) and the harness undercounts every one of them.

This library is the single source of truth for AC discovery.

## Three supported layouts

| Layout | Pattern | Count |
|--------|---------|-------|
| Split-file | `tests/acceptance_*.rs` | 1 AC per file |
| Monolithic | `fn (ac\|new_ac\|ext)[0-9]+_…` in `tests/acceptance.rs` | 1 AC per fn |
| Mock | `tests/mocks/ac<N>.rs` | 1 AC per file |

## Public API

```rust
pub struct AcInventory {
    pub total: usize,
    pub by_layout: Layouts,
    pub names: Vec<String>,  // sorted
}

pub struct Layouts {
    pub split_file: usize,
    pub monolithic_fns: usize,
    pub mock_files: usize,
}

/// Discover all ACs declared under <crate_dir>/tests/
pub fn discover(crate_dir: &std::path::Path) -> std::io::Result<AcInventory>;

/// Count passing AC tests in cargo test stdout
pub fn count_passing(test_stdout: &str) -> usize;
```

## CLI

```bash
autobuilder-ac-counter <crate-dir> [--format json|human]
```

## Acceptance Criteria (7/7 passing)

1. `discover` on 3 `acceptance_*.rs` files → `total=3, split_file=3`
2. `discover` on `acceptance.rs` with `ac1_x`, `new_ac1_y`, `ext1_z` → `total=3, monolithic_fns=3` (**the mqo-mcp-server undercount fix**)
3. `discover` on `tests/mocks/ac1.rs` + `tests/mocks/ac2.rs` → `mock_files=2`
4. Mixed layout → correct summed total and per-layout split
5. `count_passing` counts ok lines for all families; FAILED lines ignored
6. Empty / missing `tests/` → `total=0`, no panic
7. `names` is sorted and deterministic across runs

## Install

```toml
[dependencies]
autobuilder-ac-counter = { git = "https://github.com/joeyen-atscale/autobuilder-ac-counter" }
```
