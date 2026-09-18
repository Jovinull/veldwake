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

M5 built the first one: `veldwake-character`'s `CharacterCompiler` takes a descriptor and returns a `CompiledCharacter` — geometry, material regions, a skeleton, a collision representation, gait parameters, and an identity. It is worth naming what that implementation did *not* need, because the list above implies more machinery than a first compiler requires.

It has no cache. A humanoid compiles in well under a millisecond and there is one of them, so a key would cost more thought than it saves; the identity fingerprint that a cache key would be built from exists anyway, because determinism needs it. It has no target profile, because there is one target. It has no intermediate representation and no primitive library: the body is boxes, placed by construction rules derived from proportions, and SDF or CSG would be machinery in search of a shape. What it does have is the part of the list that earns itself immediately — canonical descriptors, typed validation that rejects a body rather than clamping it silently, three separate versions folded into one fingerprint, and locked fixtures with previews.

The construction-rule idea is the one worth carrying forward. Three proportions are computed rather than declared — the waist from the hips, the leg gap from the torso, the torso height from the head — because rounding two independent fractions to even voxel widths collides at some body heights, and bounded seed variation must never be able to produce an invalid asset. A compiler that can reject its own seed is not bounded.

## Cache correctness

Conceptual key:

```text
hash(canonical descriptor + generator version + style version + target profile)
```

Generated cache entries are disposable. Canonical descriptors, generator source, version metadata, and tests are not. Never hand-edit generated artifacts.

## Visual regression fixtures

Plan stable fixtures such as `forest_seed_001`, `character_seed_001`, `sword_seed_001`, `creature_seed_001`, and `village_seed_001`, with controlled camera, light, seed, renderer settings, and tolerance. Perceptual diffs assist review; they do not replace art-direction judgment.

M5's character fixtures are the worked example, and they are locked by signature rather than by pixels: three descriptors (golden, sturdy, varied), eight named poses, and seven constants that the compiler is checked against on every test run. A pixel fixture would have been the wrong tool here for the reason KI-006 already records, and the signatures catch a changed voxel, a changed bone, or a changed gait without a GPU. The controlled cameras exist too — ten named poses with a stated intent each, asserted to stand above ground at both stand points — but they frame captures for a person to judge, not for a differ.
