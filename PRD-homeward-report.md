# PRD: homeward-report — the owner's side, and the open API

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/homeward
Vision: visions/homeward.md

## TL;DR

Everything below this PRD builds the haystack; this one serves the owner looking
for their needle. An owner submits one photo of their lost dog or cat plus a
coarse last-seen location and a brokered contact; homeward matches it continuously
against the live store and fires a match alert the moment a new matching intake
appears — then auto-expires the report when it's stale or the pet is home. It also
exposes the **open read API** that is homeward's core differentiator: the
interoperability layer no incumbent provides.

## Why this exists

Phase 1 research identified both the unmet user need and the strategic wedge:

- **The owner's real pain is fragmentation** — today they must post to 5–8 sites
  and re-check shelter pages daily. A single submission that searches the whole
  aggregated store and *pushes* alerts solves the actual problem. Petco Love Lost
  does this but is closed; the differentiator is to be **open** (Phase 1 synthesis:
  "nobody exposes a public query/match API or open dataset" — the single biggest
  gap).
- **Privacy is the sharpest liability on the owner side** (Phase 1 §2): lost
  reports carry human PII (contact, location). Posture: collect the minimum,
  **strip EXIF** on upload (silent home-GPS leak), **coarsen location** (ZIP/radius,
  never street address), **broker contact** (relay token, never publish raw
  phone/email), **auto-expire** reports (CCPA data-minimization), one-click delete.
- **Match alerts must not cause false-match distress** (Phase 1 §3b): alerts say
  "a possible match appeared — review the animal," never "we found your pet," and
  link to the shelter so a human confirms in person.
- **Stray-hold timing is owner-critical** (Phase 1 §3a): when a match is a stray in
  its hold window, the alert must convey the reclaim deadline.

This consumes `homeward-match` (ranking) over the `homeward-ingest` store, closing
the loop schema → connectors → ingest → embed → match → **report**.

## What this builds

Extends `homeward` with a `homeward-report` crate + `homeward-reportd` service:

- **Report intake** — `submit(LostReport)` that: validates via homeward-schema,
  **strips EXIF** from uploaded photos before they reach homeward-embed,
  **coarsens** the last-seen location to the configured precision, mints a
  **brokered contact token** (opaque relay; raw contact stored encrypted, never
  exposed in any read response), and sets an `expires` (default 90 days,
  owner-renewable).
- **Continuous matching** — on each `intake.new`/`intake.updated` event from
  homeward-ingest, re-run homeward-match for active reports of that species/region;
  when a candidate crosses the `strong` (or configurable) threshold and is new,
  enqueue a **match alert**.
- **Match alerts** — deliver via the brokered channel a message framed as
  *"a possible match appeared at <shelter>, <coarse area> — please review the
  animal"* with a link to the source listing and, if the candidate is a stray in
  hold, the `reclaimable_until` deadline. Never asserts a confirmed match. Dedup
  alerts so the same candidate isn't re-sent.
- **Lifecycle** — `mark_reunited` (owner closes the loop; record retained only as
  needed then purged), auto-expire on TTL, one-click `delete` that purges PII
  immediately (CCPA deletion).
- **Open read API** — a documented, rate-limited, read-only HTTP API over the
  aggregated **shelter** store (NOT the PII report store): query current intakes by
  species/coarse-geo/intake-window, and an image-similarity search endpoint
  (proxying homeward-match for found-pet lookups). Returns hotlinked image URLs +
  source attribution, honors per-source ToS, and offers an opt-in open-data export.
  This is the "be the federator/interoperability layer" wedge — exposed for vets,
  311 systems, and other apps. The PII report store is **never** queryable through
  this API (separate trust zone, Phase 1 §2.1).
- CLI: `homeward report submit <file>`, `homeward report status <id>`,
  `homeward report serve` (the API + matching daemon).

Non-goals: outward syndication to PawBoost/Pet FBI/Nextdoor (future `/dream extend
homeward`), microchip-registry federation, a graphical front-end. This is the
owner data path + alerts + the open read API.

## Acceptance criteria

1. `submit` strips EXIF from an uploaded photo (an EXIF-GPS-bearing fixture yields
   a stored/forwarded image with no EXIF), coarsens the last-seen location to the
   configured precision, and stores raw contact only behind a brokered token.
2. No read path — API response, candidate, or alert — ever returns a raw
   phone/email or a street-level location for a report (asserted across the API
   surface); contact is always the brokered token.
3. A new intake that crosses the configured match threshold against an active
   report enqueues exactly one alert (re-delivery of the same candidate is
   deduped), and the alert text is candidates-not-confirmation framed (no "we
   found your pet"); a stray-in-hold candidate's alert includes
   `reclaimable_until`.
4. Reports auto-expire at TTL and `delete` purges all PII for a report
   immediately; an expired/deleted report produces no further alerts and is absent
   from all stores.
5. The open read API serves current **shelter** intakes filtered by species/coarse-
   geo/intake-window with hotlinked image URLs + source attribution, is rate-
   limited, and **cannot** reach the PII report store (a test asserts report PII is
   unreachable via every API route).
6. The image-similarity API endpoint accepts a found-pet photo and returns a
   ranked candidate shortlist (via homeward-match) with explanations and no
   confirmed-match assertion.
7. `homeward report submit` / `status` / `serve` work end-to-end against a seeded
   store: submitting a report, ingesting a matching intake, and observing the
   resulting alert — covering both a dog and a cat report.
