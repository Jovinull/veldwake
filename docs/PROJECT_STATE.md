# Project state

Last updated: 2026-09-18

## Stage

**M4 — Beautiful Terrain Vertical Slice is complete and merged. M1, M2, M3, and M4 are all in `main`.** M4 merged through [PR #8](https://github.com/Jovinull/veldwake/pull/8) at merge commit `abadadff6ad1251e4f291d272577da6121d37540`, after independent branch QA, a green pull-request CI run, and a green post-merge CI run on the merge commit. Branch QA added seven hardenings before the merge: cache/source identity mismatch rejected before worker startup, typed validation of hostile `TerrainConfig` input, an exhaustive 1,875-chunk behavioural signature folded into the cache identity, complete vegetation bounds proved against the finite region, corrected hydrology seam semantics, the render interval named `renderer_render_wall` rather than CPU submit or GPU time, and a shared WGSL sky constant. The workspace has 275 tests; the final release D3D12 smoke on the Intel Iris Xe host passed for `m4-golden`, `m4-golden-banded`, and the M3 diagnostic regression.

**M5 — Procedural Character has completed independent branch QA on `feat/m5-procedural-character` and is ready for external review/PR.** Nothing is merged and no pull request is open. QA corrected a continuous finite-region edge error in the terrain ground adapter, made requested character compilation/upload failure fatal rather than silently omitting the feature, and hardened degenerate public IK inputs. The final independent D3D12 visual/motion exit gate inspected fresh clean client-area captures, all eight frozen walk phases, flat and terraced motion, accepted KI-018/KI-019/KI-020 as documented, and passed M3/M4 regression plus M5 lifecycle smokes without validation/device-loss/fatal/panic markers. See [`planning/M5_PROCEDURAL_CHARACTER.md`](planning/M5_PROCEDURAL_CHARACTER.md) and [`agents/HANDOFF.md`](agents/HANDOFF.md).

**M3 — Streaming World is complete and merged. M3A, M3B, M3C, and M3D are all in `main`.** M3D merged through [PR #7](https://github.com/Jovinull/veldwake/pull/7) at merge commit `bfc9db1eec085390f9148efbb2a14d61d1fa0d6e` after independent branch QA, external review, and a green remote CI run; M3C merged through [PR #6](https://github.com/Jovinull/veldwake/pull/6) at `c669929b00427c2f438529b572931400a24b6d3d`. **M4 — Beautiful Terrain Vertical Slice** added `veldwake-procedural`, one deterministic verdant-highland-valley region under the golden seed `0x5645_4c44_5741_4b45`, a `ChunkSource` boundary in streaming, and a stylized renderer with sun, shadow map, procedural sky, fog, water specular, and two weather states, all constrained by [`audiovisual/STYLE_BIBLE.md`](audiovisual/STYLE_BIBLE.md). See [`planning/M4_BEAUTIFUL_TERRAIN_SLICE.md`](planning/M4_BEAUTIFUL_TERRAIN_SLICE.md). M3C0–M3C3 delivered edge-generic voxel grids, one coarse LOD, mixed-resolution seams, atomic presentation transitions, measured baseline/banded profiles, and keyboard debug views. LOD remains opt-in: mutation-boundary accounting shows only a 16.9% presentation-owned chunk-mesh high-water saving while upload bytes rise 81.6%, and two short-lived ready frontier deficits remain recorded honestly (KI-014). M3B merged through [PR #5](https://github.com/Jovinull/veldwake/pull/5) at merge commit `b5473dbb7836b65e6c6c5662a8abf6f02b6e8043`; M3A merged through [PR #3](https://github.com/Jovinull/veldwake/pull/3) at `af1cabc913a9500eefbcd647a56c881d2c1288c0`. The repository remains a non-playable engineering proof.

## What works

- M5: `veldwake-character` compiles one humanoid descriptor into a character, with no dependency on the world generator and no GPU, window, camera, or filesystem types. A descriptor is fourteen proportion fractions, a palette choice, a seed, and a bounded variation amount; validation is typed and three proportions are guaranteed by construction rather than by rejection, so bounded variation can never produce an invalid body. The compiler emits sixteen rigid voxel body parts at twelve character voxels per world unit — the golden humanoid is `2.3333` world units — meshed by the existing exposed-face mesher through one reused `32³` scratch grid, with their own ten-slot palette and the `VoxelId` range `128..192`. A sixteen-bone skeleton is derived from the same proportions. Locomotion is analytical: idle, walk and run as closed-form joint curves whose phase advances with distance and whose blend thresholds are in leg lengths per second, with two-bone leg IK solving each foot against a `GroundSampler` that returns the top face of the topmost solid voxel. A capsule and sixteen boxes are the collision representation. Identity, geometry, skeleton and collision each have their own fingerprint and three fixtures are locked by signature. The client adapts all of this to M4 in one module: eleven lines answer the ground query from `TerrainField`, two closed diagnostic courses and two stand points make captures reproducible, and ten named camera poses frame them. Character geometry uploads once; a frame writes sixteen eighty-byte uniforms and issues sixteen world draws and sixteen shadow draws through the same WGSL lighting function terrain uses.

- M4: `veldwake-procedural` generates a finite 800 × 96 × 800 voxel region from a named seed, generator version, style-contract version, and art-control descriptor, all folded into one cheap `WorldIdentity::fingerprint()` that keys the disk cache. Terrain is a composed field — meandering valley axis, macroform, masked ridges, masked mid and fine detail, a river carve driven by a monotone water surface — and every stage is a pure function of world position. Water cannot run uphill or step at a seam by construction. Materials are eleven named surfaces with strata banded on world height, so cliffs show horizontal colour breaks. Vegetation is a jittered lattice with a provable six-voxel minimum spacing, written as world geometry across chunk boundaries. Streaming gained a two-method `ChunkSource` with a `TerrainChunkSource` adapter; the M3 diagnostic corridor still reaches the runtime the same way it always did. The client renders it with a directional sun, a 2048-square PCF shadow cascade, a procedural sky, height-aware distance fog, restrained specular that only water and rock carry, and clear/overcast weather on `F3`.

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
- M3C2 hardening (restage order): `Renderer::stage_chunk` rejects, then releases the obsolete replacement, then allocates, then installs, so presentation-owned chunk-mesh bytes never hold two replacements of one chunk inside a call the bridge cannot observe. The precondition is proven, not assumed: the bridge restages only when the staged stamp is no longer the target, and such a replacement can never be committed. The committed mesh is untouched and keeps drawing. The benchmark path reports `restaged = 0` in every run, so the high-water figures are unchanged and the path is covered by three regression tests instead.
- M3C3: `debug` module and one `LineList` pipeline. `F1` cycles `Off`/`Lod`/`Residency`/`Boundaries`, `F2` toggles boxes. `Lod` tints `Lod1`; `Residency` and `Boundaries` expose runtime/presentation state. In `Off`, per-frame debug primitive allocations, debug uniform writes, and debug draws are all zero and the mesh upload budget is untouched. The fixed debug pipeline/unit geometry remain allocated from startup, and slots used earlier remain pooled (KI-012).
- M3D: an opt-in experimental disk cache of source results inside `crates/streaming`, explicitly not a save. `ChunkCache::open` performs cold temporary cleanup and a footprint walk on the caller during client event-loop initialization; no cache I/O occurs in the frame hot path, and all per-chunk lookup/decode/publish work runs on the streaming worker. A 48-byte little-endian header carries explicit identity, length, encoding, and FNV-1a integrity data; malformed reads are capped at 196,657 bytes before decoding. The private decoder returns typed errors and never converts corruption into AIR or absence. The runtime fingerprint includes a locked behavioral signature computed in tests over the complete finite corridor and an absent shell, so a source-output change fails until cache identity is deliberately updated. Temp-and-rename publishes one semantically equivalent value per key without `fsync`; cross-platform concurrent rename is not claimed to be physically write-once. Rejected-entry deletion failures preserve the source result and have a dedicated counter instead of being called repairs. `RuntimeMetrics::cache` stays separate and zero when disabled. Raw remains the default despite run-length's 169× win on the 97%-AIR fixture: worst-case RLE is exactly 3× raw. Every recorded warm run remains slower than regeneration for this trivial source (KI-015, KI-016).
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
- Independent M3D branch QA passes 184 headless tests (38 voxel, 90 streaming, 56 client), all current build/lint/test/rustdoc/security/documentation gates, and the complete release streaming probe. No D3D12 smoke was repeated because the QA diff changes no presentation or lifecycle behavior.

## What does not exist yet

No product world generation, player saves, authoritative world persistence, greedy meshing, generalized batching/instancing, origin rebasing, multiple workers, authoritative simulation, gameplay, physics, audio, networking, mod runtime, UI framework, or internal editor exists. LOD exists only as an opt-in M3C experiment behind `VELDWAKE_PROFILE=m3c-banded`; there is no production LOD policy. The finite diagnostic source is not a world generator, and the M3A static fixture is no longer rendered by the client (its voxel-crate tests remain).

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

**M5 — Procedural Character:** implementation and independent branch QA are complete on `feat/m5-procedural-character`; external review/PR is next. Do not merge, open a PR, or begin M6 from this branch without owner direction. See [`planning/M5_PROCEDURAL_CHARACTER.md`](planning/M5_PROCEDURAL_CHARACTER.md).
