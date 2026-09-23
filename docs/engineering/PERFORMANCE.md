# Performance policy

Status: **Accepted method; numeric budgets TBD**.

Performance claims require a workload, hardware/configuration, build profile, metrics, and captured result. “Fast” is not evidence.

## Frame-critical principles

- No blocking disk/network I/O on frame hot paths.
- Bound generation, meshing, uploads, simulation, and save work per frame/tick.
- Prefer batches and coherent data layouts; avoid global locks and avoid allocations per frame when measurement shows them material.
- Keep render, simulation, and asynchronous workloads observable separately.
- Use LOD/streaming for geometry and simulation; distant mountains are not full voxel grids and distant NPCs are not full entities.
- Optimize the dominant bottleneck; do not micro-optimize cold setup code.

## Candidate techniques by maturity

Near-term voxel work may evaluate chunking, greedy meshing or alternatives, frustum culling, asynchronous generation/meshing, distance LOD, palette/region compression, and upload budgets. Occlusion, GPU indirect rendering, meshlets, compute culling, hierarchical depth, and GPU vegetation/particles require later evidence.

## Measurement record

Benchmarks/profiles should capture commit, tool version, hardware, driver/backend, release flags, fixture/seed, warm-up, sample count, percentiles, memory, and interpretation. Regression thresholds must account for noise and run on stable fixtures.

The 1080p/60 target from concept work remains Proposed until a target hardware tier and representative scene define CPU/GPU/memory budgets.

## Current instrumentation

The client records wall-clock presentation delta, clamps camera movement after stalls, and emits a five-second aggregate of observed frame interval/FPS through `tracing`. This is diagnostic telemetry, not a performance benchmark or evidence for the proposed target. The event loop uses `ControlFlow::Wait`: visible rendering continues by chaining `request_redraw()`, while occlusion stops that chain until restoration explicitly requests another redraw. The frame callback performs no disk/network I/O and allocates no unbounded per-frame work.

M2 constructs and meshes the one diagnostic chunk once during startup, then creates immutable GPU buffers (12,672 vertex bytes and 3,168 index bytes). It does not remesh or upload voxel geometry per frame. The release probe remains the CPU measurement source; one 2026-09-16 run measured the diagnostic fixture at 36 µs, which is an observation rather than a budget.

M3A extends the same one-time path to three static chunks without introducing jobs or streaming. Neighbor-aware meshing measured 76 µs in the final release probe run on the audited host and produced 202 quads. The client uploaded 19,392 vertex bytes, 4,848 index bytes, and 48 bytes of model uniforms once. Per-chunk buffers/bind groups and linear fixture neighbor lookup are correctness-proof choices, not scale claims or accepted future budgets.

M3B1 bounds raw resident payloads plus eviction payloads plus reserved loads before dispatch. The default hard cap is 160 dense chunks (10 MiB raw voxel payload); it is a diagnostic limit, not a shipping budget. Detached mesh snapshots copy at most 76 KiB, queues hold descriptors rather than snapshots, and only one job is in flight. Release probe runs on 2026-09-16 reached idle in 5,163–7,660 µs with 63 resident payloads, 27 meshes, and 2,064,384 cumulative snapshot bytes. This one-host observation is neither a latency target nor evidence for adding workers.

M3B2 adds per-frame presentation rails: one non-blocking runtime `poll` per frame (at most 4 results integrated, one job dispatched, at most 8 CPU evictions finalized), at most 2 GPU uploads per frame under a 4 MiB soft byte limit with a single oversized mesh allowed alone, and at most 8 buffer releases per frame. Drawability is decided every frame without budget; only physical release is budgeted. The frame callback performs no snapshot construction, disk, or network work; snapshot copies happen inside the runtime `poll` before rendering. The first release client interval on the audited host held ≈58 FPS under `Fifo` vsync while uploading 9 chunk meshes (2,214,144 GPU bytes) and settling 81 tracked records; a subsequent camera move produced 14 CPU evictions and 12 GPU releases without stale results or cap blocks. The driven smoke held 60.0 FPS (16.66–16.67 ms average wall frame) in every five-second traversal interval across 853 loads, 212 meshes, 106 uploads, and 319 evictions, with peak resident payload 3,997,696 bytes and peak GPU residency 2,223,504 bytes. Frame time is dominated by vsync in this scene, so these numbers bound nothing; capture integration and upload timing separately before changing any rail.

