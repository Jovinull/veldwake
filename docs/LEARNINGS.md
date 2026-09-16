# Reusable learnings

Last updated: 2026-09-16

- On the audited Windows host, PowerShell did not initially expose Cargo because Rust was absent; rustup was installed with `--no-modify-path`. New shells may need `%USERPROFILE%\.cargo\bin` added explicitly for the session or user environment.
- Intel Iris Xe reports only 128 MiB dedicated video memory but about 8 GiB shared graphics memory. Do not treat WMI's `AdapterRAM` value as reliable VRAM capacity for an integrated GPU.
- D3D12 feature level 12_1 and Vulkan 1.4 are both available. This does not create a need for the Vulkan SDK: the runtime/driver is distinct from developer SDK tooling.
- Codex CLI `0.154.0` discovers repository skills under `.agents/skills`, not a guessed legacy project path. Repository instructions that should always apply belong in `AGENTS.md`; create a skill only for a reusable triggered workflow.
- The source transcript contains research claims tied to its date and several community links. Preserve them as leads; re-verify time-sensitive facts before making product or dependency decisions.
- `wgpu 30` surface presentation uses `CurrentSurfaceTexture` plus `Queue::present`; older tutorial code that calls `SurfaceTexture::present` does not match this API. Keep examples pinned to the selected version.
- A window-owned `Arc<Window>` permits a sound `'static` surface lifetime without transmute or other lifetime hacks. Window creation still belongs in `ApplicationHandler::resumed` through the active event loop.
- On the audited host, the intentionally D3D12-only M1 build selected Intel Iris Xe, `Bgra8UnormSrgb`, `Fifo`, and `Opaque`. These are observed capabilities, not portable requirements; deterministic selection helpers retain fallbacks.
- Zero-sized windows must suspend surface configuration/rendering, and focus loss must clear input state. Both are explicit lifecycle behavior rather than exceptional failures.
- `ControlFlow::Wait` alone does not prevent idle work when `RedrawRequested` unconditionally queues another redraw. The render result must distinguish retry from suspension, stop the redraw chain for zero-size/occluded surfaces, and explicitly request redraw on restoration.
- For the dense M2 `32³` reference chunk, `VoxelId(u16)` occupies exactly 65,536 logical payload bytes. The exposed-face checkerboard worst case would emit 98,304 quads and 393,216 vertices, so `u16` indices are insufficient even though ordinary fixtures are much smaller; the reference mesh therefore uses `u32` indices.
