# M1 — Rendering Foundation

Status: **In progress**  
Started: 2026-09-16

## Scope

Create one Windows-first runnable client package with cohesive internal platform, input, camera, diagnostics, and rendering modules. It opens a `winit 0.30` application, initializes a minimal D3D12 `wgpu` surface/device/queue, and renders a code-generated colored cube with depth buffering and a perspective camera. Startup logs report the actual selected adapter/backend/surface configuration.

## Acceptance criteria

- `ApplicationHandler`/`EventLoop::run_app` lifecycle with active-loop window creation and clean exit.
- Real surface, compatible adapter, minimum-feature device/queue, format/present/alpha selection, depth target, pipeline, and WGSL shader.
- Code-generated diagnostic cube visible with correct depth and perspective.
- GPU-independent camera/input state supports WASD, vertical movement, yaw/pitch, aspect updates, normalized diagonal movement, focus reset, and bounded presentation delta.
- Zero-size/minimize skips configuration/rendering; resize/scale change reconfigures safely.
- Surface lost/outdated reconfigures, timeout is observable, out-of-memory exits fatally.
- Meaningful headless tests run in CI; plain `cargo nextest run --workspace` must reject an accidentally empty suite.
- Manual Windows smoke test exercises movement, repeated resize, minimize/restore, focus loss, and clean close, with actual adapter/backend recorded.

## Non-goals

No voxel/chunk/terrain/worldgen, gameplay/controller, physics, ECS, assets/textures/models, audio, networking, saves, mods, editor/UI framework, generalized render graph, or job system.

## Dependencies selected from primary registry metadata

Verified against crates.io/Cargo metadata on 2026-09-16:

| Crate | Version | Features | Reason |
|---|---:|---|---|
| `wgpu` | 30.0.1 | `std`, `dx12`, `wgsl`; defaults off | Windows D3D12 rendering and WGSL only; no Metal/GLES/WebGPU/Vulkan build in M1. |
| `winit` | 0.30.13 | `rwh_06`; defaults off | Stable current application/window lifecycle; avoids Unix defaults in Windows-first M1. |
| `glam` | 0.33.7 | `std`; defaults off | GPU-independent vectors/matrices and camera math. |
| `tracing` | 0.1.44 | `std`; defaults off | Structured diagnostics without unused attribute macros. |
| `tracing-subscriber` | 0.3.23 | `fmt`, `env-filter`, `ansi`; defaults off | Concise output and `RUST_LOG` filtering. |
| `pollster` | 1.0.1 | none; defaults off | Blocks only during one-time GPU initialization without an async runtime. |
| `bytemuck` | 1.25.2 | `derive`; defaults off | Safe POD encoding for immutable vertices and camera uniforms under the no-unsafe policy. |

Primary records: [wgpu](https://crates.io/crates/wgpu/30.0.1), [winit](https://crates.io/crates/winit/0.30.13), [glam](https://crates.io/crates/glam/0.33.7), [tracing](https://crates.io/crates/tracing/0.1.44), [tracing-subscriber](https://crates.io/crates/tracing-subscriber/0.3.23), [pollster](https://crates.io/crates/pollster/1.0.1), and [bytemuck](https://crates.io/crates/bytemuck/1.25.2). `winit 0.31.0-beta.3` was rejected because M1 has no pre-release blocker.

## Dependency boundaries

`veldwake-foundation` stays dependency-free and unchanged. `veldwake-client` owns presentation only. Its pure camera/input modules do not import `winit` or `wgpu`; the application module translates platform events and supplies presentation delta. Renderer state is diagnostic/disposable and contains no authoritative world or gameplay state.

## Manual validation plan

On the audited Windows desktop: launch with `RUST_LOG=info`, confirm adapter/backend/config logs and cube/depth/perspective visually; move/rotate/ascend/descend; hold keys then change focus; repeatedly resize and minimize/restore; observe frame timing summary; close with Escape and window close; inspect output for uncaptured validation or surface errors. Record facts below after execution—never infer PASS from a successful build.

## Actual results

Pending implementation and validation.
