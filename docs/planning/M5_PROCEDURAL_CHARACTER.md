# M5 — Procedural Character

Status: **in progress on `feat/m5-procedural-character`**
Base: `main` at merge commit `abadadff6ad1251e4f291d272577da6121d37540`

M4 proved the world can look like something. M5 has to prove that a person can stand in it and belong there — produced by systems, not by a modelling tool. The exit criterion is the same kind as M4's: a real capture of a real client on the audited host, judged against a written contract, backed by headless tests and measurements.

This document is written as the milestone lands, phase by phase. Anything not yet measured is not claimed.

## Scope, copied from the roadmap

One humanoid descriptor/compiler path, consistent voxel geometry and materials, a generated skeleton, locomotion, a terrain contact and IK subset, a collision representation, a preview/fixture pipeline, and style-rule evidence. Nothing else.

## Phases

| phase | contents | status |
|---|---|---|
| M5A | character style contract, descriptor, validation, compiler, rigid voxel geometry, character materials, fixtures, probe | complete |
| M5B | skeleton, rest pose, part binding, collision representation | complete |
| M5C | renderer integration, preview scene, first captures | pending |
| M5D | locomotion, terrain contact and leg IK, diagnostic course, captures | pending |
| M5E | evidence, written visual assessment, documentation close-out | pending |

## M5A and M5B — the character domain

### A new crate: `veldwake-character`

The crate test in [`../engineering/ARCHITECTURE.md`](../engineering/ARCHITECTURE.md) was applied before the directory existed, and the deciding arguments were dependency direction and build isolation rather than "characters deserve a crate":

- `veldwake-procedural` is documented as answering *what is in the world at a given place*. A character is not addressed by position, so putting the compiler there would make that crate's own charter false.
- `veldwake-streaming` depends on `veldwake-procedural`. A character compiler living in `procedural` would make the streaming worker compile skeleton, IK, and locomotion code it never calls.
- The domain owns its own identifier range, its own contract versions, its own style document, and its own fixtures.

It depends on `veldwake-voxel`, `glam`, and `std`. It **does not depend on `veldwake-procedural`**, which was an explicit reversal of the first design sketch: the only thing a character needs from terrain is the height under a foot, and that is a two-method trait the client implements. Nothing was made public in `procedural` to serve this crate, and the small deterministic hash the character domain needs is its own, with a test that pins it to the published FNV-1a and SplitMix64 vectors so "the same function as the world generator" stays a checkable statement rather than a comment.

`glam` is a new dependency edge, not a new workspace dependency: the version is the one `apps/client` already pins, and the alternative — reimplementing `Quat`, `Vec3`, and `Mat4` — is the duplication `AGENTS.md` warns about. The client consumes the character's matrices directly, so sharing the type also removes a conversion layer.

```text
voxel <- procedural <- streaming <- client
  \                                  /
   `------- character -------------'
