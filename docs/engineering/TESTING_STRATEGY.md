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

The workspace has **755 tests** that run by default and **758** with `--run-ignored all`; the ignored ones are run deliberately: M6's offline listening fixture, M7's whole-region reachability report — which M8 extended with a landmark section — and M7's adversary placement re-derivation.

M7's tests are organised around the three things that could silently be wrong.

**The rule could read the wrong height.** `a_step_is_judged_from_the_ground_under_the_body_and_never_from_the_pelvis` runs three bodies on identical trajectories with the pelvis settled, half a unit behind and forty units behind, and requires bit-identical results; `a_body_climbs_consecutive_one_voxel_terraces_without_stalling` and `a_real_encounter_walks_a_stepped_ramp_without_the_pelvis_stopping_it` walk a real staircase at the real speed, the second of them through the whole authoritative tick loop with the pelvis filter running. The bounds are pinned at exactly `max_step_up` and `max_drop` and at an epsilon past each, and `every_block_reason_is_reachable_and_the_first_that_applies_is_reported` produces all six refusal causes and asserts their precedence.

**The audit could be about a different world than the game.** `the_cached_grid_agrees_with_the_runtime_adapters` compares the sampled grid against the production `TerrainGround` and `TerrainWalkability` at every column centre of six chunks. The audit itself calls `veldwake_combat::check_move` rather than reimplementing it, so there is one rule; the grid is only the data it reads.

**The water predicate could disagree with the drawn world.** `traversal_water_agrees_with_the_voxels_the_generator_writes` generates six chunks over three vertical levels and asserts, column by column, that traversal is refused exactly where a water voxel exists — TRAVERSE-001. `the_continuous_and_the_voxel_water_predicates_genuinely_disagree` proves the distinction is not pedantry by requiring the disagreement set to be non-empty, and asserts traversal follows the voxels. The region edge is tested as a half-open rectangle, fractional positions are tested to be judged by the column that contains them, and negative coordinates are ordinary inputs.

**And a topological result is not a runtime result.** `the_route_is_walkable_in_a_real_encounter` drives a real `Encounter` tick by tick over the real adapters along the derived waypoints and asserts the body arrives, never enters water, never leaves the region, never exceeds the walk speed per tick while walking, and that the adversary wakes. It is the test that would catch any divergence between the settled-column breadth-first search and what a body driven by intent actually does.

Two client tests carry the timing claims. `traversal_is_the_same_walk_at_every_rate_the_catch_up_cap_allows` delivers the same exact twenty seconds and the same input trace in 50, 60, 144 and 240 frames a second and compares the authoritative end state bit for bit, with streaming and the camera deliberately excluded. `a_frame_at_the_catch_up_cap_can_lose_a_tick_to_the_remainder` states the thirty-hertz boundary with a number instead of leaving it to a playtest. `the_dodge_gate_is_read_inside_the_tick_loop_not_above_it` walks a body to within one frame of the aggro radius and fires a four-tick frame, because a gate hoisted above the loop would make a dodge depend on how the frames were cut.

### What M7's branch QA added

Eight tests, from four questions a reviewer asked that the branch had not.

