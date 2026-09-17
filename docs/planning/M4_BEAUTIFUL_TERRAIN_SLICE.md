# M4 — Beautiful Terrain Vertical Slice

Status: **implemented on `feat/m4-beautiful-terrain-slice`, awaiting branch QA**
Base: `main` at merge commit `bfc9db1eec085390f9148efbb2a14d61d1fa0d6e`

M4 is the first milestone whose success is a picture. M1 proved a window, M2 proved a chunk, M3 proved that chunks stream. All three rendered a checkerboard, because none of them had any content to render. M4 replaces the checkerboard with one deliberately shaped region — a verdant highland valley — and lights it well enough to judge whether Veldwake looks like anything.

The exit criterion is therefore aesthetic as well as technical: a real screenshot of the golden region has to be recognisably closer to the art direction than the diagnostic corridor was, and it has to be deterministic, streamable, cacheable, headlessly tested, and measured on the audited host.

## Implementation result

### A new crate: `veldwake-procedural`

Generation lives in its own crate, between `veldwake-voxel` and `veldwake-streaming`. It knows nothing about GPUs, windows, cameras, frames, files, or scheduling; everything it produces is a pure function of a chunk coordinate and a world identity.

| module | question it answers |
|---|---|
| `noise` | what is the value of a field at a position? |
| `identity` | which world is this, and under which rules? |
| `terrain` | how high is the ground, what is it made of, what grows here? |
| `vegetation` | which plants stand where, and what shape are they? |
| `material` | what does a material mean to a chunk and to a renderer? |
| `generator` | what voxels does one chunk contain? |
| `region` | which places and values are locked as fixtures? |

`veldwake-voxel` stays a generic container with no terrain semantics. The one table that maps a material to a `VoxelId` and to a colour lives in `procedural::material`, so neither the voxel crate nor the renderer carries a second palette that could drift.

### World identity

`WorldIdentity` is a `WorldSeed` plus a `TerrainConfig`. Its `fingerprint()` hashes the seed, `TERRAIN_GENERATOR_VERSION`, `STYLE_CONTRACT_VERSION`, a locked exhaustive behavioural signature, and the full configuration descriptor, including the region extent and every art control. The signature covers all 1,875 chunks of the canonical golden world but is computed only by a test; runtime fingerprinting remains descriptor-cost. It automatically trips on changed golden output, not every possible other-seed change: `TERRAIN_GENERATOR_VERSION` remains the required explicit cache-contract bump for a general generator algorithm change.

`TerrainConfig` remains inspectable public data, but it is not a bag of unchecked numbers: `TerrainConfig::validate` and fallible `TerrainGenerator::new` reject non-finite values, inverted extents/valley widths, non-positive lattice spacings and scales, negative amplitudes, spacing below the six-voxel tree theorem, and terrain/water controls whose conservative vertical envelope leaves the declared region. The golden constructor remains simple because its compile-time descriptor is validated at construction.

The golden slice is `WorldSeed(0x5645_4c44_5741_4b45)` with `TerrainConfig::golden()`. Every fixture, screenshot, and number in this document refers to that world. A diagnostic world is chosen explicitly by seed (`VELDWAKE_WORLD=seed:…`); there is no general configuration system and no runtime art-control tuning.

Random streams are named, never sequential. `StreamLabel` declares one stream per generator stage, derived from the seed and the generator version, so adding a draw to vegetation cannot perturb the terrain.

### The region

`RegionExtent::GOLDEN` is chunks `-12..=12` in `x` and `z` and `0..=2` in `y`: 800 × 96 × 800 voxels, 1,875 chunks. Outside it the generator returns `None`, which the streaming adapter turns into `SourceChunk::KnownAbsent` — authoritative absence, distinct from a chunk that has not loaded yet and from a chunk that is present and empty.

### Terrain

The field is composed, not a single noise call. Each stage answers one question and the sample reports which stage decided the result:

```text
valley axis (meander)  -> distance from the axis
                       -> macroform: floor, shoulder, highland plateau
                       -> ridges, masked to the highlands
                       -> mid and fine detail, masked away from the floor
                       -> river carve, driven by the water surface
                       -> height
water surface (monotone downstream) + height -> water cells
height + slope + water distance -> moisture, biome zone, material
```

`Landform` is classified by distance from the axis against the two configured widths — `valley_floor_half_width` and `highland_onset` — rather than by a threshold on an interpolant, so the boundaries stay where the configuration says they are. `Ridge` is a crest standing above its own plateau by more than 78 percent of the ridge amplitude.

