# Current handoff

Last updated: 2026-09-16

## Current position

M1, M2, M3A, and M3B are merged into `main`; M3B landed through PR #5 at merge commit `b5473dbb7836b65e6c6c5662a8abf6f02b6e8043` and the Claude Code entry point through PR #4 at `994e9863936441606d9bd675e1ea62bc74300bf9`. M3C0 (edge-generic `DenseGrid`, `CoarseGrid`, generic mesher, `downsample_2x`) and M3C1 (headless LOD core: `LodLevel`, banded selection with history, level-aware stamps, mixed-resolution seams, per-level metrics and probe) are implemented on `feat/m3c-lod-debug` with all 32-edge and M3B evidence unchanged; renderer, debug views, and the benchmark are planned only. `veldwake-voxel` owns CPU chunk/coordinate/neighborhood/snapshot/mesh contracts; `veldwake-streaming` owns headless residency, worker orchestration, and per-frame observability; the client's `streaming` module bridges camera demand to a `ChunkPresentation` implemented by the renderer, which owns only disposable GPU state keyed by `ChunkCoord`.

## Continue here

Continue M3C from its section in [`planning/M3_STREAMING_WORLD.md`](../planning/M3_STREAMING_WORLD.md), with owner approval before each step: renderer/bridge support for `Lod1` meshes (scale 2 in the model uniform, per-level GPU bytes and counts, `presented` keyed as today) → switch the client to `StreamingConfig::m3c_diagnostic()` only after that → `Lod0`-only radius-3 baseline versus banded measurement with the recorded decision rule and debug views off → keyboard-toggled debug views that never touch the mesh upload budget. Until the renderer scales coarse meshes, the client must stay on the default `Lod0Only` profile. Stop and report if M3C0 cannot keep the locked topology. Do not introduce product world generation, saves/cache, ECS, gameplay, physics, networking, multiple workers, origin rebasing, a render graph, or generalized batching.

## Read before continuing

- [`../../AGENTS.md`](../../AGENTS.md)
- [`../engineering/ARCHITECTURE.md`](../engineering/ARCHITECTURE.md)
- [`../engineering/INVARIANTS.md`](../engineering/INVARIANTS.md)
- [`../engineering/PERFORMANCE.md`](../engineering/PERFORMANCE.md)
- [`../audiovisual/ART_DIRECTION.md`](../audiovisual/ART_DIRECTION.md)
- ADRs 0001 and 0002

## Immediate risks

- Do not turn the documented future crate map into empty crates.
- Do not let the M2 voxel representation or mesher depend on `wgpu`, `winit`, or the diagnostic camera.
- The M1 cube has been replaced by the M2 fixture; its palette and framing remain diagnostic presentation, not game art direction.
- Keep chunk dimensions, material encoding, coordinate order, and mesh winding explicit and tested; accidental conventions will become expensive compatibility constraints.
- The M2 convenience mesher explicitly uses `BoundaryPolicy::Expose`; future streaming work must use deliberate availability policy rather than silently equating “not loaded” with AIR.
- Negative world coordinates use Euclidean division and are locked at `0`, `31`, `32`, `-1`, `-32`, `-33`, plus range extrema. Preserve this contract.
- One model uniform/bind group and a linear fixture lookup per chunk are intentionally limited M3A diagnostics, not accepted scalable batching or residency designs.
- M3B1 does not equate an unavailable/loading neighbor with known AIR. Jobs carry a global non-reused request token plus center/neighbor generations; results are accepted only while every stamp remains current.
- The default 27/81/125 demand counts, 160-payload cap, one worker, and observed probe timing are diagnostic evidence, not target-world performance promises. Measure before expanding workers, residency, or upload throughput.
- Keep drawability and deallocation separate in the bridge: a presented mesh whose stamp is no longer current is deactivated in the same update, before uploads and regardless of the release budget. Never reorder that to "upload first".
- `ChunkPresentation` has exactly two implementors (renderer, test double). It is a testability seam, not a render abstraction; do not add methods the bridge does not call.
- Camera anchoring must stay `floor`-based with explicit non-finite/out-of-range rejection; `as i64` truncates toward zero and would misplace every negative sub-voxel position.
- With one worker, `finalize_evictions` before `dispatch_one` means an eviction backlog cannot trigger `hard_cap_blocks`; the cap holds through accounting. Re-validate that reasoning if worker count or poll order ever changes.
- M3C must never let a coarse level hide a known crack or turn an unavailable neighbor into AIR; the seam rule is decided in the plan and tested per direction and occupancy case before any rendering work.
- Keep every new mesher or slab path generic over `EDGE`; the 32-edge fingerprints and topology tests are the tripwire for accidental edge-specific code.
- `Lod1` meshes are in 16-cell units; uploading them unscaled would draw a quarter-size chunk. Do not enable `Banded` in the client before the renderer applies the level scale.
- The seam rule's orientation convention is `from_neighbor`'s: `face` is the direction from the center, and slab constructors pick the neighbor's touching layers themselves. The first seam oracle got this backwards; the tests now encode the convention.
- Retries are for the host linker lock only (`LNK1104`, KI-008). A failing assertion, panic, test, Clippy, or build error is evidence and is never re-run until it passes.
- The no-LOD wider-radius baseline is part of M3C's evidence, not an afterthought; LOD stays enabled only if the recorded decision rule passes on the audited host.
- Treat integrated Intel Iris Xe as one conservative host, not the target matrix.
- Do not change repository visibility, replace/configure remotes, publish releases, choose a license, or present the working title as cleared without owner authorization.
