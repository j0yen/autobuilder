# PRD: cradle ↔ morsel bake integration (`cradle bake`)

**Author:** Claude (Opus 4.7) for Joe Yen
**Status:** Draft v0.1
**Date:** 2026-05-27
**Depends on:** [[cradle]] v0.1 (harvest + features + train shellout), [[morsel]] v0.1 (`morsel bake`)
**Worked example:** baking `models/redirect/checkpoint.safetensors` → `crates/morsel-redirect/src/weights.rs`

---

## TL;DR

[[cradle]] v0.1 shipped the *harvest + train-shellout* core of the
PRD-cradle pipeline. The `cradle bake` subcommand was deliberately
stubbed (returns a typed `BakeDeferred` error pointing at this PRD)
because [[morsel]] had just shipped on the same day and the bake CLI
surface was not yet load-bearing for any consumer.

This PRD lands the bake step. After it ships, `cradle build redirect`
executes the full PRD-cradle.md pipeline end-to-end:

```
~/.claude/projects/**/*.jsonl
     │ cradle harvest
     ▼
models/redirect/data/{train,val,test}.jsonl
     │ cradle train (uv run python train.py)
     ▼
models/redirect/checkpoint.safetensors
     │ cradle bake ← NEW
     ▼
output/morsel-redirect/src/weights.rs (const-baked tensors)
     │
     ▼
consumer's Cargo.toml: morsel-redirect = "path/to/output/morsel-redirect"
```

---

## 1. Why this exists

1. **morsel bake is the missing arrow.** PRD-cradle.md §4.4 explicitly
   delegated baking to `morsel bake`. v0.1 of cradle ships without that
   arrow because morsel landed on the same day; coupling cradle's
   first release to morsel's CLI surface would have multiplied churn
   risk. Now morsel is settled, the arrow can land.

2. **Receipt 7 (held-out accuracy) needs an end-to-end pipeline.**
   PRD-cradle §4.5 calls for a receipt-gated bake. That gate is only
   useful once the bake step actually exists; v0.1's `BakeDeferred`
   stub kept the receipt theoretical.

3. **First consumer (`episode`) is waiting.** PRD-cradle §4.6 cites
   `episode/src/turn_observer.rs` as the canonical consumer for
   `morsel-redirect`. Episode can't `cargo add` the crate until the
   bake step produces it.

---

## 2. What ships in this PRD

| Surface | Status before | Status after |
| --- | --- | --- |
| `cradle bake <model>` subcommand | returns `BakeDeferred` error | shells out to `morsel bake`, writes baked crate |
| `cradle build <model>` "phase 2 deferred" notice | printed on stdout | replaced with `cradle: build <model> bake ok` |
| `models/<name>/spec.toml::bake.arch` field | not parsed | parsed; passed as `morsel bake --arch <arch>` |
| `models/<name>/spec.toml::bake.quant` field | not parsed | parsed; passed as `morsel bake --quant <quant>` |
| Output crate location | n/a | `output/morsel-<name>/` (gitignored under cradle root) |
| Receipt 7 (model accuracy gate) | not wired | reads `models/<name>/metrics.json::test_accuracy`, asserts `≥ spec.threshold` |

---

## 3. Functional requirements

### 3.1 The bake step

`cradle bake <model>` shells out to `morsel bake` with stable args:

```
morsel bake \
  --in models/<name>/checkpoint.safetensors \
  --arch <spec.bake.arch> \
  --quant <spec.bake.quant> \
  --out output/morsel-<name>/
```

cradle does NOT re-implement the bake logic; it owns the CLI surface
and the error reporting, morsel owns the codegen.

### 3.2 Receipt 7: held-out accuracy

After the train step writes `models/<name>/metrics.json`, the
existing `cradle bake` adds one gate:

```
test_accuracy = read(models/<name>/metrics.json).test_accuracy
threshold     = read(models/<name>/spec.toml).threshold
if test_accuracy < threshold:
    return AccuracyBelowThreshold(test_accuracy, threshold)
```

The autobuilder gate for any *downstream* consumer that pins
`morsel-<name>` gets a free quality signal via this receipt — the
weights it depends on were proved to clear the spec's accuracy bar.