There are no scattered magic constants: every amplitude, width, and threshold is a named field of `TerrainConfig`, and every fractal is a named `Fbm` with its feature size stated. Nothing depends on generation order, on which chunk asks, or on any sequential state, so negative coordinates and chunk boundaries are ordinary inputs.

### Hydrology as a cue, not a solver

There is no global hydrology simulation, and there is no erosion. What exists is one geometric construction that cannot produce the failures a naive river produces:

- `water_surface(x)` falls linearly downstream from `water_source_height` at the region's upstream edge. It is a continuous, strictly decreasing function of world `x`, so **the river cannot run uphill and no discontinuity can be introduced at a chunk seam**. Voxelization uses `floor`, so adjacent columns may legitimately differ by one top-water voxel on either side of any coordinate, including a seam; that is slope quantization, not a boundary step. Both properties are construction, not tuning.
- The channel bed is defined *relative to the water surface*, `water_surface(x) - river_bed_depth`, so the bed descends exactly as fast as the water does and the channel keeps its depth from one end of the region to the other without ever cutting above the waterline.
- The carve is a smooth bell centred on the meandering axis, widened and deepened around `pond_centre_x` to form the pond.
- Water fills every cell below the local water surface and above the terrain, and nothing else. Where the ground rises above the surface, the water simply stops.

The valley floor sits at 18.0 and the water surface never exceeds 15.6, so the meadow cannot flood; a test walks the floor at three offsets along the whole axis and asserts zero submerged samples.

### Materials and strata

Eleven materials: two grasses, soil, rock, deep rock, sediment, water, trunk, foliage, foliage highlight, and shrub. Identifiers start at 64 so a terrain chunk and an M2/M3 diagnostic chunk can coexist without ambiguity.

Surface selection is a priority order: rock on any slope at or above `cliff_slope`, sediment within two voxels of the water surface, then highland or meadow grass. **Sediment always separates grass from water**, which a test checks by walking outward from the axis and asserting that the first grass column stands clear of the waterline.

Below the surface, soil thins with slope and vanishes under rock. Under the soil, rock alternates between its two tones **on a fixed world-height band of five voxels**, not by depth below the surface. That choice is the difference between a cliff that reads and one that does not: a heightmap cliff exposes only as many voxels per column as the terrain drops, which on any walkable gradient is fewer than the depth a depth-banded second tone would start at, so every face would be one flat colour. Banding by world height gives every exposed face horizontal colour breaks that line up across neighbouring columns, the way sedimentary layers do. It is the single most visible change in the whole slice.

### Vegetation

Vegetation is world geometry, not entities. A tree is a set of voxels written into whatever chunks it overlaps.

Placement is a lattice, never a sequential draw. Each lattice cell is jittered by a hash of that cell, capped at half the slack between the lattice spacing and the minimum spacing, which makes the six-voxel minimum trunk separation **a theorem rather than a hope**: two neighbouring anchors are `tree_spacing` apart and each moves by at most that cap. A chunk enumerates every tree that can reach into it by scanning the cells within one maximum tree reach of its bounds, so generating either side of a boundary produces the same tree, voxel for voxel. A descriptor is emitted only when its complete tree canopy/trunk or shrub footprint fits the finite horizontal and vertical region; an anchor near an edge cannot silently hang into a `KnownAbsent` chunk.

Rejections are the rules: too steep, in or beside the water, on a surface that is not soil-backed grass, outside the region, or not dense enough for its biome zone. Tree height is 8 to 11, canopy diameter 5 or 7, and the two are tied together so every tree stays inside the style bible's 40-to-70-percent canopy-to-height band. The canopy highlight is a hash of the world position restricted to the sunlit upper half, which keeps it a lit form rather than salt-and-pepper.

Low vegetation is a two-by-two base with a narrower crown. A one-voxel shrub was tried first and read as speckle in the captures, which is exactly what the style bible rejects.

### Biome

One dominant biome with micro-zones decided by slope, height above water, landform, and a moisture field: `RiverBank`, `Meadow`, `MeadowWood`, `Slope`, `Highland`, `RockyRidge`. Zones drive material choice and vegetation density; none of them is a per-chunk random draw.

### Streaming integration

`veldwake-streaming` gained a two-method `ChunkSource` trait — `fingerprint()` and `load(ChunkCoord)`. `DiagnosticChunkSource` implements it unchanged, and `TerrainChunkSource` adapts the generator by mapping `Option<Chunk>` to `SourceChunk`. The runtime gained `with_source` and `with_source_and_cache`; `new` and `with_cache` keep their diagnostic behaviour so the whole M3 regression suite still tests what it claims to.

