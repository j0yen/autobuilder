<!--
deferred_acs: [2, 3, 4, 5, 6]   # asset/human-gated; inline-int form so scan-prds.sh parses it (named-block form silently became []). See mock_justifications.
mock_unjustified_for: [2, 3, 4, 5, 6]
mock_justifications:
  2: "Stock wake URLs+sha256 are gated on the rouse-wake-vad-models dependency (must first publish tflite→onnx assets), and the wintermute manifest entry carries an all-zero placeholder sha256 because the trained wintermute.onnx does not exist yet (training is the offline, non-CI AC4/AC5 step). We deliberately do not invent real digests for non-existent files; the manifest structure itself is unit-tested in models::tests. Asset gap, not a code defect."
  3: "Loading a real wake .onnx under ort requires an actual model asset; the feature front-end shape/contract IS unit-tested (features::tests), but a fixture-model load-and-run needs a committed .onnx we are not permitted to fabricate, so this sub-claim is asset-gated."
  4: "A --smoke training run needs TensorFlow + microWakeWord in the venv plus a network pull of MIT-RIR/negative-feature corpora; it is asset/network-gated and not CI-deterministic. The autonomously-checkable half (--help documents the pipeline + exit codes) is covered by tests/train_harness_cli.rs."
  5: "Requires a trained wintermute.onnx installed under a prefix and a recorded human 'wintermute' utterance; hardware/asset-gated and human-in-the-loop by the PRD's own wording."
  6: "False-accept/false-reject sanity requires real recorded speech/silence fixtures and a trained model to score them; asset-gated, reported by the training harness's verify stage rather than CI."
-->

# PRD: wintermute-wake-word — a custom "wintermute" wake word, end-to-end

Status: Draft v0.1
build_priority: high
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-audio
Vision: visions/rouse.md
Depends on: rouse-wake-vad-models (base provisioning — MUST be corrected first, see §0), wintermute-audio-inference (detector load path)
Codename: *shibboleth* — the house should answer to its own name.

## TL;DR

Make the laptop wake to the spoken word **"wintermute"** (not just the
stock `hey-jarvis`/`okay-nabu`/`hey-mycroft`). This is two things: a
small Rust change to register the new wake word as a first-class option,
and the real work — **training a custom wake-word model**, since no
pretrained "wintermute" model exists anywhere. It also forces a
correction of the wake stack's base, which — verified 2026-06-02 — does
not currently work for *any* wake word (see §0).

## §0 — Base defect this PRD must fix first (verified 2026-06-02)

The wake path has **never fired**, for any word. Three concrete causes,
all confirmed live this session:

1. **No wake/VAD model is installed.** `/usr/share/wintermute/models/wake/`
   is empty; a disk-wide `find -iname '*.onnx'` shows only the TTS model
   (`en_US-lessac-medium.onnx`). At runtime `load_or_null_wake`
   (`inference.rs:337`) sees no file → falls back to `NullWakeDetector`
   → `WakeOutcome::NotDetected` forever. `wake=hey-jarvis` in the
   startup log is a config *label*, not a loaded model.
2. **The model manifest is broken** (`models.rs:79-112`, shipped by
   `rouse-wake-vad-models`). Every wake URL **404s**: the repo moved
   `kahrendt/microWakeWord` → `OHF-Voice/micro-wake-word`, and the
   release tag/asset paths in the manifest do not exist. `fetch-models`
   cannot have ever succeeded. Some pinned sha256s also look like
   placeholders (e.g. hey_mycroft `7a1c3e9d…`).
3. **Format mismatch — the deepest issue.** Upstream micro-wake-word
   ships **`.tflite`** models (verified via the GitHub releases API:
   `v2.1_models` → `hey_jarvis.tflite`, `okay_nabu.tflite`, `vad.tflite`,
   …). There are **zero `.onnx` assets**. But `inference.rs` loads with
   `ort` (ONNX Runtime) and the manifest pins `.onnx` filenames. And the
   runtime contract in `OnnxWakeDetector::process` (`inference.rs:102`)
   feeds the model **raw normalized PCM `[1, 1280]`** and reads a scalar —
   whereas micro-wake-word models consume **MFCC/spectrogram feature
   frames** produced by a separate audio preprocessor (40 features per
   ~20 ms window), not raw PCM. So even a correctly-downloaded model
   would not run against the current code.

**Consequence:** "add wintermute" cannot be purely additive. The base
must first be made real: correct URLs/format, a tflite→onnx conversion
(or an `ort` tflite path / a feature front-end), and an MFCC preprocessor
matching the model's true input contract. Fixing this also makes
`hey-jarvis` work for the first time.

## What this builds

Extends `~/wintermute/wintermute-audio/` (rust-extend; preserves existing
behavior and tests). Three layers:

### 1. Register the wake word (small, deterministic)
- Add `Wintermute` to the `WakeWord` enum (`config.rs:14`), parse
  `"wintermute"` in `WakeWord::parse` (config.rs:31), and return label
  `"wintermute"` from `as_label`. Extend the unknown-word error message
  to list it. Update the round-trip parse test.
- Add a `wintermute` entry (`ModelKind::Wake`, `filename:
  "wintermute.onnx"`) to the `MANIFEST` (`models.rs:79`), pinned by
  URL + sha256 to the trained artifact's published location (a release
  asset under `j0yen/wintermute-wake-models` or equivalent), license
  recorded.

