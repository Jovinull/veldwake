# Observability

Status: **Accepted requirement; M3 streaming telemetry and debug views implemented, M4 adds world and headless world inspection, broader engine observability remains incremental**.

The client's startup line reports the selected world, its fingerprint, the named camera pose, the camera position, and the weather state, so a capture can be tied to exactly the world and viewpoint that produced it. `terrain-probe` is the headless counterpart and needs no GPU: it prints landform, zone, height, and material maps of the region, a full report for any column including how far above the surface a camera must stand to clear every plant, one chunk's material histogram and mesh cost, the locked regional signature with every named probe, measured canopy coverage over the meadow, and per-chunk generation and meshing timings.

Development builds should explain performance and procedural causality, not merely display failures.

## Runtime telemetry

A future overlay may show FPS, CPU/GPU frame time, frame phases, RAM, VRAM estimate/budgets, draw calls, triangles, visible/resident chunks, generation and meshing queues, entities, simulation ticks, network latency/traffic, save work, cache hits, and audio voices/DSP time.

Metrics need units, sampling windows, percentiles where relevant, and low enough overhead for routine use. Structured `tracing` spans should connect async generation, uploads, saves, and simulation work by stable IDs.

## World inspection

Tools should query a coordinate/entity/settlement and report inputs and causes such as biome, elevation, temperature, humidity, geology, resources, seed path, generator/version, civilization/faction influence, population, danger, routes, events, and materialization state.

## Failure diagnostics

- Validation errors identify the descriptor/path/version and rejected constraint.
- Async jobs expose queue, priority, age, cancellation, and outcome.
- Save/network errors are never silently replaced with arbitrary defaults.
- GPU adapter/backend/features/limits and validation messages are recorded at startup during rendering milestones.

Procedural generation without decision provenance is considered incomplete because regressions become impractical to reproduce.

## What the M3 streaming client reports today

The client emits exactly two aggregate `info` lines every five seconds, never one line per chunk per frame: `M3B streaming state` (what exists right now) and `M3B streaming work and budgets` (what has happened, cumulatively or per interval). Counters are cheap integers; nothing here is sampled or estimated. This section maps questions to counters so the next agent does not read sixty names looking for three.

**Is the visible world complete?**
`gaps_closed` with its per-cause split (`gaps_lod`, `gaps_neighbor`, `gaps_membership`, `gaps_data`, `gaps_unattributed`), `gap_frames_total`, `gap_frames_max`, `gap_max_simultaneous`, `gap_current_missing`, `gap_frames_with_missing`. A gap is a chunk that **was drawn**, stopped being drawn while still in render demand, and has not returned. That definition has a blind spot by construction: a chunk that was never drawn cannot open a gap, so a frontier that never appears reads as zero.

The ready-but-undrawn family closes the blind spot by counting render-demand chunks whose replacement exists on the CPU but is not on screen: `ready_undrawn_*` for the total, then one counter per reason, because the reasons have very different meanings. `frontier_pipeline_pending_*` is a chunk still moving through load and mesh, `ready_awaiting_upload_*` is the per-frame upload budget, `ready_blocked_transition_*` is a replacement held back by its transition group, and `committed_missing_*` is the runtime considering a mesh committed while the bridge is not drawing it, which is a bug rather than a delay. A short transient in the first two is normal; a sustained `ready_blocked_transition` is the starvation pattern that once hid four seconds of missing floor behind zero gaps. No counter here replaces a mid-movement screenshot.

**Why is a chunk not drawn yet?**
`blocked_groups_now`, `blocked_group_max`, `constrained_undrawn_now`, `constrained_undrawn_max` describe transition groups that could not commit and how many undrawn members are legitimately waiting on a drawn neighbor whose seam toward them changes (KI-011). `transition_pending` counts committed meshes whose target already differs.

**Did the atomic commit hold?**
`transition_commits`, `transition_chunks`, `committed_retained`, `committed_dropped`, `restaged`, `staged_discarded`. `presentation_commit_failures` counts groups the presentation refused because a member had no staged replacement, and `commit_invariant_failures` counts the unreachable case of the runtime committing fewer chunks than the presentation swapped. Both must be zero; a non-zero value is a broken invariant, not a budget effect.

**How many presentation-owned chunk-mesh GPU bytes are held?**
`chunk_mesh_committed_bytes` and `gpu_staged_bytes` are the two live components, `chunk_mesh_total_bytes` their sum, and `peak_chunk_mesh_total_bytes` the highest simultaneous sum observed. `peak_chunk_mesh_committed_bytes` and `peak_staged_bytes` are component peaks and **must never be added together**: they occur in different frames, so their sum overstates the real high-water mark, while quoting the committed peak alone understates it by the entire cost of atomic transitions, which hold a replacement and the mesh it replaces at the same time. Reporting the committed peak alone once overstated LOD's memory saving by roughly a factor of three; the recorded figures live in `PERFORMANCE.md`, not here. Sample the owner of the memory, and sample often enough to catch the peak: a per-update sample can miss a spike that a staging mutation creates and a group commit resolves inside the same update.

