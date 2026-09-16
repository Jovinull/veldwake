# Project state

Last updated: 2026-09-16

## Stage

**M2 — Voxel Prototype is active in planning** on `feat/m2-voxel-prototype`. M1 was merged into `main` through [PR #1](https://github.com/Jovinull/veldwake/pull/1) at merge commit `ef653238be903daa6a1cbf74a9e31e3465cf8b57`. The repository has a small, observable Windows/D3D12 diagnostic renderer; it remains a non-playable engineering proof.

## What works

- Source provenance and SHA-256 are recorded.
- Repository constitution, ADR process, memory protocol, setup guide, and CI are defined.
- Rust stable `1.98.1` with `rustfmt` and Clippy is available on the audited Windows host.
- A dependency-free Rust 2024 foundation crate compiles and validates the workspace.
- `veldwake-client` uses current `winit 0.30` lifecycle APIs and `wgpu 30` to render a code-generated colored cube with depth and a perspective camera.
- Input and camera behavior are GPU-independent and covered by 12 headless tests; focus loss clears held input and presentation delta is bounded.
- Startup diagnostics report the actual adapter/backend/surface configuration; lightweight presentation timing is reported every five seconds at `info`.
- `cargo-deny` and `cargo-audit` are part of the dependency gates now that runtime dependencies exist.

## What does not exist yet

No voxel/chunk model, world generation, authoritative simulation, gameplay, audio, networking, save format, mod runtime, UI framework, or internal editor exists. The diagnostic client is not a game.

## Current decisions

- Working title: Veldwake; no legal clearance claimed.
- Custom engine in Rust; no Unity, Unreal, Godot, Bevy, or equivalent central engine.
- Planned renderer foundation: `wgpu` + WGSL + `winit` + `glam`; D3D12 is the expected initial Windows backend.
- Authoritative gameplay is presentation-independent; single-player follows a local-server-compatible boundary.
- Procedural-first audiovisual production is a defining constraint, governed by style rules and compiled caches.
- Rust versions are pinned; dependencies enter only with an immediate capability and justification.

See accepted decisions in [`adr/`](adr/README.md).

## Build, test, run

From a shell where Cargo is on `PATH`:

```text
cargo build --workspace
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace
cargo test --workspace --doc
cargo deny check
cargo audit
cargo run -p veldwake-client
```

The runnable binary is diagnostic presentation content only. See [`environment/SETUP.md`](environment/SETUP.md).

## Tools on the audited host

Git, Git LFS, GitHub CLI, Visual Studio 2022 Build Tools/MSVC, Windows SDK, LLVM, Ninja, Python, Codex CLI, rustup, Cargo, rustc, rustfmt, Clippy, cargo-nextest, cargo-deny, and cargo-audit. See the environment report for versions and tools intentionally deferred.

## Known problems and blocks

- No project license has been chosen.
- The working title lacks formal trademark/domain/store clearance.
- Target hardware tiers and memory/frame budgets are not yet approved.
- Visual style bible and accessibility baseline remain to be authored during the relevant milestones.
- The public repository is `https://github.com/Jovinull/veldwake`; changing visibility, remotes, releases, or other publication policy requires owner authorization.

## Active milestone

**M2 — Voxel Prototype:** define and validate one GPU-independent CPU voxel/chunk representation, one deterministic fixture, checked read/write behavior, a measured baseline mesher, and the minimum upload/render integration for that single chunk. See [`planning/M2_VOXEL_PROTOTYPE.md`](planning/M2_VOXEL_PROTOTYPE.md). No voxel implementation exists yet; streaming, world generation, LOD, persistence, ECS, gameplay, and multiple chunks remain excluded.
