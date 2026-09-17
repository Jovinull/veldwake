# Project state

Last updated: 2026-09-17

## Stage

**M3B — Streaming Runtime is complete and merged. M3C0–M3C3 are implemented and independently QA-hardened on `feat/m3c-lod-debug`: edge-generic voxel grids, one coarse LOD, mixed-resolution seams, atomic presentation transitions, measured baseline/banded profiles, and keyboard debug views. LOD remains opt-in. Correct mutation-boundary accounting shows only a 16.9% presentation-owned chunk-mesh high-water saving while upload bytes rise 81.6%; the transition model prevents mixed seams and prevents presentation-only invalidations from blinking previously drawn chunks, but QA observed two short-lived ready frontier deficits and records them honestly (KI-014).** M3B merged through [PR #5](https://github.com/Jovinull/veldwake/pull/5) at merge commit `b5473dbb7836b65e6c6c5662a8abf6f02b6e8043`, and the Claude Code entry point through [PR #4](https://github.com/Jovinull/veldwake/pull/4) at `994e9863936441606d9bd675e1ea62bc74300bf9`; M3A merged through [PR #3](https://github.com/Jovinull/veldwake/pull/3) at `af1cabc913a9500eefbcd647a56c881d2c1288c0`. The repository remains a non-playable engineering proof.

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
- M3C1: `LodLevel`, `NeighborPresentation`, `LodSelection::{Lod0Only, Banded}` (Chebyshev band `d <= 1` / `d == 2` keeps / `d >= 3`), `StreamingConfig::m3c_diagnostic()` (343/637/729, cap 810 as transient headroom), level-aware `MeshStamp`/`NeighborStamp`, per-level snapshots with the coarse-occupancy seam rule (`FaceSlab::downsampled_from`, `coarse_occupancy_of`), `lod_swaps`/`stale_lod_results`, per-level summary counts, and a two-profile `streaming-probe`. `StreamingConfig::default()` still behaves exactly as M3B. Hardening: LOD-aware scheduler (`Lod0` mesh → load → `Lod1` mesh, fairness after four meshes), coarse-seam suppression under full fine coverage (`FaceSlab::fine_coverage_of`), one crate-private `CoarseTally` for every coarsening, `stale_lod_results` covering neighbor presentation changes, and separate `snapshot_build`/`lod1_derivation`/`worker_mesh_lod0`/`worker_mesh_lod1` timings (derivation ≈65 µs mean, kept in the orchestration path).
- Historical M3C2 result (superseded metrics): two-`vec4` `ModelUniform` (origin never scaled, `scale.x` 1 or 2) with `world = translation + local * scale`; per-level residency/upload totals; `StreamingConfig::m3c_baseline()`; and `VELDWAKE_PROFILE` selection. Its −57% GPU figure counted committed buffers only and is not the current memory result.
- M3C2 transition model: committed/target/pending separation, `SeamContract`, presentation-only retention, data invalidation, seam-connected `transition_groups`, staged presentation, and coalescing by stamp removed the 909 gaps where previously drawn meshes blinked. Historical “0 ready-undrawn” runs did not prove all frontier geometry visible; the current split counters below supersede that interpretation.
- M3C2 hardening: `ChunkPresentation::commit_staged_group` is all-or-nothing, while `StreamingRuntime::commit_group_with` preflights unique/non-empty ready membership and holds the runtime's exclusive mutable borrow through presentation swap and CPU commit. A refused presentation changes neither side; bridge bookkeeping happens only after success. Mutation-boundary byte sampling observes every successful stage and group commit, rather than only update end. The QA benchmark recorded baseline 12,101,888 versus banded 10,053,696 presentation-owned chunk-mesh bytes (−16.9%); this explicitly excludes depth, pipelines, debug resources, and driver/wgpu retention.
- Coverage observability separates unknown frontier pipeline latency, non-empty CPU-ready meshes awaiting upload, ready meshes blocked by transition groups, and the invariant failure “runtime committed but bridge missing.” On the QA path: `ready_undrawn_max = 2`, entirely transition-blocked/constrained; `committed_missing_max = 0`. Non-zero `ready_undrawn` is a temporary known-geometry coverage deficit, not “zero holes” (KI-014).
- M3C3: `debug` module and one `LineList` pipeline. `F1` cycles `Off`/`Lod`/`Residency`/`Boundaries`, `F2` toggles boxes. `Lod` tints `Lod1`; `Residency` and `Boundaries` expose runtime/presentation state. In `Off`, per-frame debug primitive allocations, debug uniform writes, and debug draws are all zero and the mesh upload budget is untouched. The fixed debug pipeline/unit geometry remain allocated from startup, and slots used earlier remain pooled (KI-012).
- The client builds and meshes the static fixture chunks once at startup, converts voxel IDs to diagnostic colors in presentation code, and creates immutable GPU buffers without per-frame remeshing or upload.
- Signed `ChunkCoord(i32)` and `WorldVoxelCoord(i64)` conversions use checked Euclidean semantics; borrowed neighborhoods distinguish known voxel data from missing chunks.
- Neighbor-aware CPU meshing removes solid seams in all six directions. The deterministic three-chunk fixture has 51 solids, fingerprint `0xe65ae5533c4db16a`, and exact topology 202 quads / 808 vertices / 1,212 indices.
- The client renders the static chunks at signed offsets using immutable per-chunk model uniforms. CPU meshes remain local and the renderer still owns no authoritative world state.
- `veldwake-streaming` is a headless std-only orchestration crate over `veldwake-voxel`: deterministic demand/retention sets, globally unique request tokens, bounded CPU residency, one bounded worker, lazy priority queues, owned center-plus-face-slab mesh snapshots, and generation-stamped stale-result rejection.
- The finite diagnostic source reports `Present(Chunk)` or `KnownAbsent`; unavailable neighbors delay meshing. Only render-demand chunks request meshes, while dependency/retention records can remain CPU-only.
- `StreamingConfig` rejects `retention_radius < render_radius + dependency_halo` with typed errors and checked arithmetic; CPU eviction finalization is bounded (default 8 per update) while retired payloads keep counting against the hard cap.
- The client streams: camera position → `floor` → `WorldVoxelCoord::split` → demand center, updated only on chunk change; `StreamingBridge` polls the runtime once per frame, deactivates any GPU mesh whose stamp is no longer current before uploading, uploads under a 2-per-frame / 4 MiB soft budget with oversized accounting, and releases buffers at most 8 per frame.
- The renderer keeps `BTreeMap<ChunkCoord, GpuChunkSlot>` with committed/drawable and staged state; it knows no streaming stamps. GPU-independent client tests use an in-memory presentation double.
- Five-second aggregate diagnostics report demand, residency, mesh states, queues, GPU residency, uploads/bytes, removals, budget hits, stale drops, and byte totals.
- Independent M3C branch QA passes 141 headless tests (38 voxel, 50 streaming, 53 client), all repository quality/security/documentation gates, both release probes, and Windows/D3D12 smoke for all three profiles. The debug-view rerun validates F1/F2 plus the precise `Off` contract after pooled slots exist.

## What does not exist yet

No product world generation, disk cache/saves, greedy meshing, generalized batching/instancing, origin rebasing, multiple workers, authoritative simulation, gameplay, physics, audio, networking, mod runtime, UI framework, or internal editor exists. LOD exists only as the M3C experiment on this feature branch, behind `VELDWAKE_PROFILE=m3c-banded` and never enabled by default; there is no production LOD policy. The finite diagnostic source is not a world generator, and the M3A static fixture is no longer rendered by the client (its voxel-crate tests remain).

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

**M3C — Initial LOD + Streaming Debug Visualization:** M3C0–M3C3 are implemented, QA-hardened, and awaiting external review/PR. LOD stays behind `VELDWAKE_PROFILE=m3c-banded`; the corrected chunk-mesh high-water saving is 16.9%, and short-lived frontier coverage deficits remain visible in the split metrics (KI-013, KI-014). Do not begin the swap-churn investigation or another milestone until this branch is externally reviewed. See [`planning/M3_STREAMING_WORLD.md`](planning/M3_STREAMING_WORLD.md).
