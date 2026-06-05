# PRD: homeward-connectors — pull pets from real public sources, ToS-compliant

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/homeward
Vision: visions/homeward.md

## TL;DR

The store is only as good as what feeds it. This PRD builds the **source-connector
framework** plus the first working connectors: **RescueGroups.org** (free national
JSON:API — adoptable breadth, photos) and **municipal Socrata/SODA** feeds
(Austin, Dallas, Sonoma, Long Beach — the STRAY tier with `Found_Location` and
`Chip_Status`). Each connector fetches politely (conditional requests, rate
limits), normalizes into `PetRecord`, and records provenance — so the rest of the
fleet sees one clean stream regardless of source shape.

## Why this exists

Phase 1 research pinned down exactly which sources are viable and how to treat
them — this PRD encodes that, so the knowledge doesn't rot:

- **Petfinder is OUT** — its public API was retired **Dec 2, 2025**. Do not build
  on it. (This corrects the obvious-but-stale instinct to "just use Petfinder.")
- **RescueGroups.org JSON:API v5** — free API key in the `Authorization` header,
  national US+Canada, multi-photo URLs, `updatedDate` for delta. ToS **permits a
  cached derivative search product** if refreshed ≥weekly, org data deletable
  within 1 business day, no resale of the raw feed, images hotlinked.
- **Municipal Socrata feeds** are the **stray gold mine**: verified live schemas —
  Austin Intakes `fdzn-9yqv` (`Intake Type=Stray`, `Found Location`), Dallas
  `qgg6-h4bd` (`Intake_Type`, `Kennel_Status`, `Chip_Status`, `Animal_Origin`,
  true live inventory), Sonoma `924a-vesw` (null `Outcome Date` ⇒ still here).
  Free, SODA `$where`/`$order`/`$limit`, delta via `$where=:updated_at > 'ts'`,
  free app token lifts throttling. **No photos** in these — link back to the
  shelter page.
- **Politeness is legally load-bearing** (Phase 1 §1c/§5): post-*hiQ*/*Van Buren*,
  scraping public data isn't a CFAA crime, but ToS breach + server-load +
  image-copyright are real — so APIs first, conditional requests, identifying
  user-agent, honor robots.txt, never bulk-copy full-res images.

## What this builds

Extends the `homeward` workspace with a `homeward-connectors` crate:

- **`Connector` trait** — `async fn poll(&self, since: Option<Cursor>) ->
  Result<Vec<PetRecord>, ConnectorError>` plus `fn provenance(&self) ->
  Provenance` and `fn cadence_hint(&self) -> Duration`. Each connector owns its
  normalization into `homeward-schema::PetRecord`.
- **Polite HTTP core** shared by all connectors:
  - conditional requests: send `If-None-Match`/`If-Modified-Since`, treat **304**
    as "no work" (cheap delta).
  - identifying `User-Agent` with project + contact URL; honor `robots.txt` and
    `Retry-After`; per-host rate limit with exponential backoff on 429/5xx.
  - **never** download full-res image bytes — store the source URL in `PhotoRef`.
- **`RescueGroupsConnector`** — JSON:API v5 paging, key from config/env, maps
  fields → `PetRecord` (species, breeds, photos, `updatedDate` → `last_seen`),
  marks `IntakeType::Adoptable`, provenance class `api`. Caches static lookups
  (breeds/species) as the ToS requires.
- **`SocrataConnector`** — generic SODA client parameterized by `{domain,
  dataset_id, column_map}`, shipped pre-configured for Austin, Dallas, Sonoma,
  Long Beach. Maps `Intake_Type`→`IntakeType` (STRAY/FoundReport/OwnerSurrender),
  `Found_Location`→`found_location_text`, `Chip_Status`→`ChipStatus`,
  availability from `Kennel_Status`/null-`Outcome_Date`. Delta via the dataset's
  update timestamp. Provenance class `open-data`.
- **`ConnectorRegistry`** + a `homeward-connectors` CLI subcommand
  (`homeward connectors poll <name> [--since <cursor>] [--limit N]`) that runs one
  connector and prints normalized `PetRecord`s as JSON — directly testable and
  the seam `homeward-ingest` drives.
- Covers **dogs and cats**: connectors request/normalize both species (Socrata
  `Animal_Type`, RescueGroups `species`); a connector must not silently drop cats.

Non-goals: scheduling/orchestration, dedup, departure detection (all
homeward-ingest); storage; ML. Connectors are stateless pollers + normalizers.

## Acceptance criteria

1. The `Connector` trait + polite HTTP core exist; a connector that receives a
   `304 Not Modified` returns an empty result without error (delta no-op),
   verified against a mocked HTTP server.
2. `RescueGroupsConnector` normalizes a captured/mocked JSON:API v5 response into
   `PetRecord`s with species, breeds, photo URLs, and `last_seen` populated from
   `updatedDate`; provenance class is `api`.
3. `SocrataConnector` normalizes a captured/mocked Austin or Dallas SODA response,
   correctly mapping `Intake_Type=STRAY` → `IntakeType::Stray`, `Found_Location`
   → `found_location_text`, and `Chip_Status` → `ChipStatus`; provenance class is
   `open-data`.
4. Both connectors yield **both** dog and cat records from a mixed-species fixture
   (no species is dropped).
5. The polite HTTP core sends a conditional request header on a repeat poll and an
   identifying `User-Agent`, and applies a per-host minimum interval / backoff
   (asserted against the mock server's recorded request log).
6. No connector path writes raw image bytes anywhere — `PhotoRef`s carry only
   source URLs (grep-asserted in test + type-enforced by schema).
7. `homeward connectors poll <name>` runs a configured connector end-to-end
   against a mock and prints valid `PetRecord` JSON; an unknown connector name
   exits non-zero with a clear message.
