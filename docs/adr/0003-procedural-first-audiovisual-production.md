# ADR-0003: Procedural-first audiovisual production

- Status: Accepted
- Date: 2026-09-16
- Owners: Project owner
- Supersedes: None
- Superseded by: None

## Context

The creator explicitly does not want routine dependence on manual modeling, texturing, pixel art, sound recording, or music composition. The intended differentiator is coherent high-quality audiovisual content produced by code and systems, not a cheap or intentionally crude presentation.

## Decision

Adopt a procedural-first pipeline: semantic descriptors, constrained generators/grammars, voxel/SDF/CSG geometry, vertex/procedural materials, procedural skeletons/animation/IK/VFX, audio DSP, and motif/harmony-based adaptive music. Expensive outputs compile to versioned/hash-addressed caches when appropriate. A versioned STYLE_BIBLE and stable visual/audio fixtures govern coherence.

This is procedural-first, not ideological zero-assets. A narrow exception may be approved when it materially improves quality, accessibility, legality, or performance without making manual production the routine bottleneck.

## Alternatives considered

- Traditional Blender/texture/DAW pipeline: conflicts with creator constraints and production model.
- Runtime generative-AI asset dependence: introduces consistency, reproducibility, cost, provenance, and offline/runtime risks.
- Unconstrained random generation: high variation but weak identity and quality.

## Positive consequences

- Production aligns with software engineering and agent strengths.
- Reproducible families of content, small descriptors, automated previews/tests, and cacheable outputs.
- Potentially distinctive coherent identity and high content-to-download ratio.

## Negative consequences

- Significant generator, art-direction, tooling, and validation engineering.
- Visual/audio quality is a central technical risk, not automatically solved.
- Some exceptions may be contentious and need explicit review.

## Future implications

Create the STYLE_BIBLE before content-generator proliferation. Each generator needs semantic/style/runtime validation and representative fixtures. Procedural output quality must be proven in vertical slices before scale.
