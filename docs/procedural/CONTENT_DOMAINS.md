# Procedural content domains

Status: **Exploratory domain models under accepted procedural philosophy**.

This document preserves generator ideas from the source without prematurely defining schemas.

## Characters

A future character descriptor/genome may include species, body morphology, height/proportions, head shape, hair/beard, palette, age style, culture, equipment, and seed. A compiler can combine voxel/SDF/CSG primitives at a finer voxel scale than terrain, producing geometry, material regions, skeleton attachment points, collider hints, and LODs.

Skeletons are generated from morphology rather than one model file per character. Parametric cycles control stride, cadence, hip motion, arm swing, posture, and emotion; IK handles feet/terrain and hands/weapons. Generated variation must stay within silhouette/proportion/style constraints.

## Creatures

Creature descriptors may include morphology family, size, limb count, neck/head/tail structures, surface/material, element/affinity, behavior/gameplay role, and seed. Specialized family generators—not one universal randomizer—can emit geometry, skeleton, collider, locomotion parameters, gameplay tags/stats, and audio descriptors.

No creature is accepted merely because it compiles. Locomotion, hitboxes, attack telegraphs, encounter role, silhouette, animation contacts, and audio require validation. A six-legged ash creature is a controlled variant only if the family can animate and fight readably.

## Vegetation

L-systems or project-specific growth grammars may generate trunk and branch hierarchy, leaf clusters, roots, collision, wind response, and LOD. Species parameters include branch angle/frequency, trunk width, height, leaf density/shape, gravity bias, environment response, and seed. Biome placement and species form must reflect climate, soil/geology, competition, and art direction where useful.

## Settlements and architecture

Settlement generation is growth and constraint solving, not random house scattering. A culture grammar may define roof/window/wall style, local materials, street width, tower frequency, preferred shapes, decoration, palette, defenses, and public spaces. Terrain, water, resources, trade routes, danger, history, population, and function influence layout.

A mining town, trading capital, isolated village, and settlement built around ruins should differ for causal and cultural reasons. Growth/decline needs compatible stages so world simulation can add, repurpose, damage, and rebuild structures without visual incoherence.

## Items and equipment

Descriptors may include category, material, craftsmanship, culture, age/wear, geometry, maker, previous owner/history, and constrained enchantment. Geometry can inform reach, apparent mass, balance, blocking profile, animation, and acoustic resonance, but gameplay must remain tuneable and understandable.

Loot should produce semantic objects—such as a culturally consistent weapon linked to an actual historical event—not adjective/stat permutations. Infinite combinations are not valuable if choices are indistinguishable.

## Shared validation

All domains need canonical descriptors, explicit random streams, schema/generator/style versions, budget constraints, preview tools, rejection diagnostics, representative fixtures, cache keys, and reviewable outputs.
