# Observability

Status: **Accepted requirement; implementation staged**.

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

**How much GPU memory is really held?**
`chunk_mesh_committed_bytes` and `gpu_staged_bytes` are the two live components, `chunk_mesh_total_bytes` their sum, and `peak_chunk_mesh_total_bytes` the highest simultaneous sum observed. `peak_chunk_mesh_committed_bytes` and `peak_staged_bytes` are component peaks and **must never be added together**: they occur in different frames, so their sum overstates the real high-water mark, while quoting the committed peak alone understates it by the entire cost of atomic transitions, which hold a replacement and the mesh it replaces at the same time. Reporting the committed peak alone once overstated LOD's memory saving by roughly a factor of three; the recorded figures live in `PERFORMANCE.md`, not here. Sample the owner of the memory, and sample often enough to catch the peak: a per-update sample can miss a spike that a staging mutation creates and a group commit resolves inside the same update.

**Is streaming keeping up?**
`loads_dispatched`, `meshes_dispatched`, `queued_loads`, `queued_meshes`, `jobs_in_flight`, `stale_loads`, `stale_meshes`, `stale_lod`, `fairness_loads`, `hard_cap_blocks`, `cpu_evictions`, `eviction_budget_hits`, `interval_uploads`, `interval_upload_bytes`, `interval_deferred_uploads`, `removal_budget_hits`, `upload_failures`, `oversized_uploads`, `time_to_idle_ms`. Stale counters record results correctly rejected, not failures.

**What does LOD cost?**
Per-level desired, ready, committed, and GPU counts (`lod0_*`, `lod1_*`), per-level upload totals, `lod_swaps`, and the four timing totals with maxima: `snapshot_build`, `lod1_derivation`, `worker_mesh_lod0`, `worker_mesh_lod1`. CPU mesh time on a path is their sum, and that convention must be kept when comparing runs.

**What is the debug overlay doing?**
`debug_mode`, `debug_boxes`, `debug_draws` (one draw per primitive last frame), `debug_slots` (pooled uniform buffers and bind groups, the high-water mark of the pattern's cost). With the views off both counts are zero and no uniform is written, which is what keeps benchmarks comparable.

Frame timing is `frames`, `report_seconds`, `average_wall_frame_ms`, `observed_fps`, `interval_submit_mean_us`, and `interval_submit_max_us`. Under vsync the frame average is pinned at the refresh interval and only tells you when something has fallen *below* it; submit timing moves in the opposite direction when frame rate drops, because the vsync wait leaves the measured region.