### 2. Fix the feature/inference contract (§0 item 3)
- Add an **audio feature front-end** (MFCC / log-mel, matching
  micro-wake-word's `MicroFrontend`: 40 features, configurable window/
  stride) feeding the wake model, replacing the raw-PCM `[1,1280]`
  assumption in `OnnxWakeDetector::process`. The streaming window stays
  80 ms / 1280 samples at the capture layer; features are derived per
  window with the detector carrying any needed ring-buffer context.
- Provide a **tflite→onnx conversion** step (offline, in the training
  harness via `tf2onnx`), OR document an `ort` tflite-feature path, so
  stock models *and* the custom one load through one code path. Correct
  the manifest repo/URLs/format for the three stock words as part of
  this (coordinate with `rouse-wake-vad-models`).

### 3. Train the "wintermute" model (the hard part)
- `contrib/train-wintermute.sh` (+ a `contrib/wintermute-train/` Python
  project, `uv`-managed): a reproducible pipeline that
  1. **Generates positive samples** with the installed `piper` TTS
     (`en_US-lessac-medium`) plus pitch/speed/voice variation — hundreds
     to thousands of synthetic "wintermute" utterances.
  2. **Augments** with background noise + room impulse responses
     (standard micro-wake-word augmentation; negatives from a generic
     speech/noise corpus).
  3. **Trains** a micro-wake-word streaming model (the OHF-Voice
     training recipe) to the standard feature input contract.
  4. **Exports** to `.onnx` (tflite→onnx), verifies it loads under `ort`
     with the §2 front-end, and reports val accuracy / false-accept rate.
- The trained `.onnx` + its sha256 + provenance get published and pinned
  into the manifest (§1). Training runs offline (CPU; the model is tiny
  but expect a long-ish run) and is **not** part of `cargo test`.

## Acceptance criteria

1. `WM_WAKE_WORD=wintermute wm-audio …` parses without error; `as_label`
   → `"wintermute"`; the unknown-word error lists `wintermute`. All
   pre-existing wake words still parse; clippy clean; existing tests pass.
2. `wm-audio fetch-models --list --format json` includes a `wintermute`
   wake entry with a real, reachable URL and correct sha256 — **and** the
   three stock entries now point at real, downloadable assets (the §0
   404s are gone). `--list` performs no download.
3. The feature front-end (§2) is unit-tested: a known PCM fixture
   produces the expected feature-frame shape, and a fixture wake model
   loads and runs through it under `ort` (no raw-PCM `[1,1280]`
   assumption remains). A stock model (e.g. `hey_jarvis`) loads and
   produces a finite confidence on a fixture — i.e. the base path works
   for the first time.
4. `contrib/train-wintermute.sh --help` documents the pipeline; a
   `--smoke` run (tiny sample count) produces a loadable `.onnx` and
   exits 0, proving the harness end-to-end without a full training run.
5. The trained `wintermute.onnx`, once installed into a `--prefix`, is
   loaded by the daemon (log `wake_model_loaded label=wintermute`, not
   `wake_model_missing`), and on a recorded "wintermute" utterance the
   detector emits `WakeOutcome::Detected` above `WM_WAKE_THRESHOLD`,
   publishing `wm.audio.wake { wake_word: "wintermute" }` on the bus.
   (Human-gated: requires speaking/recording.)
6. False-accept sanity: on a fixture of unrelated speech/silence, the
   model stays below threshold for the bulk of windows (report the rate;
   no hard numeric gate, but the harness must print FA/FR so the
   threshold can be tuned).
7. `cargo test --release` ≥ current+6 (enum parse round-trip incl.
   wintermute, manifest entry present + valid, feature front-end shape,
   fixture-model load-and-run, threshold gate, MODELS.json provenance for
   the new entry). `cargo deny check bans licenses sources` clean
   (training-only Python deps are out of the cargo tree).
8. README + CHANGELOG: document `WM_WAKE_WORD=wintermute`, the training
   harness, the corrected model source, and the new feature front-end;
   version bump per repo convention.

## Non-goals

1. On-device / streaming *training* — training is offline via the
   harness; the daemon only loads the resulting model.
2. Replacing the stock wake words — `wintermute` is added alongside;
   `hey-jarvis` etc. remain (and finally work after §2).
3. Multi-wake simultaneous detection — one active wake word per session,
   selected by `WM_WAKE_WORD`, as today.
4. VAD model work beyond what §2's shared load path requires — Silero VAD
   provisioning stays `rouse-wake-vad-models`' concern.
5. Wiring downstream dialog/STT behavior — once `wm.audio.wake` fires,
   the existing pipeline takes over; this PRD ends at a correct wake event.

## Risks / notes

- **tflite→onnx of a streaming model is the riskiest step**: stateful /
  streaming ops may not convert cleanly. Fallback: run the model as a
  non-streaming windowed classifier, or add a tflite execution path.
  The `--smoke` AC4 de-risks the harness before a full train.
- Synthetic-only positives (piper) can overfit to one voice; the
  augmentation + voice variation in §3 mitigates, but real-recording
  fine-tuning may be a follow-on PRD.
- Brain/dialog still stall on empty `WM_ANTHROPIC_KEY` (cloud credit
  exhausted) — out of scope here, but a real wake event won't produce a
  spoken reply until the brain tier is restored or pointed local.
