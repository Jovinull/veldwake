# Current handoff

Last updated: 2026-09-17

## Current position

M1, M2, M3A, and M3B are merged into `main`; M3C0–M3C3 are implemented and independently QA-hardened on `feat/m3c-lod-debug`, awaiting external review/PR. LOD remains opt-in. QA fixed three proof gaps: (1) chunk-mesh GPU high-water is now sampled after every staging mutation and group commit, not only at update end; the same path measures 12,101,888 baseline versus 10,053,696 banded bytes (−16.9%, KI-013); (2) coverage metrics distinguish frontier pipeline latency, ready waiting for upload, ready blocked by a transition group, and runtime-committed/bridge-missing state; the banded path observed two short-lived transition-blocked ready deficits, so “zero holes” is no longer claimed (KI-014); (3) `commit_group_with` holds the runtime's exclusive mutable borrow from group preflight through presentation swap and CPU commit, rejects duplicate/empty groups, and leaves both sides unchanged on presentation refusal. M3C3 `Off` means zero per-frame debug primitive allocations, debug uniform writes, and debug draws, with mesh upload budgets untouched; fixed startup resources and previously allocated reusable slots remain (KI-012). Final QA passed 141/141 headless tests, every repository gate/probe, and D3D12 smoke across all three profiles. `veldwake-voxel` owns CPU geometry; `veldwake-streaming` owns headless orchestration; the client bridge and renderer own disposable presentation state only.

## Continue here

Submit `feat/m3c-lod-debug` for external review/PR. Do not begin the swap-churn investigation, M3D, or M4 in this handoff. Any later LOD policy work must re-run the documented baseline/banded path and keep LOD opt-in unless the recorded decision rule passes.

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
- A group commits on both sides or on neither. `commit_group_with` validates unique/non-empty ready membership while holding the runtime's mutable borrow across the presentation's all-or-nothing `commit_staged_group` callback and CPU commit. The bridge map changes only after success. Never split this into independently fallible mutations or reintroduce per-coordinate commit.
- Chunk-mesh GPU claims must use the highest simultaneous `committed + staged` observation at mutation boundaries, especially after stage and before commit. This metric is presentation-owned chunk meshes only, not global GPU memory; depth, pipelines, debug resources, and driver/wgpu retention are excluded.
- Debug views are off by default. `Off` guarantees 0 per-frame debug primitive allocations, 0 debug uniform writes, 0 debug draws, and untouched mesh upload budget—not absence of the fixed debug pipeline/unit buffer or reusable slots retained after use. One bind group and one draw per box costs 15–38 FPS at 637 boxes (KI-012).
- The debug module reads state, it never derives it. Record states come from `tracked_states()`, presented levels from the bridge's committed stamps, groups from `transition_groups()`. A renderer-side guess about streaming state would be a new source of truth.
- Transition groups join two adjacent dirty chunks only across a drawn side whose level or seam contract toward the other changes (`seam_changes`). Keep the split coverage counters in every LOD benchmark. `frontier_pipeline_pending` is not-yet-known geometry; `ready_awaiting_upload` and `ready_blocked_transition` are known non-empty geometry not visible yet; `committed_missing` is a hard invariant failure and must remain zero. Any non-zero `ready_undrawn` is a temporary coverage hole and must be reported, even when bounded and seam-safe (KI-014).
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
