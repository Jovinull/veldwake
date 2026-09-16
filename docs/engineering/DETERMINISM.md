# Determinism

Status: **Accepted scope principle; algorithms TBD**.

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
