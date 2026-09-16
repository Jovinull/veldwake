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

## Initial physical structure

M0 contains one dependency-free `foundation` crate to validate Rust policy. M1 should introduce only the minimum application/platform/render boundaries needed for a window and frame loop. The large conceptual crate tree in the historical transcript is direction, not a scaffold instruction.
