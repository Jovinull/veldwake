# Landmark style

Status: **Accepted for M8**. `LANDMARK_STYLE_VERSION = 1`.

This document is a constraint set for the one landmark family M8 generates, in the same spirit as [`STYLE_BIBLE.md`](STYLE_BIBLE.md) and [`CHARACTER_STYLE.md`](CHARACTER_STYLE.md): every rule is a number, a ratio, a relation or a rejection criterion that the compiler, a test or a capture can be checked against. The style bible deliberately does not cover architecture, and this document exists so that it still does not — terrain rules stay in the terrain contract, landmark rules live here.

Every number below has now been confirmed by a compiled fixture, a test or a capture; the last section says which ones the implementation moved and why, and the M8 milestone document carries the measurements.

No lore. No culture, civilization or history. These are "ancient, unknown standing stones" only to the extent a viewer needs to read them as deliberate and old rather than as terrain or as a building.

## The one-sentence test

From the discovery overlook a person can point at each landmark, say which one is the tall point, which one is the broad gate, and which one is broken, and none of the three reads as a cliff, a tree, or a house.

## Relationship to the style bible

- Same voxel scale as terrain: **one landmark voxel is one terrain voxel, one world unit.** Landmarks are written into terrain chunks and meshed with them; there is no finer scale and no second mesh path.
- Same sun, shadow, fog and sky. The style bible's value rules for terrain still hold around a landmark; the rules below add to them and never override them.
- Landmark materials own their own identifier range and their own palette table. Terrain's `TerrainMaterial` is not extended.

## Silhouette classes

One family, the **Monolith**, built from three elements — a **shaft** (a stack of stone courses), a **lintel** (a beam across two shafts) and **rubble** (fallen blocks on the ground). Three classes are presets of the same descriptor:

| class | reads as | built from |
|---|---|---|
| **Spire** | one tall point | one tapering shaft ending in a point |
| **Gate** | one broad frame you can walk through | two shafts carrying a lintel, one shaft rising above it |
| **Broken** | a frame that fell | two shafts of very different height, no lintel, rubble on the fallen side |

Three classes, not three systems: every class compiles through the same code and differs only in which elements it uses and in its bands.

## Proportions

All heights and widths in voxels. Heights are measured from the landmark base course, not from the terrain under it.

| class | height | width | other |
|---|---|---|---|
| Spire | 28 to 34 | base 5 to 7, odd | top narrows to a single column over its last 3 to 5 courses; height at least 4 times the base width |
| Gate | 22 to 28 | total 13 to 19 | shafts 3 to 5 wide; opening 5 to 9; clear height under the lintel at least 14 and at least 60% of the height; lintel 2 to 3 courses; one shaft rises 1 to 3 courses above the lintel, the other does not |
| Broken | standing shaft 22 to 26 | total 13 to 19 | the other shaft truncated at 35% to 60% of the standing one; no lintel; 3 to 8 rubble blocks, every one on the ground course |

- **Every class is at least twice the tallest tree.** Trees top out at 11 voxels, so no landmark is shorter than 22. This is the relation that lets a landmark rise above a canopy line at all.
- **Taper.** No course is wider than the course below it on either axis. A spire narrows; a shaft may keep its width; nothing overhangs. The only lateral offset allowed is the spire's lean.
- **Lean.** A spire may lean by at most one column, applied once, above half its height. Gates and broken frames do not lean.

## Asymmetry

- No compiled landmark is identical to its own mirror image across either horizontal axis.
- A gate's two shafts differ in height by 1 to 3 courses (the rising shaft).
- A broken frame's two masses differ in height by at least 40% of the standing shaft.
- A spire's asymmetry comes from its lean or from its crown erosion; one of the two is always present.

## Material count and palette

Three materials, in the landmark range `224..=255`. Linear RGB, authored for the clear-weather key light, read from one table in `veldwake-procedural`.

| material | linear RGB | relative luminance | role |
|---|---|---|---|
| stone | `0.310, 0.290, 0.265` | about `0.292` | the body of every shaft |
| band | `0.215, 0.203, 0.190` | about `0.205` | darker strata courses and the crown, so the silhouette's top is its darkest part |
| cap | `0.520, 0.500, 0.455` | about `0.501` | the pale plinth course a landmark stands on |

- Order is fixed: `band < stone < cap` in luminance, and adjacent landmark materials differ by at least `0.08`, the style bible's rule for distinct materials.
- **Never mistaken for terrain:** stone differs from rock (`0.439`) and from meadow grass (`0.389`) by at least `0.08`.
- **Desaturated:** HSV saturation at most `0.30`. A landmark is old stone, not paint.
- **Cap is an accent:** at most 25% of a landmark's voxels.
- **Strata:** a band course every 3 or 4 courses on every shaft. The rhythm ties a landmark to the cliffs' horizontal strata without copying them.

## Value relationship and skyline readability

- **The crown is dark and the sky is light.** The style bible puts sky at `0.62` to `0.90`; the band material that forms every landmark's top is at most `0.23`, so an unfogged top is at least `0.39` darker than the palest sky.
- **After fog, from the discovery overlook**, the top of each initially visible landmark must be at least `0.20` darker than the sky directly behind it. This is a presentation measurement and lives in the client, not in the world generator: `a_crown_stays_darker_than_the_sky_behind_it_after_the_fog` composes the band material's own luminance with the fog that survives at the real distance, using the same `Lighting` table the renderer uploads, and compares it against the palest sky in that table.
- **Sky-backed.** The two landmarks visible from the overlook are placed so that the world proxy finds open sky behind their upper silhouette; a landmark seen against a grey cliff loses its outline.

## Recognition distance