`StreamingConfig::m4_golden()` is visible radius 6, `Lod0` only, retention 7: 2,197 render, 3,211 dependency, 3,375 retention coordinates. The M3C decision that the LOD band stays opt-in is unchanged; `m4_golden_banded()` exists for the compatibility run and is never the default. `StreamingRuntime::with_source_and_cache` validates the cache identity against `source.fingerprint()` before it starts the worker: an A cache may not be attached to B and cannot warm-replay A content as B.

### Rendering

The client now runs three passes instead of one:

1. **Shadow pass.** Depth only, from the sun, into one 2048-square cascade covering a 224-voxel box centred ahead of the camera. The box centre is snapped to a whole shadow texel, so a sub-texel camera move does not slide the sampling grid and shadow edges do not crawl. Front faces are culled so acne lands on surfaces the camera cannot see.
2. **Sky pass.** One full-screen triangle, no depth write, drawn before the world. Three-stop vertical gradient with a soft sun disc and two halo lobes.
3. **World pass.** Directional sun, hemispheric ambient blended between a cool sky term and a warm ground bounce by surface normal, a three-by-three percentage-closer shadow filter, a small fixed per-axis value offset so voxel form reads even out of the sun, a tight specular lobe that only water and rock carry, and exponential distance fog with a height component.

Fog resolves toward the style bible's fog colour along the horizon and toward the sky gradient as the view ray tilts up, so a ridge dissolves into what is drawn behind it instead of into a flat band. The gradient function is the same one the sky pass uses.

Every art value the shaders read comes from `lighting.rs`, which transcribes the style bible; no shader invents a colour or an intensity. The vertex carries position, normal, colour, and specular — forty bytes, up from twenty-four.

Weather is exactly two states, toggled by `F3`: clear and overcast. Overcast lowers the sun to 0.42, raises ambient to 0.62, cools the sun colour, greys the sky and fog, thickens fog by 1.85, and halves water's specular. It is a switch, not a simulation.

## Observed evidence

All numbers below were measured on the audited Windows 11 host with the Intel Iris Xe adapter over D3D12, release builds, golden seed. They are observations on one machine, never targets.

### Generation and meshing, headless

`terrain-probe bench 256` over the region's first 256 chunks:

| measure | value |
|---|---|
| generation, mean | 0.967 ms per chunk |
| generation, median / p95 / max | 0.951 / 1.228 / 1.567 ms |
| meshing (standalone, no neighbours), median / p95 | 0.602 / 0.743 ms |
| solid voxels | 6,855,033 over 256 chunks |

The standalone mesh sizes above the bench prints are not the streamed sizes: without neighbours the mesher walls off all six chunk faces. The streamed figures are in the client section.

### Vegetation coverage

`terrain-probe vegetation` over the band `x -360..360`, `z -120..120`: 631 trees, 1,975 shrubs, 115,968 grass columns, canopy over 26,308 of them — **22.7 percent**, inside the style bible's 35-percent ceiling. Tree heights are spread across all four allowed values.

### Client, golden profile, settled

`m4-golden`, pose `depth-stack`, 1600 × 900, `Fifo`, debug views off:

| measure | value |
|---|---|
| time to idle | 59,653 ms |
| tracked / known absent | 3,211 / 2,767 |
| presented chunks | 351 (147 of them empty) |
| GPU-resident chunk meshes | 204 |
| presentation-owned chunk-mesh bytes | 65,992,424 |
| GPU quads | 358,619 |
| CPU resident payload | 29,097,984 bytes |
| CPU mesh bytes | 48,772,184 |
| snapshot build, total | 20,395 µs |
| worker mesh, total / max | 213,838 µs / 3,490 µs |
| frame interval, settled | 16.83 ms mean, 59.4 FPS observed |
| renderer render wall time, mean / max | 12,638 µs / 30,163 µs |
| stale loads / stale meshes / cap blocks | 0 / 0 / 0 |
| gap frames / ready-undrawn max / commit failures | 0 / 0 / 0 |

The frame interval is vsync-bound at 60 Hz, so it measures the presentation cadence rather than the renderer's headroom. **Renderer render wall time wraps the complete `Renderer::render()` call, including surface acquisition, encoding, submission, and presentation; it is neither isolated CPU-submit time nor GPU time.** No GPU timestamps were taken, and none of these numbers may be read as GPU cost.

