# Development setup

## Supported bootstrap host

The repository currently validates on Windows 11 x86-64 with Visual Studio 2022 Build Tools, Windows SDK, and the pinned Rust MSVC toolchain. Other platforms are not rejected, but are unverified until a milestone adds support and CI.

## Prerequisites

1. Git.
2. On Windows, Visual Studio Build Tools with **Desktop development with C++** / MSVC x64 and a current Windows SDK.
3. [rustup](https://rustup.rs/) from the official Rust project.

Do not install Vulkan SDK merely for the Windows `wgpu` path. M1 is expected to use D3D12 through `wgpu`.

## Rust and PATH

The repository's `rust-toolchain.toml` installs/selects exact Rust `1.98.1` with `rustfmt` and Clippy. Ensure Cargo's bin directory is available in the current PowerShell session:

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
rustup show
```

Install nextest if missing:

```text
cargo install --locked cargo-nextest --version 0.9.144
```

CI and local setup intentionally use `cargo-nextest 0.9.144`, the current stable crates.io release verified on 2026-09-16. Record the actual installed version and warnings when changing build tools; update CI and this command together.

Install the dependency-policy tools at the same intentional versions as CI:

```text
cargo install --locked cargo-deny --version 0.20.2
cargo install --locked cargo-audit --version 0.22.2
```

## Validate

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features
cargo nextest run --workspace
cargo test --workspace --doc
cargo metadata --format-version 1 --no-deps
cargo deny check
cargo audit
```

M1 contains meaningful headless tests, so an empty nextest suite is a failure.

Run the Windows diagnostic renderer with:

```powershell
$env:RUST_LOG = "info"
cargo run -p veldwake-client
```

The client logs two aggregate `M3B streaming` lines every five seconds; there is no per-chunk logging. The headless streaming evidence comes from `cargo run --release -p veldwake-streaming --bin streaming-probe`.

If a command or graphical desktop is not available, report it as BLOCKED rather than substituting an unrecorded tool.

## Optional tools

Do not preinstall the entire future tool list. Add coverage, benchmarks, profiling, fuzzing, CMake, or shader utilities only when the relevant configuration or subsystem exists. Prefer official releases and `cargo install --locked`; preserve provenance and warnings in the environment report.
