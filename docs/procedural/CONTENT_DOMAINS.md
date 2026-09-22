# Procedural content domains

Status: **Exploratory domain models under accepted procedural philosophy; the character domain has a first worked implementation in M5, and the architecture domain has a first, deliberately tiny one in M8**.

This document preserves generator ideas from the source without prematurely defining schemas.

## Characters

A future character descriptor/genome may include species, body morphology, height/proportions, head shape, hair/beard, palette, age style, culture, equipment, and seed. A compiler can combine voxel/SDF/CSG primitives at a finer voxel scale than terrain, producing geometry, material regions, skeleton attachment points, collider hints, and LODs.

Skeletons are generated from morphology rather than one model file per character. Parametric cycles control stride, cadence, hip motion, arm swing, posture, and emotion; IK handles feet/terrain and hands/weapons. Generated variation must stay within silhouette/proportion/style constraints.

M5 implemented the smallest honest version of that paragraph and nothing beyond it, so the parts that are now real can be separated from the parts that are still ideas.

**Real.** One humanoid archetype. A descriptor of fourteen proportion fractions, a palette choice, a seed and a bounded variation amount, canonicalized and typed-validated. A compiler emitting sixteen rigid voxel body parts at `1/12` of a world unit against terrain's `1`, with material regions from a ten-slot palette and its own `VoxelId` range. A sixteen-bone skeleton derived from the same proportions rather than authored. Analytical idle, walk and run cycles whose phase advances with distance and whose thresholds are leg-relative, with stride, cadence, hip pitch, knee flex, arm swing, pelvis bob and sway, and torso lean as parameters. Two-bone analytical leg IK against a ground query. A capsule-and-boxes collision representation. Variation bounded so a seed cannot leave the style bands.

**Not real, and deliberately not invented.** Species, head shape, hair or beard as geometry, age style, culture, equipment, emotion, hand IK, LODs, and any second morphology family. The generated skeleton has attachment points only in the sense that every bone is one; nothing attaches yet.

The one structural lesson is about scale. "A finer voxel scale than terrain" is not a parameter to pick once — it is a visual relationship that has to be judged in a frame containing both. The first ratio was arithmetically reasonable and produced a person shorter than the undergrowth, with a leg shorter than one terrain voxel.

## Creatures

Creature descriptors may include morphology family, size, limb count, neck/head/tail structures, surface/material, element/affinity, behavior/gameplay role, and seed. Specialized family generators—not one universal randomizer—can emit geometry, skeleton, collider, locomotion parameters, gameplay tags/stats, and audio descriptors.

No creature is accepted merely because it compiles. Locomotion, hitboxes, attack telegraphs, encounter role, silhouette, animation contacts, and audio require validation. A six-legged ash creature is a controlled variant only if the family can animate and fight readably.

## Vegetation

L-systems or project-specific growth grammars may generate trunk and branch hierarchy, leaf clusters, roots, collision, wind response, and LOD. Species parameters include branch angle/frequency, trunk width, height, leaf density/shape, gravity bias, environment response, and seed. Biome placement and species form must reflect climate, soil/geology, competition, and art direction where useful.

## Settlements and architecture

Settlement generation is growth and constraint solving, not random house scattering. A culture grammar may define roof/window/wall style, local materials, street width, tower frequency, preferred shapes, decoration, palette, defenses, and public spaces. Terrain, water, resources, trade routes, danger, history, population, and function influence layout.

A mining town, trading capital, isolated village, and settlement built around ruins should differ for causal and cultural reasons. Growth/decline needs compatible stages so world simulation can add, repurpose, damage, and rebuild structures without visual incoherence.

M8 implemented the smallest honest fragment of that paragraph and nothing beyond it.

**Real.** One family, the Monolith, with three silhouette classes — Spire, Gate, Broken — drawn from a seed into a canonical descriptor, validated against style bands, and compiled once into voxels, per-column occupancy and silhouette measurements that everything downstream reads. Terrain-scale voxels, a three-material palette in its own `VoxelId` range, and placement by the world's own rules: level, dry, far enough apart, visible from where the composition intends and hidden from where it does not. A vegetation reservation that removes the plants a structure would otherwise stand inside. Solid geometry the client turns into a movement veto, so a visible wall refuses a body and a gate's opening admits one.

**Not real, and deliberately not invented.** Culture, roof and window grammar, streets, settlement layout, growth and decline stages, function, history, interiors, decoration, inscriptions, and any second family. There is no causal reason these three stand where they stand beyond composition — no prior civilisation, no trade route, no ruin of anything. The owner's own reaction after playing M8 is the honest statement of where this domain is: simple, sufficient for discovery, and still close to the visual language of familiar voxel games (KI-036).

## Items and equipment

Descriptors may include category, material, craftsmanship, culture, age/wear, geometry, maker, previous owner/history, and constrained enchantment. Geometry can inform reach, apparent mass, balance, blocking profile, animation, and acoustic resonance, but gameplay must remain tuneable and understandable.

Loot should produce semantic objects—such as a culturally consistent weapon linked to an actual historical event—not adjective/stat permutations. Infinite combinations are not valuable if choices are indistinguishable.

### What M9 added to the items domain

M9 implemented the smallest fragment of the paragraph above and stopped there.

**Real.** A second accepted weapon, compiled from a descriptor by the same
compiler, in the same `VoxelId` range and the same material table, under a
palette this repository already declared and had never drawn. Geometry that
means something in play: a longer blade reaches further because the hit sweep
follows the blade the viewer can see, not because a number says so. A weapon
that exists as an object in the world — standing at a fixed place, drawn through
the same rigid-part path, taken by an explicit verb — rather than only in a
hand.

**Not real, and deliberately not invented.** Category, craftsmanship, culture,
age, wear, maker, previous owner, provenance, enchantment, rarity, loot tables,
drops, inventory, item identifiers and any generator that produces a third
weapon. There are exactly two accepted weapons, both named fixtures, both
locked, and a third is a decision rather than a configuration.

The honest statement of where this domain is: the found weapon has geometry that
changes a decision, and no history whatsoever. Nothing in the world explains why
it is standing in that gate — which is the same gap `WORLD_GENERATION.md`
records for the gate itself.

## Shared validation

All domains need canonical descriptors, explicit random streams, schema/generator/style versions, budget constraints, preview tools, rejection diagnostics, representative fixtures, cache keys, and reviewable outputs.