### 3.3 Spec.toml extension

```toml
# models/redirect/spec.toml (after this PRD)
name = "redirect"
input_shape = "turn_pair_v1"
label_source = "redirect_v1"
threshold = 0.85
auc_threshold = 0.85

[bake]
arch  = "logreg"     # or "tiny_mlp" — must match morsel's --arch enum
quant = "q8"         # or "f32" — must match morsel's --quant enum
crate_name = "morsel-redirect"
```

If `[bake]` is absent, `cradle bake <model>` returns a clear error
naming the missing field rather than guessing.

### 3.4 The `cradle build <model>` post-condition

After this PRD ships:

```
$ cradle build redirect
cradle-harvest: pos=42 neg=42 sessions=12 turns=2113 split=8/2/2
cradle: build redirect harvest ok
cradle: build redirect train ok
cradle: build redirect bake ok → output/morsel-redirect/
cradle: build redirect: receipt 7 (test_accuracy=0.91 >= 0.85)
```

Exit code 0. Any stage failure short-circuits with the stage name in
the error message.

---

## 4. Acceptance criteria (sketch — finalized at intent-card time)

- AC1: `cradle bake redirect` invokes `morsel bake` with arch/quant
  from spec; failure path surfaces morsel's stderr verbatim.
- AC2: `cradle bake redirect` returns `MetricsBelowThreshold` when
  `metrics.json::test_accuracy < spec.threshold`.
- AC3: `cradle bake redirect` returns `BakeSpecMissing` when
  `[bake]` table is absent from spec.toml.
- AC4: `cradle build redirect` runs all three stages and short-circuits
  on stage failure with the stage name in the error.
- AC5: Generated `output/morsel-<name>/Cargo.toml` declares
  `morsel-<name>` and depends on `morsel` (the runtime crate).
- AC6: Generated `output/morsel-<name>/src/weights.rs` compiles under
  the same workspace lint posture as cradle (no warnings, no unsafe).
- AC7: Re-running `cradle bake <model>` produces bit-identical output
  (determinism gate for downstream).

---

## 5. Non-goals

1. **Re-implementing morsel bake.** This PRD wires cradle to morsel;
   it does not duplicate logic.
2. **Training in Rust.** Train still shells out to Python via uv.
3. **Other models in the bake-supported set.** v0.1.1 bakes only
   `redirect` to validate the wiring. session-productivity and
   playbook-match get their own PRDs once their label extractors land.
4. **Auto-publishing baked crates to crates.io.** Output crates live
   in `output/` under cradle root, consumer pins via path.
5. **Online / runtime re-bake.** Bake is offline, one-shot.

---

## 6. Risks

- **morsel CLI surface drift.** If morsel bumps its `--arch` enum
  vocabulary, cradle bake breaks. *Mitigation:* pin morsel's exact
  version in cradle's Cargo.toml (not crates.io, but the morsel-bake
  binary's expected version recorded in a constant).
- **Receipt 7 is gameable.** A trivially-perfect model on imbalanced
  data hits high accuracy. *Mitigation:* also assert `auc_threshold`
  (already in spec.toml v0.1, just not yet read).

---

## 7. Phasing

| Phase | Scope |
| --- | --- |
| **0** | Intent-card + 7 ACs above. Scaffold via `/autobuilder`. |
| **1** | Wire bake shellout. `cradle bake redirect` happy path. |
| **2** | Receipt 7 wiring. `cradle build redirect` end-to-end with metrics gate. |
| **3** | Determinism receipt for baked output (re-bake → bit-identical). |
| **4** | First consumer integration: `episode/src/turn_observer.rs` adds `morsel-redirect = { path = "..." }` and calls `redirect_probability()`. |

---

## 8. Relationship to other PRDs

- **[[cradle]]** — this PRD strictly extends cradle. No replacement,
  no fork.
- **[[morsel]]** — consumed via `morsel bake`. No morsel changes
  required.
- **[[episode]]** — first consumer of the baked `morsel-redirect`
  crate. Episode's own PRD references this one once both exist.
- **autobuilder** — gates the bake step's receipt 7 like any other
  receipt; no autobuilder changes required.
