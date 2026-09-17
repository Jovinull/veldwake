# Art direction as a system

Status: **Accepted direction; the first checkable style specifics are in [`STYLE_BIBLE.md`](STYLE_BIBLE.md), authored for M4.**

This document states the direction. The style bible states the constraints it implies as numbers a test can check: shape language, detail frequency by distance, proportions, value and contrast bands, the base palette, material rules, cliff readability, foreground-to-background separation, sun position and colour, shadow behaviour, fog, sky, water, the two weather states, and the visual-noise limits that are rejection criteria in a screenshot review. It carries a style contract version that participates in the terrain generator fingerprint, so a rule that moves a voxel invalidates cached chunks.

The target is **stylized high-quality voxel fantasy**. Voxel structure remains visible and intentional. The target is not photorealism and not “Minecraft with a shader.” Beauty should come from coherent silhouettes, composition, palette, light, atmosphere, movement, material response, and controlled detail.

## Future STYLE_BIBLE

Before generators proliferate, establish a versioned style bible containing:

- shape language and silhouette rules;
- character/creature proportions and readable exaggeration;
- palette, contrast, value grouping, and biome/culture relationships;
- material response and weathering;
- spatial/detail frequency by viewing distance;
- lighting, atmosphere, fog, shadow, water, weather, and color-grading rules;
- animation timing, weight, pose language, and exaggeration;
- cultural visual grammar for architecture, clothing, tools, and ornament.

The bible should define constraints and examples, not merely adjectives. Generators must expose meaningful art controls and reject pathological combinations.

## Geometry and materials

Candidate techniques include voxel primitives, multiple voxel resolutions, SDFs, CSG, shape grammars, vertex colors, world-space procedural materials, and generated meshes. Terrain may use coarser spatial units than characters, props, or equipment. Exact scales are TBD and must be tested for readability, memory, editing, and meshing cost.

Traditional painted textures should be uncommon, not dogmatically impossible. Any exception must serve quality/accessibility/performance and remain producible without making manual asset work the routine bottleneck.

## Rendering direction

Future capabilities may include chunk streaming, meshing, LOD, frustum/occlusion culling, shadows, ambient occlusion, atmospheric scattering, fog, water, vegetation motion, weather, particles, cloud shadows, restrained bloom, and color grading. Capability order follows measurable visual value and target-hardware cost.

## Procedural animation and VFX

Skeletons and motion are derived from morphology families and parametric cycles, then improved with IK, terrain contacts, weapon constraints, balance, secondary motion, and gameplay-authored timing. VFX must clarify action and world state; random particles do not substitute for readable impact.
