# M1 — Rendering Foundation

Status: **Implementation complete on `feat/m1-rendering-foundation`; pending remote PR validation and merge**
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
| `tracing-subscriber` | 0.3.23 | `fmt`, `env-filter`; defaults off | Concise output and `RUST_LOG` filtering without an unnecessary ANSI dependency. |
| `pollster` | 1.0.1 | none; defaults off | Blocks only during one-time GPU initialization without an async runtime. |
| `bytemuck` | 1.25.2 | `derive`; defaults off | Safe POD encoding for immutable vertices and camera uniforms under the no-unsafe policy. |

Primary records: [wgpu](https://crates.io/crates/wgpu/30.0.1), [winit](https://crates.io/crates/winit/0.30.13), [glam](https://crates.io/crates/glam/0.33.7), [tracing](https://crates.io/crates/tracing/0.1.44), [tracing-subscriber](https://crates.io/crates/tracing-subscriber/0.3.23), [pollster](https://crates.io/crates/pollster/1.0.1), and [bytemuck](https://crates.io/crates/bytemuck/1.25.2). `winit 0.31.0-beta.3` was rejected because M1 has no pre-release blocker.

## Dependency boundaries

`veldwake-foundation` stays dependency-free and unchanged. `veldwake-client` owns presentation only. Its pure camera/input modules do not import `winit` or `wgpu`; the application module translates platform events and supplies presentation delta. Renderer state is diagnostic/disposable and contains no authoritative world or gameplay state.

## Manual validation plan

On the audited Windows desktop: launch with `RUST_LOG=info`, confirm adapter/backend/config logs and cube/depth/perspective visually; move/rotate/ascend/descend; hold keys then change focus; repeatedly resize and minimize/restore; observe frame timing summary; close with Escape and window close; inspect output for uncaptured validation or surface errors. Record facts below after execution—never infer PASS from a successful build.

## Actual results

The workspace now has one `veldwake-client` executable with internal application, diagnostics, input, camera, and renderer modules. The unchanged `veldwake-foundation` crate remains dependency-free. No new ADR was required because this physical shape implements ADR-0001/0002 rather than changing their durable decisions.

The renderer creates a hidden window during `resumed`, initializes the compatible adapter/device/surface, configures only nonzero sizes, then displays the window. It renders a 24-vertex/36-index colored diagnostic cube from Rust constants through an embedded WGSL shader and `Depth32Float` depth target. Selection policy prefers an sRGB format, `Fifo` presentation, and `CompositeAlphaMode::Auto`; each uses the first advertised capability as fallback. The audited host's observed alpha result was `Opaque`, which is runtime evidence rather than the selection preference. Lost/outdated/suboptimal surfaces are reconfigured, timeout is logged, and out-of-memory, internal, validation, or device-loss errors cause an observable fatal exit.

The pure camera/input boundary supplies normalized six-axis movement, right-mouse yaw/pitch, pitch limits, finite perspective projection, aspect guards, focus reset, and a 100 ms presentation-delta clamp. Twelve headless tests cover this behavior plus key and surface-option translation. The plain nextest gate replaced the temporary M0 empty-suite exception everywhere current.

The final lifecycle hardening uses `ControlFlow::Wait` and chained `request_redraw()` only while rendering can make progress. A zero-sized or GPU-reported occluded surface returns a suspended outcome and stops the redraw chain; timeout remains a retry. Nonzero resize and de-occlusion explicitly request redraw, so restoration wakes the waiting loop without sleeps, timers, or helper threads. This is platform lifecycle behavior rather than artificial pure logic, so the existing 12 meaningful tests were preserved and host smoke testing supplies the relevant evidence.

### Windows host smoke test

On the audited Windows desktop, the client selected:

- adapter: `Intel(R) Iris(R) Xe Graphics` (integrated);
- backend: `Dx12`;
- driver: `32.0.101.7088`;
- surface: `Bgra8UnormSrgb`, `Fifo`, `Opaque`;
- initial physical surface size: 1600 × 900 on the observed run.

The code-generated cube was visually inspected with distinct front/top faces and correct perspective/depth. WASD movement and right-mouse look changed the view. The window was repeatedly resized (900 × 650, 1400 × 820, and 1000 × 700), minimized, and restored without a crash or validation error. During a held-key focus transition, the debug log recorded input reset; two subsequent controlled captures were byte-identical, confirming movement was not stuck. Escape requested a clean shutdown. Presentation telemetry near the display refresh rate was observed for this trivial scene but is not a benchmark or a 60 FPS claim.

After the lifecycle hardening, the same Windows host was exercised again with continuous camera movement, repeated resize, held-key focus loss, six seconds minimized, restore, resumed rendering, and Escape shutdown. While minimized, frame telemetry stopped and a lightweight process observation showed no continuing busy activity; after restoration, surface reconfiguration and continuous redraw resumed. This is qualitative lifecycle evidence, not a CPU benchmark.

### Dependency policy

`cargo-deny 0.20.2` checks the actual Windows dependency graph for RustSec advisories, an explicit encountered-license allowlist, denied wildcards/unknown sources, and duplicate visibility. It passes with warnings for two upstream duplicate pairs: `hashbrown` and `syn`. `cargo-audit 0.22.2` independently scans `Cargo.lock` and reports no known vulnerabilities. Workspace crates remain unpublished and are ignored by the dependency-license gate; this does not select a project license.

The selected feature tree contains `wgpu` `dx12`/`std`/`wgsl` and `winit` `rwh_06`; it does not enable Vulkan, Metal, GLES, or WebGPU backends.

### Final validation

| Gate | Result |
|---|---|
| `cargo fmt --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo build --workspace --all-features` | PASS |
| `cargo nextest run --workspace` | PASS — 12/12 |
| `cargo test --workspace --doc` | PASS |
| `cargo metadata --format-version 1 --no-deps` | PASS |
| `cargo deny check` | PASS with the documented duplicate warnings |
| `cargo audit` | PASS — no known vulnerabilities in 204 locked dependencies |
| Windows graphical smoke test | PASS — visual/lifecycle/input checks above; Escape exit code 0 |