M3C2 measured one coarse level against a `Lod0`-only baseline at the same radius-3 visible distance on the same driven path (see the M3C2 table in the milestone document): resident GPU bytes fell 57% (12,101,888 → 5,206,128 peak) and quads likewise, but total upload bytes rose 74% (39,053,296 → 67,864,768) and CPU mesh time on the path rose 41% (274,069 → 386,583 µs) because band crossings swapped 618 chunk levels and issued 1,262 mesh jobs against 473. Both profiles held 60 FPS under vsync; render-submit timing was inconclusive there. By the recorded rule LOD is not the default. The M3B and M3C profiles are selectable with `VELDWAKE_PROFILE`.

The M3C2 follow-up (transition model) re-measured the same path on the same day: presentation gaps 909 → 0 for `m3c-banded`, with 648 level swaps, 1,388 mesh jobs, 548 uploads / 76,214,896 bytes (baseline 167 / 41,275,744), largest transition group 78 chunks, CPU mesh time on the path 380,377 µs against a same-day baseline of 164,698 µs (baseline totals varied 164,698–274,069 µs across runs), time to idle 13.3 s for both, render-submit max 17.8 ms, no GPU validation error. The upload-bytes rule still fails, so LOD remains opt-in; the model buys correctness of transitions, not throughput.

**The banded peak is run-dependent.** Five driven `m3c-banded` runs of the identical path, three with the restage-order fix and two rebuilt from the audited `c877130`, measured a peak simultaneous chunk-mesh total between 9,734,816 and 9,800,128 against the stable 12,101,888 baseline, while the QA run recorded below measured 10,053,696. That is a saving between 16.9% and 19.6%. The peak depends on which chunks are staged when the worker and the upload budget line up on a time-based path, so quote the range or quote a number together with its run. Every one of those runs reported `restaged = 0`, which is why the restage-order fix left the figures unchanged.

**Chunk-mesh GPU bytes, corrected twice.** Counting only committed meshes produced the original −59% claim. Adding staged bytes but sampling only at update end produced −19%; that still missed old committed buffers coexisting with replacements staged and committed in the same update. The bridge now samples after each stage and group commit. Independent QA on the same driven path with debug off recorded baseline **12,101,888** versus banded **10,053,696** peak simultaneous presentation-owned chunk-mesh bytes, only **−16.9%**. Upload bytes were 41,275,744 versus 74,961,072 (+81.6%), mesh jobs 501 versus 1,394, and worker mesh time 117,779 versus 198,175 µs (+68.3%, timing is noisy). Both profiles remained vsync-bound at 60 FPS. `presentation_commit_failures`, `commit_invariant_failures`, and `committed_missing_max` were zero. This metric excludes depth textures, pipelines, fixed/reused debug resources, and driver/wgpu retention; do not call it global GPU memory.

**Debug views (M3C3).** Measured with an idle camera and no screen capture, banded profile, 12-second windows: `Off` and `Lod` hold 60.0 FPS at 16.66 ms and issue no debug line draw; `Boundaries` at 180 primitives runs 46.7–47.9 FPS (20.9–21.4 ms); `Residency` at 637 boxes runs 37.6 then 15.4 FPS (26.6 then 64.8 ms). One uniform buffer, one bind group, and one draw per primitive is the enabled cost (KI-012). `Off` is instrumented as zero per-frame debug primitive allocations, debug uniform writes, and debug draws, while leaving fixed startup resources and reusable slots resident.

## M4 golden slice

Audited Windows 11 host, Intel Iris Xe over D3D12, release builds, golden seed `0x5645_4c44_5741_4b45`. One-host observations, never targets.

**Generation and meshing, headless** (`terrain-probe bench 256`, first 256 chunks of the region):