- **Does the column agree with the drawn world, and where?** `the_ground_query_matches_the_generated_voxels_at_every_column_centre` builds an oracle out of generated chunks — topmost terrain voxel per column, vegetation and water excluded — and requires the ground query to equal its top face at every column centre. `away_from_a_column_centre_the_ground_query_may_differ_but_only_within_two_voxels` then asserts the *opposite* away from the centre: that the disagreement is real, so nobody simplifies it away, and that it never exceeds two voxels. Together they are why the reachability audit is an upper bound, stated as a measurement.
- **Is the water veto column-shaped?** It is not, and the test that assumed so failed within seconds. `the_water_veto_disagrees_inside_a_column_only_along_the_waterline` says the true thing instead: `TerrainWalkability` samples continuously, so a column near the drawn waterline disagrees with itself, and every column that does has a neighbour of the opposite verdict. Never inland.
- **Can the route be walked by something that is not the audit?** `no_step_of_the_route_cuts_a_corner_between_two_blocked_columns` rules out the classic eight-connected squeeze, with a guard against becoming vacuous if the route loses its diagonals. `the_runtime_accepts_every_step_of_the_route_it_walks_continuously` walks every leg in walk-speed increments and puts each one to `check_move` with the production adapters — thousands of continuous samples, none refused.
- **Is the adversary's clearing verified by anything that did not choose it?** `the_adversary_stands_where_the_drawn_voxels_allow_a_fight` re-derives level, dry and clear from the generated voxels without touching `SurfaceGrid` or the placement predicates, and bounds the walk with a Chebyshev lower bound that needs no graph.
- **Is "the region" one idea or two?** The extent becomes a continuous half-open rectangle for the veto and integer column indices for the grid, by the same arithmetic done twice. `the_grid_indexes_exactly_the_region_its_bounds_describe` checks both corners in both directions, so the two cannot drift apart unnoticed.
- **What does `check_move` do with a number that cannot be ordered?** `a_move_is_refused_for_every_shape_of_unorderable_number` covers infinities as well as `NaN`, at both ends of the move and from the ground sampler itself. `the_arena_bounds_the_destination_and_never_the_body_already_outside_it` pins the rule that reads like an oversight, including the half QA guessed wrong: a step that stays outside is refused however much closer it gets.

## Landmarks

M8's tests answer four questions that could each be silently wrong, and they are deliberately split between the crate that composes the world and the client that shows it.

**The composition could be an accident.** `the_golden_composition_is_locked` compares the derived plan against `LANDMARK_BEHAVIOR_SIGNATURE`; `the_plan_is_a_pure_function_of_the_world` derives twice and compares fingerprints, origins, base courses, crown columns and compiled geometry. The rest of the plan's tests assert the composition's *meaning* rather than its coordinates: three classes, two first choices and one reveal, the first pair at least `min_separation_degrees` apart as seen from the overlook, the reveal within its band of the landmark that reveals it, nothing crowding anything, everything inside the region and under its ceiling, and a reservation covering every column a landmark fills.

**The chunks could disagree with the plan.** `a_landmark_reads_the_same_from_every_chunk_that_holds_a_piece_of_it` walks every chunk a landmark touches and requires the drawn voxel to equal `material_at` inside the bounds and to be no landmark material outside it. `a_landmark_rests_on_terrain_in_every_column_it_fills` reads the generated voxels under and over each column: terrain under a grounded course, air under a lintel, air over the top — and cross-checks the count of opening columns against the compiled silhouette. `a_landmark_crossing_a_seam_is_written_identically_from_both_sides` generates the same chunks in both orders. `no_plant_stands_inside_a_landmark` asks the one canonical `WorldVegetation` view, which is the same composition chunk generation uses.

**A visible wall could be walkable, or a gate could not be.** `a_body_cannot_walk_into_a_landmark` refuses a grounded column through the runtime veto, the cached grid and `standable`; `a_landmark_blocks_the_runtime_rule_and_not_only_the_audit` gets `MoveBlockReason::Traversal` out of `check_move`; `the_keep_out_is_the_widest_body_the_world_carries` compiles both rigs and pins the keep-out to the adversary's capsule rather than to a constant somebody typed; `the_gate_is_a_gate_and_a_body_can_walk_through_it` runs a breadth-first search with the real movement spec from one side of the gate to the other and requires the path to cross the footprint with stone at least a body's height overhead, and `the_runtime_walks_through_the_gate_and_not_only_the_audit` re-walks that crossing in walk-speed increments through the production adapters, because a column graph is an upper bound.

**The placement could be about a world nobody looks at.** The client's `landmark` module projects each landmark through the real follow camera with the real projection and the real fog table, and `the_world_proxy_and_the_presentation_agree_about_what_is_visible` requires that anything the world proxy calls visible the camera can actually frame. The two levels measure different things on purpose — the proxy does occlusion and no framing, the oracle does framing and no raycasting — and the tests say which is which.

### What branch QA added

M8's own tests ask the plan and the compiler whether they agree with themselves. Branch QA added a second layer that does not: oracles built from the voxels the generator writes, from a world of the same identity composed **without** landmarks, and from the authoritative tick loop.

