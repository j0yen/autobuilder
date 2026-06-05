# PRD: homeward-ingest — the freshness engine that keeps the store true

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/homeward
Vision: visions/homeward.md

## TL;DR

Connectors produce streams of `PetRecord`s; this PRD turns those streams into a
single, fresh, de-duplicated, self-expiring canonical store. It orchestrates
connectors on a per-source adaptive cadence, merges the same animal seen on
multiple sources, and — the hard part — detects when an animal has *left* a
shelter (reclaimed/adopted/transferred) so a searching owner never chases a stale
listing. This is what makes "real-time database" honest rather than aspirational.

## Why this exists

Phase 1 research established that "real-time" here is bounded by upstreams that
refresh every 30 min to daily, and that the genuinely hard engineering is dedup
and **departure detection**, not fetching:

- The same dog appears on the shelter site + RescueGroups + a municipal feed;
  without entity resolution the store triple-counts it.
- An adopted/reclaimed animal simply **stops appearing** — there is rarely a
  "removed" event. Petco Love Lost's own admitted weakness is inconsistent,
  stale-prone feeds (it shows adoptable pets to compensate). The research's
  recommended departure strategy: trust an explicit status field when present,
  else **two-strikes absence** (missing from two consecutive full syncs of a
  source) + 404-on-canonical-URL + a **TTL backstop**.
- Freshness comes from **conditional-request delta polling + adaptive cadence**
  (AIMD per source), not from hammering — poll a 30-min-refresh source every
  ~30 min, back off on 304-heavy/erroring sources, speed up high-churn urban
  shelters.
- Dedup key: prefer the stable `source_animal_id` (carried by `PetRecord` from
  homeward-schema); else cluster on `(species, breed, sex, approx age, shelter,
  intake_date)` + a **perceptual hash (pHash)** of the primary photo.

The `~/wintermute/recall` daemon is the model for a durable, restart-surviving
local store + orchestration loop.

## What this builds

Extends `homeward` with a `homeward-ingest` crate + `homeward-ingestd` daemon:

- **Canonical store** — a local embedded DB (sqlite via `rusqlite`, matching the
  toolkit's sqlite-first habit) of `PetRecord`s keyed by `canonical_id`, with a
  source-id index and a pHash index, surviving restart and reboot.
- **Orchestrator** — runs each registered connector on its own adaptive interval
  (AIMD: increase interval after a 304/no-change poll, decrease after a high-churn
  poll), honoring the connector's `cadence_hint` as a floor. Persists per-source
  cursors so a restart resumes the delta, not a full re-pull.
- **Dedup / entity resolution** — on each incoming record: match by
  `source_animal_id` within source; across sources, cluster by attribute key +
  pHash similarity (configurable Hamming threshold). Maintain one canonical record
  per cluster with a `sources: Vec<Provenance>` list; never lose a source URL.
- **Departure detection** —
  1. explicit status field (`Availability::Departed`, `outcome_date` set) → expire;
  2. **two-strikes absence** from consecutive full syncs of a source → mark
     departed (two-strikes avoids flapping on partial feeds);
  3. 404 on the canonical listing URL during conditional re-fetch → expire;
  4. **TTL backstop** — any record not re-confirmed within N×(source cadence) is
     auto-expired so nothing goes permanently stale.
  Update `last_seen`/`last_confirmed` on every re-observation.
- **ToS compliance hooks** — honor per-source deletion (drop an org's records
  within 1 business day on request), refresh cadence floors (RescueGroups ≥weekly),
  and never persist image bytes (only URLs from `PhotoRef`).
- **Events** — emit lightweight change events (`intake.new`, `intake.departed`,
  `intake.updated`) so homeward-embed and homeward-match can react to deltas
  instead of rescanning. (Bus-agnostic: a callback/queue interface; wiring to
  agorabus is optional and out of scope here.)
- CLI: `homeward ingest run` (daemon), `homeward ingest stats` (counts by source/
  species/status/freshness), `homeward ingest get <canonical_id>`.

Non-goals: the connectors themselves (homeward-connectors), photo embedding
(homeward-embed), matching/owner side. Storage + orchestration + dedup + expiry.

## Acceptance criteria

1. `homeward-ingestd` runs registered connectors, writes normalized `PetRecord`s
   to a restart-surviving sqlite store, and resumes from persisted per-source
   cursors after a restart (no full re-pull) — proven by a test that restarts the
   ingest loop and asserts the cursor was honored.
2. Adaptive cadence: a source returning no changes (304/empty) has its poll
   interval increased, and a high-churn source decreased, never below the
   connector's `cadence_hint` floor (unit-tested AIMD).
3. Dedup: the same animal delivered via two connectors (same `source_animal_id`
   OR matching attributes + near-duplicate primary-photo pHash) collapses to one
   canonical record carrying **both** source provenances and URLs.
4. Departure — explicit: a record whose source reports `Departed`/`outcome_date`
   is expired on next sync.
5. Departure — implicit: a record absent from **two** consecutive full syncs of
   its source is marked departed; a record absent from only **one** is NOT
   (no flapping). A record never re-confirmed within its TTL is auto-expired.
6. `last_seen`/`last_confirmed` advance on every re-observation; `homeward ingest
   stats` reports counts broken down by source, species (dog vs cat), status, and
   a freshness histogram.
7. A simulated "delete org X" request removes all of that org's records from the
   store within the operation (modeling the 1-business-day ToS SLA) and they do
   not reappear on the next sync.
