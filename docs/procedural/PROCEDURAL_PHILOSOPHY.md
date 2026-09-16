# Procedural philosophy

Status: **Accepted**.

Veldwake uses procedural generation to encode a coherent design language and causal world, not to maximize random combinations.

```text
structured generation + constraints + style grammar + deterministic systems
```

not:

```text
random() + random() + random()
```

## Rules

- A descriptor expresses semantic intent; a generator compiles it into representation.
- Seeds are explicit, stable inputs scoped to a compatible generator/world version.
- Random draws occur inside named distributions and aesthetic/mechanical constraints.
- Cultures, species, biomes, and item families share grammars so variants belong together.
- Generated results remain inspectable: inputs, stages, decisions, rejection reasons, and hashes.
- Expensive results are cacheable by stable descriptor hash; procedural production does not imply per-frame recomputation.
- Rare content is curated through distributions, validation, and composition—not accidental parameter extremes.
- A generator is incomplete until representative outputs can be evaluated automatically and by humans.

## Evaluation layers

1. Structural validity: manifold/mesh rules, bounds, skeleton compatibility, collision, budgets.
2. Semantic validity: the result satisfies its descriptor and gameplay role.
3. Style validity: silhouette, palette, proportion, material, detail frequency, and cultural grammar.
4. Runtime validity: LOD, memory, generation latency, cache behavior, and rendering/audio budgets.
5. Experiential validity: the output is readable, attractive, varied, and worth encountering.

Procedural systems amplify direction. They do not eliminate the need for taste, iteration, or rejection.
