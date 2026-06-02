# PRD: Embeddable ML Functions (codename: *morsel*)

**Author:** Claude (Opus 4.7), for me
**Status:** Draft v0.1
**Date:** 2026-05-22
**Worked example:** an embedded RNN that identifies cats from short audio clips.

---

## TL;DR

I keep wanting to drop a tiny ML decision — "is this a cat?", "is this prose or code?", "which of three buckets does this metric belong in?" — into a Rust binary I'm already shipping, and the answer today is: bring in `candle` + a 200MB model file + a runtime dependency on huggingface, or write a brittle regex. `morsel` is the missing middle: a Rust crate that lets a model author bake a small, pre-trained model directly into a single `.rs` file (weights as `const` arrays), and consumers call it as a pure function with no allocation, no IO, no runtime. One `cargo add morsel`, one `use cats::is_cat;`, done. The worked example in this PRD is an embedded RNN that takes 1 second of audio and returns `IsCat(p: f32)`. Pure CPU, single-threaded, ~200KB of `const` data.

---

## 1. Why this exists

1. **The middle is empty.** For "real" ML in Rust there's `candle`, `burn`, ONNX runtime, tract, `tch`. For "no ML" there's `if x.contains("meow")`. There's no good answer for "I want a 50-neuron LSTM compiled into my CLI so the binary is still 8MB."
2. **fastembed-rs proved the pattern works.** It's in-process, no daemon, single binary. But it ships one model (BGE) and is opinionated about embedding-only. `morsel` generalizes: same pattern, but the model author defines the input/output and the model is the source code.
3. **I already want this for things I'm building.** `/self-review` could route diagnostic-text-classification through a 5KB classifier instead of asking the LLM. `episode` could detect "user-redirect-within-3-turns" with a logistic-regression head instead of keyword matching. Both are exactly the size where ML beats heuristics and nothing-fancy beats ML.
4. **Weights as code, not assets.** The thing that makes embedded ML annoying is the `.bin` file you have to ship alongside the binary. `morsel`'s premise: at ≤1MB of weights, just inline them as `const [[f32; N]; M]` arrays. The compiler is fine. The binary grows by the size of the weights, which is the actual cost anyway.

---

## 2. Who this is for

- **Me, writing Rust CLIs.** Most of my tool ideas have a tiny ML-shaped subproblem inside.
- **Anyone shipping a Rust binary who wants ML without the deployment story.** Embedded systems, distro packagers, CLI-tool authors, library authors who don't want to drag huggingface into their dep tree.
- **Not** for: training, fine-tuning, anything >5M parameters, anything that benefits from a GPU, or anything that needs to swap models at runtime.

---

## 3. What I'd use it for (concretely)

