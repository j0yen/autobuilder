deferred_acs: [3, 4, 6, 7]
<!--
Deferred-AC justifications (live/hardware/asset-gated; the build agent does
not self-author wake fixtures per AC3's "no self-fixture" rule):
  AC3 (held-out positive fires): needs an independently-recorded wake clip with
    provenance, which the no-self-fixture rule forbids the build agent from
    authoring — asset-gated. The positive-fire path is instead certified by
    AC6's live human gate: jsy confirmed it on 2026-06-04 ("wintermute, what
    time is it" -> wm.audio.wake at confidence 0.99).
  AC4 (held-out negatives stay silent): requires a provenance-bearing >=30 s
    room-tone + non-wake-speech corpus the no-self-fixture rule forbids the
    build agent from synthesizing; false-accept measurement is a live/recorded
    step for jsy, not CI-deterministic.
  AC6 (live human gate): PENDING-USER by the PRD's own wording (explicitly
    user-certified, like skill-doctor AC7/AC11). Confirmed live by jsy on
    2026-06-04: a spoken "wintermute" produced a wm.audio.wake envelope within
    the 500 ms budget at 0.99 confidence.
  AC7 (daemon health): wm-audio active 60 s post-restart, NRestarts=0, UDS
    subscribers still read full-rate PCM — a runtime check requiring the running
    daemon plus a restart on the user's box; hardware/runtime-gated, not CI.
-->

# PRD: wintermute-audio — wake-word mel front-end + model provisioning repair

**Author:** Claude Opus 4.8 (crash-recovery diagnosis 2026-06-02), for jsy
**Status:** Draft v0.1
**Date:** 2026-06-02
**Vision:** visions/companion.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-audio
**build_version_bump:** minor
**Supersedes (broken parts of):** PRD-wintermute-audio-inference (commit 7a6aa1c), PRD-rouse-wake-vad-models (commit c658e90)
**Codename:** *features* — the detector was handed raw waveform and asked to recognize a spectrogram.

## TL;DR

`wm-audio` ships an ONNX wake path, but **wake-word detection has never actually fired**, for three compounding reasons found during crash recovery:

1. **Contract mismatch (the wall).** `OnnxWakeDetector::process` (`src/inference.rs`) feeds the model a raw-PCM tensor `[1, WAKE_WINDOW_SAMPLES]` = `[1, 1280]` (80 ms @ 16 kHz). Every real microWakeWord model — **and** the locally-trained `out-smoke/wintermute.onnx` — expects **mel features `[1, 186, 40]`** (186 frames × 40 log-mel bins). There is **no feature-extraction step** anywhere in the wake path. With a real model loaded, `sess.run` shape-mismatches → `NotDetected` every frame.
2. **Provisioning broken.** `wm-audio fetch-models` can't install anything: the wake URLs 404 (`kahrendt/microWakeWord` was renamed to `OHF-Voice/micro-wake-word`; the `okay_nabu_v0.1` release assets are gone) and the `silero_vad` manifest `sha256` is a placeholder (`6d7e0f1a2b3c4d5e…`) that never matches the real download.
3. **(Already fixed during recovery, record-only.)** The installed `~/.cargo/bin/wm-audio` was a pre-fetch-models May-28 build; rebuilt v0.6.0 at HEAD and reinstalled. No PRD work needed — listed so the build doesn't re-chase it.

This PRD adds the missing mel front-end so the detector input matches the model contract, wires the locally-trained model as the default wake model, and repairs `fetch-models` so VAD (and any future upstream wake model) provisions with real, verified artifacts.

## 1. Why this exists

- Talking at the laptop produces **zero** `wm.audio.wake` envelopes. Verified 2026-06-02: model dirs empty → `NullWakeDetector` (never fires); and even after provisioning a model, the `[1,1280]`-vs-`[1,186,40]` mismatch keeps it at zero.
- The prior inference PRD's AC3/AC6 ("wake fires on a synthetic sample" / "live human gate") **could not have genuinely passed** against a real microWakeWord model — they passed against agent-written fixtures or a never-loaded model. This is the [[agent-written-fixtures-tautology]] failure mode; this PRD's ACs are written to be validated on **independent, held-out audio the build agent did not author.**
- The companion vision gates STT and dialog on wake. Until wake fires, the whole input chain is dark downstream of capture (capture itself works — pw-record child live, 16 kHz mono confirmed).

## 2. What this builds

### 2.1 Mel feature front-end (the core fix)

Add a feature extractor that converts the rolling PCM window into the **exact** `[1, 186, 40]` log-mel representation the model was trained on. The feature config is NOT free to choose — it MUST byte-for-byte mirror the training pipeline's preprocessor (`contrib/wintermute-train/` — the microWakeWord `micro_features`/mel spec: window length, hop, mel-bin count = 40, frame count = 186, log/scale, dtype). Source the parameters from the training code, not from guesswork.

- New `src/features.rs`: `fn mel_window(pcm: &[i16]) -> [[f32; 40]; 186]` (or `ndarray`), plus the ring-buffer that accumulates enough PCM (~1.5 s) to fill 186 frames before the first inference.
- `OnnxWakeDetector::process` rewired: build the input tensor as `[1, 186, 40]` from `features::mel_window(...)`, not `[1, WAKE_WINDOW_SAMPLES]`. Delete the raw-PCM `[1, N]` path and the misleading `// model input shape: [1, N]` comment.
- `WAKE_WINDOW_SAMPLES` / `WAKE_STRIDE_SAMPLES` redefined to mean the mel-window stride, or replaced by feature-frame constants. Keep `NullWakeDetector` fallback unchanged.

