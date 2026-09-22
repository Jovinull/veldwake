# World generation

Status: **Proposed pipeline**, governed by accepted determinism and causal-coherence principles. **M4 implemented its first, deliberately small stage.**

What exists in `veldwake-procedural` after M4 is one finite region, not a world pipeline: a meandering valley axis, a three-part macroform, ridges and detail masked to where they belong, a river carved relative to a monotone water surface, one biome with six micro-zones decided by slope, height above water, landform, and moisture, and a vegetation grammar placed on a jittered lattice. It is conceptually compatible with the elevation to hydrology to climate to vegetation ordering below, and it implements none of the global stages: no tectonics, no plate history, no erosion, no global hydrology solver, no climate simulation, and no civilisation. M8 added the first content that is neither terrain nor vegetation: three landmarks of one family, derived once per world before any chunk is generated, placed from terrain, water, canopy and clearance alone, and written into the same chunks. It occupies the position of "ruins" in the stage list below without any of the history that is supposed to produce them — the structures are composed to be seen and walked to, not deduced from a past. That is a deliberate gap and the reason this document still says *proposed pipeline*.

Hydrology in M4 is a geometric cue whose properties — water cannot run uphill and no boundary-induced discontinuity can occur at a chunk seam — are guaranteed by construction rather than by a solver; voxel `floor` quantization may still change a top-water voxel between adjacent sloped columns. See [`../planning/M4_BEAUTIFUL_TERRAIN_SLICE.md`](../planning/M4_BEAUTIFUL_TERRAIN_SLICE.md).

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