| measure | value |
|---|---|
| generation per chunk: min / median / p95 / max / mean | 0.785 / 0.951 / 1.228 / 1.567 / 0.967 ms |
| meshing per chunk, standalone: median / p95 / max | 0.602 / 0.743 / 1.248 ms |

Standalone meshing walls off all six chunk faces because it has no neighbours, so its sizes are not the streamed sizes. The streamed figures are below.

**Client, `m4-golden` profile, pose `depth-stack`, 1600 x 900, `Fifo`, debug views off, settled:**

| measure | value |
|---|---|
| time to idle | 59,653 ms |
| tracked / known absent / presented | 3,211 / 2,767 / 351 |
| GPU-resident chunk meshes / quads | 204 / 358,619 |
| presentation-owned chunk-mesh bytes | 65,992,424 |
| CPU resident payload / CPU mesh bytes | 29,097,984 / 48,772,184 |
| snapshot build, total | 20,395 µs |
| worker mesh, total / max | 213,838 / 3,490 µs |
| frame interval / observed rate | 16.83 ms / 59.4 FPS |
| renderer render wall time, mean / max | 12,638 / 30,163 µs |

The frame interval is vsync-bound at 60 Hz and therefore measures presentation cadence, not renderer headroom. **Renderer render wall time wraps the complete `Renderer::render()` call, including surface acquisition, encoding, submission, and presentation; it is neither isolated CPU-submit time nor GPU time.** No GPU timestamps were taken and none of these numbers may be read as GPU cost. The settle time is dominated by job dispatch, not by generation: see KI-017.

**LOD band against real terrain**, same pose and settle:

| measure | `m4-golden` | `m4-golden-banded` |
|---|---|---|
| presented | 351 | 351 |
| GPU quads | 358,619 | 118,477 |
| chunk-mesh bytes | 65,992,424 | 21,806,552 |
| CPU mesh bytes | 48,772,184 | 16,112,872 |
| time to idle | 59,653 ms | 59,589 ms |

−67% settled chunk-mesh bytes at the same camera. This is settled committed bytes at one pose, not the mutation-boundary simultaneous peak KI-013 measures, and the two must not be compared as if they were the same quantity.

## M5 procedural character

Measured on the audited Windows 11 / Intel Iris Xe host, release profile, and stated as observations on that host rather than as budgets.

**Compiling a character** (release `character-probe bench`, golden fixture, 64 iterations): min `723` µs, median `813` µs, p95 `1,098` µs, max `1,367` µs, mean `847` µs. It happens once, at startup, for one character. There is deliberately no cache: the identity fingerprint a cache key would be built from exists anyway, because determinism needs it, but a key would cost more thought than a sub-millisecond compile saves.

**Memory.** Transient scratch peak `65,536` bytes — one reused `32³` grid for the whole compile, not one per part. CPU mesh payload `246,704` bytes. Skeleton `768` bytes, collision representation `460` bytes. GPU geometry `335,056` bytes: `290,240` of 40-byte vertices, `43,536` of `u32` indices, and `1,280` of part uniforms. Geometry is static and uploaded once.

**Per frame.** `1,280` dynamic bytes — one 80-byte uniform per part, a model matrix and four shading parameters — plus 16 world draws and 16 shadow draws. Posing one character costs `0.45` µs for the animation and the matrices and `2.25` µs with terrain contact and leg IK, so the contact solve is `1.80` µs of it, measured over 4,000 frames.

Those figures are also logged by the client at startup, and the probe prints the same ones, so the two can be compared rather than assumed. They disagreed once — the probe was accounting a 64-byte transform per part against the renderer's 80-byte uniform — and that is exactly why both print it.

**A/B against the same scene with no character.** Same world, camera pose (`character-in-scene`), weather and 100-second settle; `m4-golden`; no captures during the measured intervals except the single frame at the end. Eight five-second intervals each:

| | character off | character on |
|---|---|---|
| `renderer_render_wall_mean_us`, per interval | 11,767–12,150 | 11,155–11,914 |
| mean of those | **12,031** | **11,589** |
| `renderer_render_wall_max_us` | 14,404–17,233 | 15,689–21,310 |
| `observed_fps` | 59.99–60.02 | 60.01 |
| terrain `lod0_gpu_bytes` | 93,268,896 | 93,268,896 |

**The character's frame cost is below what this measurement can resolve.** The run with the character is `442` µs *faster* on the mean, which is the wrong sign for a cost and is therefore run-to-run variation rather than a saving. Both runs are vsync-bound at 60 FPS, and the render-wall interval wraps surface acquisition and presentation, so it moves with the compositor as much as with the work. What can be said without a measurement is the accounting: 32 extra draws and 1,280 dynamic bytes a frame against a terrain residency of 93 MB, and 335 KB of static geometry against it. Do not quote the negative difference as a benefit; quote the accounting.


## M3D disk cache experiment

Release `streaming-probe` on the audited Windows host, one identical settle of the default profile per phase, three repetitions. Residency is identical in every phase and encoding: 81 tracked, 63 resident, 4,128,768 resident bytes, 2,511,648 CPU mesh bytes, 81 load jobs, 0 stale results, 0 hard-cap blocks.

| phase | raw, time to idle (µs) | run-length, time to idle (µs) |
|---|---|---|
| cache off | 7,280 / 6,187 / 6,520 | 14,137 / 5,500 / 6,022 |
| cold | 106,706 / 82,666 / 83,308 | 112,203 / 62,944 / 75,151 |
| warm | 36,887 / 20,480 / 37,415 | 15,585 / 13,202 / 14,809 |
| warm again | 42,124 / 16,618 / 24,002 | 16,116 / 11,340 / 16,052 |

Warm is reproducible: 81 lookups, 63 present hits, 18 absence hits, 0 misses, 0 source fallbacks, 0 writes, stable footprint.

### Re-measured against real terrain in M4

Same probe, same default profile, but on `TerrainChunkSource` centred at `(0, 1, 0)`. One run per phase.

| phase | raw | run-length |
|---|---|---|
| cache off, time to idle | 60,493 µs | 67,866 µs |
| cold | 109,949 µs | 109,708 µs |
| warm | 18,953 µs | 10,670 µs |
| warm again | 16,655 µs | 11,424 µs |
| disk bytes, 81 entries | 4,132,656 | 107,904 |
| encode, mean / max | 92 / 279 µs | 15 / 108 µs |
| decode, mean / max | 76 / 182 µs | 14 / 48 µs |

Both M3D conclusions move once the source is not trivial. The cache is about six times faster than regeneration warm, where against the diagnostic fixture it was slower (KI-016 is now scoped to that source), and run-length became the default payload encoding: 97.4% less disk for the same content, encoding six times faster and decoding five times faster, with an unchanged and still-bounded worst case.

Disk footprint for the same 81 entries: raw 4,132,656 bytes (65,584 per present entry, 48 per absence), run-length 24,438 bytes, a factor of 169 on this fixture. Codec cost per chunk: raw encode 154 µs mean / 515 µs max and decode 156 / 293; run-length encode 22 / 200 and decode 13 / 67.

**The cache is slower than no cache on this fixture.** The original three-run set measured warm at 13–37 ms against 6–7 ms cacheless. The final QA run measured raw warm at 15–17 ms against 5 ms cacheless and RLE warm at about 8 ms against 4 ms cacheless. The diagnostic source is cheaper than disk plus decode. These numbers measure mechanism cost, not benefit; re-measure against genuinely expensive generation before enabling a cache by default (KI-016).

Independent QA first produced two much slower samples and then found the cause: the bounded reader reserved the 196,657-byte maximum for every file, including 48-byte absence entries and tiny RLE entries. Retaining the hard `take(MAX_ENTRY_BYTES + 1)` limit but allowing the vector to grow from the actual input removed that avoidable allocation. The final post-fix raw off/cold/warm/warm-again sample was 5,437/50,624/17,119/14,684 µs; RLE was 4,226/45,733/8,411/8,518 µs. Logical counts and footprints were unchanged. Single-run wall-clock values remain noisy, and even the corrected sample shows no benefit over regeneration.

