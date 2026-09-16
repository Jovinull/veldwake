# Veldwake

> **Working title:** `Veldwake` has not received legal trademark clearance.

Veldwake is a planned 3D voxel action RPG about exploration in a persistent, systemic procedural world. Geography, creatures, settlements, economies, and events can evolve, while player actions leave observable consequences. Its audiovisual identity is intended to be produced predominantly through code and constrained generators rather than a traditional manual asset pipeline.

Current status: **M0 — Repository & Engineering Foundation**. There is no playable game or renderer yet.

## Principles

- **Wonder:** discovery should create genuine curiosity.
- **Mastery:** progression should unlock possibilities, not only larger numbers.
- **Consequence:** the world should remember changes the player can observe.
- Structured generation uses constraints, style grammars, determinism, and validation—not unconstrained randomness.
- Authoritative simulation remains independent from rendering and client presentation.

## Initial technical direction

Rust 2024 on pinned stable Rust, with `wgpu`, WGSL, `winit`, and `glam` planned for the first rendering milestone. This is a custom engine project; future candidates such as Rapier3D, `cpal`, `zstd`, `quinn`, and WebAssembly are not dependencies until their milestone needs them.

## Setup and validation

See [environment setup](docs/environment/SETUP.md). With Cargo available:

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --no-tests=pass
```

## Start here

- [Repository rules](AGENTS.md)
- [Documentation index](docs/INDEX.md)
- [Current project state](docs/PROJECT_STATE.md)
- [Game vision](docs/vision/GAME_VISION.md)
- [Architecture](docs/engineering/ARCHITECTURE.md)
- [Roadmap](docs/planning/ROADMAP.md)
- [Current handoff](docs/agents/HANDOFF.md)
