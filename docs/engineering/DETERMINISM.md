# Determinism

Status: **Accepted scope principle; first worked implementation in M4**.

Since M4 there is a worked example of the rules below in `veldwake-procedural`: `WorldSeed::stream(StreamLabel)` derives one named child stream per generator stage from the seed and the generator version, so adding a draw to vegetation cannot perturb terrain; `WorldIdentity::fingerprint()` folds the seed, generator/style versions, every art control, and a locked behavioural signature into one cheap cache key; `region::GOLDEN_REGION_SIGNATURE` remains a compact nineteen-coordinate fixture, while the test-only `GOLDEN_WORLD_BEHAVIOR_SIGNATURE` exhaustively hashes all 1,875 golden-region chunks and is what couples output behaviour to that cache key. Nothing in that crate consumes a sequential generator or observes iteration order: every spatial decision is a hash of a world position.

Determinism is a compatibility contract, not a blanket claim that every floating-point operation is bit-identical on every platform.

## Strong candidates

- Canonical seed derivation and random-stream partitioning.
- World-generation results covered by `WORLD-001` within declared compatible versions.
- Descriptor canonicalization and procedural asset cache keys.
- Save migration transforms and golden fixtures.
- Protocol identifiers and authoritative ordering where replay/reconciliation require it.

## Not promised today

- Bitwise-identical renderer output across GPU vendors/drivers.
- Universal bitwise physics equivalence across all CPUs/platforms.
- Identical audio sample output across every backend.
- A permanently unchanged world when the explicit world/generator version changes.

## Design requirements

- Never use process-global implicit randomness in authoritative generation.
- Derive named child streams from stable domain labels/coordinates/IDs so adding one random draw does not unnecessarily perturb unrelated systems.
- Specify ordering before hashing/serializing unordered collections.
- Persist world and generator versions with the seed.
- Document floating-point tolerance or quantization where equivalence is not exact.
- Golden seeds test intended contracts, not every byte of an incidental implementation.

Changing a deterministic contract requires compatibility analysis, fixture updates with review evidence, and migration/regeneration policy.
