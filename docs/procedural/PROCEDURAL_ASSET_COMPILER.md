# Procedural asset compiler

Status: **Proposed architectural concept**.

Procedural assets should compile from semantic descriptors into optimized runtime artifacts:

```text
descriptor
  -> validate and canonicalize
  -> generator / grammar / SDF / CSG / voxel operations
  -> voxel volume or geometry
  -> material assignment
  -> mesh and collision/skeleton metadata
  -> optimization and LOD
  -> stable content hash
  -> cache / runtime package / preview
```

## Goals

- Separate authored rules from generated artifacts.
- Make outputs reproducible and inspectable.
- Avoid regenerating expensive geometry every frame or launch.
- Allow headless compilation and test fixtures.
- Preserve generator, descriptor-schema, style, and runtime format versions in cache keys.
- Produce previews that agents and humans can compare.

## Asset families

Potential compilers include vegetation, architecture, props, equipment, characters, and creatures. Character and creature descriptors may produce geometry, a compatible procedural skeleton, collider hints, animation parameters, gameplay tags, and sound descriptors. This does not imply a universal generator; morphology families such as humanoid, quadruped, avian, serpentine, insectoid, construct, and amorphous may require specialized grammars.

## Cache correctness

Conceptual key:

```text
hash(canonical descriptor + generator version + style version + target profile)
```

Generated cache entries are disposable. Canonical descriptors, generator source, version metadata, and tests are not. Never hand-edit generated artifacts.

## Visual regression fixtures

Plan stable fixtures such as `forest_seed_001`, `character_seed_001`, `sword_seed_001`, `creature_seed_001`, and `village_seed_001`, with controlled camera, light, seed, renderer settings, and tolerance. Perceptual diffs assist review; they do not replace art-direction judgment.