```

### Descriptor and compiler

```text
CharacterDescriptor -> validate() -> ValidatedDescriptor -> CharacterCompiler -> CompiledCharacter
```

`ValidatedDescriptor` is the only input the compiler accepts and the only way to obtain one is `validate()`, so an invalid character cannot reach geometry. The descriptor is fourteen proportion fractions, a palette choice of three tone families, a seed, a bounded variation amount, an archetype with one variant, and a schema version.

Validation runs on the **derived integer body**, not only on the input fractions, because rounding is where a plausible ratio turns into a one-voxel limb. `CharacterDescriptorError` has ten variants and every one is reachable from a test.

Three rules are guaranteed by **construction** rather than by rejection, following M4's "make a spacing rule a theorem instead of a hope":

- the waist is at most the hip width minus two voxels, so it always reads;
- the gap between the legs is at least two voxels;
- the torso height is derived from the neck rather than declared, so a descriptor cannot place the head below the shoulders.

The first two exist because two independent fractions rounded to even voxel widths collide at some body heights; the third because two independent controls for "where the shoulders are" and "where the chin is" can contradict each other. Rejecting those would have made bounded seed variation able to produce an invalid character, which is not what bounded means.

### The reference body

`CHARACTER_VOXELS_PER_WORLD_UNIT = 16`, so one character voxel is `0.0625` world units while a terrain voxel stays one world unit. The golden humanoid is 28 character voxels — `1.75` world units — tall.

| measure | voxels | ratio |
|---|---|---|
| height | 28 | — |
| head height / width / depth | 5 / 4 / 3 | head is `0.179` of height |
| leg length, ground to hip | 14 | `0.500` of height |
| arm length, shoulder to fingertip | 12 | `0.429` of height |
| shoulder span | 12 | `3.00` head widths |
| hip width / waist width | 8 / 6 | waist is `0.750` of hip |
| limb thickness | 3 | `0.107` of height |
| foot length | 5 | `1.67` shin depths |

Joint heights: ankle 3, knee 9, hip 14, spine 17, chest 19, shoulder 22, neck 24, chin 23.

### Geometry: rigid voxel body parts

Sixteen parts, one per bone, each a small dense voxel volume meshed by `veldwake-voxel`'s existing exposed-face mesher with `BoundaryPolicy::Expose` chosen deliberately — a free-standing part has no neighbour and every outward face is one a viewer can see.

Rigid rather than skinned. The style bible requires voxel structure to stay visible and intentional, and skinning shears the grid; rigid parts also make the geometry static, so a character is compiled once, uploaded once, and never re-uploaded, and animation writes transforms only. Every non-root part extends at least one voxel past its joint toward its parent, which is the joint-overlap rule, asserted against the compiled volumes.

Compilation reuses exactly one `32³` scratch grid — 65,536 transient bytes for the whole run — writing a part, meshing it, and clearing exactly the cells it wrote. A test rebuilds the first part after a whole pass and compares it against a fresh compiler, which is what proves the clearing is complete.

| measure | golden humanoid |
|---|---|
| parts | 16 |
| solid voxels | 1,168 |
| quads | 1,688 |
| CPU mesh payload | 229,568 bytes |
| GPU geometry (40-byte vertices, `u32` indices, one 64-byte transform per part) | 311,616 bytes |
| transient scratch peak | 65,536 bytes |
| skeleton | 16 bones |

Every part meshes to exactly the surface of a solid box, which a test checks as `2(wh + hd + wd)` quads per part.

### Materials

`CharacterMaterial` is a separate enum from `TerrainMaterial` with its own declared range, `128..192`, ten variants in use. The global allocation table now lives in `ARCHITECTURE.md` so an identifier is never a global magic number. Disjointness from air, from the M2/M3 diagnostic identifiers `1`, `2`, and `7`, and from the terrain reservation is proved in `veldwake-character`; the client proves separately that the renderer's ordered lookup cannot be ambiguous.

A character colour is not a constant, because the descriptor picks tones, so the palette is a compiled artefact: three skin tones, three hair tones, and three six-slot garment schemes resolve to one `CompiledPalette` of ten linear-RGB colours.

The value-separation rule is checked against the **compiled voxels**, not against a hand-written list of pairs: a test walks every part of every one of the 27 palette combinations, finds every face-adjacent pair of distinct materials, and asserts at least `0.08` of relative luminance between them. A geometry change that creates a new material boundary therefore fails a test rather than showing up in a capture.

### Skeleton

Sixteen bones, stored in topological order so composing world transforms is one forward pass with no recursion:

```text
root -> spine -> chest -> head
                   |-> upper-arm -> forearm -> hand   (x2)
root ------------- |-> thigh -> shin -> foot          (x2)
```

`Root`, `Spine`, and `Chest` are three bones rather than one torso specifically so the chest can counter-rotate against the pelvis; without that a walk reads as sliding while swinging the arms. `Foot` is separate because it is the bone that proves slope adaptation, and `Hand` because an arm whose hand does not counter-rotate reads as a stick — not as a weapon socket, which is M6. There is no clavicle, no neck, no finger, and no twist bone.

Proved headlessly: exactly one root, a unique parent per bone, acyclicity by bounded walk, `parent index < child index`, unique names, a bounded count, finite and rigid rest transforms, mirror symmetry of the rest pose, joints at the heights the body says, and agreement between the single forward pass and manual chaining.

`Transform` is rotation plus translation with **no scale field**, so a rigid character cannot stretch a bone by construction rather than by assertion.

### Collision representation

Representation, not simulation. Derived from the compiled geometry so it cannot drift from the body it describes:

- one upright **body capsule** in world units, whose cap centres are solved so that every rest-pose voxel satisfies the capsule's own containment test rather than merely sitting in a bounding box;
- one **part box** per bone in that part's grid space, so a posed world AABB is the same matrices the renderer draws with.

No physics dependency was added and none is proposed for M5: nothing is simulated, so a physics engine would be a dependency with no capability. `AGENTS.md` forbids exactly that.

### Fixtures

Three named characters — `golden`, `sturdy`, and `varied` — because one body cannot prove a compiler exists. `sturdy` differs in every proportion and in the palette; `varied` is the golden proportions under bounded seed variation. Geometry, skeleton, and collision carry **separate** fingerprints so a failure says which one moved, and each fixture also has one folded behaviour signature. Eight named poses lock the states captures are taken at.

No pixel comparison is an oracle anywhere, for the reason KI-006 already records.

### Headless evidence, M5A and M5B

The workspace has **383 tests** (38 voxel, 69 procedural, 97 streaming, 71 client, 108 character); every pre-M5 test is unchanged.

## Findings that are not yet resolved

- **One terrain voxel is taller than the character's leg.** A terrain voxel is one world unit; the golden humanoid's leg is `0.6875`. The character therefore cannot step onto a one-voxel terrain terrace, and a walk across a terraced slope puts a swinging foot through the riser. This is a scale finding rather than a solver defect, and it is a test — `a_terrain_scale_terrace_is_taller_than_the_leg_and_is_bounded_not_hidden` — so that it stays a known quantity. What the solver still guarantees there is that nothing becomes infinite, no joint leaves its range, and a planted foot rests on what is under it. Whether the chosen scale survives is a visual question and is answered in M5C.

## Gates

Recorded when each phase completes.
