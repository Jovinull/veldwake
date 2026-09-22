# Determinism

Status: **Accepted scope principle; worked implementations in M4 and M5**.

Since M4 there is a worked example of the rules below in `veldwake-procedural`: `WorldSeed::stream(StreamLabel)` derives one named child stream per generator stage from the seed and the generator version, so adding a draw to vegetation cannot perturb terrain; `WorldIdentity::fingerprint()` folds the seed, generator/style versions, every art control, and a locked behavioural signature into one cheap cache key; `region::GOLDEN_REGION_SIGNATURE` remains a compact nineteen-coordinate fixture, while the test-only `GOLDEN_WORLD_BEHAVIOR_SIGNATURE` exhaustively hashes all 1,875 chunks of the canonical golden world and is what couples that world's output to the cache key. It is not a universal multi-seed proof: `TERRAIN_GENERATOR_VERSION` remains the explicit contract bump for any general algorithm change, including one that does not change golden output. Nothing in that crate consumes a sequential generator or observes iteration order: every spatial decision is a hash of a world position.

M5 applies the same rules to a second domain, and repeating them was a decision rather than an oversight. `veldwake-character` carries its own sixty-four-bit FNV-1a and SplitMix64 rather than depending on `veldwake-procedural` for them: a character is not addressed by world position, so making a headless character crate compile a world generator to borrow two hash helpers would buy code reuse at the price of the dependency direction in [`ARCHITECTURE.md`](ARCHITECTURE.md). The duplication is thirty lines, it is pinned against published FNV-1a vectors and two SplitMix64 outputs, and it is documented at the point of duplication.

A character has three separate versions, because three different things can change independently: `CHARACTER_SCHEMA_VERSION` for the descriptor's shape, `CHARACTER_COMPILER_VERSION` for how a descriptor becomes voxels, and `CHARACTER_STYLE_VERSION` for the art rules in [`CHARACTER_STYLE.md`](../audiovisual/CHARACTER_STYLE.md). All three fold into `CharacterIdentity`, and therefore into a compiled character's fingerprint. They are deliberately **not** part of any chunk cache key: a character is not world content, and changing a proportion must not invalidate terrain.

Below the identity fingerprint sit three structural ones — geometry, skeleton and collision — plus a behavioural signature that hashes named poses through the whole locomotion and contact path. That split is what makes a failure legible: a repaint changes the identity and leaves the geometry fingerprint alone, a proportion change moves geometry and skeleton, and a gait change moves only the behavioural signature. The locked fixture constants in `fixture.rs` are checked against the compiler on every test run, so any of those changing without a version bump is a failing test rather than a surprise in a capture.

Determinism is a compatibility contract, not a blanket claim that every floating-point operation is bit-identical on every platform.

## Strong candidates

- Canonical seed derivation and random-stream partitioning.
- World-generation results covered by `WORLD-001` within declared compatible versions.
- Descriptor canonicalization and procedural asset cache keys, including character descriptors: a compiled character is a pure function of its descriptor, and a posed character is a pure function of a compiled character, a runtime state, and a ground query.
- Combat, which is integer-stepped end to end. `Encounter::step` advances exactly one tick; every duration in the domain is a tick count; authored seconds are compiled to ticks before they can reach the runtime. The client's accumulator is integer too — elapsed nanoseconds times the tick rate against `1_000_000_000`, keeping the sub-tick remainder — so the same twenty seconds delivered at 30, 60 and 144 Hz produce the same 2,400 ticks and the identical trace `0xff3d7fb060b64aa5`, with no capped frame and no dropped tick. (The earlier `2,399` and `0x0807fcad37689f9f` were a probe that truncated each frame duration and therefore delivered `19.99` seconds; M6 branch QA corrected it and this document was reconciled in M7 against a fresh `combat-probe partition` run rather than against memory.) The adversary draws from named FNV-1a streams with an explicit decision counter, never from a sequential global source.
- Presentation driven by combat, for the same reason: the camera impulse, the effect chips and the synthesised voices are all functions of integer tick counts and fixed tables, with no wall clock and no random source, so a capture fixture can be compared against another capture rather than only looked at.
- Traversal, since M7, for the same reason and with its own evidence: the same exact elapsed time and the same input trace, cut into 50, 60, 144 and 240 frames a second, leave the player's position, facing, gait phase, smoothed height, health and action, the adversary's position and health, and the brain's state and decision count **bit-identical**. The reachability audit, the named route, its signature and the adversary's placement are pure functions of the world identity, the compiled movement spec and the traversal rule version. The tick an adversary wakes on is a function of the input trace.
- Landmarks, since M8, at every level: the plan is a pure function of the world identity — two derivations of the same identity give the same fingerprint, sites, base courses and compiled geometry — and it is derived once, when the world is built, rather than during generation, so no chunk can observe a half-built composition. Placement draws from two named streams, `LandmarkPlacement` and `LandmarkShape`, and every search iterates a fixed lattice in a fixed order with ties broken by coordinate. Chunk output is order-independent: branch QA generated the chunks a landmark spans under six permutations, one per fresh world in isolation, and through a clone, and compared fingerprints — identical every time. The composition is locked by `LANDMARK_BEHAVIOR_SIGNATURE`, which is the plan's own fingerprint and is folded into the world fingerprint, so a landmark that moves invalidates every cached chunk.
- Save migration transforms and golden fixtures.
- Protocol identifiers and authoritative ordering where replay/reconciliation require it.

## Not promised today

- **The frame-by-frame streaming anchor sequence.** The anchor is sampled once per rendered frame, so *when* each demand centre is visited is a function of how the frames fell, not of the simulation. What is deterministic is the body's position at each tick; the chunk-centre path follows from it, and nothing is locked as a signature over raw anchor samples.
- **That an exact partition never hits the catch-up cap.** A thirtieth of a second is exactly `MAX_TICKS_PER_FRAME` ticks, so at thirty hertz the clock's remainder can reach a fifth tick and the cap discards it — and **whether it does depends on how the spare nanoseconds are spread across the frames**. `combat-probe partition` front-loads them and measures zero capped frames; spreading them through the run does not. Both deliver exactly twenty seconds. The ticks that do run are the same walk either way, which `a_frame_at_the_catch_up_cap_can_lose_a_tick_to_the_remainder` asserts; the simulation slows rather than changing, which is the failure M6 chose.
- Bitwise-identical renderer output across GPU vendors/drivers.
- Universal bitwise physics equivalence across all CPUs/platforms.
- Identical audio sample output across every backend. The synth itself is deterministic — the same requests give the same samples, and a test asserts it — but the device's sample rate, channel count and buffer size are the host's, and a different rate renders different samples.
- A permanently unchanged world when the explicit world/generator version changes.

## Design requirements

- Never use process-global implicit randomness in authoritative generation.
- Derive named child streams from stable domain labels/coordinates/IDs so adding one random draw does not unnecessarily perturb unrelated systems.
- Specify ordering before hashing/serializing unordered collections.
- Persist world and generator versions with the seed.
- Document floating-point tolerance or quantization where equivalence is not exact.
- Golden seeds test intended contracts, not every byte of an incidental implementation.

Changing a deterministic contract requires compatibility analysis, fixture updates with review evidence, and migration/regeneration policy.