- `qa_a_landmark_only_ever_replaces_air` differences every chunk a landmark touches against that landmark-free baseline and inspects every voxel that differs. It is the proof that no terrain or water moved, and it needs no cooperation from the code that wrote the landmarks.
- `qa_landmark_chunks_do_not_depend_on_the_order_they_are_asked_for` generates the same chunks under six permutations, one at a time in fresh worlds, and through a clone.
- `qa_the_keep_out_agrees_with_the_voxels_a_viewer_can_see` reads the landmark solids out of the chunks and checks the traversal veto against them at quarter-column resolution, in both directions: never walkable where a body would hold stone, never fenced off where it would not.
- `qa_a_real_encounter_walks_a_body_through_the_gate_and_into_its_pillars` drives a real encounter through the opening and into a pillar with the same machinery and opposite expectations.
- `qa_a_plant_the_grammar_proposes_inside_a_reservation_is_gone_from_the_world`, `qa_no_landmark_stands_on_a_shoreline` and `qa_the_overlook_is_a_place_the_finished_world_lets_a_body_stand` check the composition's own promises against the field and the drawn voxels.
- `the_fingerprint_reacts_to_every_landmark_control` closes a gap: the identity's "reacts to every input" test predates the landmark controls and did not cover them.
- `the_compiler_refuses_a_descriptor_it_would_have_to_guess_at` is the regression for the one defect QA found.

## The weapon exchange

M9 adds twenty-nine tests across the domain and the client, and the workspace
now has **799** — 796 by default and three `#[ignore]`d, all of which pass when
run explicitly.

What the domain asserts, all through the authoritative tick loop rather than
about the code:

- an encounter with no reward configured cannot exchange, refuses nothing and
  counts nothing, so every M6, M7 and M8 fixture is provably inert;
- an exchange needs a world that offers a site, and is refused out of range,
  while attacking, dodging, staggered or defeated;
- **attack outranks dodge outranks interact**, and a press a swing consumed does
  not fire later as a stale latch — asserted over two hundred following ticks;
- the adversary resolves to the original weapon and the original spec in every
  armament state (ARM-002);
- the player's weapon, spec and swept blade all follow the armament;
- the historical aim-assist range is **exactly** `2.9000` for both M6 bodies and
  larger for the found weapon;
- **ARM-001** by driving a real defeat: the reset restores the body, its
  position, its facing and its health, and leaves the armament alone. The
  victory case under `Remain` is asserted the same way;
- an exchange is its own inverse, and a new encounter starts over;
- the found weapon connects from further away **in the real loop**, measured by
  a sandbox drill against a dormant target so only the player's weapon is in
  play;
- the sidegrade relation — the closing time the reach buys is within eight ticks
  of the lock the commitment costs — asserted as a relation, not a lock.

What the client asserts:

- the anchor is the gate's own opening centre offset by one column along the
  gate's span axis, still inside the footprint and off the walking line;
- the site is dry, level to a voxel over the interaction radius, clear of final
  vegetation, outside every landmark keep-out, standable and reachable on foot,
  all from the **production adapters**;
- a session does not begin inside its own reward;
- the planted blade's tip lands on the ground it was put on, at the column
  centre, the right way up and the right length;
- **the gate is still walkable with the weapon standing in it**, in both
  armament states, driven through a real encounter (LAND-002 re-proved);
- **resolving a reward writes no voxel into the world**, by generating the
  chunks around the gate from a generator the reward was resolved against and
  one it was not and comparing them — the guard that a future change making
  placement reach into generation would trip;
- `E` maps to interact and no other key does;
- an interact-only first input arms the session and is not swallowed, and an
  interact is not suppressed while the adversary sleeps;
- a held exchange key latches once per press.

The M9 locks are `FOUND_WEAPON_GEOMETRY_FINGERPRINT`,
`FOUND_WEAPON_IDENTITY_FINGERPRINT`, `FOUND_ENCOUNTER_SIGNATURE` and
`REWARD_BEHAVIOR_SIGNATURE`. The found encounter uses the **same trace format**
as `GOLDEN_ENCOUNTER_SIGNATURE` and is not expected to equal it.
