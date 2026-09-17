# Character style

Status: **Accepted for the M5 vertical slice**. Character style contract version `1`.

This document does for characters what [`STYLE_BIBLE.md`](STYLE_BIBLE.md) does for terrain: it states constraints as numbers, ratios, colours, and rejection criteria that a generator or a test can be checked against. It is not a mood board and it does not repeat the style bible.

## Relationship to the style bible

The style bible is the **global** contract and it still applies to characters without modification. Nothing here overrides it.

| owned by `STYLE_BIBLE.md` and unchanged for characters | owned by this document |
|---|---|
| sunlight direction, colour, intensity | character voxel scale |
| ambient, shadow behaviour, shadow floor | body proportions and ratios |
| fog, sky, weather states | limb thickness and extremity exaggeration |
| relative-luminance bands and contrast rules | joint overlap |
| HSV saturation ceilings | silhouette rules |
| "voxel structure stays visible and intentional" | character palette slots |
| "not photorealism, not Minecraft with a shader" | posture and animation timing |
| the one-sentence test for a terrain frame | allowed joint ranges |

The two documents carry **separate version numbers on purpose**. `STYLE_CONTRACT_VERSION` participates in the terrain generator fingerprint, so a terrain rule change invalidates cached chunks. A character rule change must not do that: it invalidates the character identity fingerprint and nothing else.

`CHARACTER_STYLE_VERSION = 1`.

The version participates in `CharacterIdentity` and therefore in `CompiledCharacter::fingerprint()`. Changing a rule here that moves a voxel, a bone, or a pose requires bumping it, which forces the locked fixtures to be reviewed. It is deliberately **not** part of any chunk cache key.

## The one-sentence test

A character frame passes when a person can tell at a glance that they are looking at a person — head, shoulders, waist, two separate legs, two arms — from far enough away that individual voxels are no longer countable, and can still see that the person is built from deliberate blocks when they walk up to it.

## Visual scale

- **`CHARACTER_VOXELS_PER_WORLD_UNIT = 16`.** One character voxel is `0.0625` world units. Terrain remains one voxel per world unit; this is the documented ratio between the two domains and the only place it is declared.
- The golden humanoid is **28 character voxels tall**, that is `1.75` world units.
- A character's total height must lie between `20` and `40` character voxels. Below twenty the head cannot carry a face; above forty the voxel grid stops reading as deliberate blocks at conversational distance.
- **No character feature may be thinner than two voxels in any axis.** This is the character-scale analogue of the terrain rule that rejects single-voxel bumps.

Scale is a visual claim, not an arithmetic one. It is judged against a tree (`8`–`11` world units), against a cliff (at least `4` world units), and against a single terrain voxel, in the captures named in the milestone document. If those captures reject the ratio, this document's version is bumped and the fixtures are re-locked.

## Proportions

All fractions are of total body height unless stated. The compiler derives integer voxel extents from them and both the descriptor validation and the compiled result are checked against the bands below.

| measure | band | golden value |
|---|---|---|
| head height / total height | `0.150` to `0.200` | `0.179` (5 of 28) |
| leg length (ground to hip) / total height | `0.440` to `0.560` | `0.500` (14 of 28) |
| arm length (shoulder to fingertip) / total height | `0.340` to `0.460` | `0.429` (12 of 28) |
| shoulder span / head width | at least `1.80` | `3.00` (12 over 4) |
| hip width / shoulder span | `0.500` to `0.850` | `0.667` (8 over 12) |
| waist width / hip width | at most `0.950` | `0.750` (6 over 8) |
| limb thickness / total height | `0.070` to `0.160` | `0.107` (3 of 28) |
| foot length / shin depth | `1.30` to `2.20` | `1.67` (5 over 3) |
| hand depth / forearm depth | `1.20` to `1.80` | `1.33` (4 over 3) |

The chest box is the hip box's width; the shoulders are made by where the arms hang, not by a wider chest. The shoulder span is therefore the control the geometry reads when it places an arm, and the waist is the abdomen box that sits narrower than both.

Head height between one fifth and one sixth of the body is the heroic band. A realistic one-seventh-and-a-half head disappears at distance; a one-third chibi head is a different game.

**Fingertip reach.** With the arms at rest the fingertips must fall between the hip joint and the knee joint. An arm that stops above the hip reads as a stump; one that passes the knee reads as an ape.

## Silhouette

Rejection criteria, checked against the compiled bind pose without a renderer:

- **The waist reads.** Waist width is strictly less than both hip width and chest width.
- **The head reads.** Head width is at most `0.60` of the shoulder span.
- **The legs read.** At least one voxel of horizontal gap separates the two legs below the hips in the rest pose.
- **The arms read.** At rest, each arm's outermost voxel lies outside the chest's outermost voxel on the same side.
- **The body is one piece.** Every compiled part's volume contains its bone's joint origin, so no rotation can open a hole (see joint overlap).

## Construction rules

Three proportion rules are guaranteed by construction rather than checked after the fact, because rounding two independent fractions to even voxel widths can collide at some body heights and a generator should answer with a narrower waist rather than with a rejection:

- the **waist** is at most the hip width minus two voxels;
- the **gap between the legs** is at least two voxels;
- the **torso height** is derived from the head, not declared: the shoulder joint sits one neck gap below the chin, so a descriptor cannot place the head below the shoulders.

The remaining proportion rules are rejections with typed errors, because there is no sensible value to substitute.

## Joint overlap

Body parts are rigid. A rigid part rotating about a joint opens a gap unless the two volumes overlap there.

