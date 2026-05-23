# PRD: autobuilder-evolve-safety — patch-safety primitives under adversarial discipline

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1 — phase 1 of evolve.rs extraction
**Date:** 2026-05-23
**Sibling to:** `PRD-receipt.md`, `PRD-gate.md`

---

## TL;DR

`autobuilder-evolve-safety` is a Rust lib crate (~80 LoC) that owns the
load-bearing decision functions of evolve.rs's auto-apply path:
**`is_pure_addition_diff`** (does this diff body contain any `-` lines in
its hunks?) and the two fingerprint hashers
(**`Suggestion::fingerprint`** and **`PatchSuggestion::fingerprint`**) that
dedupe applied suggestions via `applied.log`.

Why this scope and not the rest of evolve.rs: a bug in `is_pure_addition_diff`
that returns `true` for a diff containing `-` lines causes the autobuilder
loop to **silently delete content from the skill tree** on auto-apply —
irreversible destructive edits to files outside the project repo. Today the
function is asserted by one happy-path commit (`6ab776d evolve: phase C —
auto-apply pure-addition template-drift via patch -p0`) with zero adversarial
coverage. That asymmetry justifies extraction at exactly this surface.

`derive_suggestions` (330+ lines, the cross-proposal aggregator that decides
WHICH advisory becomes a `PatchSuggestion`) is deferred to phase 2 — the
proposal-JSON-coupling makes extraction non-trivial and the inner safety
check is what protects the loop today regardless.

---

## 1. Why this exists

evolve.rs auto-applies suggestions in two flavors:

| Suggestion type | Auto-apply mechanism | Safety check |
|---|---|---|
| `Suggestion` (append-only) | append lines to target file | construction-time: build only emits append-only suggestions |
| `PatchSuggestion` (in-line) | `patch -p0 -i <diff>` to target file | `is_pure_addition_diff(&diff_body)` must return true |

The `Suggestion` path is safe by construction — `apply_suggestion` only
calls `append`. The `PatchSuggestion` path is safe only if
`is_pure_addition_diff` correctly identifies diffs that have no `-` lines
in their hunk bodies. If it returns `true` for a diff with `-` lines, the
system runs `patch -p0` and silently removes lines from the skill tree.
Subsequent runs would produce different scaffolds for downstream projects.

The current implementation:

```rust
fn is_pure_addition_diff(diff_body: &str) -> bool {
    let mut in_hunk = false;
    for line in diff_body.lines() {
        if line.starts_with("@@") { in_hunk = true; continue; }
        if !in_hunk { continue; }
        if line.starts_with("---") || line.starts_with("+++") { in_hunk = false; continue; }
        if line.starts_with('-') { return false; }
    }
    true
}
```

Implicit assumptions worth testing:
- An unterminated hunk header followed by `-` somewhere → caught
- Multi-hunk diffs with `+` in hunk 1 and `-` in hunk 2 → caught
- A `---` line ENDS the hunk (file-header convention) — but what if the
  next line is `+++` then `@@` again? Multi-file diffs need to keep
  tracking.
- Empty hunk body → return `true`? Probably yes (vacuously).
- A line that starts with `-` but is followed by `--- a/foo` (file
  header inside a hunk body)? Shouldn't happen in a valid unified diff
  but parsers are paranoid.

The fingerprint functions need:
- Determinism: same input → same output, byte-for-byte
- Avalanche: changing any byte changes the digest (sha256 gives this)
- Collision-resistance for realistic inputs (sha256 gives this)
- For `Suggestion`: fingerprint over `(target_path, appended_lines)` —
  reordering lines must change the fingerprint
- For `PatchSuggestion`: fingerprint over `(target_path, diff_body)` —
  changing any byte of diff body must change the fingerprint

---

## 2. Public surface

```rust
/// Decide whether a unified-diff body is safe to auto-apply.
/// Safe means: no `-` lines inside any hunk body.
pub fn is_pure_addition_diff(diff_body: &str) -> bool;

/// Compute a stable fingerprint for an append-only suggestion.
pub fn append_fingerprint(target: &Path, appended_lines: &[String]) -> String;

/// Compute a stable fingerprint for an in-line patch suggestion.
pub fn patch_fingerprint(target: &Path, diff_body: &str) -> String;
```

That's it. Three pure functions, no types beyond what the caller already
has. The existing `Suggestion::fingerprint` and `PatchSuggestion::fingerprint`
methods inside the bin become one-liners delegating to these.

---

## 3. Acceptance criteria

All MUST. Unfakeable scalar `evolve_safety_invariants_passing` (target=6).

### AC1 (MUST) — pure-addition: positive cases

Hand-crafted diffs that ARE pure additions return `true`:
- Single hunk, only `+` and ` ` lines
- Multiple hunks (multi-file diff) all pure-addition
- Empty hunk body
- Diff with no hunks at all (`---`/`+++` headers but no `@@`)

**Test:** `tests/acceptance_ac1.rs`

### AC2 (MUST) — pure-addition: negative cases (load-bearing)

Hand-crafted diffs that contain `-` lines anywhere in any hunk body return
`false`. Includes:
- Single hunk with one `-` line
- Multi-hunk diff with `-` only in hunk 2
- Multi-file diff with `+` in file 1, `-` in file 2
- A `-` line immediately after `@@`
- A `-` line immediately before the next file header

