# PRD: homeward-schema — one normalized record for every sheltered pet

Status: Verified-completed 2026-06-05
build_target: rust-lib
Vision: visions/homeward.md

## TL;DR

A dozen heterogeneous sources describe the same thing — a dog or cat in a shelter
— in a dozen incompatible shapes (RescueGroups JSON:API, municipal Socrata
columns, vendor feeds). Before anything can aggregate, dedup, embed, or match,
there must be **one canonical record** they all normalize into, plus an
owner-side **lost report** type and an honest **provenance** model. This PRD is
that foundation crate: the types, their validation, and their (de)serialization.
It ships no network code — it is the vocabulary the rest of the fleet speaks.

## Why this exists

Phase 1 research surfaced that the source landscape is irreducibly heterogeneous:
- **RescueGroups.org** (the free national JSON:API v5) exposes `species`,
  `breeds.primary/secondary`, multi-photo URLs, `updatedDate`, lat/lon — but is
  **adoptable-oriented** and carries no stray flag.
- **Municipal Socrata feeds** carry the opposite: `Intake_Type` ∈ {STRAY, Owner
  Surrender, Found Report, ...}, `Found_Location`, `Chip_Status` ("SCAN NO CHIP"),
  `Kennel_Status`, `Intake_Date`, `Outcome_Date` — but usually **no photos**.
- Vendor feeds (Shelterluv `in custody`, Petango) sit in between and are
  per-shelter keyed.

No single source has all the fields; each has a different notion of "still here."
A canonical record must therefore make species, status, intake-type, location
granularity, microchip status, photos, and provenance **explicit and optional**,
so a partial source degrades cleanly instead of forcing fiction. The dedup,
embed, and match PRDs all key on this record; getting it right first prevents
churn. The pattern mirrors `~/wintermute/recall`'s typed-record core.

The seed explicitly covers **both dogs and cats** ("also include cats"), so
`Species` is a first-class enum, not a dog-only assumption.

## What this builds

A new cargo workspace `~/wintermute/homeward/` whose first crate is
`homeward-schema` (rust-lib), exporting:

- **`Species`** — `Dog | Cat` (extensible enum; unknown species rejected at
  ingest, not silently coerced).
- **`PetRecord`** — the canonical animal:
  - identity: `source: SourceId`, `source_animal_id: Option<String>` (stable id
    where the source exposes one — critical for dedup/departure), `canonical_id`
    (homeward's own ULID).
  - descriptive: `species`, `breed_primary/secondary`, `sex`, `age_bucket`,
    `size`, `colors`, `markings_text`.
  - **status**: `IntakeType { Stray, FoundReport, OwnerSurrender, Transfer,
    Adoptable, Unknown }` and `Availability { InCustody, Adoptable, Departed,
    Unknown }` (kept distinct — a stray in its legal hold is `InCustody` but NOT
    `Adoptable`).
  - microchip: `ChipStatus { Scanned(chip:Option<String>), ScanNoChip,
    NotScanned, Unknown }`.
  - location: `ShelterLocation` with **coarse** geo (city/county + optional
    lat/lon rounded to a configurable precision) and `found_location_text`.
  - media: `photos: Vec<PhotoRef>` where `PhotoRef` holds a **source URL to
    hotlink** + optional license/attribution — never raw image bytes (copyright
    posture, Phase 1 §1d).
  - lifecycle: `first_seen`, `last_seen`, `last_confirmed`, `intake_date`,
    `outcome_date`.
- **`LostReport`** — the owner side:
  - `species`, descriptive fields mirroring `PetRecord`, `photos: Vec<PhotoRef>`,
    `last_seen: CoarseLocation` (ZIP / radius, never street address),
    `contact: BrokeredContactToken` (an opaque relay handle, **not** raw
    phone/email — privacy posture, Phase 1 §2), `created`, `expires`,
    `status { Active, Reunited, Expired }`.
- **`SourceId`** + **`Provenance`** — which source, fetch timestamp, source URL,
  and ToS class (api / open-data / scraped) so downstream can honor per-source
  rules (deletion SLA, image handling).
- **Validation**: constructors/validators that reject impossible records (e.g.
  `Adoptable` availability while `IntakeType::Stray` inside a hold window is
  flagged), normalize colors/breeds against a controlled vocab, and round geo to
  the configured coarse precision.
- **Serde** round-trip (JSON) for all types; stable field names; forward-compatible
  (`#[serde(default)]` on optionals) so a future field never breaks an old record.

Non-goals: any network/connector code (homeward-connectors), storage (ingest),
ML (embed). Pure types + validation + serde.

## Acceptance criteria

1. The `homeward` cargo workspace exists with `homeward-schema` as a library
   crate that builds clean (`cargo build`) and passes `cargo test`.
2. `Species` covers `Dog` and `Cat`; constructing a `PetRecord` with an
   unrecognized species string fails with a typed error (no silent default).
3. `PetRecord` keeps `IntakeType` and `Availability` as distinct fields, and a
   validator flags the contradiction "Availability::Adoptable + IntakeType::Stray
   within hold" (the stray-hold guardrail, Phase 1 §3a).
4. `PhotoRef` stores a source URL + optional attribution and **cannot** hold raw
   image bytes (type-level: there is no bytes field) — encoding the hotlink/no-
   bulk-copy copyright posture.
5. `LostReport.contact` is a `BrokeredContactToken` opaque type with no public
   accessor that returns a raw phone/email string; `last_seen` is a coarse
   location type with no street-address field (privacy posture).
6. Every public type round-trips through JSON serde without loss, and
   deserializing a record that is missing any optional field succeeds via serde
   defaults (forward compatibility) — proven by tests.
7. Geo coarsening rounds any provided lat/lon to the configured precision on
   construction; a test asserts a precise coordinate is stored only at coarse
   resolution.

---

## Archive trailer

Verified-completed:
  AC1 — paired with acceptance_ac1.rs: ac1_build_and_test_infrastructure
  AC2 — paired with acceptance_ac2.rs: ac2_unknown_species_fails_with_typed_error + ac2_known_species_{dog,cat,aliases}
  AC3 — paired with acceptance_ac3.rs: ac3_stray_adoptable_is_flagged + ac3_intake_and_availability_are_distinct_fields
  AC4 — paired with acceptance_ac4.rs: ac4_photo_ref_no_bytes_field + ac4_photo_ref_json_roundtrip
  AC5 — paired with acceptance_ac5.rs: ac5_brokered_contact_token_no_raw_accessor + ac5_coarse_location_no_street_address
  AC6 — paired with acceptance_ac6.rs: ac6_pet_record_json_roundtrip + ac6_lost_report_missing_optionals_default
  AC7 — paired with acceptance_ac7.rs: ac7_precise_coord_stored_at_coarse_precision_2dp

Output: /home/jsy/wintermute/homeward (homeward-schema crate, workspace member)
Tests: 33 passing (cargo test --release -p homeward-schema)