Thread boundary: when opted in, `ChunkCache::open` synchronously sweeps temporary files and walks the footprint once during client event-loop initialization. It is cold setup, not frame-hot-path work, but it can grow with the unbounded cache and is part of KI-015. Per-chunk reads, codec work, fallback, and publication run on the single streaming worker. No directory scan occurs per frame. An individual read is capped at 196,657 bytes (one beyond the largest valid current entry), so a hostile file cannot turn one bounded worker job into an arbitrary allocation.

## M6 combat slice, measured on the audited host

Headless, from `combat-probe bench`:

| measure | value |
|---|---|
| combat tick, mean over 12,000 ticks | `5.463` µs |
| a second of simulation at `120` Hz | `0.656` ms of one core |
| ground queries per tick | `14.96` |
| one hit query at 16 substeps | `0.1447` µs |
| worst sweep substeps observed in a fight | 4 of a bounded 16 |
| weapon compile, median of 64 runs | `85.3` µs |

The bench's worst single tick is `134` µs — an order of magnitude above the mean and unrelated to the work. It is desktop scheduling; the mean is the figure. These were re-measured after branch QA, with a sweep that now interpolates both the blade and the moving body.

In the client, with an encounter running:

| measure | value |
|---|---|
| static GPU bytes for two actors and two weapons | 985,648 |
| dynamic upload per frame | 2,720 bytes |
| world draws / shadow draws | 34 / 34 |
| effect instances at one hit | 12 chips, 384 bytes, one draw |
| effect instances with nothing in flight | 16 readout pips, 512 bytes, one draw |
| particle high-water in a full fight | 12 of a fixed pool of 96 |
| audio callbacks over a 37.5 s fight | 10,412, rendering 4,998,336 frames |
| audio buffer high-water | 1,056 frames (`22.0` ms at 48 kHz) |
| audio voice high-water | 1 of 8; zero displaced |
| audio peak sample | `0.2323` against a `0.92` limit |

`renderer_render_wall` measured a mean of `12,091` µs with the encounter off and `10,697` µs with it on, over identical 75-second settles. **The run with more work in it measured faster.** That is noise: the figure is dominated by present and vsync, it is wall time around the render call and not GPU time, and it is not a measure of what combat costs. What combat costs is the `0.656` ms per second above.


## M7 traversable region, measured on the audited host

Release, Intel Core i5-1335U / Intel Iris Xe / D3D12, `m4-golden` profile, golden seed. One-host observations, never targets.

### The reachability audit, headless

| measure | value |
|---|---|
| grid | 800 x 800 = 640,000 columns |
| sample the whole region | `590` ms |
| breadth-first audit, forward and reverse, with barrier attribution | `273` ms |
| memory, allocated once at final size | heights `2.56` MB, water `0.64` MB, distance and predecessor `5.12` MB, symmetric labels `0.64` MB, queue up to `2.56` MB |
| adversary placement search | about `14,000` ms — 18,303 columns passed the cheap filters, 1,312 were examined for vegetation |

The audit is cheap enough to run in the ordinary test set. The placement search is not, so it is `#[ignore]`d and the binary reads the locked result. **No test asserts any of these durations**; they are the observation the placement decision was made from.

### Streaming under a walking body

A `239`-unit walk at `3.40` u/s over seventy seconds, no captures taken during it.

| measure | value |
|---|---|
| loads dispatched per second | 25–42 |
| the rail's ceiling (one job per `poll`, one `poll` per frame at 60 Hz) | about 60 |
| `gaps_closed`, `gap_frames_total`, `gap_max_simultaneous` | 0, 0, 0 |
| `ready_undrawn_max`, `ready_awaiting_upload_max`, `ready_blocked_transition_max` | 0, 0, 0 |
| `committed_missing_max` | 0 |
| `frontier_pipeline_pending` while walking | 0–20 |
| `stale_loads`, `stale_meshes`, `hard_cap_blocks`, `upload_failures` | 0, 0, 0, 0 |
| `presentation_commit_failures`, `commit_invariant_failures` | 0, 0 |
| `anchor_rejections` | 0 |
| `eviction_budget_hits` over the walk | 53 |
| total upload bytes over the walk | `164,575,552` |