**Is streaming keeping up?**
`loads_dispatched`, `meshes_dispatched`, `queued_loads`, `queued_meshes`, `jobs_in_flight`, `stale_loads`, `stale_meshes`, `stale_lod`, `fairness_loads`, `hard_cap_blocks`, `cpu_evictions`, `eviction_budget_hits`, `interval_uploads`, `interval_upload_bytes`, `interval_deferred_uploads`, `removal_budget_hits`, `upload_failures`, `oversized_uploads`, `time_to_idle_ms`. Stale counters record results correctly rejected, not failures.

**What did the experimental disk cache do?**
`cache_lookups`, present/absence hits, misses, stale/corrupt rejects, read failures, rejected entries removed, rejected-entry delete failures, source fallbacks, write attempts/writes/skips/failures, bytes read/written, and encode/decode total/max time appear in the same aggregate work line. They stay zero when `VELDWAKE_CACHE_DIR` is unset. Cache accounting is integrated before request-token validation: a stale load remains rejected by streaming while the disk work that already happened remains visible. A delete failure is not called a repair; repeated reject + delete-failure + skipped-write growth identifies a poisoned entry that could not be replaced. Cold-open footprint and temporary-sweep results are logged once at startup, not per frame.

**What does LOD cost?**
Per-level desired, ready, committed, and GPU counts (`lod0_*`, `lod1_*`), per-level upload totals, `lod_swaps`, and the four timing totals with maxima: `snapshot_build`, `lod1_derivation`, `worker_mesh_lod0`, `worker_mesh_lod1`. `lod1_derivation` is a measured subset of `snapshot_build`, not an additional phase. Total measured CPU work for snapshot construction plus worker meshing is therefore `snapshot_build + worker_mesh_lod0 + worker_mesh_lod1`; report `lod1_derivation` separately to explain the snapshot cost and never add it twice.

