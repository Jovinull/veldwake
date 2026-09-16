# ADR-0001: Rust custom-engine foundation

- Status: Accepted
- Date: 2026-09-16
- Owners: Project owner
- Supersedes: None
- Superseded by: None

## Context

Veldwake requires a long-lived, high-performance, testable engine whose main content-production problem is code: voxel geometry, procedural generation, simulation, rendering, animation, DSP, tooling, and caches. The creator does not want Unity, Unreal, or an equivalent ready-made engine and wants the repository to be suitable for Codex/agent engineering.

## Decision

Use stable Rust 2024, pinned exactly per repository, for the custom engine foundation. Use libraries for solved infrastructure rather than reimplementing platforms. The initial rendering milestone will use `wgpu`/WGSL, `winit`, and `glam`; exact crate versions/features are selected at M1 point of need.

Do not introduce Unity, Unreal, Godot, Bevy, or an equivalent central engine without a superseding ADR.

## Alternatives considered

- C++: mature and capable, but greater routine memory-safety/build complexity for an agent-heavy project.
- C#: productive, but the intended low-level custom stack and no-engine constraint favor Rust.
- Zig: interesting but not selected as the primary foundation for the first version.
- Raw Vulkan/D3D12: too much platform/swapchain/device work unrelated to product identity; `wgpu` retains engine ownership above the API.

## Positive consequences

- Strong safety/default tooling, Cargo workspace, tests/lints, and low-level control.
- Portable GPU abstraction without surrendering renderer/world/engine design.
- Relevant ecosystem precedent for voxel/open-world work.

## Negative consequences

- Rust learning/compile-time cost and evolving graphics APIs.
- Custom engine scope remains substantial.
- `wgpu` abstraction may limit or delay backend-specific features.

## Future implications

Dependencies remain point-of-need decisions. Nightly is not the project default. M1 must verify current primary documentation, backend limits, adapter diagnostics, and a conservative dependency feature set.
