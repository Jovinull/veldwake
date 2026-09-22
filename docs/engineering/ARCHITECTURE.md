# Architecture

Status: **Accepted dependency principles; physical crate map evolves by capability**.

## Logical domains

```text
platform       window, input, clocks, OS integration
render         GPU resources, frame graph/passes, presentation
voxel          voxel representation, meshing interfaces, edits
procedural     descriptors, generators, compiler/cache contracts
world          spatial state, chunks/regions, persistence-facing model
simulation     time, aggregate/local materialization, causal systems
gameplay       commands, rules, combat/progression interactions
physics        collision and dynamics adapter
animation      skeleton, procedural motion, IK, pose output
audio          DSP, mixer, spatial audio, music state
network        versioned transport/protocol adapters
server         authority, orchestration, persistence
client         intent, snapshots/views, presentation orchestration
tools          compilers, inspectors, previews, benchmarks
```

Logical separation does not require one crate per noun. Create a crate only when ownership, build isolation, dependency direction, reuse, or testability justifies the boundary.

## Dependency laws

```text
platform ---------> client/presentation adapters

procedural -------> world data (GPU-independent)
world ------------> simulation/gameplay inputs
gameplay ---------> domain primitives and declared service interfaces
simulation -------> world/gameplay primitives
physics ----------> simulation adapter boundary

authoritative core ----X----> render/audio/UI/platform presentation
server ----------------X----> render/audio/UI
render ----------------------> immutable/interpolated presentation views
client ----------------------> commands/intents to authority
```

`X` marks forbidden dependencies. Renderer state is disposable presentation state; it does not own authoritative entities, chunks, inventory, progression, or world history.

## Execution model direction

- Authoritative simulation uses a defined timestep strategy; rendering may interpolate.
- Generation, meshing, save compression, and disk I/O execute outside frame-critical work.
- Work queues need priority, cancellation, budgets, and observability before a custom general job system is justified.
- Global simulation is aggregate and low frequency; local simulation materializes detail.
- Headless server and generation tests operate without GPU/window/audio devices.

## Voxel identifier ranges

`VoxelId` is a `u16` and `veldwake-voxel` assigns it no meaning. Content domains own disjoint declared ranges so an identifier is never a global magic number, and the disjointness is proved by tests rather than by this table alone.

| range | owner | declared by |
|---|---|---|
| `0` | air | `veldwake-voxel` |
| `1..=63` | M2/M3 diagnostic fixture and corridor | `veldwake-voxel`, `veldwake-streaming` |
| `64..=127` | terrain (`64..=74` used today) | `procedural::material::FIRST_TERRAIN_ID` |
| `128..=191` | characters (`128..=137` used today) | `character::material::CHARACTER_ID_FIRST` |
| `192..=223` | weapons (`192..=197` used today) | `combat::material::FIRST_WEAPON_ID` |
| `224..=255` | landmarks (`224..=226` used today) | `procedural::landmark::LANDMARK_ID_FIRST` |
| `256..=65535` | unallocated | — |

`veldwake-character` proves its own range is disjoint from terrain's, `veldwake-combat` proves the weapon range `192..224` is disjoint from both, `veldwake-procedural` proves the landmark range `224..256` is disjoint from terrain's, and `apps/client` — the one place all five lookups are visible at once — proves the renderer's ordered lookup cannot be ambiguous. Landmarks did **not** extend `TerrainMaterial`: they are a separate table in a separate range, because a landmark is not a kind of ground. A new domain takes the next free range and adds its own round-trip and disjointness tests; it never extends somebody else's enum.

## Current physical structure

