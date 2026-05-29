# PRD: wm-verify — taste the dish before it reaches her

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/thrift.md
**build_target:** rust-lib
**build_into:** /home/jsy/wintermute/wm-verify
**Depends on:** (none — pure, in-process; consumed by brain-backend-ladder)
**Codename:** *taster* — the one who checks the plate before it's served.

## TL;DR

The thrift switching strategy is local-first: a 3B model answers most turns. The
risk isn't the 3B model *failing* (that's caught by `wm-local-llm`'s `Escalate`)
— it's the 3B model **confidently answering wrong or weirdly**, which a
failure-only ladder is blind to. `wm-verify` is the cheap, instant, in-process
quality gate that inspects a generated answer *before it is spoken* and returns
`Accept` or `Reject{reason}`. A reject is the signal for `brain-backend-ladder`
to climb a rung. No network, no model call, no clock dependence — pure heuristic
+ optional self-consistency, fast enough to run on every local turn.

## 1. Why this exists

- **The strategy needs a soft-failure detector.** thrift's switching strategy
  (vision, "Switching strategy", mechanism C) requires deciding "is this local
  answer good enough to speak?" — distinct from "did generation fail?"
  (`wm-local-llm`'s job). Without it, local-first means speaking unverified 3B
  output to an elderly user. With it, the cheap tier is safe to default to.
- **It must be near-free or local-first is a latency loss.** Running a cloud
  judge on every local turn would erase the cost saving and blow the voice
  latency budget. The gate must be heuristic and instant.
- **Conservative in the safe direction.** A false reject = an unnecessary climb
  (costs a little money/latency). A false accept = a bad answer spoken to jsy's
  mother. The gate leans toward `Reject` when uncertain — but not so hard it
  always escalates (that would defeat local-first). One tunable threshold.

## 2. What this builds

A pure library crate at `~/wintermute/wm-verify/` (no async, no I/O).

```rust
pub struct VerifyCtx<'a> { pub utterance: &'a str, pub expected_lang: Lang, pub min_confidence: f32 }
pub enum Verdict { Accept, Reject { reason: RejectReason } }
pub enum RejectReason { Empty, Refusal, Looping, WrongLanguage, Disclaimer, NonAnswer, Inconsistent }
pub fn verify(answer: &str, ctx: &VerifyCtx) -> Verdict;
pub fn verify_consistency(samples: &[&str], ctx: &VerifyCtx) -> Verdict; // self-consistency over N samples
```

### 2.1 Heuristic checks (instant, ordered cheap→dear)

- **Empty / whitespace-only** → `Reject(Empty)`.
- **Refusal patterns** — "I'm just an AI", "I cannot", "as an AI language model",
  "I don't have the ability" — a companion must not deflect like a chatbot →
  `Reject(Refusal)`.
- **Looping / degeneration** — n-gram repetition ratio above a threshold (small
  models loop) → `Reject(Looping)`.
- **Wrong language** — detected language ≠ `expected_lang` (lightweight, pure-Rust
  detection) → `Reject(WrongLanguage)`.
- **Inappropriate boilerplate/disclaimer** — medical/legal disclaimers, "consult
  a professional" canned tails inappropriate for companion chit-chat →
  `Reject(Disclaimer)`.
- **Non-answer** — answer is empty-of-content relative to a question (e.g. only a
  greeting back to a real question; heuristic length/structure check) →
  `Reject(NonAnswer)`.

### 2.2 Self-consistency (optional, caller supplies samples)

`verify_consistency(samples)` — when the ladder generated >1 local sample (cheap
on a 3B), reject if they disagree materially (`Inconsistent`), accept if they
converge. This catches confident hallucination that single-sample heuristics
miss. The caller decides when the extra sample is worth it.

### 2.3 Non-goals

- It does NOT judge factual correctness against ground truth (no knowledge base,
  no cloud call) — it catches *shape* failures, not *truth*. Truth-sensitive and
  safety-flagged turns are handled by `wm-router`'s safety pre-route (start
  cloud), not by this gate.
- It does NOT call any model or network.

## 3. Acceptance criteria

1. `verify("", …)` and whitespace-only → `Reject(Empty)`.
2. Refusal-pattern fixtures ("I'm just an AI…", "I cannot help with that") →
   `Reject(Refusal)`; a normal helpful answer with the word "can" in it is NOT
   falsely rejected (no naive substring over-trigger) — proven with a paired
   fixture.
3. A degenerate looping output (repeated n-grams above threshold) →
   `Reject(Looping)`; normal repetition (a repeated short word in fluent text) is
   accepted.
4. An answer in the wrong language vs `expected_lang` → `Reject(WrongLanguage)`;
   correct-language answer accepted.
5. A canned medical/legal disclaimer tail → `Reject(Disclaimer)`.
6. **No false reject on good answers:** a committed fixture set of ≥15 normal,
   helpful, on-topic companion answers ALL return `Accept` (the local-first
   guarantee — the gate must not nuke the cost saving by over-escalating).
7. `verify_consistency` rejects materially divergent samples (`Inconsistent`) and
   accepts converging ones — proven with two fixture pairs.
8. The threshold (`min_confidence` / repetition cutoff) is configurable; verdicts
   carry a typed `RejectReason`; `verify` is deterministic (same input → same
   verdict; no clock, no RNG, no network) and total (never panics on any
   `&str`, including non-UTF8-boundary-safe slicing — proven with adversarial
   inputs: emoji, very long strings, control chars).
9. `cargo test` green; `cargo clippy -D warnings` clean (new crate, high bar);
   MSRV 1.85; no let-chains; no `unwrap`/`expect`/`panic` in `src/`.