**What is the debug overlay doing?**
`debug_mode`, `debug_boxes`, `debug_draws` (one draw per primitive last frame), `debug_slots` (pooled uniform buffers and bind groups, the high-water mark of the pattern's retained cost), `debug_primitive_allocations`, and `debug_uniform_writes`. With the view `Off`, per-frame debug draws, primitive allocations, and uniform writes are zero; `debug_slots` may remain non-zero after a view has been used because the renderer deliberately retains and reuses those fixed-capacity resources. Mesh upload budgets are independent of debug work. Fixed startup pipeline/unit-geometry resources and retained slots mean that `Off` is not a claim of zero total debug memory.

Frame timing is `frames`, `report_seconds`, `average_wall_frame_ms`, `observed_fps`, `renderer_render_wall_mean_us`, and `renderer_render_wall_max_us`. The render-wall interval wraps the whole `Renderer::render()` call (including surface acquisition and presentation), so it is not CPU-submit or GPU time. Under vsync the frame average is pinned at the refresh interval and only tells you when something has fallen *below* it; render-wall timing moves in the opposite direction when frame rate drops, because the vsync wait leaves the measured region.

## What the M5 character client reports today

Two lines, both scoped so that a run with no character says nothing about one.

**What was compiled, and is it the character this build claims?**
One `character ready` line at startup: `selection`, the identity `fingerprint`, the `geometry` fingerprint, `height_voxels` and `height_units`, `parts`, `quads`, `gpu_bytes`, `dynamic_upload_bytes_per_frame`, `world_draws`, `shadow_draws`, and `grounded`. The two fingerprints are separate on purpose — a repaint moves the identity and leaves the geometry alone — and the byte figures are the ones the `character-probe` prints, so a disagreement between probe and client is visible rather than assumed. An `character uploaded` line reports the same geometry from the renderer's side.

**What is the character doing?**
A `character state` line every five seconds: `selection`, `elapsed`, `x`, `z`, `base_height`, `facing_degrees`, `speed`, `phase`, the `moving` and `run` blend weights, `grounded`, and per side `stance` and `clearance`, together with the `landform` and `zone` of the column underneath. `clearance` is the signed distance from a sole to the block top it should be resting on, so a contact defect is a number in the log before it is a defect in a capture — but the number is not the evidence, because a sole can be exactly on a surface the viewer cannot see. Both are checked.

Frame timing is unchanged and is where the character's cost is read: `renderer_render_wall_mean_us` and `renderer_render_wall_max_us` with the character off and on, at the same world, camera, weather and settle.

## What the M6 combat client reports today

Two lines on the same five-second cadence the streaming and character reports use, so one capture aligns with one interval.

`combat state` — where the fight is: mode, whether it is armed and whether it is frozen, the tick index, how many ticks a named moment still owes, the distance between the bodies, each side's action, elapsed ticks, health, position and facing, the adversary's brain state and timer, the outcome, and the three input-latch flags split so a stuck latch names itself.

`combat work` — what it has done and what it cost: ticks run, ticks this run, ticks the frame cap discarded, the most ticks one frame ran, frames that ran none, the clock's own dropped and capped-frame counters, swings, hits, whiffs, dodges, refused dodges, staggers, defeats and resets per side, separations, blocked moves per side, hit queries, the worst sweep substep count, suppressed multi-hits, dropped events at both levels, the camera's strike count and whether it is moving, live particles split by effect with their high-water and dropped counts, effect draws, instances and instance bytes, and the audio block — whether a device is open, its rate and channels, callbacks, frames, voices started and displaced, voice and buffer high-water marks, peak sample in millionths, device errors, and the queue's waiting, consumed and dropped counts.

The counters that must stay zero are stated as such: `events_dropped`, `frame_events_dropped`, `vfx_dropped`, `audio_queue_dropped` and `audio_device_errors`. A non-zero value in any of them is a bound that was too small, and the number says by how much.

With `VELDWAKE_ENCOUNTER=off` neither line is emitted at all, no encounter is built and no audio device is opened — the absence is the observation, and it is how the off contract is checked rather than by looking at a screenshot.


## What the M7 traversal client reports today

Three additions, all on the existing five-second cadence.

**Which route this session is about to walk.** One `traversal route ready` line at startup in `traverse` mode: the traversal rule version, the start and goal columns, the column and waypoint counts, the length in world units, the walking duration at the compiled speed, the chunk boundaries crossed on each axis, the measured route signature **beside the locked one and whether they match**, the audited grid size, how many columns are reachable and standable, whether the highland is reachable, and the region's half-open bounds. One `traversal checkpoint` line per named place, with the claim its name makes. Deriving it costs one region sample and one breadth-first walk — under a second in release, once — and it is what ties a capture or a log to a route rather than to a hope.

**Where the world is streaming around, and whether the enemy is awake.** `combat state` gains `traversal`, `streaming_anchor` (`body` or `camera`), `adversary_dormant`, `dodges_suppressed` and `outcome_settled`. A settled outcome means the defeat hold finished and the policy chose not to reset, which is how walking away from a won fight is read from a log rather than from a screen.

**Where the camera is.** `camera_x`, `camera_y`, `camera_z` and `camera_detached`. A frame that does not contain the body is a question the log should be able to answer, and the first M7 shoreline captures forced exactly the reverse — reasoning backwards from a screenshot to a camera position. KI-026 was characterised from these three numbers in one run.

**A body held against a barrier slides; it does not block.** `slid_moves_player` and `slid_moves_adversary` sit beside `blocked_moves_*`. `MoveResult::blocked` is only true when *no* axis of a proposal was legal, so a body pinned at a waterline that still travels along the shore reports zero blocked moves for as long as it is held. The first water run walked into the river, stopped dead in `z` and slid twenty-three world units west with `blocked_moves = 0` the whole way; with the counter it reports `slid = 195` on the tick it reaches the shore.

**Where a frozen moment actually stopped.** `encounter frozen at a named moment` now reports `tick`, `offset` and `frozen_at` rather than the moment's tick alone. Branch QA found that `moment:confirmed-hit` and `moment:confirmed-hit+6` both printed `tick=499` while the second was frozen at `505`, and that a milestone document had recorded the line's number as the fact. A value computed from another belongs in the log beside it.

**What branch QA measured with all of this.** The `combat state` cadence is five seconds, which is fine for a session and too slow for an event: a player defeat, its `2.5` s hold and the encounter reset all fit inside one interval, and a loop polling the log four times a second never observed a zero health. Evidence about anything shorter than the report interval has to come from continuous capture, not from the log. The interval was left alone.

## What the M8 landmark world reports today

Nothing new in the client's frame-loop telemetry, deliberately: a landmark is voxels in a chunk, and the streaming and combat lines already describe those. What M8 adds is headless.

`terrain-probe landmarks` prints the plan and, before it, **re-derives the plan and refuses to print if the second derivation disagrees with the world's own** — determinism as a runtime check rather than as a claim. It reports the derivation's cost in the current build profile, the plan fingerprint, the overlook's column, ground face and clearing radius, and for each landmark its role, class, crown column, distance from the overlook, vertical bounds, footprint, height, opening columns and their height, the silhouette the world proxy measured with how much of it falls under the elevation limit, whether it is backed by sky, the descriptor and geometry fingerprints, which landmark reveals it, its voxel count by material, span axis, lean and seed.

The `#[ignore]`d whole-region reachability report gained a landmark section: the overlook, and for each landmark the nearest standable column, how many steps it is from the route start and how far out the search had to look — which is how "can a player actually get there" is answered with the game's own movement rule rather than with a straight line. It also prints which landmark hosts the fight and what the hosted placement search returns.
