# PRD: The Repository as Landscape

**Author:** Claude (Opus 4.7), for jsy
**Status:** Draft v0.1 — art project / cartography
**Date:** 2026-05-22
**Audience:** jsy, Katherine, Maria
**Form:** large fine-art print (A1 or larger), edition of ~5
**Cadence:** one-off or annual

---

## TL;DR

Every git repo you own becomes a literal topographic landscape: the file tree is terrain, commit density is elevation, author identity is biome. The result is a USGS-style map you can hang on a wall. The aesthetic isn't data-viz — it's hand-drawn cartography in the spirit of Tolkien or 19th-century mapmakers, computationally generated from your actual code.

---

## 1. Why this exists

1. Software is intangible. You can sit in front of a 50,000-file repo for years and never *see* it. A topographic print makes it visible at a glance.
2. Most repo visualizations look like noise (gource, treemap) because they treat code as data. Cartography has been making complex spatial information legible for centuries — borrow that vocabulary.
3. On a wall, your work has *physical* presence. K and M can see "this is what Joe builds" without needing to read code.

## 2. Who this is for

- **Primary:** you. One large print of your "continent" (all your repos).
- **Secondary:** K and M. They can walk the map; recognize the named "lands."
- The map is a *portrait*. Anyone looking at it should be able to ask "what's this peak?" and you can answer.

## 3. Form

- A1 (594×841mm) archival print, hand-finished colors (offset or giclée).
- Each repo is a "country" — its files form the topography, with paths laid out via force-directed graph or a treemap reflowed into organic shapes.
- Elevation: `log(commit_count)` for that file/directory.
- Biome: hue mapped to file type (Rust = pine forest, Python = scrubland, Markdown = grassland, JSON = wetland).
- Author trails: where another contributor has committed, soft contour lines mark the "territory."
- Annotations:
  - major files named on the map (Cargo.toml as a town, README as a cathedral)
  - compass rose with a self-portrait glyph
  - legend in the lower right: biome → language, elevation → commit density
- Inset: a small map showing wintermute & autobuilder continents in relation.

## 4. Process

```
walker (Rust): for each repo, build a tree of
  (path, language, commit count, last touched, primary author)
   ↓
layout engine: graphviz or custom force-directed → 2D coordinate per file
   ↓
relief generator: convert layout + elevation → contour lines (gdal-style)
   ↓
cartographer (Inkscape SVG or Mapbox GL): apply biome colors, contours, labels
   ↓
high-res PDF → fine-art print
```

The cartographer step is the high-skill bit. Worth one round-trip with a real designer (or letting Claude do a first pass and you taste-test).

## 5. Cadence

One-off for the inaugural print. Annual update if the topology changes substantially (new continent, new biomes).

## 6. Non-goals

1. **An interactive web visualization.** Different project. The print is the artifact.
2. **Real-time updating.** A printed map is a moment, not a feed.
3. **Public/portfolio piece.** Personal artifact unless deliberately released.
4. **Quantitative accuracy.** Cartographic art, not scientific viz — biome boundaries are *interpreted*, not exact.

## 7. Phasing

| Phase | Scope |
| --- | --- |
| 0 | Walker emits JSON for wintermute alone |
| 1 | Layout + elevation → SVG mockup |
| 2 | Cartographer pass: biomes, labels, legend |
| 3 | First A1 print; iterate; second edition |

## 8. Risks

- **Beautiful intent, ugly output.** Most generated maps look like blobby fractals. *Mitigation:* a real designer (or strict adherence to actual cartographic style) is non-optional in Phase 2.
- **Distortion.** Repos with one massive file or one huge contributor warp the topology. *Mitigation:* log scaling; manual smoothing.
- **Privacy.** Some repos are private; their topology may not be shareable. *Mitigation:* private repos render as silhouette only.
- **Scale.** Some repos have 100k+ files. *Mitigation:* coalesce subdirectories below a threshold into single regions.

## 9. Open questions

1. Hand-drawn type for labels (commissioned), or a cartographic font (e.g. Marydale)? Hand-drawn wins on personality.
2. Should the legend be on the map, or on a separate small companion print?
3. Inset: just your repos, or include the open-source repos you've contributed to (their landmasses on a horizon)?
4. K and M: do they get their own continent (their own repos rendered)? Could be a paired set hung in three frames.
