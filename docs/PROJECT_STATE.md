# Project state

Last updated: 2026-09-16

## Stage

**M3A — Multi-chunk Correctness is merged. M3B — Streaming Runtime (M3B1 headless runtime plus M3B2 camera-driven GPU integration) is implemented on `feat/m3b-streaming-runtime`, validated by headless gates and a driven Windows/D3D12 smoke, and submitted for review.** M3A merged through [PR #3](https://github.com/Jovinull/veldwake/pull/3) at merge commit `af1cabc913a9500eefbcd647a56c881d2c1288c0`. The repository remains a non-playable engineering proof.

## What works

- Source provenance and SHA-256 are recorded.
- Repository constitution, ADR process, memory protocol, setup guide, and CI are defined.
- Rust stable `1.98.1` with `rustfmt` and Clippy is available on the audited Windows host.
- A dependency-free Rust 2024 foundation crate compiles and validates the workspace.
- `veldwake-client` uses current `winit 0.30` lifecycle APIs and `wgpu 30` to render the deterministic M2 voxel fixture with depth, back-face culling, and a perspective camera.
- Input, camera, surface selection, model translation, and diagnostic color behavior are covered by 14 client tests; focus loss clears held input and presentation delta is bounded.
- Startup diagnostics report the actual adapter/backend/surface configuration; lightweight presentation timing is reported every five seconds at `info`.
- `cargo-deny` and `cargo-audit` are part of the dependency gates now that runtime dependencies exist.
- `veldwake-voxel` provides a dependency-free dense `32³` chunk, checked local access, a stable diagnostic fixture, and a CPU exposed-face reference mesher with headless correctness tests.
- The client builds and meshes the static fixture chunks once at startup, converts voxel IDs to diagnostic colors in presentation code, and creates immutable GPU buffers without per-frame remeshing or upload.
- Signed `ChunkCoord(i32)` and `WorldVoxelCoord(i64)` conversions use checked Euclidean semantics; borrowed neighborhoods distinguish known voxel data from missing chunks.
- Neighbor-aware CPU meshing removes solid seams in all six directions. The deterministic three-chunk fixture has 51 solids, fingerprint `0xe65ae5533c4db16a`, and exact topology 202 quads / 808 vertices / 1,212 indices.
- The client renders the static chunks at signed offsets using immutable per-chunk model uniforms. CPU meshes remain local and the renderer still owns no authoritative world state.
- `veldwake-streaming` is a headless std-only orchestration crate over `veldwake-voxel`: deterministic demand/retention sets, globally unique request tokens, bounded CPU residency, one bounded worker, lazy priority queues, owned center-plus-face-slab mesh snapshots, and generation-stamped stale-result rejection.
- The finite diagnostic source reports `Present(Chunk)` or `KnownAbsent`; unavailable neighbors delay meshing. Only render-demand chunks request meshes, while dependency/retention records can remain CPU-only.
- `StreamingConfig` rejects `retention_radius < render_radius + dependency_halo` with typed errors and checked arithmetic; CPU eviction finalization is bounded (default 8 per update) while retired payloads keep counting against the hard cap.
- The client streams: camera position → `floor` → `WorldVoxelCoord::split` → demand center, updated only on chunk change; `StreamingBridge` polls the runtime once per frame, deactivates any GPU mesh whose stamp is no longer current before uploading, uploads under a 2-per-frame / 4 MiB soft budget with oversized accounting, and releases buffers at most 8 per frame.
- The renderer keeps `BTreeMap<ChunkCoord, GpuChunkMesh>` with an `active` flag and draws only active entries; it knows no streaming stamps. Twenty-three GPU-independent client tests use an in-memory presentation double.
- Five-second aggregate diagnostics report demand, residency, mesh states, queues, GPU residency, uploads/bytes, removals, budget hits, stale drops, and byte totals.

## What does not exist yet

No product world generation, disk cache/saves, LOD, greedy meshing, generalized batching/instancing, origin rebasing, multiple workers, authoritative simulation, gameplay, physics, audio, networking, mod runtime, UI framework, or internal editor exists. The finite diagnostic source is not a world generator, and the M3A static fixture is no longer rendered by the client (its voxel-crate tests remain).

## Current decisions

- Working title: Veldwake; no legal clearance claimed.
- Custom engine in Rust; no Unity, Unreal, Godot, Bevy, or equivalent central engine.
- Planned renderer foundation: `wgpu` + WGSL + `winit` + `glam`; D3D12 is the expected initial Windows backend.
- Authoritative gameplay is presentation-independent; single-player follows a local-server-compatible boundary.
- Procedural-first audiovisual production is a defining constraint, governed by style rules and compiled caches.
- Rust versions are pinned; dependencies enter only with an immediate capability and justification.

See accepted decisions in [`adr/`](adr/README.md).

## Build, test, run

From a shell where Cargo is on `PATH`:

```text
cargo build --workspace
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace
cargo test --workspace --doc
cargo deny check
cargo audit
cargo run -p veldwake-client
cargo run --release -p veldwake-voxel --bin voxel-probe
cargo run --release -p veldwake-streaming --bin streaming-probe
```

The runnable binary is diagnostic presentation content only. See [`environment/SETUP.md`](environment/SETUP.md).

## Tools on the audited host

Git, Git LFS, GitHub CLI, Visual Studio 2022 Build Tools/MSVC, Windows SDK, LLVM, Ninja, Python, Codex CLI, rustup, Cargo, rustc, rustfmt, Clippy, cargo-nextest, cargo-deny, and cargo-audit. See the environment report for versions and tools intentionally deferred.

## Known problems and blocks

- No project license has been chosen.
- The working title lacks formal trademark/domain/store clearance.
- Target hardware tiers and memory/frame budgets are not yet approved.
- Visual style bible and accessibility baseline remain to be authored during the relevant milestones.
- The public repository is `https://github.com/Jovinull/veldwake`; changing visibility, remotes, releases, or other publication policy requires owner authorization.

## Active milestone

**M3B — Streaming Runtime:** M3B1 and M3B2 are implemented on the feature branch; all gates and the driven Windows/D3D12 smoke passed; the pull request to `main` is open for review. Do not begin any M3C scope (LOD, cache/persistence experiment, debug visualization, more workers) before acceptance. See [`planning/M3_STREAMING_WORLD.md`](planning/M3_STREAMING_WORLD.md). World generation, saves, LOD, ECS, gameplay, physics, networking, and biomes remain excluded.
