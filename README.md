# Veldwake

> **Working title:** `Veldwake` has not received legal trademark clearance.

Veldwake is a planned 3D voxel action RPG about exploration in a persistent, systemic procedural world. Geography, creatures, settlements, economies, and events can evolve, while player actions leave observable consequences. Its audiovisual identity is intended to be produced predominantly through code and constrained generators rather than a traditional manual asset pipeline.

Current status: **M8 — Discoverable Landmarks is complete and merged; M1 through M8 are all in `main`. Combat Initiative / Spacing, an unnumbered slice after it, is complete and merged — owner-accepted and passed by independent QA. M9 is paused, unmerged, on its own branch, waiting to consume combat initiative and re-run its weapon-choice gate.** The Windows-first client streams one deterministic 800 x 96 x 800 voxel region — a verdant highland valley with a meandering river, a pond, cliffs banded like sedimentary rock, forest pockets, and low vegetation — around the camera through `wgpu`/D3D12, lit by a directional sun with a filtered shadow map, a procedural sky, height-aware distance fog, and two weather states. Generation is a pure function of a seed and a chunk coordinate, so the same world appears every run. LOD stays opt-in from M3C. M5 adds a generated humanoid to that region: one descriptor compiles to sixteen rigid voxel body parts on a sixteen-bone skeleton, walks and runs on analytical gaits whose phase advances with distance, and puts its soles on the block tops the viewer can see. M6 adds the first thing in the repository that can be played: two combatants in a clearing, one procedurally generated sword, an adversary that telegraphs and commits, a swing that connects or misses on swept geometry, damage, stagger, knockback, a camera that moves only when a hit lands, voxel chips off the contact, a diegetic health readout, and impact audio that is synthesised rather than recorded. M7 makes the region traversable: the player's body walks continuously through the whole generated world with streaming anchored on it, water blocks movement, and an adversary placed in the world by a reachability audit sleeps until you come near, fights, and — win or lose — the session carries on. M8 gives that region something to walk *to*: three generated landmarks of one procedural family — a spire, a gate and a broken monolith — placed by the world's own composition rules around a discovery overlook, two of them visible from it in opposite directions and the third revealed only by walking. A landmark is solid, the gate is genuinely walked through in the authoritative simulation and in the client, and the adversary now stands at one of the two landmarks a player can choose between. Combat initiative gives the adversary a reason to be watched: a lunge from middle distance along a line it commits to before it moves, a step back to reopen that distance, and a long opening when the lunge meets nothing. It remains an engineering proof rather than a game: there is no persistence, no progression, no second enemy, no menu and no world beyond one deterministic region.

## Principles

- **Wonder:** discovery should create genuine curiosity.
- **Mastery:** progression should unlock possibilities, not only larger numbers.
- **Consequence:** the world should remember changes the player can observe.
- Structured generation uses constraints, style grammars, determinism, and validation—not unconstrained randomness.
- Authoritative simulation remains independent from rendering and client presentation.

## Initial technical direction

Rust 2024 on pinned stable Rust, with `wgpu`, WGSL, `winit`, and `glam` in the presentation client. This is a custom engine project, and a dependency enters only when a milestone needs it. `cpal` is now one: M6 added it, pinned exactly and with no backend features enabled, for the procedural-audio device adapter. Rapier3D, `zstd`, `quinn` and WebAssembly are still not dependencies — none of them is declared anywhere in the workspace or compiled on the Windows target.

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

`VELDWAKE_ENCOUNTER` runs the combat slice and is `off` by default: `armed` is playable, `script` runs the reference fight unattended, and `moment:<name>` or `moment:<name>+<ticks>` freezes a named moment for a capture. `initiative` is the combat-initiative session at the open clearing, and `initiative:<read|spam|spam-read>` and `initiative:<driver>@<tick>` drive or freeze it for QA. While an encounter is armed, `WASD` moves relative to the camera, `J` or the left mouse button attacks, `K` or `Space` dodges, `F4` detaches the camera to look around, and `F1` cycles to a combat view that draws the volumes a hit is decided by. With it off the client behaves exactly as it did before M6: free-fly camera, no combat simulation, no second actor, no particles, and no audio device opened at all.

## Start here

- [Repository rules](AGENTS.md) (Claude Code entry point: [CLAUDE.md](CLAUDE.md))
- [Documentation index](docs/INDEX.md)
- [Current project state](docs/PROJECT_STATE.md)
- [Game vision](docs/vision/GAME_VISION.md)
- [Architecture](docs/engineering/ARCHITECTURE.md)
- [Roadmap](docs/planning/ROADMAP.md)
- [Current handoff](docs/agents/HANDOFF.md)