**Test:** `tests/acceptance_ac2.rs`

### AC3 (MUST) — pure-addition: proptest invariant

For any diff body generated by mixing random `+`, `-`, ` ` lines inside a
synthesized `@@` hunk header, `is_pure_addition_diff` returns `true` iff
no line inside any hunk body starts with `-`. The implementation and the
spec-from-proptest agree on every generated sample.

**Test:** `tests/acceptance_ac3.rs`

### AC4 (MUST) — fingerprint determinism

For any `(target, appended_lines)` pair, `append_fingerprint` returns the
same string twice. For any `(target, diff_body)` pair, `patch_fingerprint`
returns the same string twice. Both fingerprints are 64-char lowercase hex.

**Test:** `tests/acceptance_ac4.rs`

### AC5 (MUST) — fingerprint sensitivity (proptest)

For any pair of `(target_a, lines_a)` and `(target_b, lines_b)` that
differ in any byte of any component, `append_fingerprint(a) !=
append_fingerprint(b)`. Same for patch_fingerprint. Reordering the lines
in `appended_lines` changes the fingerprint (order matters for digest
binding, not for semantic equality).

**Test:** `tests/acceptance_ac5.rs`

### AC6 (MUST) — parent integration (post-merge, env-gated)

After subtree-merge into `autobuilder/crates/evolve-safety/` and shim of
the in-bin call sites, `scripts/run-metrics.sh` on the parent autobuilder
repo still reports `ac_passing_count: 7`. Same two-key env-var gate as
prior crates (`AUTOBUILDER_PARENT_REPO` + `AUTOBUILDER_EVOLVE_SAFETY_AC6_RUN_INTEGRATION`).

**Test:** `tests/acceptance_ac6.rs`

---

## 4. Hard constraints

- `rust_edition = "2024"`
- `target_kind = "lib"`
- `deny_unsafe = true`
- `max_deps = 1` — only `sha2` is needed
- `msrv = "1.85"`
- `max_lib_lines = 100` — the entire lib is three pure functions

---

## 5. Five whys

1. **Why extract this scope and not all of evolve.rs?** The auto-apply safety
   check is the load-bearing decision; `derive_suggestions` is policy logic
   that affects WHICH suggestions get considered, not whether the apply
   itself is safe. Extracting the safety primitives lets the proptest cover
   the load-bearing decision without taking on the proposal-JSON-coupling
   that makes the derive function hard to test in isolation.
2. **Why /autobuilder vs hand-extract?** Same reason as PRD-receipt and
   PRD-gate: the discipline forces falsifiable ACs and proptest coverage.
   Here AC3's proptest is the load-bearing artifact — generate random
   diff bodies, prove the implementation agrees with a spec computed
   line-by-line.
3. **Why fingerprints in the same crate as the diff check?** Both belong to
   the safety/identity surface of the auto-apply path. Coupling them in one
   crate matches the consumer's mental model: "the bits evolve.rs uses to
   decide whether to apply and to dedupe."
4. **Why keep `derive_suggestions` in the bin for now?** It's 330+ lines of
   business logic coupled to the proposal JSON shape. Extracting it cleanly
   requires either a custom proposal struct (more refactor) or accepting
   `serde_json::Value` everywhere (less invariant strength). Defer to
   phase 2 when the cost/benefit shifts.
5. **Why expose three free functions instead of methods?** The in-bin
   `Suggestion` and `PatchSuggestion` structs stay private to the bin;
   exposing them in the lib would force serde derives + visibility shuffling.
   Three free functions taking `&Path` and `&[String]` is the minimum
   coupling.

---

## 6. Non-goals

1. Extracting `derive_suggestions` (deferred, see five-whys #4).
2. Extracting `load_proposals`, `apply_*`, `write_*`, `git_commit_if_repo`
   (all IO-coupled; orchestration glue stays in the bin).
3. Changing the digest scheme (sha256 hex byte-for-byte preserved).
4. Adding new safety checks (e.g. "diff must not exceed N lines") — out of
   scope for v0.1.
5. A CLI exposing the safety checks from the command line — deferred.

---

## 7. Unfakeable scalar

```json
{
  "name": "evolve_safety_invariants_passing",
  "lower_is_better": false,
  "harness_command": "scripts/run-metrics.sh",
  "target": 6
}
```

---

## 8. Phasing

| Phase | Scope |
|-------|-------|
| 0 | PRD + intent-card + scaffold. Baseline iter. |
| 1 | Edit-agent migrates `is_pure_addition_diff` (verbatim), implements `append_fingerprint` and `patch_fingerprint` (derived from existing impl), fills 6 ACs. |
| 2 (deferred) | Extract `derive_suggestions`. Requires writing a typed proposal struct first or accepting `serde_json::Value` in the lib. Scope its own PRD. |
| Phase-4 (this PRD) | Subtree-merge into `autobuilder/crates/evolve-safety/`. Shim three call sites in `autobuilder/src/evolve.rs` (the existing `Suggestion::fingerprint`, `PatchSuggestion::fingerprint`, and `is_pure_addition_diff` invocation in `derive_suggestions`). Verify parent harness still 7/7. |