The 60-second settle is the dominant cost and is not a rendering cost. The runtime dispatches at most one worker job per `poll`, and `poll` runs once per frame, so at 60 Hz the pipeline is bounded at about sixty jobs per second whatever the worker can actually do. The golden profile needs 3,211 loads plus 351 meshes. Recorded as KI-017.

### LOD band compatibility

Same pose, same settle, `m4-golden-banded`:

| measure | `m4-golden` | `m4-golden-banded` |
|---|---|---|
| presented | 351 | 351 |
| GPU-resident meshes | 204 | 212 |
| GPU quads | 358,619 | 118,477 |
| chunk-mesh bytes | 65,992,424 | 21,806,552 |
| CPU mesh bytes | 48,772,184 | 16,112,872 |
| time to idle | 59,653 ms | 59,589 ms |
| LOD swaps / stale LOD / gap frames | — | 0 / 0 / 0 |

Against real terrain the band settles to **67 percent fewer chunk-mesh bytes** at the same camera, which is a far larger saving than the 16.9-to-19.6-percent simultaneous-peak saving M3C measured against the diagnostic fixture (KI-013). The two numbers measure different things — settled committed bytes at one camera against simultaneous peak over a traversal — and the difference is mostly that real terrain has vastly more surface for coarsening to remove. The band remains opt-in and was not used for any golden capture.

### Disk cache re-measured against real terrain

`streaming-probe terrain`, default profile (81 chunks) centred at `(0, 1, 0)`, cache in the system temporary directory:

| phase | raw | run-length |
|---|---|---|
| no cache, time to idle | 60,493 µs | 67,866 µs |
| cold, time to idle | 109,949 µs | 109,708 µs |
| warm, time to idle | 18,953 µs | 10,670 µs |
| warm again, time to idle | 16,655 µs | 11,424 µs |
| disk bytes, 81 entries | 4,132,656 | 107,904 |
| encode, mean / max | 92 / 279 µs | 15 / 108 µs |
| decode, mean / max | 76 / 182 µs | 14 / 48 µs |

Two conclusions, both reversals of an M3D finding:

- **The cache is now clearly worth having.** Against the diagnostic fixture it cost more than regenerating (KI-016), because the fixture was a trivial generator. Against real terrain, a warm run reaches idle in 11 ms where regeneration takes 68 — about six times faster. KI-016 stands as written for the diagnostic source and is now scoped to it.
- **Run-length is the current experimental default payload encoding.** M3D chose raw because the only content was 97 percent air, where run-length's bounded worst case looked like the larger risk. The 81-chunk terrain cache sample is strongly favourable (107,904 bytes instead of 4,132,656; 97.4-percent reduction, encode about six times faster and decode about five times faster), but it is not a representative distribution by landform and must not be generalized to future worlds. The cache itself remains experimental; retain both decoders and re-measure stratified valley/water, cliff, vegetation, highland, and sky chunks before treating this default as a production compression policy. Worst-case RLE remains exactly 3× raw and bounded by `MAX_ENTRY_BYTES`.

### Visual evidence

Final branch-QA evidence was recaptured at `f9c12d628f92b59b685d3493a3c0213332214ef8`: six maximized-client frames (`1536 x 792` actual client area) after a 70-second settle, `m4-golden`, clear weather, plus `valley-wide` overcast. The release client reported Intel Iris Xe / D3D12, world fingerprint `0x96aef6bb59dca573`, idle coverage, and exit code 0 with no validation, device-lost, or fatal error. The full golden smoke also exercised F1/F2/F3, resize, minimize/restore, focus loss with movement held, and motion across positive and negative coordinates. A separate `m4-golden-banded` settled traversal reported active Lod0 and Lod1 meshes with no validation error; the M3 diagnostic smoke and clean capture preserved the checkerboard material fixture. The captured frames show continuous shorelines and terrain, grounded vegetation, readable strata and water, coherent sky/fog, and a visibly flatter overcast state. Their remaining cube-canopy and uniform-meadow character is the documented style limitation, not a new regression.

Measured luminance bands of `depth-stack` (top to bottom, sky included in the first two):

| band | p05 | median | p95 | spread |
|---|---|---|---|---|
| 0 | 0.125 | 0.867 | 0.867 | 0.741 |
| 1 | 0.578 | 0.683 | 0.867 | 0.289 |
| 2 | 0.517 | 0.616 | 0.687 | 0.170 |
| 3 | 0.433 | 0.580 | 0.641 | 0.208 |

