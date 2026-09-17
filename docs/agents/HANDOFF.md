# Current handoff

Last updated: 2026-09-16

## Current position

M1, M2, M3A, and M3B are merged into `main`; M3B landed through PR #5 at merge commit `b5473dbb7836b65e6c6c5662a8abf6f02b6e8043` and the Claude Code entry point through PR #4 at `994e9863936441606d9bd675e1ea62bc74300bf9`. M3C0 (edge-generic grid and mesher), M3C1 (headless LOD core), and M3C2 (GPU presentation of both levels, per-level residency, `m3c_baseline` profile, `VELDWAKE_PROFILE` selection, and the measured baseline-versus-LOD comparison) are implemented on `feat/m3c-lod-debug` with all 32-edge and M3B evidence unchanged. LOD is implemented but not the default: on the benchmark path it cut GPU bytes 57% but raised upload bytes 74% and CPU mesh time 41% because of level-swap churn. The M3C2 follow-up added the transition model: a committed mesh stays drawn through presentation-only invalidations until its transition group (chunks whose level changes plus the drawn neighbors whose seam contract toward them changes) is fully meshed, staged on the GPU, and committed in one update; data invalidations still deactivate immediately. On the benchmark path it took the banded profile from 909 presentation gaps (up to 232 frames, 62 chunks at once) to 0 gaps and 0 ready-but-undrawn updates, with uploads and mesh jobs unchanged from M3C2, so LOD stays opt-in. Two hardenings followed: the group commit is now all-or-nothing on both sides with the presentation asked first (`commit_staged_group`), and GPU bytes are sampled from the presentation every update with the peak taken on the simultaneous `committed + staged` sum, which corrects LOD's GPU saving from −57% to −19% (KI-013). M3C3 is implemented: `F1` cycles `Off`/`Lod`/`Residency`/`Boundaries`, `F2` toggles boxes, one `LineList` pipeline, off by default and free when off; 637 residency boxes cost 15–38 FPS on the audited host (KI-012). `veldwake-voxel` owns CPU chunk/coordinate/neighborhood/snapshot/mesh contracts; `veldwake-streaming` owns headless residency, worker orchestration, and per-frame observability; the client's `streaming` module bridges camera demand to a `ChunkPresentation` implemented by the renderer, which owns only disposable GPU state keyed by `ChunkCoord`.

## Continue here

Continue M3C from its section in [`planning/M3_STREAMING_WORLD.md`](../planning/M3_STREAMING_WORLD.md), with owner approval before each step: the transition model, the commit and GPU-accounting hardenings, and M3C3 are done and measured (KI-009 split; KI-011, KI-012, KI-013 open) → next is the swap-churn investigation measured on the same benchmark path (`bench` procedure: 12 s settle, pitch down, D 8+8, A 16+12, W 10, S 16, D 12, rest; compare `VELDWAKE_PROFILE=m3c-baseline` and `m3c-banded`) before any change to the default profile. Do not make LOD the default without re-passing the recorded rule. Stop and report if M3C0 cannot keep the locked topology. Do not introduce product world generation, saves/cache, ECS, gameplay, physics, networking, multiple workers, origin rebasing, a render graph, or generalized batching.

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
- Keep drawability and deallocation separate in the bridge: a presented mesh whose data is no longer current (content change, unload, neighbor unloaded or reloaded, unavailable neighbor, leaving render demand) is deactivated in the same update, before uploads and regardless of the release budget. Only a presentation-only change (`LodChange`, `NeighborPresentation`, `Membership` while still in render demand) whose target `differs_only_by_presentation` may keep the committed mesh drawn, and then only until its transition group commits. Never reorder that to "upload first" and never widen the retained set.
- A group commits on both sides or on neither. The presentation is asked first through `commit_staged_group`, which verifies every member before changing any slot; the runtime refuses a group that is not fully ready; the bridge marks nothing committed when either refuses. Never reintroduce a per-coordinate commit whose result is ignored.
- GPU memory claims must use the simultaneous `committed + staged` peak. Adding the committed peak to the staged peak overstates it, and reporting the committed peak alone understates it by the whole cost of atomic transitions: that mistake turned a −19% saving into a claimed −57%.
- The debug views are off by default and must stay free when off (0 draws, 0 uniform writes, untouched mesh upload budget), otherwise every benchmark on this path becomes incomparable. One bind group and one draw per box costs 15–38 FPS at 637 boxes (KI-012); that is accepted diagnostic cost, not a reason to add instancing or batching.
- The debug module reads state, it never derives it. Record states come from `tracked_states()`, presented levels from the bridge's committed stamps, groups from `transition_groups()`. A renderer-side guess about streaming state would be a new source of truth.
- Transition groups join two adjacent dirty chunks only across a drawn side whose level or seam contract toward the other changes (`seam_changes`). Joining every dirty adjacency chained the whole frontier to the re-dirtied `Lod0` ring and starved entrants for seconds while the gap counter read zero; keep `ready_undrawn_*` and `constrained_undrawn_*` in every LOD benchmark and treat a non-zero `ready_undrawn` as a hole.
- The seam-coherence checker's rule is part of the contract: a drawn contract toward an undrawn neighbor in render demand must match that neighbor's target level unless the drawn chunk's own replacement toward that face is pending. Do not "fix" a checker failure by weakening it to "any undrawn neighbor is fine".
- A run's zero-gap counter is not visual proof. Mid-movement captures (and burst captures assembled into contact sheets when a hole is suspected) are the evidence; a counter that only sees chunks that were drawn before cannot see entrants that never appeared.
- `ChunkPresentation` has exactly two implementors (renderer, test double). It is a testability seam, not a render abstraction; do not add methods the bridge does not call.
- Camera anchoring must stay `floor`-based with explicit non-finite/out-of-range rejection; `as i64` truncates toward zero and would misplace every negative sub-voxel position.
- With one worker, `finalize_evictions` before `dispatch_one` means an eviction backlog cannot trigger `hard_cap_blocks`; the cap holds through accounting. Re-validate that reasoning if worker count or poll order ever changes.
- M3C must never let a coarse level hide a known crack or turn an unavailable neighbor into AIR; the seam rule is decided in the plan and tested per direction and occupancy case before any rendering work.
- Keep every new mesher or slab path generic over `EDGE`; the 32-edge fingerprints and topology tests are the tripwire for accidental edge-specific code.
- `Lod1` meshes are in 16-cell units; the renderer applies `scale.x = 2` in the model uniform and never scales the translation. Any new presentation path must keep both levels spanning the same `32³` world volume (test `both_levels_span_the_same_world_volume_at_positive_and_negative_chunks`).
- Benchmarks and driven smokes need an idle desktop: injected keys reach only the foreground window. The bench script re-asserts focus before every input and aborts as invalid when it cannot; a run with `cpu_evictions = 0` on the traversal path did not move and must be discarded.
- The seam rule's orientation convention is `from_neighbor`'s: `face` is the direction from the center, and slab constructors pick the neighbor's touching layers themselves. The first seam oracle got this backwards; the tests now encode the convention.
- Retries are for the host linker lock only (`LNK1104`, KI-008). A failing assertion, panic, test, Clippy, or build error is evidence and is never re-run until it passes.
- The no-LOD wider-radius baseline is part of M3C's evidence, not an afterthought; LOD stays enabled only if the recorded decision rule passes on the audited host.
- Treat integrated Intel Iris Xe as one conservative host, not the target matrix.
- Do not change repository visibility, replace/configure remotes, publish releases, choose a license, or present the working title as cleared without owner authorization.