- Every non-root part's volume extends **at least one voxel past its joint origin toward its parent**.
- Every part's volume **contains its own bone origin**.
- Consequence, and the rule a test asserts: for every parent/child pair the two volumes share at least one voxel cell in the rest pose.

## Detail frequency

- A material region on a body part is at least **two voxels** in the direction it varies. One-voxel material speckle is rejected exactly as it is on terrain.
- At most **four distinct materials** are visible on any one body part.
- The face carries at most **two** detail voxels (the eyes). A voxel character does not get a nose at this scale, and trying reads as dirt.
- The whole character uses at most **ten** distinct materials.

## Palette and value

Linear RGB, authored for the style bible's clear-weather key light, and read through the same `luminance()` weights the terrain palette is checked with.

- Every character material's relative luminance lies in `0.16` to `0.80`. The band is deliberately wider than terrain's lit band on both ends: a character must read against the meadow (`0.35`–`0.72`), against shadowed rock, and as a silhouette against the sky (`0.62`–`0.90`).
- **Adjacent material slots differ by at least `0.08` in relative luminance**, the same separation terrain uses, so a material boundary on the body is visible without relying on hue.
- The **skin and the primary garment** differ by at least `0.12`, because that is the boundary that carries the reading of where the body is inside the clothes.
- HSV saturation ceiling is `0.62` for garments and accents and `0.55` for skin, hair, and leather. Anything more saturated reads as toy plastic.
- The character carries **no specular**. Water is the style bible's liquid cue and nothing on a character may compete with it.

## Palette slots

Ten semantic slots. A palette choice fills them from declared tone tables; no colour is written twice and no generator invents one.

| slot | role |
|---|---|
| skin | face, hands, lower forearms |
| skin shade | the underside band that keeps the neck and the palms from reading flat |
| hair | scalp and fringe |
| tunic primary | dominant garment surface |
| tunic secondary | the band that breaks the tunic so the torso is not one block |
| trouser cloth | legs above the boot |
| belt | the horizontal break at the waist |
| boot leather | foot and lower shin |
| accent | trim, one narrow vertical stripe |
| eye dark | the two face detail voxels |

## Posture

- The rest pose is **symmetric** and is a measurement pose, not a presentation pose. It is what fixtures lock.
- The idle pose is **not** the rest pose. It carries a small asymmetric weight shift and a breathing cycle: chest rise between `0.15` and `0.60` of one voxel, period between `2.5` and `5.0` seconds.
- At rest the arms hang with a small outward angle, between `3` and `10` degrees, so they do not read as fused to the torso.

## Animation

Timing and amplitude, all as functions of the compiled proportions rather than as absolute numbers.

| rule | band |
|---|---|
| walk duty factor (fraction of the cycle a foot is planted) | `0.55` to `0.70` |
| run duty factor | `0.30` to `0.50` |
| stride length at the gait's reference speed | `0.55` to `1.30` of leg length |
| pelvis vertical bob, peak to peak | `0.02` to `0.08` of leg length, at twice the step frequency |
| pelvis lateral sway, peak to peak | `0.00` to `0.06` of hip width, at the step frequency |
| chest counter-rotation against the pelvis | opposite phase, `0.4` to `1.2` times the pelvis yaw amplitude |
| arm swing | opposite to the leg on the same side, always |
| forward lean, run | `2` to `12` degrees |

**Foot travel is stride, not tuning.** The locomotion phase advances with distance travelled, so `stride length x steps per unit distance = 1` holds by construction. A gait whose feet slide is a bug, not a parameter.

## Allowed joint ranges

Degrees, measured from the rest pose. These are rejection criteria for any pose the animation or the IK produces, not suggestions.

| joint | axis | range |
|---|---|---|
| spine, chest | yaw | `-18` to `18` |
| spine, chest | pitch | `-12` to `20` |
| head | yaw | `-45` to `45` |
| head | pitch | `-30` to `30` |
| shoulder | pitch | `-75` to `75` |
| shoulder | roll (outward) | `0` to `35` |
| elbow | flexion | `0` to `135` (never hyperextends) |
| wrist | pitch | `-30` to `30` |
| hip | pitch | `-55` to `75` |
| hip | roll | `-12` to `12` |
| knee | flexion | `0` to `140` (never hyperextends) |
| ankle | pitch | `-35` to `35` |

Angles are signed rotations about the bone's local `X` axis and positive moves the bone's tip forward, toward `-Z`. The knee and the elbow carry **flexion as a non-negative magnitude** and the pose applies the anatomically correct direction for each, which is what makes "never hyperextends" a property of the representation rather than a range check.

An elbow or a knee that passes through zero in the wrong direction is a rejection, not a soft limit: it is the single most recognisable animation failure and no amount of exaggeration excuses it.

## Terrain contact

- A planted foot's sole sits on the surface the player can see, which is the **top face of the topmost solid terrain voxel**, not a smoothed field. A contact model that disagrees with the drawn blocks is wrong however elegant it is.
- Permitted deviation of a planted sole from that surface is **one character voxel**, that is `0.0625` world units.
- The pelvis is lowered by whatever the more demanding leg requires, so neither knee reaches full extension while a foot is planted.
- A foot may pitch to the local slope by at most the ankle range above.

## What this document does not cover

Faces beyond two eye voxels, hair as geometry, clothing as geometry, equipment, weapons, capes, cloth simulation, creatures, quadrupeds, crowds, character LOD, facial animation, emotes, combat poses, hit reactions, ragdoll, character customization surfaces, and any second archetype. Those belong to later milestones and must not be invented here.
