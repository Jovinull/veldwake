# Testing strategy

Status: **Accepted direction; capabilities arrive with systems**.

## Layers

- Unit tests: local algorithms, invariants, edge cases, and error behavior.
- Integration tests: subsystem boundaries, formats, command flows, caches, and headless authority.
- Property tests: geometry, coordinate transforms, parsers, seed derivation, migrations, and round trips where broad input space matters.
- Golden seeds: stable worldgen and procedural-content fixtures with explicit compatibility intent.
- Visual regression: controlled scene/seed/camera/light/backend captures and perceptual review.
- Fuzzing: save/network/mod/parser boundaries and generator descriptors, initially where platform support is practical.
- Benchmarks/profiling: measured hot paths and representative end-to-end fixtures.
- Soak/replay tests: streaming, save/load, materialization, and long-running simulation when available.

## Test ethics

A failing test is evidence. Do not delete it, reduce its assertion, increase tolerance, regenerate a golden, or add a fallback solely to turn CI green. Explain whether behavior, test, or requirement is wrong; preserve evidence and review intentional changes.

## Deterministic visual fixtures

Planned fixtures include forest, character, sword, creature, and village seeds. Store canonical configuration alongside the baseline: generator/style/renderer versions, camera, light, viewport, quality, and allowed variance. Cross-vendor GPU differences may require backend-specific policy; do not claim pixel identity prematurely.

## Current client applicability

Fifty-three GPU-independent client tests cover camera/input, surface selection, signed transforms, GPU payload sizing, streaming bridge transactions/coverage metrics, and debug-state mapping/geometry. CI uses plain `cargo nextest run --workspace`; an accidentally empty suite is a failure.

GPU/window behavior remains a separate Windows host smoke test because CI must not require a graphical adapter. Coverage, property tests, fuzzing, automated visual regression, and performance benchmarks remain **NOT YET APPLICABLE**, not “passing.”

## Current M2 applicability

Eleven dependency-free voxel tests cover chunk strides and index uniqueness, bounds versus air, mutation and solid-count invariants, the locked diagnostic fixture/fingerprint and its exact 132/528/792 topology, empty/single/adjacent/solid chunks, all six face directions, internal-face removal, and triangle winding. The release `voxel-probe` reports topology, logical payload bytes, and one diagnostic CPU timing sample; it is not a benchmark or regression threshold. GPU/window behavior is validated separately by the Windows host smoke test.

## Current M3A applicability

Twenty dependency-free voxel tests now additionally lock Euclidean world/chunk/local conversion (including negatives and range extremes), checked axial-neighbor overflow, explicit known-versus-missing sampling, both boundary policies, solid/solid and solid/AIR seams in all six directions, canonical fixture order/fingerprint, and aggregate 202/808/1,212 topology. Existing M2 tests remain unchanged in meaning. The Windows smoke separately covers signed placement, visible external boundaries, seam appearance/culling, camera traversal, resize, minimize/restore, focus loss, and clean Escape shutdown.

## Current M3B applicability

Twenty-two voxel tests include owned slab sampling in every face direction and exact topology equivalence between the borrowed M3A neighborhood and the owned snapshot mesher. Thirty-one streaming tests cover deterministic demand sets, signed movement and oscillation, valid/invalid configuration (retention covering the halo, checked radius addition, zero budgets, oversized radius, `render ⊆ dependency ⊆ retention`), CPU-only dependency retention, cap reservation and cap-safe teleport, bounded eviction finalization, a full backlog refusing loads until released, cap invariance while a real-worker teleport backlog drains, request-token ABA/overflow, stale load and mesh stamps, source absence versus temporary unavailability, `KnownAbsent` render chunks never reporting a pending mesh, retention-only chunks never being offered for rendering, neighbor arrival, unload during work, load/mesh fairness, the finite diagnostic source, the real worker path, and clean shutdown.

Client streaming remains tested against an in-memory `ChunkPresentation` double with the real runtime and worker: camera anchoring, upload/release budgets, draw-set reconciliation, transitions, atomic group commit, coverage classification, LOD, debug modes, and restage order all stay headless. GPU-specific renderer behavior remains covered by contract tests plus the recorded D3D12 milestone smoke.

M3D has 40 headless cache tests. They cover layout/endianness; raw and RLE round trips; empty chunk versus known absence versus missing file; every short header; hostile raw/RLE lengths and runs; arbitrary coordinates and voxel IDs; checksum/identity/version rejection, including a damaged structural field classifying as corrupt while a valid resealed foreign field is stale; exact raw, absence, and 3× worst-case RLE sizes; bounded oversized-file reads; traversal-free names; normal and concurrent publication; fingerprint isolation; temporary sweeping; cold/warm behavior; cleanup/write failures preserving source results; repeated poisoned-entry observability; raw↔RLE preference changes; clearing; exhaustive finite-source behavioral identity; stale token rejection with cache accounting preserved; and cacheless counters remaining zero. The workspace now has 275 tests (38 voxel, 69 procedural, 97 streaming, 71 client); every pre-M4 test remains.

