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
cargo install --locked cargo-nextest
```

Record the actual installed version and warnings when changing build tools.

## Validate

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace
cargo nextest run --workspace --no-tests=pass
```

M0 deliberately has no artificial tests. Remove the no-tests allowance as soon as the first behavioral test lands.

There is no application to run in M0. If a command is not available, report it as BLOCKED rather than substituting an unrecorded tool.

## Optional tools

Do not preinstall the entire future tool list. Add `cargo-deny`, `cargo-audit`, coverage, benchmarks, profiling, fuzzing, CMake, or shader utilities only when the relevant configuration or subsystem exists. Prefer official releases and `cargo install --locked`; preserve provenance and warnings in the environment report.
