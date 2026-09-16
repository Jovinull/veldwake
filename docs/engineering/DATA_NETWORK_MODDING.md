# Saves, networking, and modding

Status: **Requirements accepted; implementations deferred**.

## Persisted data

- Every durable format includes a magic/type identity where useful and an explicit schema version.
- Separate canonical world facts from disposable caches and presentation artifacts.
- Define corruption, atomicity, backup, validation, and migration behavior before player data is entrusted to a format.
- Generator/world/style versions accompany seed-addressed content.
- Unknown/new fields and forward/backward compatibility require an explicit policy; Serde alone is not a schema strategy.
- Compression (likely `zstd`) wraps a versioned format; it does not define it.

## Networking

- Server authority validates commands; clients submit intent and render views/snapshots.
- Protocol messages are versioned and bounded before accepting untrusted input.
- Transport is replaceable at the authority boundary. QUIC/`quinn` is only a candidate.
- Headless operation, interest management, prediction/reconciliation, and compatibility will be designed with measured gameplay needs.

## Modding

WebAssembly is the preferred future candidate because it can provide language-neutral sandboxed execution. A mod system must define:

- versioned host APIs and compatibility ranges;
- capability-based permissions rather than ambient filesystem/network access;
- CPU, memory, recursion, and execution/fuel limits;
- deterministic interfaces where authoritative state needs them;
- metadata, discovery, dependency ordering, conflicts, and signatures/trust policy;
- single-player/server ownership and client requirements;
- migration/removal behavior for persisted mod data.

Do not add Wasmtime or design a broad public API before a narrow real mod use case exists.