**The one-job-per-poll rail keeps up with a walking player with about a third of its capacity spare, and produced no coverage failure.** That is the answer KI-017 was asked for at this speed, and nothing was changed. A run mode would roughly double the demand rate and would need this measured again.

### Renderer scale while walking

| measure | value |
|---|---|
| chunk meshes resident and drawn | 291–294 |
| GPU quads | 471,874–475,660 |
| presentation-owned chunk-mesh bytes | 84.9–88.6 MB, peak `93,268,896` |
| world draws | 291–294 chunk draws, plus 34 actor and weapon parts, plus one instanced effect draw |
| `renderer_render_wall`, mean / max | `10,649`–`10,967` µs / `13,902`–`18,018` µs |
| frame interval / observed rate | `16.666` ms / `60.00` FPS |

M4's settled `depth-stack` pose recorded 204 meshes, 358,619 quads and 65,992,424 bytes at the same profile. A body at eye level in the meadow draws about **44% more chunks and 32% more quads**, because a low camera sees more of the demand cube as non-empty. It remains vsync-bound at 60 FPS. **No frustum culling, batching or instancing work was done, because no bottleneck appeared.**

### The domain running for a whole session

Unchanged from M6: `5.463` µs per tick, `0.656` ms of one core per simulated second, about `10.9` µs per 60 Hz frame. Over the seventy-second walk the clock reported one capped frame and one dropped tick in total.

### Branch QA, re-measured on the same host

Independent runs, 2026-09-21, release, `m4-golden`, golden seed, `1920 x 991` client area. Observations on one host, never targets, and deliberately not forced to match the figures above.

**The audit, again.** `800 x 800 = 640,000` columns sampled in `390` ms and audited in `160` ms, against the `590` ms and `273` ms recorded above. Same machine, same build profile, a different run: the durations move and the *answers* do not. Every reachability number, the route, and `GOLDEN_ROUTE_SIGNATURE = 0x08c10aea5280b90f` came back identical. This is why no test asserts a duration.

**Startup coverage.** `M3C streaming reached idle coverage time_to_idle_ms = 63,032` with `507` frames presented. The seventy-five-second settle every driven session uses is sized from this and has about twelve seconds of margin.

**A whole tour, vsync-bound.** Sixty-three five-second frame reports across a session that walked to the river, was held at the waterline, walked back, climbed the route and approached the adversary: mean frame interval `16.668` ms, minimum `16.658`, median `16.666`, maximum `16.778`, i.e. `60.00` FPS throughout with no report mean straying more than `0.11` ms. At the end of it: `297` GPU-resident chunk meshes, `477,338` quads, `87,839,696` bytes of chunk geometry, `2,197` LOD0 chunks desired and `0` LOD1.

**The discontinuity the rail is not sized for.** An encounter reset moves the streaming anchor `164.5` units in one tick.

| after the reset | `load_queued` | `loading` | `mesh_waiting` | `known_absent` | `render` |
|---|---|---|---|---|---|
| `+1.4` s | 1,450 | 1 | 859 | 1,356 | 2,197 |
| `+10.2` s | 971 | 1 | 427 | 1,703 | 2,197 |
| `+20.1` s | 429 | 1 | 96 | 2,171 | 2,197 |
| `+25.7` s | 141 | 1 | 24 | 2,437 | 2,197 |
| `+30.1` s | 0 | 0 | 0 | 2,579 | 2,197 |

Drained in `30.2` s, sampled at 1 Hz against a report published every five, so the true figure is somewhere in `(25.7, 30.1]` s. `render` never moved: the resident set was not lost, the far field was missing. KI-030. **Nothing was changed in response** — the rail's answer to KI-017 for a *walking* body, measured above, is unaffected by this, and prefetching around a teleport is a milestone's decision.

