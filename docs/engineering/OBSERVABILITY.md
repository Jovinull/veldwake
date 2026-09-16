# Observability

Status: **Accepted requirement; implementation staged**.

Development builds should explain performance and procedural causality, not merely display failures.

## Runtime telemetry

A future overlay may show FPS, CPU/GPU frame time, frame phases, RAM, VRAM estimate/budgets, draw calls, triangles, visible/resident chunks, generation and meshing queues, entities, simulation ticks, network latency/traffic, save work, cache hits, and audio voices/DSP time.

Metrics need units, sampling windows, percentiles where relevant, and low enough overhead for routine use. Structured `tracing` spans should connect async generation, uploads, saves, and simulation work by stable IDs.

## World inspection

Tools should query a coordinate/entity/settlement and report inputs and causes such as biome, elevation, temperature, humidity, geology, resources, seed path, generator/version, civilization/faction influence, population, danger, routes, events, and materialization state.

## Failure diagnostics

- Validation errors identify the descriptor/path/version and rejected constraint.
- Async jobs expose queue, priority, age, cancellation, and outcome.
- Save/network errors are never silently replaced with arbitrary defaults.
- GPU adapter/backend/features/limits and validation messages are recorded at startup during rendering milestones.

Procedural generation without decision provenance is considered incomplete because regressions become impractical to reproduce.
