# PRD: homeward-embed — embed every pet photo, search by image

Status: Draft v0.1
build_target: mixed
build_into: /home/jsy/wintermute/homeward
Vision: visions/homeward.md

## TL;DR

This is the ML core the seed demanded ("we will need ML to match dogs by
photos"). It builds the photo-embedding pipeline: detect-and-crop the animal,
embed the crop with an open, commercially-usable vision model, and index the
vectors for sub-second similarity search. Given an owner's lost-pet photo it
returns the top-k visually-similar shelter animals. It ships with an **honest
held-out evaluation harness** — because an image matcher that grades itself on
self-generated fixtures proves nothing.

## Why this exists

Phase 1 research determined the realistic, hardware-feasible, license-clean
approach and the traps to avoid:

- **v1 pipeline:** YOLO body-crop → **DINOv2 ViT-B/14** embedding (Apache-2.0,
  *commercially usable*) → L2-normalize → **HNSW** vector index → cosine kNN.
  Query latency is dominated by one forward pass (<1s); 100k×768-d index ≈ 300MB,
  fits in RAM; the one-time gallery embed is the only heavy cost (amortized: embed
  each photo once at intake).
- **Crop before embed** — raw kennel backgrounds pollute generic embeddings; an
  off-the-shelf YOLO (COCO already has `dog` *and* `cat` classes — cats included
  for free) is the highest-ROI preprocessing step. Don't gate enrollment on a
  *face* crop (intake faces are often non-frontal); body crop is primary.
- **License discipline:** DINOv2/OpenCLIP are permissive; **MegaDescriptor is
  CC-BY-NC (non-commercial)** and PetFace-trained weights are research-gated.
  v1 must use the permissive path so the result is shippable.
- **Accuracy is a shortlist, not an oracle.** On PetFace dog verification CLIP
  scores 91.9% AUC vs ArcFace 99.0% — generic embeddings are the floor. Treat v1
  as "narrow tens of thousands of intakes to a human-reviewable shortlist," never
  "find the pet." A v2 fine-tune (ArcFace on PetFace, which has 46,755 dog
  individuals **and** cat individuals with identity labels) is the upgrade path.
- **Honest eval is non-negotiable** ([[feedback_agent_written_fixtures_tautology]]:
  a wm-router safety claim of 100% collapsed to 73.5% on a held-out set). Re-ID
  accuracy inflation is a documented trap: eval on **truly held-out individuals**
  (never dogs/cats seen in training) and on **realistic** photos (Flickr-Dog-style,
  not clean studio crops).

`~/wintermute/recall`'s embedder + vector-index is the structural precedent —
same shape, image model instead of BGE text.

## What this builds

A Python subtree `homeward/embed/` (the fleet's one non-Rust component; `uv`-managed
per the toolkit Python convention) exposing a small service the Rust side calls:

- **Detector/cropper** — YOLO (v8/v11, COCO weights) cropping the largest `dog`
  or `cat` detection; fall back to whole-image if no detection (logged, not
  dropped).
- **Embedder** — DINOv2 ViT-B/14 (Apache-2.0) over the crop → 768-d L2-normalized
  vector. Model choice configurable (ViT-S for CPU-only boxes — this laptop is
  CPU-only; ViT-S ~150MB index/100k and far faster per the research).
- **Index** — HNSW (via `hnswlib` or FAISS `IndexHNSWFlat`) of gallery vectors,
  persisted to disk, append-only as intakes arrive; maps vector → `canonical_id`.
- **Service interface** — a localhost socket/HTTP endpoint (the sidecar the vision
  doc favors) with: `enroll(canonical_id, image_url)` (embed + index one intake),
  `query(image_bytes_or_url, k, species_filter) -> [(canonical_id, score)]`,
  `reembed_all`. Rust (homeward-ingest/match) calls it; no Python types leak into
  the Rust crates.
- **Eval harness** — `homeward/embed/eval.py`: load a held-out set (PetFace and/or
  Flickr-Dog splits with **individual** identity labels, downloaded separately —
  research-gated, not vendored), build a gallery of known individuals, query with
  held-out photos of the *same* individuals never seen at index time, and report
  **Rank-1 / Rank-5 / Rank-20 retrieval accuracy + mAP**. The harness MUST refuse
  to evaluate on images whose individual IDs overlap the gallery's training IDs
  (guards the tautology).
- EXIF is stripped from any owner-uploaded query image before processing (privacy;
  homeward-report owns the upload, but embed must not persist or leak EXIF).

Non-goals: structured/geo filtering + ranking fusion (homeward-match), the owner
UX and alerts (homeward-report), training a fine-tuned model (v2, future). v1 is
frozen-embedder retrieval + honest eval.

## Acceptance criteria

1. `homeward/embed/` builds a runnable `uv` environment and a smoke test embeds a
   sample image to a fixed-dimension L2-normalized vector (norm ≈ 1.0).
2. The detector crops a `dog` and a `cat` test image to the animal before
   embedding, and falls back to whole-image (with a logged warning) when no
   animal is detected — neither case drops the image.
3. `enroll` adds a vector→`canonical_id` entry to a disk-persisted HNSW index that
   survives process restart; `query` returns the k nearest `canonical_id`s with
   cosine scores, honoring an optional `species_filter`.
4. Query end-to-end (embed one image + kNN over a ≥1k-vector index) completes in
   under ~2s on this CPU-only laptop (ViT-S config), and the index for 100k
   vectors is documented to fit in RAM.
5. The eval harness reports Rank-1/Rank-5/Rank-20 and mAP on a held-out set of
   **unseen individuals**, and **errors out** if any query individual-ID is
   present in the gallery/training IDs (anti-tautology guard) — proven by a test
   that feeds overlapping IDs and asserts the refusal.
6. The pipeline uses only permissively-licensed weights (DINOv2/OpenCLIP/YOLO);
   the README/config documents that MegaDescriptor and PetFace weights are
   non-commercial/research-gated and are NOT bundled.
7. EXIF metadata present on an input image is not written to disk or returned by
   the service (asserted by a test on an EXIF-bearing fixture).
