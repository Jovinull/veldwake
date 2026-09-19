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
- Combat, which is integer-stepped end to end. `Encounter::step` advances exactly one tick; every duration in the domain is a tick count; authored seconds are compiled to ticks before they can reach the runtime. The client's accumulator is integer too — elapsed nanoseconds times the tick rate against `1_000_000_000`, keeping the sub-tick remainder — so the same twenty seconds delivered at 30, 60 and 144 Hz produce the same 2,399 ticks and the identical trace `0x0807fcad37689f9f`. The adversary draws from named FNV-1a streams with an explicit decision counter, never from a sequential global source.
- Presentation driven by combat, for the same reason: the camera impulse, the effect chips and the synthesised voices are all functions of integer tick counts and fixed tables, with no wall clock and no random source, so a capture fixture can be compared against another capture rather than only looked at.
- Save migration transforms and golden fixtures.
- Protocol identifiers and authoritative ordering where replay/reconciliation require it.

## Not promised today

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
