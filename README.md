# Veldwake

> **Working title:** `Veldwake` has not received legal trademark clearance.

Veldwake is a planned 3D voxel action RPG about exploration in a persistent, systemic procedural world. Geography, creatures, settlements, economies, and events can evolve, while player actions leave observable consequences. Its audiovisual identity is intended to be produced predominantly through code and constrained generators rather than a traditional manual asset pipeline.

Current status: **M4 — Beautiful Terrain Vertical Slice is complete and merged; M5 — Procedural Character is implemented on `feat/m5-procedural-character` and awaiting independent branch QA.** The Windows-first client streams one deterministic 800 x 96 x 800 voxel region — a verdant highland valley with a meandering river, a pond, cliffs banded like sedimentary rock, forest pockets, and low vegetation — around the camera through `wgpu`/D3D12, lit by a directional sun with a filtered shadow map, a procedural sky, height-aware distance fog, and two weather states. Generation is a pure function of a seed and a chunk coordinate, so the same world appears every run. LOD stays opt-in from M3C. M5 adds a generated humanoid to that region: one descriptor compiles to sixteen rigid voxel body parts on a sixteen-bone skeleton, walks and runs on analytical gaits whose phase advances with distance, and puts its soles on the block tops the viewer can see. It is still an engineering proof, not a playable game: there is no persistence, no player control, and no gameplay.

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

Diagnostic controls: WASD moves, Space/Control move vertically, hold the right mouse button to look, and Escape exits. `F1` cycles the streaming debug views (off, level tint, residency wireframes, chunk boundaries and transition groups) and `F2` toggles the wireframe boxes. `Off` performs no per-frame debug primitive allocation, debug uniform write, or debug draw; fixed startup debug resources and previously pooled slots remain allocated. `F3` toggles the weather between clear and overcast. Set `RUST_LOG` to change diagnostic filtering, `VELDWAKE_PROFILE` (`default`, `m3c-baseline`, `m3c-banded`, `m4-golden`, `m4-golden-banded`) to choose the streaming profile, `VELDWAKE_WORLD` (`golden`, `diagnostic`, `seed:<value>`) to choose the world, and `VELDWAKE_POSE` to start at one of the named golden camera poses.

## Start here

- [Repository rules](AGENTS.md) (Claude Code entry point: [CLAUDE.md](CLAUDE.md))
- [Documentation index](docs/INDEX.md)
- [Current project state](docs/PROJECT_STATE.md)
- [Game vision](docs/vision/GAME_VISION.md)
- [Architecture](docs/engineering/ARCHITECTURE.md)
- [Roadmap](docs/planning/ROADMAP.md)
- [Current handoff](docs/agents/HANDOFF.md)
