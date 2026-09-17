# Rendering vision

Status: **Accepted direction; techniques and budgets selected by milestone evidence**.

The renderer should present stylized high-quality voxel fantasy while remaining a consumer of world/simulation state. It does not own canonical chunks, entities, or gameplay.

## Capability progression

1. Window/device/surface/frame/input/camera and adapter diagnostics. **Complete in M1** with a disposable code-generated cube and no game-content claim.
2. One chunk and an explicit meshing baseline. **Complete and merged in M2:** dependency-free exposed-face CPU mesh, one-time immutable `u32` GPU upload, depth/back-face culling, and diagnostic ID colors.
3. Multi-chunk correctness. **Complete and merged in M3A:** neighbor-aware local meshes and a static signed-coordinate fixture rendered with one presentation translation per chunk. This is not streaming or final batching.
4. Streaming, prioritized async generation/meshing, residency, and initial LOD. **Streaming, async meshing, and bounded residency are complete and merged in M3B:** camera-driven demand, one worker, stamp-validated results, per-frame draw-set reconciliation, and upload/release budgets. **Initial LOD is implemented in M3C behind a profile** (one coarse 2× level with a coarse-occupancy seam rule, atomic seam-coherent transitions, measured against a no-LOD baseline and kept opt-in by the recorded rule) and **streaming debug visualization is implemented in M3C3** (keyboard-toggled level tint, residency wireframes, and chunk-boundary/seam/transition overlays on a separate `LineList` pipeline, off by default).
5. Coherent sunlight, shadows, ambient response, sky/atmosphere/fog, water, vegetation, weather subset, and stable captures.
6. Later evidence-driven culling/batching/GPU-driven techniques.

Potential techniques include greedy meshing or a selected alternative, frustum culling, occlusion culling, asynchronous meshing, hierarchical terrain LOD, indirect rendering, meshlets, compute culling, GPU particles/vegetation, atmospheric scattering, fog, shadows, water, cloud shadows, and color grading. Listing a technique does not authorize implementation.

## Spatial detail concept

The source proposed concentric levels—near full simulation, farther high-detail world, lower-detail kilometers, distant terrain silhouettes, and global aggregate simulation. Exact radii (the transcript illustrated 200 m, 1 km, 8 km, and 30 km+) are exploratory and must derive from visibility, world scale, gameplay, memory, generation latency, and target hardware.

Terrain, props, and characters may use different voxel scales. The renderer must preserve silhouette and material identity across LOD transitions and expose chunk/mesh/upload/culling metrics.

## Quality and correctness

- Stable scenes capture camera, lighting, weather, seed, quality settings, backend, and renderer version.
- Debug views inspect normals, LOD, chunks, culling, overdraw, materials, and lighting.
- Visual regression assists review but tolerances account for GPU/backend variation.
- Shader/material complexity and atmosphere are budgeted; effects do not compensate for incoherent geometry or art rules.
