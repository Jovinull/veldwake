# Veldwake

> **Working title:** `Veldwake` has not received legal trademark clearance.

Veldwake is a planned 3D voxel action RPG about exploration in a persistent, systemic procedural world. Geography, creatures, settlements, economies, and events can evolve, while player actions leave observable consequences. Its audiovisual identity is intended to be produced predominantly through code and constrained generators rather than a traditional manual asset pipeline.

Current status: **M3B streaming runtime is merged; M3C initial LOD is implemented on its feature branch and measured — one coarse level cuts resident GPU bytes 57% but costs 74% more upload bytes on a moving camera, so it stays opt-in; streaming debug visualization is next**. The Windows-first diagnostic client streams a finite code-defined chunk corridor around the camera through `wgpu`/D3D12 under explicit residency and upload budgets. It is an engineering proof, not a playable world: there is no world generation, persistence, LOD, or gameplay.

## Principles

- **Wonder:** discovery should create genuine curiosity.
- **Mastery:** progression should unlock possibilities, not only larger numbers.
- **Consequence:** the world should remember changes the player can observe.
- Structured generation uses constraints, style grammars, determinism, and validation—not unconstrained randomness.
- Authoritative simulation remains independent from rendering and client presentation.

## Initial technical direction

Rust 2024 on pinned stable Rust, with `wgpu`, WGSL, `winit`, and `glam` in the presentation client. This is a custom engine project; future candidates such as Rapier3D, `cpal`, `zstd`, `quinn`, and WebAssembly are not dependencies until their milestone needs them.

## Setup and validation

See [environment setup](docs/environment/SETUP.md). With Cargo available:

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace
cargo run -p veldwake-client
cargo run --release -p veldwake-streaming --bin streaming-probe
```

Diagnostic controls: WASD moves, Space/Control move vertically, hold the right mouse button to look, and Escape exits. Set `RUST_LOG` to change diagnostic filtering and `VELDWAKE_PROFILE` (`default`, `m3c-baseline`, `m3c-banded`) to choose the streaming profile.

## Start here

- [Repository rules](AGENTS.md) (Claude Code entry point: [CLAUDE.md](CLAUDE.md))
- [Documentation index](docs/INDEX.md)
- [Current project state](docs/PROJECT_STATE.md)
- [Game vision](docs/vision/GAME_VISION.md)
- [Architecture](docs/engineering/ARCHITECTURE.md)
- [Roadmap](docs/planning/ROADMAP.md)
- [Current handoff](docs/agents/HANDOFF.md)
