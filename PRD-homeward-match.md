# PRD: homeward-match — turn a lost photo into a ranked, honest shortlist

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/homeward
Vision: visions/homeward.md

## TL;DR

Visual similarity alone over-returns: a black lab embeds close to a thousand other
black labs nationwide. This PRD fuses the image kNN (homeward-embed) with the
structured facts in the store (species, coarse geography, intake-date window,
size/color) into a single calibrated, ranked shortlist for a given lost report —
and enforces the domain's central UX contract: results are **candidates for human
confirmation, never confirmations**.

## Why this exists

Phase 1 research made the failure modes concrete:

- **Geo + recency are essential filters.** A lost pet is found near where it went
  missing, recently. The store already carries coarse `ShelterLocation`,
  `intake_date`, and `Found_Location` (from the Socrata connectors) — matching
  must use them to cut the visual candidate set to the plausible region/time,
  not rank the whole country.
- **Species and attributes are cheap precision.** A cat report should never
  surface dogs; a "small/tan" report should down-weight large/black candidates.
  (Both species are first-class per the "also include cats" seed.)
- **False-match distress is a real harm** (Phase 1 §3b). Petco Love Lost presents
  *possibilities* for human confirmation, not "we found your dog." homeward must
  do the same: calibrated confidence, ranked list, explicit "review the actual
  animal" framing — never an automated reunion claim.
- **Stray-hold awareness** (Phase 1 §3a): a stray within its legal hold is the
  *most* important match to surface to an owner (reclaim window!), and must be
  flagged as in-custody/reclaimable, not "adoptable."

## What this builds

Extends `homeward` with a `homeward-match` crate:

- **`match(report: &LostReport, opts) -> RankedCandidates`** pipeline:
  1. **Structured prefilter** over the homeward-ingest store: species ==,
     `Availability::{InCustody, Adoptable}` (exclude `Departed`), coarse-geo within
     a radius of the report's last-seen location, `intake_date` within a window
     (default: ± a configurable span around the lost date, since a pet may be
     picked up before or after the report).
  2. **Visual kNN** via homeward-embed `query` restricted to the prefiltered
     `canonical_id` set (or kNN-then-filter when the set is large).
  3. **Score fusion + calibration** — combine the cosine similarity with
     attribute agreement (breed/size/color/sex) and geo/recency proximity into a
     single calibrated score in [0,1] with documented buckets
     (`strong | possible | weak`); calibration is fit/validated on held-out data,
     not asserted.
- **`RankedCandidates`** — ordered list of `{canonical_id, score, bucket,
  why: MatchExplanation}` where `MatchExplanation` names the contributing signals
  ("visually similar; same coarse area; intake 2 days after lost") for
  transparency/auditability (the open-matcher wedge).
- **Stray-hold flagging** — candidates with `IntakeType::Stray` inside a hold
  window are tagged `reclaimable_until` (computed from a per-jurisdiction hold
  table; default conservative) and sorted with priority, since reclaim is
  time-critical.
- **Contract enforcement** — the API returns `Candidates`, and the type carries no
  `confirmed`/`is_match` boolean; a doc-test/usage example demonstrates the
  candidates-not-confirmation framing. No auto-notification logic here (that, with
  human-review gating, is homeward-report).
- CLI: `homeward match report <report.json> [--radius-km] [--date-window-days]
  [--k]` printing the ranked shortlist with explanations.

Non-goals: the embedding model itself (homeward-embed), owner submission/alerts/
expiry and the public API (homeward-report), source ingestion. Match is the
fusion + ranking + calibration layer.

## Acceptance criteria

1. `match` returns candidates only of the report's species and only from
   non-`Departed` records (a cat report never returns a dog; an expired listing
   never appears) — unit-tested against a seeded store.
2. The structured prefilter restricts candidates to a coarse-geo radius and an
   intake-date window around the lost date; widening the radius/window
   monotonically grows the candidate set (tested).
3. Visual kNN scores from homeward-embed are fused with attribute and geo/recency
   agreement into a single score in [0,1] with `strong|possible|weak` buckets; the
   fusion is deterministic for fixed inputs (golden test).
4. Each candidate carries a `MatchExplanation` naming its contributing signals;
   the returned type has **no** `confirmed`/`is_match` field (candidates-not-
   confirmation enforced at the type level).
5. A stray candidate within its hold window is flagged `reclaimable_until` and
   ordered with priority over equal-scored non-stray candidates.
6. Calibration buckets are fit and reported on a held-out labeled set (matched vs
   non-matched pairs), not hand-asserted — the test loads a held-out split and
   checks bucket precision is reported.
7. `homeward match report <file>` prints a ranked shortlist with per-candidate
   scores, buckets, and explanations; an empty result (no plausible candidates)
   is reported clearly, not as an error.
