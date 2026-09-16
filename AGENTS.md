# Veldwake repository constitution

Read this file, `docs/PROJECT_STATE.md`, `docs/INDEX.md`, the relevant subsystem documents, relevant ADRs, and `docs/agents/HANDOFF.md` before changing the repository.

## DOCUMENTATION IS PART OF THE IMPLEMENTATION

A task is not complete merely because code works. It is complete only when the next agent can understand the resulting behavior, decisions, limitations, evidence, and remaining work without rediscovering them.

Preserve reusable knowledge in the canonical document for its subject. Update documentation in the same change that makes it inaccurate. Historical source material and accepted/superseded ADRs are immutable records, explicitly marked as such. Do not create session diaries, duplicate canonical content, or comment every line.

Document material discoveries: architectural and design decisions; confirmed or refuted hypotheses; bugs and fixes; limitations and workarounds; dependency, API, format, protocol, configuration, or tooling changes; benchmarks and regressions; procedural-generation findings; deliberate debt; and unresolved problems. Do not assume the next agent will rediscover them.

## Before creating or changing a system

1. Search the repository for an equivalent implementation, document, TODO, issue reference, or dependency.
2. Read that subsystem's documentation and relevant ADRs.
3. Inspect existing dependency direction and tests.
4. Make the smallest coherent change that satisfies the current capability.
5. Do not add speculative dependencies, crates, abstractions, or extension points.

## Non-negotiable boundaries

- Simulation never depends on rendering.
- World generation never requires a GPU.
- Server and authoritative gameplay never depend on presentation.
- The renderer consumes state; it never defines authoritative game state.
- Single-player must remain compatible with `client <-> local authoritative server`.
- Persisted formats are explicitly versioned; world generation is seed-addressable and version-aware.
- Blocking I/O does not belong on render/game-frame hot paths.
- `unsafe` is forbidden by default. A necessary exception requires explicit safety invariants, `SAFETY` documentation, dedicated review, and an ADR when structural.

## Engineering conduct for agents

Never delete or weaken a failing test merely to obtain green; silence a lint without a documented reason; remove an inconvenient benchmark; change a requirement or architecture silently; add a giant abstraction without a demonstrated use; duplicate a system because discovery was skipped; add a dependency before checking existing solutions; edit generated artifacts when a canonical source exists; ignore a relevant error with `let _ =`; or hide failure behind a silent fallback or arbitrary default.

Runtime code must not scatter `unwrap()` or use `expect()` as error handling. Narrow exceptions are allowed in tests, proven invariants, and controlled bootstrap code when the reason is local and explicit.

## Definition of done

For every material change, assess and record as applicable:

- implementation, formatting, lints, build, and tests;
- regression/property/golden/visual tests appropriate to the risk;
- benchmark or profile evidence for performance claims;
- current documentation, ADRs, invariants, risk register, and known issues;
- `docs/PROJECT_STATE.md` and current `docs/agents/HANDOFF.md` when state changed;
- new commands, dependencies, format/protocol changes, migration, and breaking changes.

Run the applicable gates:

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --no-tests=pass
```

The `--no-tests=pass` exception exists only while M0 contains no behavioral code. Remove it from all local/CI instructions as soon as the first real test is added; an empty test suite must then fail.

If a tool is unavailable, report `BLOCKED` or `NOT YET APPLICABLE`; never claim `PASS`. See `docs/quality/DEFINITION_OF_DONE.md`.

## Scope guardrails

Veldwake is a working title. Do not change repository visibility, replace/configure remotes, publish releases, choose a project license, or claim trademark clearance without owner approval. Do not introduce Unity, Unreal, Godot, Bevy, or an equivalent central game engine without a superseding ADR. Keep the dependency surface small and the repository free of routine manually authored art/audio requirements.
