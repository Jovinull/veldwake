# Technical vision

Status: **Mixed**; labels below distinguish accepted choices from candidates.

## Accepted foundation

- Rust 2024 on an exact pinned stable toolchain; nightly only for a specific documented tool.
- Custom engine architecture, not a ready-made game engine.
- `wgpu 30.0.1` + WGSL, `winit 0.30.13`, and `glam 0.33.7` for the initial renderer/platform milestone. M1 compiles only the native D3D12 backend and keeps default backend features disabled.
- Strong separation of authoritative simulation from presentation.
- Structured logging with `tracing` when runtime work begins.
- Versioned persisted formats and seed/version-aware generation.
- Tooling and procedural compilers separated from runtime presentation.

## Candidates to evaluate at point of need

- `rayon` plus a project-specific scheduler where dependency graphs/priorities require it;
- Rapier3D for initial physics rather than writing a rigid-body solver;
- `cpal` as low-level audio I/O with project-owned DSP/mixing;
- `zstd` for region/save/cache compression;
- QUIC via `quinn` for future transport;
- WebAssembly/Wasmtime components for future modding;
- Tracy and GPU timestamps for profiling;
- `egui` for development tooling only, not necessarily game UI.

Candidates are not promises and must not be added before an immediate capability, compatibility check, license/security review, and architectural fit.

## Performance intent

The historical concept proposes 1080p/60 FPS on “mid-range” hardware without mandatory upscaling. This is a **target hypothesis**, not an approved budget: target hardware, quality preset, scene, percentile, and frame-time/memory budgets remain open.

## Platform intent

Windows/D3D12 is the initial validated path. The M1 client deliberately compiles only D3D12; architectural boundaries keep later Vulkan/Metal support possible without treating unbuilt backends as supported. A Vulkan SDK is not required merely to use `wgpu` on Windows.