M4 adds 69 procedural tests and extends the streaming and client suites. The procedural tests are organised around what could silently break:

Branch QA adds cache/source mismatch rejection before worker startup, hostile public terrain-config rejection, exhaustive 1,875-chunk behavioural cache-identity locking, a complete generated vertical-envelope walk, and every x seam of the golden river. `CacheFormatError` remains private because no external crate encodes or decodes entries.

- **Determinism**: a chunk generated twice is byte-identical; generating a wide neighbourhood first does not change a chunk; two adapter instances agree; a different seed produces a different world and a different fingerprint.
- **Space**: height is continuous across chunk boundaries and the origin; negative coordinates are ordinary inputs; the batch sampling path agrees bit for bit with the direct one.
- **Hydrology**: the water surface never rises downstream; the channel bed stays below it along the whole axis; the meadow never floods; every x seam has no boundary-induced discontinuity while allowing adjacent-column floor quantization; water never stands above the ground it touches.
- **Material**: identifiers round-trip and never collide; air and the M3 diagnostic identifiers are not terrain materials; the palette satisfies the style bible's value separation and saturation ceiling; sediment always separates grass from water; strata band on world height and line up between neighbouring columns.
- **Vegetation**: every tree obeys the style bible's proportions; no two trunks come closer than the minimum spacing; trees stand on dry gentle grass and never in water; a tree crossing a seam is written identically from both chunks; canopy highlight stays under its ceiling; canopy coverage of the meadow stays inside the style-bible ceiling and above a bareness floor.
- **Fixtures**: named probes still describe the place they name; the locked regional signature moves with the world and with the seed; every camera pose stands in open air, clear of the ground and of any plant.

The camera-pose test exists because three of the six poses were first written from arithmetic and put the camera inside a hillside or a canopy. A pose is a fixture and gets the same treatment as content. Retries are permitted only for KI-008/LNK1104, never functional failure. `streaming-probe` is diagnostic evidence, not a benchmark threshold.

## Current M5 applicability

The workspace has **403 tests**: 38 voxel, 69 procedural, 97 streaming, 111 character, 88 client. Every pre-M5 test is unchanged in meaning.

`veldwake-character`'s 111 tests carry the whole domain, and the split is deliberate. Structure is locked by fingerprint: identity, geometry, skeleton and collision each hash separately, so a repaint moves one and a proportion change moves three, and three fixtures plus eight named poses are checked against the compiler on every run. Validation is tested by rejection, with a typed error per rule and a hostile descriptor per error. Geometry is tested against its own arithmetic — every part meshes to exactly `2(wh + hd + wd)` quads except the one carved part, every part contains its joint, the scratch grid is left clean, and left and right parts compile to identical meshes. Materials are tested by walking all 27 palettes over every compiled voxel and requiring `0.08` of luminance separation between any two face-adjacent materials, which is a style rule turned into an assertion.

Locomotion is where the interesting tests are, because this is the part a capture cannot judge. Joint angles are asserted finite and inside the contract's ranges at every phase of every gait, and separately asserted never to *need* the clamp at a nominal speed. The blend is asserted monotone and continuous in speed. Stride times cadence is asserted to be the speed it was asked about. The two that earned their keep are `a_planted_foot_does_not_slide_on_level_ground`, which measures how far a planted ankle wanders across a stance and found the walk inverted, and `a_terrain_scale_terrace_is_bounded_and_reported`, which bounds what a stance crossing a terrace can do and requires the solver to say plainly when it could not reach.

The client's 88 tests keep the boundary honest without a GPU: the identifier ranges of all three content domains are proved disjoint where all three tables are visible, the ground adapter is proved to return block tops and to keep absence absent, and its finite terrain extent is proved as a continuous half-open rectangle at both negative and fractional final-column edges. The portrait clearing is proved level, open and free of vegetation, the terrace stand is proved to have a step in front of the toes, every camera pose is proved to stand above ground at both stand points, and each of the three diagnostic courses is proved to stay in the region, stay dry, close into a loop, and — for the two that are meant to — either stay level or actually climb.

Automated visual regression remains **NOT YET APPLICABLE** for characters exactly as it is for terrain (KI-006, KI-021). The locked signatures catch a changed voxel, bone or gait constant without a renderer; nothing checks that the result still looks right, and the written assessment in the milestone document is the artefact that stands in for it.

## Combat

The combat domain is headless by construction, so everything it claims is asserted without a GPU, a window or an audio device. Four kinds of test carry it:

- **Property tests with independent oracles**, not tautologies. Partition equivalence compares three frame rates against each other rather than against the formula that produced them; the aim window is measured by standing a passive body at a bearing and swinging, not by re-deriving the reach; a swing's hits-per-swing is counted against the events, not against the intent that requested them.
- **Locked signatures with written reasons.** `GOLDEN_WEAPON_GEOMETRY_FINGERPRINT`, `GOLDEN_WEAPON_IDENTITY_FINGERPRINT`, `GOLDEN_ENCOUNTER_SIGNATURE` and `GOLDEN_ACTION_POSE_SIGNATURE`. Every re-lock carries an OLD/NEW/WHY paragraph beside the constant naming the semantic change that required it. Re-locking to make a test pass is the failure mode these exist to prevent.
- **Bounds asserted as properties.** A full event buffer drops and counts rather than growing; a full particle pool refuses and counts; a full sound queue refuses and counts; the voice limit displaces the oldest and counts. Each of those is a test, because "bounded" written in a comment is not bounded.
- **The edge cases an ordering bug hides in.** Tick boundaries off by one, the first and last tick of an active window, simultaneous attacks, a double knockout, a hit on the last active tick, a dodge one tick before, at and after an active window, a reset while effects are in flight, the accumulator cap and its remainder, zero-tick frames, a huge frame spike, coincident bodies, separation against the arena edge and against terrain a body cannot climb, and rotation-only, translation-only and combined blade sweeps.

The audio is the one place where the tests stop short on purpose. Amplitude, envelope, decay, band separation, voice limits, clipping, determinism, silence on idle and whiff-against-hit are all assertions. Whether an impact *sounds* like an impact is not, and the offline listening fixture exists so a person can decide rather than so a test can pretend to.

### What branch QA added

Four properties that were assumed rather than checked, and each was wrong:

- **A declared bound is the bound.** The sound queue said sixty-four and delivered sixty-three. A capacity test now fills it to the declared number.
- **A probe measures what it says it measures.** The partition probe truncated each frame duration, so "twenty seconds" was `19.99` and produced `2,399` ticks. It now delivers the exact interval, and the headless clock test carries the same exact-total property.
- **A sweep is relative.** The blade was swept against the victim's *end* position, and then against both endpoints with a `max()` bound — but two bodies closing on each other move, relative to one another, by the *sum* of their advances. Both are fixed and both have a test that fails under the old rule.
- **A named moment means what its name says.** `successful-dodge` only checked that a dodge was in progress while a blade was live; the capture at it showed the player staggered ten ticks later. The predicate now requires the swing to run out its active window without touching the dodger, and the test asserts it from the event stream rather than from the action's own bookkeeping.

The aim assist that followed is bounded by tests rather than by intent: at swing start only and never during it, inside its cone, inside its range, symmetric left and right, and never applied to a dodge.


## Traversal

The workspace has **690 tests**, three of them `#[ignore]`d and run deliberately: M6's offline listening fixture, M7's whole-region reachability report, and M7's adversary placement re-derivation.

M7's tests are organised around the three things that could silently be wrong.

**The rule could read the wrong height.** `a_step_is_judged_from_the_ground_under_the_body_and_never_from_the_pelvis` runs three bodies on identical trajectories with the pelvis settled, half a unit behind and forty units behind, and requires bit-identical results; `a_body_climbs_consecutive_one_voxel_terraces_without_stalling` and `a_real_encounter_walks_a_stepped_ramp_without_the_pelvis_stopping_it` walk a real staircase at the real speed, the second of them through the whole authoritative tick loop with the pelvis filter running. The bounds are pinned at exactly `max_step_up` and `max_drop` and at an epsilon past each, and `every_block_reason_is_reachable_and_the_first_that_applies_is_reported` produces all six refusal causes and asserts their precedence.

**The audit could be about a different world than the game.** `the_cached_grid_agrees_with_the_runtime_adapters` compares the sampled grid against the production `TerrainGround` and `TerrainWalkability` at every column centre of six chunks. The audit itself calls `veldwake_combat::check_move` rather than reimplementing it, so there is one rule; the grid is only the data it reads.

**The water predicate could disagree with the drawn world.** `traversal_water_agrees_with_the_voxels_the_generator_writes` generates six chunks over three vertical levels and asserts, column by column, that traversal is refused exactly where a water voxel exists — TRAVERSE-001. `the_continuous_and_the_voxel_water_predicates_genuinely_disagree` proves the distinction is not pedantry by requiring the disagreement set to be non-empty, and asserts traversal follows the voxels. The region edge is tested as a half-open rectangle, fractional positions are tested to be judged by the column that contains them, and negative coordinates are ordinary inputs.

**And a topological result is not a runtime result.** `the_route_is_walkable_in_a_real_encounter` drives a real `Encounter` tick by tick over the real adapters along the derived waypoints and asserts the body arrives, never enters water, never leaves the region, never exceeds the walk speed per tick while walking, and that the adversary wakes. It is the test that would catch any divergence between the settled-column breadth-first search and what a body driven by intent actually does.

Two client tests carry the timing claims. `traversal_is_the_same_walk_at_every_rate_the_catch_up_cap_allows` delivers the same exact twenty seconds and the same input trace in 50, 60, 144 and 240 frames a second and compares the authoritative end state bit for bit, with streaming and the camera deliberately excluded. `a_frame_at_the_catch_up_cap_can_lose_a_tick_to_the_remainder` states the thirty-hertz boundary with a number instead of leaving it to a playtest. `the_dodge_gate_is_read_inside_the_tick_loop_not_above_it` walks a body to within one frame of the aggro radius and fires a four-tick frame, because a gate hoisted above the loop would make a dodge depend on how the frames were cut.
