# Source extraction and provenance notes

Reviewed: 2026-09-16

The transcript contains an initial user request followed by two long assistant analyses and a second user research request. Assistant recommendations are not automatically owner decisions. This extraction applies the explicit bootstrap brief as the newest authoritative instruction and uses status labels in canonical documents.

## Classification

### Accepted

- Working title `Veldwake`, explicitly not legally cleared.
- Core concept: 3D voxel action RPG, exploration, persistent procedural/systemic world, observable consequences.
- Predominantly code/system-generated audiovisual production with high quality and art direction.
- Wonder, Mastery, and Consequence as the final design pillars.
- Rust stable/custom engine; initial `wgpu`/WGSL/`winit`/`glam` direction; no central ready-made engine.
- Strong separation of authority/simulation/generation from presentation; local-server-compatible single-player.
- Deterministic/version-aware generation where specified, versioned persistence, strict quality/memory protocol.

### Proposed

- Very large finite world, progression without rigid classes, specific traversal modes, aggregate economic fields, procedural history stages, compiler/cache shape, and capability roadmap.
- Rapier3D, `cpal`, `rayon`, `zstd`, Tracy, and development UI candidates.
- 1080p/60 performance intent, pending a target device/scene/budget.

### Exploratory

- Specific voxel scales, chunk/radius examples, creature morphology families, weapon physics influence, architectural grammars, synthesis techniques, musical algorithms, QUIC/`quinn`, WebAssembly/Wasmtime, meshlets/GPU-driven rendering, and exact world-history scope.
- Market/community claims and all cited review/sales/release counts.

### Rejected or non-goal

- A Cube World/Veloren/Minecraft clone.
- Unity, Unreal, routine Blender/manual art/audio pipeline, photorealism, “random plus random” generation, full microscopic simulation, or an initial MMORPG.
- Creating every conceptual crate and future dependency during bootstrap.

### TBD

License, name clearance, business/distribution model, target hardware/budgets, accessibility, combat specifics, finite world scale/topology, save compatibility horizon, construction scope, multiplayer product model, and mod policy.

## Resolved tensions

| Earlier statement | Later/authoritative resolution |
|---|---|
| Placeholder name “Project World” | Superseded by working title `Veldwake`; still not legal clearance. |
| Adventure/Mastery/Creation/World Memory as four proposed pillars | Superseded by the transcript's final Wonder/Mastery/Consequence triad; the earlier concepts remain mechanisms. |
| Modding “added early” | Architectural compatibility is early; implementation is explicitly deferred by the bootstrap brief. |
| Vast/infinite procedural framing | Current preference is a very large finite world for coherent global history; exact topology remains Proposed. |
| Long conceptual crate tree | It describes logical domains, not crates to create on day one. M0 has one crate. |
| Listed library/version claims | Libraries are candidates/initial direction; exact current versions and features are chosen at point of need. |
| “No assets” shorthand | Interpreted as no routine manual asset production; generated/cached runtime artifacts and justified narrow exceptions remain possible. |
| Determinism language | Strong where contracts need it (worldgen/migrations/cache identity), not universal bitwise equality across all hardware/render/audio. |

## Information intentionally not promoted to current fact

Time-sensitive product statistics, version numbers mentioned by the assistant, community anecdotes, Codex feature claims, and preliminary brand searches remain historical research leads. They require current primary-source verification before decisions or public claims.