Sky sits at 0.867, inside the style bible's 0.62-to-0.90 band and lighter than the terrain in front of it, so silhouettes read. The background band is compressed to 0.517–0.687 while the foreground keeps 0.433–0.641, which is the atmospheric value compression the style bible asks for.

#### Written assessment

**What is beautiful.** The banded cliffs. The horizontal strata turned the valley wall from a grey ramp into the region's landmark, and they are the single change that most separates these captures from a voxel-game screenshot. `pond-shore` and `forest-pocket` both work as photographs: a foreground tree with a readable trunk, a mid-ground water body or canopy, and a banded wall behind, separated by air rather than by outline. Shadows do real work — tree shadows on the meadow give the ground scale, and the canopy self-shadowing gives the forest volume. The water's specular highlight, once the camera faces the sun's reflection, is unmistakably a liquid cue that no terrain surface has.

**What is generic.** The tree silhouette. There is one tree family with two sizes, and at any distance the canopies read as a field of similar blobs. The meadow is a single flat green with no ground-level variation other than shrubs; real ground would have patchiness at a scale between the fine detail and the material bands. The highland plateau, seen alone, is the least interesting part of the region.

**What still looks like Minecraft.** Nothing, in the cliff and water shots. In the open-meadow shots, the uniform grass and the cube-stacked canopies still read as a block game, and the trunk-plus-blob tree is the most recognisable borrowing. The terracing on gentle slopes — one-voxel steps across a wide gradient — is the other tell; it is inherent to a heightmap at this voxel size and would need a different surface representation to remove.

**Where the visual reading fails.** Overcast on a rock-dominated frame is close to the flat grey mush the style bible rejects: the `cliff-face` overcast capture is legible but barely, because the strata contrast is what carries that frame and overcast compresses exactly that. Framing a pose without foreground produces a washed pastel image; three of the six poses had to be re-placed after the first captures showed the camera either inside a hillside or inside a canopy, which is now a test.

**Which changes had the biggest impact**, in order: world-height rock strata; steepening the valley wall from an 88-voxel transition to 52, which turned a ramp into a cliff; putting the cameras above the canopy so frames have a foreground; facing the pond camera into the sun's reflection so water's defining cue is visible at all; and giving shrubs a two-by-two base so low vegetation reads as plants instead of speckle.

## Accepted behaviour

- The visible radius is 208 voxels and the valley is 264 voxels wide, so the far rim is never drawn from the near rim. The far side dissolves into fog before the wall would appear, which reads as depth rather than as a cut edge, but a shot showing both rims is not possible at this profile.
- Water is opaque. Transparency, refraction, and motion are out of scope and recorded as a limitation, not pretended away.
- The shadow map is one cascade covering 224 voxels. Beyond it the world is lit, not dark, because a box edge that darkens everything past it is worse than no shadow there.
- The valley wall is steep enough that `BiomeZone::Slope` occupies only a narrow band at the foot and the crown of the wall. That is the terrain being a cliff, not a classification bug.
- Settling the golden profile takes about a minute. See KI-017.

## Explicit non-goals

None of the following was implemented, and none may be inferred from this milestone: tectonic simulation, plate history, global erosion, a global hydrology solver, caves, procedural structures or settlements, civilisation or history, gameplay, a player, characters, physics, collision, combat, audio, networking, an ECS, multiplayer, authoritative saves or player edits, multiple worker threads, a generalised render graph, a generalised material engine, physically based rendering, cloud or volumetric simulation, a cascaded shadow framework, ocean simulation, or an editor.

The disk cache remains an experiment and a discardable accelerator. It is still not a save format and still promises nothing across builds.

## Gates

| gate | result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace` | PASS, superseded by 275-test branch-QA run |
| release `terrain-probe` | PASS |
| release `streaming-probe` | PASS |
| Windows/D3D12 driven smoke, six poses, two weather states | PASS, exit code 0, no validation errors |
| `cargo nextest run --workspace` | PASS, 275 tests in branch QA |
| `cargo deny check` | PASS — advisories, bans, licenses, sources all ok; KI-007's duplicate warnings are unchanged |
| `cargo audit` | PASS — 207 crate dependencies scanned, no advisory. No dependency was added in this milestone |
| 1080p/60 budget | NOT YET APPLICABLE — the budget is Proposed and unaccepted; captures are 1600 × 900 and vsync-bound |
