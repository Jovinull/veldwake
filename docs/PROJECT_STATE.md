# Project state

Last updated: 2026-09-16

## Stage

**M3B — Streaming Runtime is complete and merged. M3C0 (edge-generic dense grid and mesher) and M3C1 (headless LOD core: banded selection, level-aware stamps, mixed-resolution seams) are implemented on `feat/m3c-lod-debug`; renderer scaling, debug views, and the LOD benchmark are planned, not implemented.** M3B merged through [PR #5](https://github.com/Jovinull/veldwake/pull/5) at merge commit `b5473dbb7836b65e6c6c5662a8abf6f02b6e8043`, and the Claude Code entry point through [PR #4](https://github.com/Jovinull/veldwake/pull/4) at `994e9863936441606d9bd675e1ea62bc74300bf9`; M3A merged through [PR #3](https://github.com/Jovinull/veldwake/pull/3) at `af1cabc913a9500eefbcd647a56c881d2c1288c0`. The repository remains a non-playable engineering proof.

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
- M3C0: `DenseGrid<const EDGE>` with heap storage, `GridCoord<EDGE>`, `Chunk = DenseGrid<32>`, `CoarseGrid = DenseGrid<16>`, const-defaulted `ChunkNeighborhood`/`FaceSlab`/`OwnedMeshingSnapshot`/`MeshBoundaryError`, one edge-generic mesher loop, and the explicit `Chunk::downsample_2x` (any-solid occupancy, majority material, lowest-ID tie). All 32-edge fingerprints and topology are unchanged; the coarse diagnostic fixture is locked at 16 solids / 82 quads. `GridCoord` components are `usize`.
- M3C1: `LodLevel`, `NeighborPresentation`, `LodSelection::{Lod0Only, Banded}` (Chebyshev band `d <= 1` / `d == 2` keeps / `d >= 3`), `StreamingConfig::m3c_diagnostic()` (343/637/729, cap 810 as transient headroom), level-aware `MeshStamp`/`NeighborStamp`, per-level snapshots with the coarse-occupancy seam rule (`FaceSlab::downsampled_from`, `coarse_occupancy_of`), `lod_swaps`/`stale_lod_results`, per-level summary counts, and a two-profile `streaming-probe`. `StreamingConfig::default()` still behaves exactly as M3B. Hardening: LOD-aware scheduler (`Lod0` mesh → load → `Lod1` mesh, fairness after four meshes), coarse-seam suppression under full fine coverage (`FaceSlab::fine_coverage_of`), one shared `CoarseTally` for every coarsening, `stale_lod_results` covering neighbor presentation changes, and separate `snapshot_build`/`lod1_derivation`/`worker_mesh_lod0`/`worker_mesh_lod1` timings (derivation ≈65 µs mean, kept in the orchestration path).
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

**M3C — Initial LOD + Streaming Debug Visualization:** M3C0 and M3C1 are implemented headlessly. Next: renderer/client scaling of `Lod1` meshes (16-cell units × 2) and per-level GPU accounting, the `Lod0`-only radius-3 baseline versus banded benchmark with debug views off, then keyboard-toggled debug views that never touch the mesh upload budget. See the M3C section of [`planning/M3_STREAMING_WORLD.md`](planning/M3_STREAMING_WORLD.md). World generation, saves/cache, ECS, gameplay, physics, networking, multiple workers, origin rebasing, render graph, generalized batching, and biomes remain excluded.