- `crates/foundation`: dependency-free policy anchor; still intentionally small.
- `crates/voxel`: dependency-free CPU voxel representation as an edge-generic `DenseGrid<EDGE>` (`Chunk = DenseGrid<32>`, `CoarseGrid = DenseGrid<16>` via the explicit `downsample_2x`), signed spatial coordinates, borrowed axial neighborhoods and owned slabs generic over the edge with 32 as the default, deterministic fixtures, and one exposed-face reference mesher loop. It contains no platform, client, world-map, or GPU types.
- `crates/procedural`: deterministic CPU world generation. It depends only on `veldwake-voxel` and `std`, and contains no GPU, window, camera, frame, filesystem, or scheduling types. Generating a chunk is a pure function of its coordinate and a `WorldIdentity`, so the same call answers the same way on the streaming worker, in a headless test, and in the `terrain-probe` binary. It owns terrain meaning: the composed height and water field, biome micro-zones, the vegetation grammar, and the one table that maps a `TerrainMaterial` to a `VoxelId` and to a linear-RGB colour. That table is the documented boundary between generation and presentation — `veldwake-voxel` stays a generic container with no terrain semantics, and the renderer asks this crate for a colour instead of keeping a second palette.
- `crates/streaming`: headless CPU residency and detached-meshing orchestration. It depends on `veldwake-procedural`, `veldwake-voxel`, and `std`, owns the finite diagnostic source and authoritative resident diagnostic chunks, validates its configuration, bounds eviction finalization, and exposes a per-frame `ResidencySummary`. It contains no camera, platform, renderer, GPU, or gameplay types. Since M4 the content itself arrives through a two-method `ChunkSource` trait — `fingerprint()` and `load(ChunkCoord)` — implemented by the finite diagnostic corridor and by `TerrainChunkSource`, which adapts `veldwake-procedural` by mapping `Option<Chunk>` to `SourceChunk`. The trait is deliberately two methods: the runtime does not care how a chunk is made, only that the same coordinate always answers the same way, that absence is authoritative, and that the producer can name itself so a cache knows whose bytes it holds. It is not a plugin system and has exactly two implementations. Since M3D it also owns an optional, experimental disk cache of source results (`cache::{format, store}`). Cold `ChunkCache::open` cleanup/footprint discovery is synchronous on the caller (the client uses it once during event-loop initialization); after construction, per-chunk lookup/decode/publish is confined to the worker and never enters the frame hot path. The cache is discardable and not authoritative persistence. No crate was split out: one consumer, no build-isolation need, and no dependency inversion. `veldwake-voxel` remains filesystem-free, and the private format reads cells through its public API rather than duplicating voxel representation.
- `crates/combat`: the fixed-step combat domain. It depends on `veldwake-character`, `veldwake-voxel`, `glam` and `std`, and contains no GPU, window, camera, audio, filesystem or scheduling types. It owns combat meaning: the tick clock, the weapon compiler and its own `VoxelId` range `192..224`, the two attack specifications, the two combatants and the typed states one can be in, the swept-blade hit query, the hurt volume, kinematic movement against `GroundSampler`, the adversary's state machine, the bounded event type, and the encounter that orders all of it. It passed the crate test on ownership (a distinct domain with its own identifier range, its own style version and its own fixtures), on dependency direction (placing it in `character` would make a crate about bodies know what a hit is), and on build isolation (`veldwake-character` must not compile a combat domain it never calls). It depends on `character` and never the reverse: combat says *which action and how far through*, and `veldwake-character` owns every joint angle. Authority runs one way — `apps/client` may read an encounter and may never write one except through an intent, which is [ADR-0002](../adr/0002-presentation-independent-authority.md) applied to a fight.
- `crates/character`: deterministic CPU character compilation, skeleton, and procedural locomotion. It depends on `veldwake-voxel`, `glam`, and `std`, and contains no GPU, window, camera, frame, filesystem, or scheduling types. Compiling a character is a pure function of a descriptor; posing one is a pure function of a compiled character, a runtime state, and a ground query. It owns character meaning: the descriptor and its typed validation, the sixteen-bone humanoid skeleton, the rigid voxel body parts, the one table that maps a `CharacterMaterial` to a `VoxelId` and to a colour, the collision representation, the analytical gaits, and the two-bone leg IK. It deliberately does **not** depend on `veldwake-procedural`: a headless character crate has no business compiling a world generator, and the one thing it needs from terrain — the height under a foot — is the two-line `GroundSampler` trait that the client implements over `TerrainField`. It passed the crate test on ownership (a distinct domain with its own identifier range, its own versions, and its own style contract), on dependency direction (placing it in `procedural` would falsify that crate's documented charter), and on build isolation (`veldwake-streaming` depends on `procedural` and would otherwise compile skeleton, IK, and locomotion code it never calls).
- `apps/client`: the presentation executable. Internal `app`, `renderer`, `camera`, `input`, `streaming`, and `diagnostics` modules keep platform translation and GPU work away from pure camera/input behavior. `streaming` owns `StreamingBridge`: it floors the camera position into a demand center, polls the runtime once per frame, keeps `ChunkCoord -> MeshStamp` for presented/staged targets, deactivates data-stale meshes before uploading, retains presentation-stale meshes until their transition group commits atomically, and applies upload/removal budgets through `ChunkPresentation`. A transition group commits on both sides or neither: `StreamingRuntime::commit_group_with` preflights unique/non-empty ready membership and holds its exclusive mutable borrow through the presentation's all-or-nothing `commit_staged_group` callback and CPU commit; bridge bookkeeping follows only after success. `renderer` implements the trait over `BTreeMap<ChunkCoord, GpuChunkSlot>` and draws only committed drawable entries. The client's `debug` module turns runtime/bridge state into wireframe primitives; the renderer decides nothing about streaming.

The character reaches the screen through one adapter and one shared shader. `apps/client/src/character.rs` is the whole boundary between `veldwake-character` and the world: `TerrainGround` answers `GroundSampler` from a `TerrainField`, `CharacterScene` owns the compiled character and what drives it, and the two diagnostic courses and the portrait stand live there because a headless character crate has no business knowing where a character stands. `apps/client/src/shading.wgsl` holds the scene bindings, the sun term, the shadow lookup and `shade_surface`, and both `world.wgsl` and `character.wgsl` call it. That is a deliberate structural choice rather than a convenience: terrain and character sharing one lighting function is the only way the two cannot drift apart, and a character lit by its own copy of the sun is exactly what makes a figure look pasted onto a scene. Character geometry is static and uploaded once; per frame the client writes one eighty-byte uniform per part and issues one world draw and one shadow draw per part.

The renderer owns only disposable GPU/window state and the diagnostic voxel presentation. Voxel IDs become colors and integer chunk origins become `f32` translations only in the client adapter; CPU mesh vertices stay chunk-local. The voxel and streaming crates contain no `wgpu`, `winit`, `bytemuck`, or client types, and the renderer never sees streaming stamps or generations. The renderer does not own world, simulation, or gameplay authority. M2/M3A justify the voxel boundary; M3B justifies streaming as a second domain crate through real residency, concurrency, headless-test ownership, and a client bridge that is itself tested against an in-memory presentation double. `ChunkPresentation` is a testability seam with two implementors, not a render abstraction. The large conceptual tree remains direction, not a scaffold instruction.

## The traversal boundaries

Three seams M7 added or sharpened, each one deliberate:

- **`GroundSampler` answers where the ground is; `TraversalLegality` answers whether a body may go there.** The first is `veldwake-character`'s and is unchanged, including the part that makes it right: inside a river it reports the bed, which is what feet, IK and the pelvis need. The second is a new two-line trait in `veldwake-combat`, consulted only by `movement::check_move` and only about a move's **destination**, so a body that somehow stands somewhere forbidden can still walk out. Merging them would have turned a contact model into a rules model.
- **Movement legality is one function with one typed outcome.** `check_move` returns `Result<(), MoveBlockReason>`; `accepts` wraps it; `try_move` and the offline reachability audit both call it. An audit that recomputed "why did that fail" would be a second implementation of the rule, and the first thing to drift.
- **Where a body starts is placement, not tuning.** `EncounterSetup` carries absolute `starts`, an `Option<ArenaSpec>` and the player-victory policy; `AuthoredTuning` is durations, distances and health. A fight in a disc can be written either way; a walk from one end of a route to the other cannot, because a centre plus two large opposite offsets names a point that is neither body and neither the arena.

`apps/client/src/traversal.rs` is where the world side of all three lives: the water veto over a `TerrainField`, one sampled copy of the region's walkable surface, the reachability audit that walks it with the game's own rule, the derived route and its locked signature, and the adversary's derived placement. It lives in the client for the same reason `arena.rs` does — the combat crate has no business knowing where in a world a fight happens, and the audit is evidence machinery with one consumer. A second consumer, such as a headless server or a generation step that needs reachability, is the trigger to extract it, and doing so would be a move rather than a rewrite.

## The combat slice's boundaries

Three seams M6 added, each one deliberate:

- **`veldwake-combat` → `veldwake-character`, never the reverse.** Combat produces an `ActionOverlay` naming which action and how far through it is; the character crate turns that into sixteen joint transforms with its own curves, its own blending and its own joint clamps. Combat never computes an angle and the character crate never learns what a hit is.
- **The domain is the only authority and the client only reads it.** The client's frame advances whole ticks, hands in an `Intent`, and reacts to the `CombatEvent`s that come back. Every presentation response — damage, stagger, knockback, hitstop, the reaction pose, the impact chips, the sound and the camera impulse — originates from one event, so presentation cannot invent a hit the rules did not produce.
- **Audio is a pure synth behind a thin device.** `apps/client/src/synth.rs` has no cpal, no threads and no I/O; `apps/client/src/audio.rs` is the only code in the repository that knows cpal exists, and the two communicate through a lock-free single-producer ring of atomics. That split is what makes every claim about the sound testable headlessly, and it is [ADR-0007](../adr/0007-procedural-impact-audio-boundary.md).

## The landmark boundaries

M8 added world content that has to be visible, solid and drawn, which touches three domains at once. The boundaries it drew are the reason it did not become a fourth crate.

**Landmarks live inside `veldwake-procedural`**, in a `landmark` module, because they fail all three parts of the crate test: same owner as terrain, same lifecycle — a pure function of a coordinate and an identity — and no build-isolation need, since every consumer of the crate needs landmarks in its chunks. A separate crate would also invert the dependency, because placement reads the terrain field and the chunk generator writes the result.

**The plan is derived when a world is built, never when a chunk is generated.** `TerrainGenerator::new` returns `Result<Self, WorldError>` and holds an `Arc<LandmarkPlan>`; `generate(coord) -> Option<Chunk>` has no channel for a planning failure and must not acquire one. The generator lost `Copy` and kept a cheap `Clone`. Reuse is explicit: `TerrainGenerator::with_plan` shares a derived plan, and `WorldSelection::build` is how the client gets one world instead of three.

**Visibility has two levels and they may not merge.** `procedural::landmark::visibility` is a *world* proxy: observer column, eye height as a composition control, terrain, water, canopy, silhouette, distance, geometric clearance. It contains no field of view, no pixel, no camera, no renderer and no fog. `apps/client/src/landmark.rs` is the *presentation* oracle: the real follow camera, the real projection, the real fog table, normalised device coordinates. Placement may only use the first; evidence uses both, and a test asserts they do not contradict each other.

**The world owns no player.** The plan produces a `DiscoveryOverlook`, which is a compositional property — the place the composition is built to be seen from. The client chooses to start a session there, and `the_route_starts_at_the_world_overlook` keeps `ROUTE_START_*` honest about where that is.

**No character dimension enters the world.** The plan exposes exact solid geometry, column by column. The client, which owns `CollisionRepresentation`, turns the widest body's capsule into a `TraversalLegality` veto and a `SurfaceGrid` flag. `GroundSampler` is untouched: a landmark is a refusal, never a surface, and nothing walks on one.

**One vegetation truth.** Once a landmark suppresses plants, the raw grammar no longer describes the world. `WorldVegetation` — grammar, field and plan — is the only thing that answers "is a plant actually here?", and chunk generation composes exactly it.

