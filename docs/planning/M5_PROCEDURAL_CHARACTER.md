# M5 — Procedural Character

Status: **complete and merged**
Merged through [PR #9](https://github.com/Jovinull/veldwake/pull/9) at merge commit `5bebc4fa5195b427216a296fff810a294b7bbd7b`, parents `abadadff6ad1251e4f291d272577da6121d37540` and `3c0406dfafc503aa1dc6d98ab1b92db8c33e0e07`. Post-merge CI [run 35352715367](https://github.com/Jovinull/veldwake/actions/runs/35352715367) completed `success`.
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
| M5C | renderer integration, preview scene, first captures | complete |
| M5D | locomotion, terrain contact and leg IK, diagnostic courses, captures | complete |
| M5E | evidence, written visual assessment, documentation close-out | complete |

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

`CHARACTER_VOXELS_PER_WORLD_UNIT = 12`, so one character voxel is `0.08333` world units while a terrain voxel stays one world unit. The golden humanoid is 28 character voxels — `2.3333` world units — tall, with a leg of `1.167`.

| measure | voxels | ratio |
|---|---|---|
| height | 28 | — |
| head height / width / depth | 5 / 6 / 5 | head is `0.179` of height |
| leg length, ground to hip | 14 | `0.500` of height |
| arm length, shoulder to fingertip | 12 | `0.429` of height |
| shoulder span | 12 | `2.00` head widths |
| hip width / waist width / chest width | 8 / 6 / 8 | waist is `0.750` of hip |
| limb thickness | 3 | `0.107` of height |
| foot length | 5 | `1.67` shin depths |

Joint heights: ankle 3, knee 9, hip 14, spine 17, chest 19, shoulder 22, neck 24, chin 23.

Two of these numbers are the second answer rather than the first, and both were changed by a capture rather than by an argument. They are recorded under [M5C](#m5c--the-character-in-the-client) with the frames that changed them: the voxel ratio was `16`, and the chest was two voxels wider than the pelvis.

### Geometry: rigid voxel body parts

Sixteen parts, one per bone, each a small dense voxel volume meshed by `veldwake-voxel`'s existing exposed-face mesher with `BoundaryPolicy::Expose` chosen deliberately — a free-standing part has no neighbour and every outward face is one a viewer can see.

Rigid rather than skinned. The style bible requires voxel structure to stay visible and intentional, and skinning shears the grid; rigid parts also make the geometry static, so a character is compiled once, uploaded once, and never re-uploaded, and animation writes transforms only. Every non-root part extends at least one voxel past its joint toward its parent, which is the joint-overlap rule, asserted against the compiled volumes.

Compilation reuses exactly one `32³` scratch grid — 65,536 transient bytes for the whole run — writing a part, meshing it, and clearing exactly the cells it wrote. A test rebuilds the first part after a whole pass and compares it against a fresh compiler, which is what proves the clearing is complete.

| measure | golden humanoid |
|---|---|
| parts | 16 |
| solid voxels | 1,312 |
| quads | 1,814 |
| CPU mesh payload | 246,704 bytes |
| GPU geometry (40-byte vertices, `u32` indices, one 80-byte part uniform) | 335,056 bytes |
| transient scratch peak | 65,536 bytes |
| skeleton | 16 bones |

Fifteen of the sixteen parts mesh to exactly the surface of a solid box, which a test checks as `2(wh + hd + wd)` quads per part. The chest is the exception and the only carved part in the body: the row where it overlaps the head is narrowed to a two-by-two column, so that the overlap which stops a turning head opening a hole stays entirely inside the head instead of standing out round the jaw as a collar. That carve is what gives the body a neck; the capture that demanded it is in [M5C](#m5c--the-character-in-the-client).

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

The workspace has **402 tests** (38 voxel, 69 procedural, 97 streaming, 87 client, 111 character); every pre-M5 test is unchanged in meaning. What each group covers is in [`TESTING_STRATEGY.md`](../engineering/TESTING_STRATEGY.md).

## M5C — the character in the client

This phase is where the design met the screen, and five things the design got wrong were found by opening a capture and looking at it. Each is recorded with the frame that rejected it, because the reasoning that produced the wrong answer was not obviously wrong on paper and will be produced again otherwise. A sixth — the one that mattered most — no capture could find, and it is under [M5D](#m5d--locomotion-terrain-contact-and-the-courses).

### What the renderer does with a character

The character draws in the same two passes as terrain and through the same lighting. `apps/client/src/shading.wgsl` now holds the scene bindings, the sun term, the shadow lookup, and `shade_surface`; `world.wgsl` and `character.wgsl` both call it. That is deliberate: terrain and character sharing one lighting function is the only way they cannot drift apart, and a character lit by its own copy of the sun is exactly the failure that makes a figure look pasted on.

Geometry is static and uploaded once. Per frame the client writes one 80-byte uniform per part — a model matrix and four shading parameters — which is **1,280 dynamic bytes per frame** for sixteen parts, and issues 16 world draws and 16 shadow draws.

### Five things the captures changed

**The scale.** `CHARACTER_VOXELS_PER_WORLD_UNIT` was `16`, putting the humanoid at `1.75` world units with a leg of `0.6875`. The scale-reference capture showed a person shorter than the undergrowth they were standing in, and the terrain test said the same thing arithmetically: a whole leg shorter than one terrain voxel, so a single terrace of its own world was a step the body could not take. The ratio is now `12`: `2.3333` world units, a leg of `1.167`, and a character that clears both a shrub and a terrace. The gait thresholds were rescaled with it, from world units per second to **leg lengths per second**, so a body of another size gets the same gait at its own speed.

**The chest.** It was authored two voxels wider than the pelvis, on the theory that the step would read as a torso. The first front portrait showed the opposite: a ledge overhanging the waist, the top of each sleeve swallowed by it, and daylight through the notch between ledge, waist and arm. The chest is now exactly the pelvis's width and the shoulders come from where the arms hang.

**The neck.** The chest's topmost row exists so a turning head cannot open a hole at its own joint, and it is entirely inside the head — except that the chest was wider and deeper than the head, so that row stood out all the way round the jaw. The capture read as a head resting on a slab. `geometry::solid_at` now narrows that one row to a two-by-two column: the joint stays covered, the column sits inside the head, and the body has a neck. It is the only carved part in the body.

**The sleeve.** The shoulder is closed by putting the innermost column of the upper arm inside the chest, so from the front the sleeve showed two voxels while the forearm below it showed three, and the arm appeared to grow at the elbow. The outermost column of the chest now takes the sleeve colour, which gives the shoulder its third voxel back and reads as a yoke. This is a material rule, not a geometry one: no voxel moved.

**Which way the character faces.** `Quat::from_rotation_y(yaw)` turns `-Z` toward `-X`, while the camera's forward at the same yaw is `+X`. The character walked backwards along the whole course, and nothing headless caught it because every headless test agreed with itself. `pose::facing_rotation` is now `Quat::from_rotation_y(-yaw)`, with tests at yaw `0`, `±π/2`, `π` and `2.4`, and a test that a walking character moves the way it faces.

### Where the captures are taken

Two stand points, because they need different ground.

- **The portrait clearing**, at `(-69, 49)`: dry, level for five world units, free of vegetation for seven, and — the condition the first clearing failed — open for sixteen, with nothing around it standing more than two world units above it. The first clearing sat at the foot of a rise and the front camera was inside the hillside behind the character. `the_portrait_stand_is_a_level_clearing` asserts all three.
- **The course corridor**, from `(46, -36)` east: level for its whole length at one block level, dry, and clear of trees, which is what a walk needs and which no column with a level disc around it also offered.

The character stands facing `-Z` at the portrait clearing, and the reason is the sun. M4's key light comes from an azimuth of `-38°`, arriving from `-X` and `-Z`; a character facing `+X` turns its whole front away from it, and the first portraits came back in the subject's own shade. Facing `-Z` puts the key on the front and the fill on the character's left.

## M5D — locomotion, terrain contact, and the courses

### Gaits are analytical and advance with distance

There are no clips and no timeline. Joint angles are closed-form functions of a gait phase, and the phase advances with **distance travelled**, not with time. That is the decision that removes foot sliding as a class of bug rather than as a symptom: with a time-driven cycle, any disagreement between speed and cadence slides the feet and a foot lock then fights the animation. Here stride length is the thing that is specified, so cadence follows from speed by construction.

Three bands — idle, walk, run — blend by speed, and the thresholds are in **leg lengths per second** rather than world units per second: `0.17` to leave idle, `0.80` for a full walk, `2.50` to begin running, `4.10` for a full run. `GaitParameters::speed_for` converts a leg-relative speed back into world units. This became necessary rather than elegant when the character's scale changed: absolute thresholds silently put the same body at a new size into the wrong gait.

The joint angles are clamped to a table of per-joint ranges taken from [`CHARACTER_STYLE.md`](../audiovisual/CHARACTER_STYLE.md), and a test asserts the nominal gaits never need that clamp: it is there for a hostile speed, not as part of the animation.

### The walk was inverted, and only a measurement said so

This is the most serious defect the milestone found, it survived every frozen capture, and it is worth the space.

The eight-phase contact sheet showed what looked like a walk: a plausible stride, contralateral arms, feet arriving flat and leaving heel-first, no holes. In motion it was a treadmill. The hip swing was a cosine of the cycle while stance was the window `[0, duty)`, and the two were half a cycle apart, so the planted foot swept **forward** through stance instead of back under the body.

What found it was a headless measurement rather than a frame: how far does a planted ankle wander across one stance? The answer was `1.39` world units against a stride of `0.99` — the foot was not planted at all. Fixing the sign was one character; making the number small took three more findings.

**Stride, duty factor and pelvis drop are one decision.** A planted foot has to give back `duty x 2 x stride` of ground relative to its own hip over a stance, so half of that is how far in front of the hip it lands. A leg standing straight has no horizontal reach at all, because the hip-to-ground distance *is* the leg's length, so the reach available is `sqrt(reach² − (reach − drop)²)` and the pelvis has to lower while moving, as it does in a real gait. With the original numbers the foot the stride asked for was simply not a place the leg could be put, and the solver clamped silently. The three are now derived together: walk stride `0.56` of leg length, duty `0.56`, drop `0.086`; run `0.85`, `0.42`, `0.114`.

**A sinusoid cannot cancel constant motion for more than half a cycle.** Whatever its amplitude, a cosine turns around before a stance of duty `0.56` ends. The stance sweep is therefore a straight line in foot offset, and the swing is the cubic that rejoins it with a matching slope at both ends, which keeps the cycle C¹ and removes the whole class of error.

**The foot's forward placement is solved, not seeded.** Setting the hip angle alone is not enough, because the knee bends the shin and the realised offset drifts from the one the stride asked for. The contact stage already solved each leg for a vertical target; it now solves for the horizontal one too, which is what makes the offset *be* the offset.

**An arm is driven by the leg it opposes.** The arms were a cosine of their own against a leg that no longer was one, and the two crossed zero three hundredths of a cycle apart — a narrow window where an arm and the leg beside it swung together. Feeding the arm the leg's offset makes the opposition exact by construction.

Measured on the golden humanoid, at `1.2` and `3.4` leg lengths per second, over 42 and 93 completed stances:

| measure | walking | running |
|---|---|---|
| planted ankle wander across one stance | `0.040` world units (`0.48` character voxels) | `0.055` (`0.66` voxels) |
| foot part wander across one stance | `0.109` (`1.31` voxels) | `0.135` (`1.62` voxels) |

The ankle is the joint the contact solver plants. The foot box around it pitches to the ground, so the part moves a little more than the ankle does, and that is a rigid sole rolling rather than a foot sliding; the test bounds it at half the foot's length.

Idle is not the rest pose. It carries a breathing cycle and a small asymmetric weight shift, which is why `idle-a` and `idle-b` are two different frames of the same standing character.

### Terrain contact is a step function, on purpose

`GroundSampler::surface(x, z)` returns **the top face of the topmost solid voxel** — the surface a viewer can see. It is deliberately not the terrain field's continuous height. A sole placed on a smoothed surface either floats above the block it is standing on or sinks into it, and the image is what this milestone is judged on. Water is not ground: a column whose surface voxel is water reports the solid bed under it.

The trait lives in `veldwake-character` and the client implements it in eleven lines over `TerrainField`. That is the whole coupling between the character and the world generator, and it is the reason `veldwake-character` does not depend on `veldwake-procedural`.

Each leg then solves a two-bone analytical IK against the sampled surface, with the pelvis following asymmetrically — rising with a `0.12` s time constant and falling with `0.035` s — because a straight rest leg has no downward headroom and a symmetric follow makes the body sink into a rise.

One distinction in that solver was a real bug and is worth keeping stated. **What a foot stands on** is the block under its own ankle. **What a swinging foot must clear** is the highest block under any part of it. Conflating the two asked the leg to lift a sole a whole terrain voxel above the pelvis, which no leg can do, so the solver clamped and the contact reported a foot most of a terrace away from where it actually was.

### Two courses, because they answer different questions

- **`meadow-crossing`**, from `(46, -36)`: level for its whole length, dry, tree-free. Stand, walk east, run east, turn, run west, walk west, turn, rest — `54` seconds, `64` world units out and `64` back, ending where and how it started. Level is the point: a walk cycle has to be judged without a terrace confusing it.
- **`terrace-climb`**, from `(41, -116)`: up seven single-voxel terraces in eighteen world units and back down, `25` seconds, also closed. This is where the contact claim is interesting.

Both are loops, and that is an evidence decision rather than an aesthetic one. The capture harness waits seventy-five seconds for the world to stream; the first course was forty-six seconds long and had always finished by then, so every capture of a walk was a capture of a character standing at the end of one. Sampling a closed loop modulo its own duration makes any settle time land somewhere in the walk, with a wrap that is continuous in position, facing and speed.

A third stand point, `(49.1, -116)`, holds the character still on one terrace with the edge a third of a world unit in front of its toes. A moving character cannot serve for that capture: a camera anchored to the foot of a climb is not anchored to a climbing subject, and two framings were thrown away learning it.

## M5E — the visual assessment

Every judgement below was made by opening the PNG and magnifying the subject, on the audited Windows 11 / Intel Iris Xe host, at 1920x991 client area, profile `m4-golden`, after a 75-second settle, exit code `0`, with no validation error, device-loss, panic or warning in any log. The captures themselves are not committed, for the reason [`EVIDENCE_HARNESS.md`](../agents/EVIDENCE_HARNESS.md) gives; this assessment is the durable artefact.

The one-sentence test in [`CHARACTER_STYLE.md`](../audiovisual/CHARACTER_STYLE.md) is the standard: *can a person tell at a glance that they are looking at a person, from far enough away that individual voxels are no longer countable, and still see deliberate blocks up close?*

### Proportion and silhouette

- **`neutral-front`** (`character-front`, `pose:rest`). Reads as a person. Head with two eyes and a hair cap, a jaw row below it, shoulders either side of the head, a rust tunic with pale sleeves and a tan placket down the front, a belt, two legs separated by a clear gap, two arms hanging to just above the knee, boots. Nothing overlaps wrongly, nothing shows daylight through it, and the chin is not lost behind a collar. This is the frame that rejected the wide chest, the missing neck and the narrow sleeve; all three are gone.
- **`three-quarter`** (`pose:rest`). Depth reads: the torso has a front and a side, the foot boxes project forward of the shins, and the limbs separate from the body. The near arm covers much of the torso at this angle, which is what an arm at rest does.
- **`side`** (`pose:rest`). Framed fifteen degrees round from a true profile, because a true profile puts the near arm exactly over the torso and loses the one thing the shot is for. Foot length forward of the ankle, arm hang, and torso depth all read.
- **`silhouette`** and **`silhouette-overcast`** (`character-silhouette`, low camera). The outline alone carries the body in both weathers: head, shoulders, waist, two separated legs. The pale sleeves are the strongest edge cue and they survive the loss of contrast under overcast.
- **`detail`** (`character-detail`, close). Face voxels, hair cap, jaw shade, collar, yoke, placket and belt all read as separate regions at conversational distance, and no material region is one voxel thin. No shadow acne anywhere on the chest, which is what the front-face-culled character shadow pass bought.
- **`fixture-ab`** (`neutral-front` beside `sturdy`). Two different people in one style. The sturdy fixture is visibly shorter and stockier — broader hips and belt, thicker limbs, shorter legs relative to its torso, a moss tunic and a deeper skin tone — and both still pass the one-sentence test.
- **`idle-sheet`** (`idle-a` beside `idle-b`). The idle is not a statue: the two frames differ in pelvis height, weight shift and arm position. The difference is small, which is the intent.

### Scale, and belonging to the region

- **`scale-reference`** (`character-scale`, ~15 units out). The character stands in meadow among M4 trees. Trunk-to-canopy is roughly three times its height, the undergrowth blocks nearby come to about its shoulder, and it reads unambiguously as a person in a forest. This is the frame that rejected `16` voxels per world unit, where the same character was shorter than the undergrowth.
- **`in-scene`** and **`in-scene-overcast`** (~19 units out). The character belongs to the scene rather than sitting on top of it: one sun, one shadow direction, the same fog, and a grounded shadow of its own. Overcast desaturates and softens the shadow; the figure still reads.

### Terrain contact

- **`foot-contact`** (`character-contact`, `pose:rest`). Both boots rest flush on the grass. No gap under a sole, no sole sunk into the block. The first framing of this pose was so close that it photographed two boots against featureless grass, which shows a sole but not what it rests on; this one includes the ground.
- **`terrace-contact`** (`character-slope`, `slope-stand`). The evidence frame for the contact claim: the character stands on a terrace with the block edge a third of a world unit in front of its toes and the step down clearly visible beside its boots. The soles sit on the block top the viewer can see, which is the whole of what `GroundSampler` promises.

### The gait, in frozen phases and in motion

- **`walk-cycle-sheet`** (eight runs at eight equally spaced phases, `character-side`). It reads as a walk. The legs alternate, the arms counter-swing on the opposite side, the feet arrive flat and leave heel-first, the knees bend through swing, and phases `0-3` mirror `4-7` as an alternating gait must. The pelvis sits visibly lower than in the rest pose, which is the derived drop rather than a slouch — it is what makes the stride reachable at all. No joint opens a hole in any of the eight, nothing clips through the torso, and every shadow is attached.

  This sheet is also the cautionary artefact of the milestone: the pre-fix version of it looked like a perfectly good walk while the walk was inverted. Eight still frames cannot tell a stride from a slide.

- **`clearing-walk-strip`** (six consecutive frames, `character-clearing`, on the level clearing). The body advances across the frame at a steady rate — about forty pixels a frame with no stutter, measured against the fixed trunks behind it — while the pose advances continuously with it. No phase-wrap hitch, no pop at a blend boundary, no same-side arm and leg, no detached shadow. Sub-voxel foot sliding cannot be resolved at this scale, which is exactly why the oracle for it is the headless measurement above and not this strip.

- **`slope-walk-strip`** (six consecutive frames, `character-slope-walk`, climbing the terraces). The character climbs, and each foot lands on a terrace top rather than in the air above one or inside one. The knees take the step height. One thing is visible here and is honest: a rigid foot arriving at a step puts its toe into the riser for a frame or two, which is the bounded overhang the contact stage documents and the price of a rigid foot on blocky ground.

- **`run-contact`** (`pose:run-contact`, `character-side`). Reads as a run rather than a fast walk: the torso pitches further forward, the arms swing higher with a strongly flexed elbow, and the legs split with the trailing leg extended. The arm crosses in front of the chest at this phase, which is a runner's arm and hides the tunic; that is framing, not a defect.

### What was looked for and not found

Taken over the frozen sheet, the two motion strips and the whole-course logs: no foot sliding beyond the measured sub-voxel residue, no hitch at the phase wrap, no discontinuity at the idle-to-walk or walk-to-run blend, no same-side arm and leg, no knee or pelvis popping between frames, no gap at any joint, no limb clipping through the torso, no foot floating above the ground or sunk into it on level terrain, no detached or missing shadow, no weightlessness, and no absurd scale. The defects that *were* found are each recorded above with the frame or the measurement that found them.

## Findings that are not yet resolved

Each of these is an open entry in [`KNOWN_ISSUES.md`](../KNOWN_ISSUES.md) rather than prose here, so that nothing depends on someone reading this document.

- **KI-018** — a planted foot is computed rather than remembered. On level ground that now costs `0.040` world units of ankle wander across a stance; on a terrace crossed mid-stance it is bounded by one terrain voxel and reported. The cure is foot pinning with the pelvis solved from the pinned feet.
- **KI-019** — the character casts no shadow on itself, because the character shadow pass culls front faces to remove acne that a `0.109`-world-unit shadow texel causes on `0.083`-unit voxels.
- **KI-020** — a rigid limb's downward-facing end cap emerges through its child at a flexed joint, about a tenth of a voxel at six degrees. Verified not to be z-fighting: captures with and without a per-part depth tie-break are identical.
- **KI-021** — character validation is one host, one adapter, and no capture is compared automatically. The locked signatures catch a changed voxel, bone or gait without a GPU; nothing checks that the result still looks right.

Two further observations that are not defects but are worth a reader's time:

- The level course corridor is tree-free but **not** shrub-free: seventeen vegetation voxels stand on the walked line, and a scan of sixty world units of it found no point with a clear sight line to a camera twelve units to the side. The character walks through undergrowth, because M5 has a collision *representation* and no collision *response*. That is why the close motion strip is taken in the portrait clearing instead.
- The diagnostic courses snap facing between legs. The turn legs stand still, so the character rotates on the spot in one frame. There is no turn-in-place animation and none was in scope; it is a property of the diagnostic path, not of the locomotion.

## Scope, and what this milestone deliberately did not build

Faces beyond the two eye voxels, hair or clothing as geometry, equipment, weapons, a second archetype, character LOD, a character controller, player input, gameplay or camera integration, any physics or collision library, ragdoll, cloth, an animation graph, emotion, hand IK, and a save format. Each of those is a later milestone and nothing here should be read as having decided it.

Two boundaries were argued rather than assumed, and both are recorded where the code is:

- **`veldwake-character` does not depend on `veldwake-procedural`.** It carries thirty lines of its own hashing rather than reach for that crate's helpers, because a headless character crate compiling a world generator to borrow two hash functions would falsify the dependency direction in [`ARCHITECTURE.md`](../engineering/ARCHITECTURE.md). The one thing it needs from the world is `GroundSampler`, which the client implements in eleven lines over `TerrainField`.
- **A character is not a chunk, so it does not travel through `ChunkSource`.** Streaming addresses content by coordinate; a character is addressed by identity. It is compiled once at startup and uploaded once.

No third-party dependency was added. The only new dependency edge in the workspace is `veldwake-client -> veldwake-character`.

## Gates

### Independent QA corrections and final visual exit gate

Independent QA found three correctness boundaries that the implementation
evidence had not exercised. `TerrainGround` now uses the region's continuous
half-open bounds (`[-384, 416)` in each golden horizontal axis), rather than
turning the last discrete column into an inclusive continuous maximum; its
regression covers both axes, fractional last-column positions, and negative
edges. A requested character now fails startup when compile or GPU upload
validation fails, rather than silently producing a world without the requested
M5 feature; the upload path also rejects a wrong part count, empty mesh, index
overflow, or non-character voxel identifier before drawing. Finally, the
public two-bone solver sanitizes non-finite lengths and poles and keeps its
degenerate reach interval non-empty, so hostile inputs cannot panic
`f32::clamp`.

The corrected branch completed its independent Windows/D3D12 visual exit gate
on 2026-09-18 at `24b9375bc891964dd5235fa652a6a65d836afa6a`. Fresh,
DPI-aware client-area captures were taken only after maximizing the single
client instance; every accepted image was `1920x991`, opened, and inspected.
No capture containing desktop, terminal, taskbar, or another window was used
as evidence. The static gallery covered front, three-quarter, side,
clear/overcast silhouette, both idle states, shadow/detail, scale, and
clear/overcast in-scene framing. It confirms the whole body stays framed, the
head/neck/torso/limb silhouette and palette read, rigid-part seams have no
holes or z-fighting, soles meet visible ground, and clear and overcast remain
readable.

Eight independently launched frozen walk phases were opened and compared in
order. They show alternating legs and opposite arm swing, flexed knees,
continuous hips/pelvis/feet, and a continuous phase 7 to phase 0 return. The
only close joint line is the documented rigid parent end cap (KI-020), not a
depth conflict or missing face. Consecutive frames from one flat-course run
show steady forward motion, correct facing, attached shadow, and no visible
phase hitch, pop, same-side gait, or body-speed/gait mismatch. Consecutive
slope-course frames plus the foot-contact and terrace-contact captures show
block-top contact without severe floating, penetration, knee inversion,
unreachable-leg stretch, or shadow detach. As documented in KI-018, a stance
crossing a terrace follows the new block rather than retaining a world-space
pin: **KI-018 CONFIRMED / ACCEPTED FOR M5**. The character casts onto terrain,
receives terrain shadow, has no self-shadow, and shows no severe acne or
peter-panning: **KI-019 CONFIRMED / ACCEPTED FOR M5**. The magnified knee,
elbow, shoulder, hip, wrist, and ankle review distinguishes the small rigid
end-cap line from a rendering defect: **KI-020 CONFIRMED / ACCEPTED FOR M5**.

Independent D3D12 lifecycle smokes completed with character off for the M3
diagnostic and M4 golden profiles, and with the real M5 course enabled. They
covered settle, traversal input, debug toggles, weather transition and return,
resize, minimize/restore, refocus, and Escape shutdown. The M3 checkerboard,
fallback material and floor remained present; M4 terrain, water, cliffs,
vegetation, sky, fog, shadows, streaming and weather remained present; M5
logged `character ready` with the expected `1814` quads, `335056` static GPU
bytes, `1280` dynamic bytes per frame, and sixteen world plus sixteen shadow
draws, and remained available after lifecycle operations. All three exited
`0`; logs contain no validation error, device loss, fatal error, or panic.

Every gate below was executed on the audited Windows 11 / Intel Iris Xe host with the toolchain in [`ENVIRONMENT_REPORT.md`](../environment/ENVIRONMENT_REPORT.md), and is reported with the `AGENTS.md` vocabulary.

| gate | command | result |
|---|---|---|
| format | `cargo fmt --all --check` | PASS |
| lint | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS, no warning |
| build | `cargo build --workspace --all-features` | PASS |
| tests | `cargo nextest run --workspace` | PASS, **402 tests run: 402 passed, 0 skipped** |
| doc tests | `cargo test --workspace --doc` | PASS |
| rustdoc | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | PASS |
| lockfile | `cargo metadata --format-version 1 --locked --no-deps` | PASS |
| dependencies | `cargo deny check` | PASS — advisories ok, bans ok, licenses ok, sources ok |
| advisories | `cargo audit` | PASS, no vulnerability |
| whitespace | `git diff --check` | PASS |
| line endings | every changed file checked for CRLF | PASS, none |
| documentation links | all 152 relative Markdown links resolved | PASS |
| terrain probe | `terrain-probe signature` | PASS, world `0x96aef6bb59dca573`, regional `0x1285779915164f6a` unchanged |
| streaming probe | `streaming-probe` | PASS, `stale_loads=0 stale_meshes=0 hard_cap_blocks=0` |
| character probe | `character-probe signature` | PASS, all seven locked values match what the compiler produces |

`cargo nextest` needed one retry for the host linker lock recorded as KI-008 (`LNK1104`) and passed on the second plain run. No other gate was re-run, and nothing was re-run after a failure that was not that lock.

### The three smokes

Driven on the audited host with the release client: settle, traverse in both signs on `x` and `z`, cycle the three debug views with their boxes, toggle the weather, restore and resize the window, minimize and restore it, drop and regain focus with a key held, and exit through Escape.

| smoke | world / profile / character | result |
|---|---|---|
| M3 diagnostic regression | `diagnostic` / `m3c-baseline` / `off` | PASS. Checkerboard corridor and the material-fallback pillar render, floor continuous, exit code `0`, `cpu_evictions = 45` so the camera really moved, no error, no validation error, no device loss, no panic, **no warning at all** |
| M4 golden regression | `golden` / `m4-golden` / `off` | PASS. Valley, river, banded cliff, forest and fog all present; overcast toggles and returns; the frame after minimize, restore and focus loss is identical to the settled one; exit code `0`, `cpu_evictions = 96`, no error and no warning |
| M5 integration | `golden` / `m4-golden` / `course` | PASS. `character ready` reports `quads=1814 gpu_bytes=335056 dynamic_upload_bytes_per_frame=1280 world_draws=16 shadow_draws=16`, matching the probe exactly; the course wraps from `elapsed=50.6` to `1.6` without a jump; `grounded=true` and `base_height=18` throughout the level corridor; exit code `0`, `cpu_evictions = 154`, no error and no warning |

One honest note about the debug views. In all three smokes they demonstrably activate: `debug_boxes` goes true, `debug_slots` reaches `672` on the diagnostic corridor and `3,250` on the golden slice, and the five-second interval containing them reports `41.8` ms average frame time against `16.6` either side — exactly the cost KI-012 records. The captured frames at those moments contain no visible wireframe, which at these camera poses is consistent with depth-tested boxes sitting behind terrain. The telemetry is the evidence here, not the pixels.