A landmark must remain recognisable at **150 world units** in clear weather: at least 3 degrees of visible height and 0.9 degrees of visible width in the real follow-camera frame. Confirmed: the revealed monolith stands 149 units from the landmark that reveals it and subtends `8.4` by `6.5` degrees, and a capture from that viewpoint shows it as a distinct pale vertical above the tree line.

## Relation to vegetation

- **Nothing grows through a landmark.** No trunk, canopy or shrub voxel may occupy any landmark voxel, and no plant's horizontal bounds may intersect a landmark's reservation (its solid footprint grown by its approach apron).
- **A landmark stands in a glade.** The apron keeps approach space around it free of plants, so a person can walk up to the stone and a fight at its foot is not a fight inside a shrub.
- **The discovery overlook is a clearing.** Plants whose anchors fall inside the overlook reservation are not grown. Whether that clearing reads as natural or as a cut circle is a visual-QA judgement, and a circle that reads as artificial is a finding, not a matter for random noise.

## Ground and support

- **Landmarks write only into air above the terrain.** They never replace a terrain voxel, never excavate, and never pave.
- **One support rule.** A landmark's base course sits one voxel above the highest terrain surface under its footprint. A footprint column whose terrain is lower receives foundation voxels, in stone, from just above its own surface up to the base course. Placement requires the terrain under a footprint to vary by at most one voxel, so a foundation is never more than one voxel tall.
- **No floating voxel.** Every landmark voxel is face-connected to the ground through landmark voxels.
- **Nothing to stand on.** No landmark surface is walkable: not a roof, not a lintel, not a rubble top. Terrain remains the only ground.

## Forbidden forms

Rejected by the compiler or by a test, whatever else a descriptor says:

- a flat-topped box, or any course wider than the course below it;
- rooms, interiors, windows, doors in a wall, stairs, battlements, roofs;
- a wall of one material with regular courses and no band;
- a mirror-symmetric instance;
- a voxel not connected to the ground;
- any surface a body could stand on above the terrain;
- a landmark that writes over terrain, water or vegetation.

## How a landmark avoids reading as a Minecraft building

It is a **mass, not a room**: wider at the foot than at the top, with no enclosed space. It is **banded, not tiled**: strata courses break the wall into horizontal bands. It is **eroded only at the top**: the crown loses stones, the foot does not. And it is **alone in a glade**, not one of many.

## Style contract version

`LANDMARK_STYLE_VERSION = 1`. It participates in the world fingerprint beside `LANDMARK_SCHEMA_VERSION`, `LANDMARK_COMPILER_VERSION` and `LANDMARK_PLAN_VERSION`. Changing a rule here that moves a voxel requires bumping it, which invalidates cached chunks.

## What the implementation confirmed, and what it changed

Written after M8 compiled the family and photographed it, so the hypotheses above stop being hypotheses.

- **Every proportion, taper, support, asymmetry and mass rule in this document is asserted by a test against a compiled monolith**, not by inspection. The golden world's three are a spire 28 voxels tall on a 7 x 7 footprint, a gate 28 tall and 13 wide with a 21-column opening and stone starting 23 voxels up, and a broken shaft 22 tall on a 4 x 17 spread.
- **Two rules moved because the compiler proved them wrong.** A spire's lean may not be zero: a symmetric spire is its own mirror, which defeats the asymmetry rule rather than satisfying it, so the lean is always one voxel one way or the other. And the "second mass" ratio is measured over **connected components of grounded columns**, not over columns: measured column-wise, a gate's opening columns carry only its lintel and the ratio reads `0.77` for a gate that is plainly two equal shafts.
- **Erosion runs top-down and never removes a cell that carries another.** Removing outer-ring voxels in place left a corner voxel resting on nothing — a floating voxel is the one thing a weathered edge may not produce.
- **The recognition rule holds with room to spare, and the number is measured.** The revealed monolith stands `149` units from the landmark that reveals it and shows `22.0` voxels of silhouette across a `17`-column span: `8.4` degrees of visible height and `6.5` of width, against the rule's `3` and `0.9`. The two first choices, at 62 and 63 units, show `19.0` by `6.4` and `19.2` by `11.7` degrees.
- **The one-sentence test was run on a person, and it half passed.** Told nothing, the M8 owner looked from the overlook, saw two structures, and called them two different destinations — one a tower, one a broken construction. Distinctness: yes, and that is the half the milestone needed. Class naming: the second one is the **Gate**, and it was not named as a gate. From the overlook its lintel is above the camera frame, so what a viewer sees at rest is two shafts with sky between them; that connection is this document's inference, not the owner's words. Anything that later tries to make a gate read as a gate should start there rather than with the proportions.
- **The owner called the family simple but sufficient, and close to familiar voxel-RPG architecture.** Recorded as KI-036 and deliberately not answered with rules: this document still describes one family with no shape grammar of its own, and giving Veldwake's built world an identity distinct from every other voxel game is a later decision with its own milestone.
- **The crown-against-sky rule is implemented where the document said it belongs** — in the client, over the real fog and the real sky table — and all three landmarks pass it at the distances the composition chose.
- **The palette reads in both weather states.** A capture under overcast keeps the silhouette and the band contrast; the stone is still darker than lit grass and lighter than nothing else in the frame.
- **A first choice does not fit inside the default camera frame at the distance the composition chose**, and that was accepted deliberately after capturing the alternative (KI-032). This document's recognition rules are about the silhouette, not about the frame; the framing decision lives in the milestone document with the two captures that settled it.

## What this document does not cover

Any second landmark family, settlements, roads, ruins with interiors, inscriptions, lighting fixtures, damage or decay over time, and anything a player builds. Those belong to later milestones and must not be invented here.
