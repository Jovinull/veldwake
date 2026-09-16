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
- `crates/voxel`: dependency-free CPU voxel/chunk representation, signed spatial coordinates, borrowed axial neighborhoods, deterministic fixtures, and exposed-face reference mesher. It contains no platform, client, world-map, or GPU types.
- `crates/streaming`: headless CPU residency and detached-meshing orchestration. It depends only on `veldwake-voxel` and `std`, owns the finite diagnostic source and authoritative resident diagnostic chunks, and contains no camera, platform, renderer, GPU, persistence, or gameplay types.
- `apps/client`: the presentation executable. Internal `app`, `renderer`, `camera`, `input`, and `diagnostics` modules keep platform translation and GPU work away from pure camera/input behavior. At M3A startup, `app` builds the static fixture and neighbor-aware local meshes; `renderer` converts borrowed meshes into client-owned GPU vertices/buffers and applies immutable per-chunk model translations.

The renderer owns only disposable GPU/window state and the diagnostic voxel presentation. Voxel IDs become colors and integer chunk origins become `f32` translations only in the client adapter; CPU mesh vertices stay chunk-local. The voxel and streaming crates contain no `wgpu`, `winit`, `bytemuck`, or client types. The renderer does not own world, simulation, or gameplay authority. M2/M3A justify the voxel boundary; M3B1 justifies streaming as a second domain crate through real residency, concurrency, and headless-test ownership. The large conceptual tree remains direction, not a scaffold instruction.