| Subproblem (in something I'm building) | Model shape | Why it beats heuristics |
| --- | --- | --- |
| `episode`: detect "user redirect within 3 turns" | LogReg over bag-of-tokens, 32-feature input | Catches "wait", "actually no", "stop, that's wrong" without enumerating phrases |
| `/self-review`: classify a journal paragraph as `finding | applied | pending | notable` | tiny MLP, 4-class | Frees me from regexing my own writing |
| `spool`: predict skill-invocation outcome from args + duration + history | 1D-CNN, 3-class | Generalizes from a couple hundred labeled invocations |
| `apipe`: route a message to the right peer-agent | k-NN over learned 16-dim embeddings | Adapts as I add new agents; no router rules |
| **Worked example:** identify cats in 1s of audio | small LSTM, 64 hidden, mel-spectrogram input | Wakes a doorbell when a stray meows on the porch; runs on a Pi |

The cat-RNN is the load-bearing example because: (a) the user asked for it explicitly, (b) audio-as-sequence is the canonical RNN shape, (c) it's a small enough model to be a credible test of the `const`-weights approach, and (d) it's adorable.

---

## 4. Functional requirements

### 4.1 Anatomy of a morsel

A "morsel" is one Rust crate produced by the model author. Public surface is exactly one function:

```rust
// crate: cat_meow
pub fn is_cat(audio_samples: &[f32; 16000]) -> f32;
//                  ^ 1s at 16kHz mono                ^ probability in [0,1]
```

Internals:

```rust
// cat_meow/src/lib.rs (the only file the consumer pulls in)
use morsel::nn::Lstm;

mod weights;   // generated; ~200KB of `pub const W_*: [[f32; …]; …]`
mod features;  // mel-spectrogram extraction, ~80 lines, no_std-friendly

pub fn is_cat(audio: &[f32; 16000]) -> f32 {
    let frames = features::log_mel(audio);                 // [N_frames × 40]
    let h = Lstm::new(&weights::LSTM_W, &weights::LSTM_B)
        .run(&frames);                                     // final hidden state [64]
    morsel::nn::dense_sigmoid(&h, &weights::HEAD_W, weights::HEAD_B)
}
```

No `Box<dyn Trait>`, no allocator, no IO, no panics in the happy path. `#[no_std]`-compatible if `features` is.

### 4.2 What `morsel` (the crate) ships

A tiny library of layer primitives that consumer crates depend on:

| Primitive | Notes |
| --- | --- |
| `Linear` / `Dense` | `y = Wx + b` |
| `Sigmoid`, `Tanh`, `ReLU`, `Softmax` | Activations |
| `Lstm`, `Gru` | Single-layer RNN cells, scan over a slice of input frames |
| `Conv1d` | 1D causal convolution for short signals |
| `Embedding` | Lookup table; `u32 → [f32; D]` |
| `LogMel`, `Mfcc` | Audio preprocessing for the worked example |
| `Argmax`, `KnnL2` | Small classification heads |

All implementations are scalar Rust by default; behind feature flags, SIMD (`std::simd` or `wide`) and a single-allocation arena for activations.

### 4.3 What `morsel` (the tool) does at training time

`morsel` is also a CLI that the *model author* runs once, offline, to turn a trained model (PyTorch / candle / safetensors) into a Rust source file:

```
morsel bake \
    --in model.safetensors \
    --arch lstm \
    --out crates/cat_meow/src/weights.rs \
    --quant q8        # optional: f32 → int8 with per-tensor scale, 4× smaller
```

Output is a `weights.rs` containing:

```rust
pub const LSTM_W_IH: [[f32; 160]; 256] = [[..; 160]; 256];
pub const LSTM_W_HH: [[f32; 64];  256] = [[..; 64];  256];
pub const LSTM_B:    [f32; 256]        = [..];
pub const HEAD_W:    [f32; 64]         = [..];
pub const HEAD_B:    f32               = ..;
pub const ARCH_FINGERPRINT: &str = "lstm-64h-40mel-v1";
```

The fingerprint is checked at consumer compile-time against the `morsel::nn::Lstm` config used in `lib.rs`. If the model author changes the architecture without rebaking, the consumer crate fails to compile with a `const_assert!` mismatch — not a runtime error.

### 4.4 Consumer story

```toml
# Cargo.toml of the consumer
[dependencies]
morsel  = "0.1"     # ~30KB of primitives
cat_meow = "0.1"    # ~200KB of weights + 80 lines of glue
```

```rust
use cat_meow::is_cat;
let p = is_cat(&samples_1s);
if p > 0.7 { ring_doorbell(); }
```

That's the whole API. No init, no `Engine::new()`, no model load.

### 4.5 Performance contract

`morsel` ships a default benchmark suite (`cargo bench`) that each consumer crate inherits. The contract a baked model promises:

- **Inference is allocation-free.** Activations live in a stack-sized `[f32; N]` arena chosen at bake time.
- **Inference is panic-free** on any input of the declared shape. (`debug_assert!` checks on shape; release builds trust the type system.)
- **Inference is deterministic.** Same input → same bit-for-bit output, on any CPU `morsel` supports.

The cat model target: <2ms per inference on a Raspberry Pi 4, <50µs on a recent x86_64.

---

## 5. Architecture

```
morsel/
├── crates/
│   ├── morsel/             # the layer primitives crate (consumed by every model)
│   ├── morsel-bake/        # the CLI: safetensors → weights.rs
│   └── morsel-macros/      # proc-macro: const-asserts arch fingerprint
└── examples/
    └── cat_meow/           # the worked example end-to-end
        ├── train/          # PyTorch script; not shipped to consumers
        ├── data/           # ESC-50 cat clips + non-cat negatives
        ├── src/
        │   ├── lib.rs      # is_cat()
        │   ├── features.rs # log_mel()
        │   └── weights.rs  # GENERATED — do not edit
        └── tests/
            └── golden.rs   # 50 labeled clips; assert classification holds
```

The split mirrors `fastembed-rs`/`hf-hub`'s split between "library code" and "model artifact" — except the model artifact lives in source code, in a crate, distributed via crates.io like anything else.

---

## 6. Non-goals

1. **Training in Rust.** Models are trained externally (PyTorch, JAX, candle). `morsel` is inference-only.
2. **Big models.** Anything where weights >5MB belongs in `candle` / ONNX. The const-array story falls over there (compile time, rustc memory, binary size).
3. **GPU inference.** Pure CPU. Adding a GPU backend kills the "embed in one binary" promise.
4. **Dynamic model loading.** If you want to swap models at runtime, you want a different tool.
5. **Quantization research.** v0.1 supports f32 and naive int8 with per-tensor scale. No GPTQ, AWQ, or k-quants.
6. **A model zoo.** `morsel` ships layer primitives + the cat example. Other models are downstream crates anyone can publish.

---

## 7. Phasing

| Phase | Scope |
| --- | --- |
| 0 | `morsel` crate with `Linear`, `Sigmoid`, `Tanh`, `Lstm`. Hand-written `weights.rs` for a toy XOR-LSTM. Property test: matches a PyTorch reference within 1e-5. |
| 1 | `morsel-bake` CLI reading safetensors → emitting `weights.rs`. Round-trip test: load → bake → run, output matches PyTorch within 1e-4. |
| 2 | The cat example end-to-end. Train an LSTM on ESC-50 cats + UrbanSound negatives, bake, ship `cat_meow` crate, document the workflow. |
| 3 | `Conv1d`, `LogMel`, int8 quantization, `morsel-macros` const-assert fingerprinting. |
| 4 | SIMD backends behind a feature flag (`portable_simd`). Benchmark against `candle` on the same workload. |
| 5 | Convert one of my own internal use cases (probably `episode`'s redirect detector) and measure: did this reduce the heuristics-vs-LLM gap I'm currently splitting? |

---

## 8. Risks

- **Compile-time blowup.** A 5MB `const [[f32; …]; …]` array could make rustc unhappy. *Mitigation:* benchmark at phase 1; if rustc chokes above ~500KB, add an `include_bytes!` fallback path that costs us the "weights are code" purity but keeps the deployment story (one binary, no asset file).
- **Inference correctness drift.** Floating-point ops don't associate; a baked model can diverge subtly from its PyTorch original. *Mitigation:* golden tests at bake time and at consumer compile time; document the 1e-4 tolerance as part of the contract.
- **The middle stays empty for a reason.** Maybe no one wants tiny embeddable ML because LLMs ate that niche. *Mitigation:* the cat example is concrete, the `episode`-redirect-detector use case is mine, and the bar for "useful to me" is much lower than "useful to a market."
- **Quantization is a tar pit.** Int8 with per-tensor scale is the simplest possible quantization and is often not enough. *Mitigation:* keep it as a v0.1 feature flag, accept that q8 is "free and usually fine"; don't promise more.
- **The RNN-for-cats example invites the "you should have used a CNN" critique.** It's deliberate: a 1s audio clip *is* a sequence, RNNs are a fine fit for sequences, and demonstrating an RNN exercises the LSTM primitive which is the hardest one to get right. A CNN version would be a 30-line follow-on.

---

## 9. Open questions

1. **Is "weights as Rust source" the right abstraction, or should v0.1 just `include_bytes!` a tiny binary blob?** Source-as-weights buys us const-time fingerprint checking and crates.io distribution; `include_bytes!` buys us faster compile times and trivially-larger models. I lean source-as-weights for ≤1MB models, blob above that.
2. **Should `morsel` expose a `#[derive(Morsel)]` for ergonomic forward passes?** Tempting, but proc-macros are friction for a v0.1. Stay procedural until I feel the pain.
3. **`no_std` from day one or eventually?** Day one if cheap. The only stdlib dependency in the primitives is `f32::exp`, which is in `core` under `libm`.
4. **Crates.io or git-only for v0.1?** Probably crates.io once the API stabilizes. The `cat_meow` crate doubles as the worked example and the integration test.
5. **Does `morsel` cooperate with `recall`?** Probably not directly — recall uses `fastembed-rs` for its embedder, which is the right tool at recall's scale (BGE is 33M params). If `morsel` ever beats `fastembed-rs` on a tiny custom embedder, recall could swap in; not a v0.1 concern.
