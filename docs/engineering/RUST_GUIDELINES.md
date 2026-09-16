# Rust guidelines

Status: **Accepted baseline**.

## Toolchain and dependencies

- Use the exact stable toolchain in `rust-toolchain.toml` and Rust 2024 edition.
- Nightly requires a named tool, narrow invocation, and documented rationale; never switch the whole project casually.
- Add a dependency only for an immediate capability after checking existing code, features, maintenance, license, security, native requirements, and alternatives.
- Prefer explicit features and minimal default features where practical. Keep versions in the workspace when multiple crates share them.

## Errors

- Return typed or contextual errors at recoverable boundaries; preserve causes.
- Do not scatter `unwrap()` in runtime code or use `expect()` as error handling.
- `unwrap`/`expect` may appear in tests, proven invariants, or controlled bootstrap code when the proof/reason is local and clear.
- Do not ignore relevant failures with `let _ =`, empty matches, arbitrary defaults, or silent fallback.
- User-facing recovery and degraded modes must be observable through diagnostics.

## Safety

`unsafe` is forbidden by workspace lint and crate attributes by default. If demonstrably required:

1. record why safe Rust cannot meet the requirement and measure the need;
2. isolate the smallest surface;
3. write explicit invariants and a `SAFETY:` justification at each block;
4. add targeted tests/fuzzing/Miri where applicable;
5. obtain dedicated review and create an ADR for structural use.

## API and data design

- Make invalid states difficult to represent where it improves correctness without abstraction inflation.
- Use explicit units/types for world coordinates, time, identifiers, versions, and seeds when ambiguity becomes real.
- Authoritative formats and protocols do not expose incidental in-memory layout.
- Avoid global mutable state and cross-subsystem singletons.
- Optimize storage/layout after profiling; do not make every type generic for hypothetical reuse.

## Hot paths

Document hot paths and budgets. Avoid per-frame allocations, global locks, blocking I/O, excessive indirection, and unbounded work there when measurement supports the concern. Prefer batching and coherent data access; keep correctness/reference implementations where useful.

## Required gates

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace
cargo deny check
cargo audit
```

An empty nextest suite is a failure. Dependency policy and RustSec audit gates are active now that runtime dependencies exist. Coverage, fuzzing, and benchmarks are added when their evidence becomes meaningful.