### 2.2 Wire the locally-trained model as default

- Default wake model path resolves to the installed trained ONNX (`/usr/share/wintermute/models/wake/wintermute.onnx`), with `WM_WAKE_MODEL` override preserved.
- Install step copies the current best trained artifact. The smoke model (`out-smoke/wintermute.onnx`) is acceptable to *wire and shape-validate* against, but is NOT expected to pass the live-accuracy AC — see Non-goals re: full training run.

### 2.3 Repair `fetch-models` manifest (`src/models.rs`)

- **silero_vad:** replace the placeholder `sha256` with the real digest of the actual download (`silero-vad` v5.1 `silero_vad.onnx`). Verify by downloading and hashing, then pin.
- **wake URLs:** the `kahrendt/microWakeWord` `.onnx` release assets no longer exist. Either (a) repoint to a real, currently-resolving source of equivalent ONNX models with correct hashes, or (b) **remove the upstream wake entries from the manifest** and document that the wake model is provided by the local training pipeline ([[project_wintermute_wake_training]]). Do NOT leave 404 URLs + fabricated hashes in the manifest. A `fetch-models` run must end with every listed entry either installed-and-verified or not listed.
- `fetch-models --list` and a dry-run path must reflect only entries that actually resolve.

### 2.4 No fabricated test fixtures

Any WAV fixtures committed must be either (a) generated deterministically by code checked into the repo (documented synthesis), or (b) third-party clips with provenance + license recorded. The live-mic AC is the source of truth; in-repo fixtures are a convenience, not the proof.

## 3. Acceptance tests

1. **AC1 — shape contract locked.** A unit test asserts the tensor handed to `sess.run` has shape `[1, 186, 40]` and dtype f32, and that `features::mel_window` output dimensions match the loaded model's declared input dims (read from the ONNX graph at load; fail loudly if they differ). This test would have caught the original bug.
2. **AC2 — mel parity with training.** A test feeds a fixed PCM buffer through `features::mel_window` and asserts the output matches a golden vector exported from the training pipeline's preprocessor (tolerance ≤ 1e-3). Proves the front-end matches what the model was trained on, not just "some 40×186 array."
3. **AC3 — held-out positive fires.** Using a wake clip **not** authored by the build agent (recorded fixture with provenance, or the user's live clip from AC6), the detector emits exactly one `wm.audio.wake` with confidence above threshold. If no independent clip is available, AC3 is BLOCKED on AC6 — do not substitute a self-generated fixture and call it passed.
4. **AC4 — held-out negatives stay silent.** ≥30 s of room tone + non-wake speech → zero `wm.audio.wake`. Report the false-accept count explicitly in the receipt.
5. **AC5 — `fetch-models` is honest.** Fresh run on an empty prefix: every manifest entry ends installed-with-matching-sha256, and `--list` shows no entry whose URL 404s. silero_vad installs and verifies. (Run in a `--prefix` temp dir to avoid needing root in the harness.)
6. **AC6 — live human gate (user-run, manual).** User speaks the wake word into the mic; a `wm.audio.wake` envelope appears on the bus within 500 ms. This AC is explicitly user-certified (like skill-doctor AC7/AC11) — the build agent records it as PENDING-USER until jsy confirms. **No synthetic substitute permitted.**
7. **AC7 — daemon health.** wm-audio active 60 s post-restart, NRestarts=0, mel front-end adds no capture-side regression (UDS subscribers still read full-rate PCM).
8. **AC8 — `cargo test --release --lib` green; `cargo deny check bans licenses sources` clean** (MSRV/baseline per [[self_recall_baseline_gate_red]]-style: compiles + tests green is the real bar).

## 4. Non-goals

1. **Training a high-accuracy model.** This PRD makes the *plumbing* correct end-to-end with the existing (smoke-grade) trained model. Producing a deploy-quality model is the separate `wintermute-train-full` run ([[project_wintermute_wake_training]]) — which the 2026-06-02 reboot killed mid-feature-gen and needs re-running. AC3/AC6 may report low recall with the smoke model; that's expected and is the training PRD's job, not this one's.
2. VAD algorithm changes — only the silero_vad manifest hash is in scope here.
3. Streaming/stateful wake inference — the trained contract is non-streaming `[1,186,40]` (tf2onnx can't convert the stateful variant per the training note). Stay non-streaming.
4. New wake words beyond the trained one.

## 5. Open questions

- Exact mel parameters (n_fft, hop, fmin/fmax, log vs pcen, int8 vs f32 scaling): **resolve by reading `contrib/wintermute-train/`, not by assuming.** If the training preprocessor and the ONNX input dtype disagree, the training pipeline is ground truth.
- Whether to keep any upstream wake URL in the manifest at all, vs. declaring the local pipeline the sole wake-model source. Lean toward removing dead URLs.
- Threshold default: start 0.6 (`WM_WAKE_THRESHOLD`), tune at AC6.

## 6. Files this PRD likely touches

- New: `src/features.rs` (mel front-end + ring buffer)
- Modified: `src/inference.rs` (`OnnxWakeDetector::process` → `[1,186,40]`; load-time shape check)
- Modified: `src/wake.rs` (window/stride constants for mel framing)
- Modified: `src/models.rs` (real silero_vad sha256; remove/repair dead wake URLs)
- Modified: `src/main.rs` / config (default wake model path → installed trained ONNX)
- New: `tests/` golden mel vector + shape-contract tests (AC1/AC2)
- Modified: install path (copy trained `wintermute.onnx` into the wake dir)
- Modified: `README.md`, `CHANGELOG.md`
