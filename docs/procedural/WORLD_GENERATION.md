# World generation

Status: **Proposed pipeline**, governed by accepted determinism and causal-coherence principles.

## Candidate stages

```text
world identity/version/seed
  -> tectonic structure
  -> geology and rock resistance
  -> elevation and erosion
  -> hydrology
  -> temperature and humidity
  -> biomes
  -> flora, fauna, and resources
  -> civilizations and cultures
  -> routes and settlements
  -> historical simulation and ruins
  -> current world state
```

This is a dependency graph, not necessarily one monolithic startup pass. Streaming, hierarchical generation, summaries, and caches will be necessary.

## Required properties

- Same compatible world version and seed yields equivalent generation outputs.
- Stage inputs/outputs are versioned and independently inspectable where practical.
- A mine reflects resources; a road connects meaningful destinations; a settlement reflects access, terrain, culture, and economy; a ruin reflects prior history.
- GPU access is never required for authoritative generation.
- Generation jobs are cancellable/prioritized and never block the render frame.
- Changes to a generator declare compatibility impact and golden-seed expectations.

## History generation

Pre-player history may establish migrations, foundations, discoveries, expansion, wars, destruction, abandoned routes, artifacts, and ruins. It should produce current gameplay affordances and explanations, not an unread database of events.

## Finite versus infinite

The current preference is a very large but finite world with continents, oceans, islands, climates, and global history. Exact topology and scale are TBD. Finite scope makes causal history and global simulation more tractable; it does not mandate loading or simulating the entire world in detail.

## Golden seeds

When implementations exist, maintain named fixtures for terrain/biome/hydrology/settlement/history cases. A generator change must distinguish intentional visual/model evolution from accidental instability.
