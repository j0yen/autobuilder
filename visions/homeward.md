# Vision: homeward — the open layer that brings lost pets home

**Authored by:** /dream (Claude Opus 4.8), with jsy
**Created:** 2026-06-04
**Status:** active
**Seed:** jsy — *"a real-time database of missing dogs... research deeply for all
public information on all dogs being hosted in shelters... reconnect them with
their owners. We will need ML to match dogs by photos."* Extended same session:
*"also include cats."* Grounded in deep external research (four parallel
research agents, 2026-06-04; citations throughout the fleet PRDs).
**Note:** This is the **first outward-facing vision** in the autobuilder — gossip
(2026-06-xx) flagged that "ZERO of 27 visions reaches outward." homeward reaches
outward by design.

## TL;DR

When a dog or cat goes missing, the animal is very often *already in a shelter* —
picked up as a stray and logged within hours — but the owner never finds out in
time. The data exists publicly and the matching is tractable, yet the owner has
to manually check 5–8 sites and squint at photos. homeward aggregates, in
near-real-time, the companion animals currently held in US shelters and rescues
(prioritizing **stray/found intakes** — the someone's-lost-pet population),
embeds every photo with an open vision model, and lets an owner submit one photo
of their lost pet to get a ranked shortlist of visually-similar animals in
shelters near them — with a match alert the moment a new matching intake appears.
The wedge versus the incumbent (Petco Love Lost, which already does national AI
matching but as a closed walled garden) is to be **open**: an open API, open
data, federation of sources the incumbent doesn't ingest, owner-controlled
records, real-time delta freshness, and an auditable, open-source matcher with
published accuracy.

## Why now (Phase 1 research, 2026-06-04)

- **The canonical aggregation API died.** Petfinder's public API was **retired
  Dec 2, 2025**. The remaining free national JSON source is **RescueGroups.org**
  (JSON:API v5, free key, ToS explicitly permits cached derivative search
  products if refreshed ≥weekly and org-deletion honored in 1 business day).
- **The stray gold mine is municipal open data.** Socrata/SODA + OpenDataSoft
  portals (Austin `fdzn-9yqv`, Dallas `qgg6-h4bd`, Sonoma `924a-vesw`, Long
  Beach, Bloomington, and *hundreds* more) uniquely tag `Intake Type = STRAY`,
  often with `Found Location`, `Chip_Status` ("SCAN NO CHIP"), kennel/current
  status, and `:updated_at` for delta polling — free, no auth. These carry the
  records most likely to be someone's lost pet, which the adoptable-only feeds
  (RescueGroups, Adopt-a-Pet, Petango) do not isolate.
- **The incumbent is closed.** Petco Love Lost runs national facial-recognition
  matching (the former Finding Rover engine) across ~3,000+ shelters, but exposes
  **no public/owner query API**, no open data, a black-box model with no
  published accuracy, US-only, and admits its intake feeds are "very inconsistent"
  (it shows *adoptable* pets to compensate). Open + auditable + broader
  federation is genuine white space.
- **The ML is feasible on modest hardware.** A v1 of YOLO body-crop → **DINOv2
  ViT-B** (Apache-2.0, commercially usable) → cosine kNN over an HNSW index
  narrows tens of thousands of intake photos to a human-reviewable shortlist;
  query latency is dominated by one forward pass (<1s). The accuracy ceiling
  (ArcFace on PetFace: 99% verification AUC vs CLIP's 91.9%) motivates a v2
  fine-tune. **PetFace** (ECCV 2024) provides 46,755 dog individuals + cat
  individuals with identity labels — the dataset that makes both training and
  *honest held-out eval* possible. (Honest eval is non-negotiable here — see
  [[feedback_agent_written_fixtures_tautology]]: validate on truly held-out
  individuals, never self-generated fixtures.)
- **We already have the index pattern.** `~/wintermute/recall` ships a BGE
  embedder + vector index; homeward's photo index is the same shape with an image
  embedder instead of text. Not a from-scratch invention.

## End-state

When homeward ships:

1. **One normalized record** describes every sheltered pet — dog or cat —
   regardless of which of a dozen heterogeneous sources it came from, with
   honest provenance and a `stray | found | surrender | adoptable` status.
2. **Near-real-time aggregation** keeps that store fresh within the bound of the
   slowest upstream (minutes-to-hours), via conditional-request delta polling,
   per-source adaptive cadence, cross-source dedup, and departure detection so
   reclaimed/adopted animals expire instead of haunting a searching owner.
3. **Every photo is embedded** with an open vision model the moment it is
   ingested, indexed for sub-second similarity search.
4. **An owner submits one photo** of their lost dog or cat (+ coarse last-seen
   location, brokered contact) and gets a ranked shortlist of candidate matches
   in nearby shelters — framed as *possibilities for human confirmation*, never
   "we found your pet."
5. **A match alert fires** when a new intake scores above threshold against an
   open lost report, within minutes of that intake appearing.
6. **It is open:** a documented public query API and opt-in open-data export,
   an auditable open-source matcher with published held-out accuracy — the
   interoperability layer the incumbents are not.

## Components (one bullet per PRD)

- **homeward-schema** — the canonical `PetRecord` + `LostReport` types and source
  provenance model that every connector and the matcher share.
- **homeward-connectors** — the source-connector framework + first connectors
  (RescueGroups.org national JSON; municipal Socrata STRAY feeds), normalizing to
  `PetRecord`, ToS-compliant (conditional requests, image hotlinking, deletion).
- **homeward-ingest** — the freshness engine: adaptive-cadence orchestration,
  cross-source dedup (stable source-id + perceptual hash), departure detection,
  canonical store.
- **homeward-embed** — the ML photo pipeline (Python): YOLO dog/cat body-crop →
  DINOv2 embedding → vector index, with an honest held-out eval harness.
- **homeward-match** — combine visual kNN with structured filters (species,
  coarse geo, intake-date window, size/color) into a calibrated, candidate-not-
  confirmation ranked shortlist for a lost report.
- **homeward-report** — owner side: submit a lost report (EXIF-stripped,
  location-coarsened, brokered contact), continuous matching + match alerts,
  auto-expiry, and the open read API.

## Order

```
homeward-schema ─► homeward-connectors ─► homeward-ingest ─► homeward-embed ─► homeward-match ─► homeward-report
                                                  └────────────────┘ (embed consumes ingested photos)
```

schema is the foundation. connectors and ingest build the fresh store. embed
indexes the photos that store holds. match reads embed + schema. report sits on
top, owner-facing, and exposes the open API.

## Open questions

- **Repo home & language split.** Proposed new cargo workspace
  `~/wintermute/homeward/` with Rust crates for schema/connectors/ingest/match/
  report and a Python subtree (`homeward/embed/`) for the vision model. Is a
  Python+Rust split acceptable, or should embedding run as a sidecar service the
  Rust side calls over a socket? (Leaning: Python embed service + Rust calls it.)
- **Commercial vs research ML.** DINOv2/OpenCLIP are Apache/permissive
  (commercial-OK) but lower accuracy; MegaDescriptor and PetFace-trained weights
  are **non-commercial / research-gated**. v1 must use the permissive path; a v2
  fine-tune on PetFace would be research-licensed — does this stay non-commercial
  / nonprofit? That choice gates which weights are legal to ship.
- **Outward federation (future fleet).** The biggest practical gap is "one report,
  every channel" — syndicating a lost report out to PawBoost (843 FB pages, 50M
  reach), Pet FBI / Lost Dogs of America (nonprofit, open ethos, best first
  partner), Nextdoor — and pulling their matches back. Not yet thought through;
  left for `/dream extend homeward`.
- **Microchip federation.** AAHA's universal lookup is human-gated, routing-only,
  and missing AVID. A programmatic chip→registry layer needs partnerships, not
  scraping — a separate honest investigation.
- **Match-confidence calibration & the false-match-distress guardrail** — the UX
  contract that results are candidates for human review, never confirmations.
- **Stray-hold awareness** — never surface a stray as "adoptable" during its
  legal hold window (3–10 days, per-state); surface it on the found/lost side so
  the owner can reclaim it.