## Combat initiative, measured on the audited host

On `feat/combat-initiative-spacing`, not merged. Release builds, `combat-probe bench 24000`, alternating a `main` binary and the branch binary four rounds each so a slow moment of the host lands on both:

| measure | `main` | branch |
|---|---|---|
| historical encounter tick, mean | `6.35`–`7.46` µs | `6.90`–`7.52` µs |
| combat initiative fight tick, mean (read policy) | — | `6.12`–`7.08` µs |

The ranges overlap and no regression is claimed or excluded below that noise. Both binaries measured slower on this day than M6's recorded `5.463` µs, which is the host, not the code.

The client, `initiative:read` against `armed`, alternating, `25` s each with no captures: `60.0` FPS and `16.66` ms average wall frame in both, vsync-bound, with `2` actors, `2` weapons, `985,648` GPU bytes, `34` world and `34` shadow draws and `2,720` dynamic bytes a frame in both — identical to M6, because the lunge is a pose and the pose is written into the same per-part transforms.

## M8 discoverable landmarks, measured on the audited host

Release unless stated, Intel Core i5-1335U / Intel Iris Xe / D3D12, `m4-golden` profile, golden seed. One-host observations, never targets.

### Building a world now derives a plan

| measure | value |
|---|---|
| `LandmarkPlan::derive`, release | `248`–`409` ms across two sessions |
| `LandmarkPlan::derive`, debug | `1.1`–`1.5` s |
| when it runs | once per `TerrainGenerator`, eagerly, before any chunk is generated |
| what it costs the suite | `veldwake-procedural` 60 s → 94 s, `veldwake-client` 114 s → 142 s, one process per test |

The two sessions disagree by about a third — `248`–`272` ms first, `316`–`409` ms when branch QA re-measured with the machine otherwise idle — and the wider range is the honest one. Nothing about the work changed between them; a single quiet sample is not a range. No test asserts any of this.

The first working derivation cost `3.2 s` in release. Five changes took it to `248` ms: a five-point pre-filter before the full level-and-dry disc, a cached water lookup, reservations that rebuild without discarding the terrain cache, solving the visible band in one pass instead of a nineteen-step binary search, and a straight-line dry test that prunes wet sites before any sight line is cast. The remaining cost is dominated by `level_and_dry` over the candidate lattice and by the sight lines themselves.

No in-process cache was added. `cargo nextest` runs one process per test, so a memo cannot help the suite; the reuse that helps is one process building the same world once, which `WorldSelection::build` and `TerrainGenerator::with_plan` make explicit.

### What the landmarks cost the world

| measure | before M8 | after M8 |
|---|---|---|
| standable columns in the region | 622,023 | 621,798 |
| steps refused for a traversal reason, whole-region audit | 2,464 | 2,840 |
| symmetric components | `[314861, 307144, 16, 2]` | `[314861, 306919, 16, 2]` |
| voxels written by the three landmarks | — | 1,740 (1,729 compiled, 11 of foundation) |

Nothing measurable changed in streaming or rendering. Branch QA measured a walking traversal rather than a still: `59.98` FPS at `16.67` ms average wall frame, `render = 2,197` demanded against `507` presented and `501,922` GPU quads, `cpu_evictions = 66`, and `gaps_closed`, `ready_undrawn_max`, `upload_failures`, `commit_invariant_failures` and `hard_cap_blocks` all zero, with `render_radius = 6` and `Lod0Only` unchanged.

A capture run at the overlook, `m4-golden`, twenty-six seconds after the window opened: `59.9` FPS at `16.69` ms average wall frame, `render = 2,197` demanded, `cpu_resident = 444`, `presented = 334`, `gpu_active = 193`, `gpu_quads = 343,812`, and `gaps_closed`, `upload_failures` and `commit_invariant_failures` all zero. The run had **not** reached idle coverage at that point — KI-017's minute to settle is unchanged — and the landmarks were drawn anyway, because at 62 and 63 units they are inside the first chunks to arrive. The three of them contribute 1,729 voxels to chunks that were already resident.
