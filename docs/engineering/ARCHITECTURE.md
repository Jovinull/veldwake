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

## Current physical structure

- `crates/foundation`: dependency-free policy anchor; still intentionally small.
- `crates/voxel`: dependency-free CPU voxel representation as an edge-generic `DenseGrid<EDGE>` (`Chunk = DenseGrid<32>`, `CoarseGrid = DenseGrid<16>` via the explicit `downsample_2x`), signed spatial coordinates, borrowed axial neighborhoods and owned slabs generic over the edge with 32 as the default, deterministic fixtures, and one exposed-face reference mesher loop. It contains no platform, client, world-map, or GPU types.
- `crates/streaming`: headless CPU residency and detached-meshing orchestration. It depends only on `veldwake-voxel` and `std`, owns the finite diagnostic source and authoritative resident diagnostic chunks, validates its configuration, bounds eviction finalization, and exposes a per-frame `ResidencySummary`. It contains no camera, platform, renderer, GPU, persistence, or gameplay types.
- `apps/client`: the presentation executable. Internal `app`, `renderer`, `camera`, `input`, `streaming`, and `diagnostics` modules keep platform translation and GPU work away from pure camera/input behavior. `streaming` owns `StreamingBridge`: it floors the camera position into a demand center, polls the runtime once per frame, keeps `ChunkCoord -> MeshStamp` for presented/staged targets, deactivates data-stale meshes before uploading, retains presentation-stale meshes until their transition group commits atomically, and applies upload/removal budgets through `ChunkPresentation`. A transition group commits on both sides or neither: `StreamingRuntime::commit_group_with` preflights unique/non-empty ready membership and holds its exclusive mutable borrow through the presentation's all-or-nothing `commit_staged_group` callback and CPU commit; bridge bookkeeping follows only after success. `renderer` implements the trait over `BTreeMap<ChunkCoord, GpuChunkSlot>` and draws only committed drawable entries. The client's `debug` module turns runtime/bridge state into wireframe primitives; the renderer decides nothing about streaming.

The renderer owns only disposable GPU/window state and the diagnostic voxel presentation. Voxel IDs become colors and integer chunk origins become `f32` translations only in the client adapter; CPU mesh vertices stay chunk-local. The voxel and streaming crates contain no `wgpu`, `winit`, `bytemuck`, or client types, and the renderer never sees streaming stamps or generations. The renderer does not own world, simulation, or gameplay authority. M2/M3A justify the voxel boundary; M3B justifies streaming as a second domain crate through real residency, concurrency, headless-test ownership, and a client bridge that is itself tested against an in-memory presentation double. `ChunkPresentation` is a testability seam with two implementors, not a render abstraction. The large conceptual tree remains direction, not a scaffold instruction.
