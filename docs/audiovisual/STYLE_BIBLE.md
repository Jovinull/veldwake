# Style bible

Status: **Accepted for the M4 vertical slice**. Style contract version `1`.

This document is a constraint set, not a mood board. Every rule here is a number, a ratio, a colour, or a rejection criterion that a generator or a shader can be checked against. It covers exactly what the M4 slice needs; it does not describe characters, creatures, architecture, clothing, or animation, because none of those exist yet and inventing rules for them would be decoration.

The target named in `ART_DIRECTION.md` stands: **stylized high-quality voxel fantasy**. Voxel structure stays visible and intentional. Not photorealism. Not Minecraft with a shader.

## Style contract version

`STYLE_CONTRACT_VERSION = 1`.

The version participates in the terrain generator fingerprint. Changing a rule in this document that alters generated geometry or material assignment requires bumping it, which invalidates cached chunks and forces golden fixtures to be reviewed. Changing a rule that only affects shading (a colour, fog density, sun angle) does not change geometry and therefore does not invalidate the cache, but it does require re-reviewing the screenshot fixtures.

## The one-sentence test

A frame passes the style bible when a person can name the landform they are looking at, tell at a glance which way is uphill and where the water drains to, and see that the world is made of deliberate blocks rather than of noise.

## Shape language

- **Terrain is layered, not lumpy.** The eye should read horizontal bands: valley floor, lower slope, shoulder, ridge. A slope that changes gradient continuously with no readable band is wrong.
- **Ridges are asymmetric.** A ridge line has a steeper face and a gentler face. Symmetric cones are rejected.
- **Cliffs are vertical or near vertical**, at least four voxels tall, and expose horizontal strata. A cliff shorter than four voxels reads as noise and should be smoothed into a slope instead.
- **Valley floors are flat enough to walk.** Gradient below roughly one voxel of rise per eight voxels of run across the meadow.
- **Macroform dominates.** At any viewing distance, the largest visible feature must come from the macroform field, never from the detail octaves. If detail is the biggest thing on screen, amplitude is too high.
- **No feature smaller than two voxels** on terrain surfaces. Single-voxel bumps are visual noise and are removed by construction, not by post-filtering.

## Detail frequency by distance

Detail must fall off with viewing distance so silhouettes stay clean.

| band | distance | what is allowed to vary |
|---|---|---|
| foreground | 0 to 48 voxels | surface material bands, individual vegetation, shoreline sediment |
| midground | 48 to 160 voxels | slope changes, cliff faces, tree clusters, water course |
| background | beyond 160 voxels | macroform silhouette and atmospheric value only |

Terrain amplitude budget, in voxels of vertical variation:

| component | amplitude |
|---|---|
| macroform: valley to highland | 40 to 56 |
| ridge field on highlands | 10 to 18 |
| mid detail | 3 to 6 |
| fine detail | 1 to 2 |

Fine detail is suppressed to zero on the valley floor and on water margins, and allowed at full amplitude only on highland and ridge zones.

## Proportions

- The valley is wider than it is deep: floor width at least three times the highland rise, so the region reads as a valley rather than a canyon.
- The river corridor occupies between one twelfth and one sixth of the valley floor width.
- Tree height is between six and eleven voxels; canopy diameter is between four and seven; canopy height is between forty and seventy percent of total tree height. Trees taller than the local cliff height are rejected.
- Trunk width is one voxel, widening to a two-by-two base only on trees at least nine voxels tall.
- Minimum spacing between tree anchors is six voxels. Closer spacing reads as a texture rather than as trees.

## Value and contrast

Value, not hue, carries the reading. The palette is chosen so that a desaturated render still shows the landform.

- Lit terrain surfaces occupy relative luminance `0.35` to `0.72`.
- Shadowed terrain occupies `0.14` to `0.32`. Shadow is never pure black.
- Sky occupies `0.62` to `0.90` and is always lighter than the terrain in front of it, so silhouettes read.
- Water is the darkest large surface, `0.10` to `0.28`, which is what separates it from the meadow.
- Maximum value contrast inside one material band is `0.12`. Larger jumps inside a band read as dirt rather than as form.
- Adjacent distinct materials differ by at least `0.08` in value, so a material boundary is visible without relying on hue.

## Base palette

Linear RGB, authored for the clear-weather key light. Every generator and shader reads these from one table; no colour is written twice.

| material | linear RGB | role |
|---|---|---|
| meadow grass | `0.243, 0.451, 0.208` | dominant valley surface |
| highland grass | `0.290, 0.435, 0.243` | drier, greyer green above the shoulder |
| soil | `0.322, 0.231, 0.149` | the band under every grass surface |
| rock | `0.443, 0.439, 0.427` | exposed cliff and steep slope |
| deep rock | `0.302, 0.298, 0.310` | below the soil column, cooler and darker |
| sediment | `0.612, 0.557, 0.427` | shoreline sand and gravel |
| water | `0.075, 0.204, 0.239` | river and pond body |
| trunk | `0.243, 0.169, 0.110` | tree trunk |
| foliage | `0.173, 0.344, 0.170` | canopy, darker than meadow so trees read against it |
| foliage highlight | `0.259, 0.447, 0.216` | canopy variation, at most thirty percent of canopy voxels |
| shrub | `0.205, 0.341, 0.168` | low vegetation |

Saturation ceiling is `0.55` for ground surfaces and `0.62` for vegetation, measured as HSV saturation. Anything more saturated reads as toy plastic and is rejected.

Water is exempt from the saturation ceiling and is governed by its value band instead. HSV saturation is a ratio against the brightest channel, so it over-reports for any colour this dark: the intended reading is a deep, cool body of water, not a vivid one, and the value band is what enforces that.

## Material rules

- **Grass** only on surfaces whose slope is below `0.55` in rise over run, above the water surface, and not buried.
- **Sediment** within two voxels of the water surface height, and on any surface within three voxels horizontally of water. Sediment always separates grass from water; grass never touches water directly.
- **Rock** on any surface whose slope is at or above `0.55`, which is what makes cliffs read.
- **Soil** occupies one to three voxels directly beneath a grass or sediment surface, thinning to one voxel as slope increases and vanishing entirely under rock.
- **Deep rock** below the soil band, always.
- **Water** fills every cell that is below the local water surface and above the terrain, and nothing else.
- A surface voxel never has a material that contradicts the voxel above it: grass is never buried, water never sits on grass without sediment somewhere on the shore.

## Cliff readability

A cliff is a run of at least four vertically adjacent rock surface voxels. On a cliff face the strata must be visible: the soil band above the cliff lip and the deep-rock band below the soil are both exposed, giving at least two horizontal colour breaks on the face. A cliff that shows one flat colour from top to bottom fails this rule.

## Foreground, midground, background separation

Separation is produced by three devices, in this order of strength:

1. **Atmospheric value compression.** Distant terrain loses contrast toward the fog colour, so background value range narrows to roughly `0.45` to `0.68`.
2. **Detail suppression.** See the distance table.
3. **Hue shift.** Distance shifts terrain slightly toward the sky hue, never toward grey.

A frame where foreground and background have the same contrast range fails.

## Sunlight

- Direction: azimuth `-38` degrees from world north, elevation `34` degrees. Low enough to cast long readable shadows, high enough to keep valley floors legible.
- Clear-weather sun colour, linear RGB: `1.000, 0.945, 0.827`. Warm, not orange.
- Sun intensity: `1.0` clear.
- Ambient is not a constant grey. Sky ambient, from above, linear RGB `0.352, 0.443, 0.561`, cool. Ground bounce, from below, linear RGB `0.208, 0.196, 0.157`, warm and dim. A surface normal blends between them, which is what keeps shadowed faces from reading as dead.
- Ambient intensity: `0.38` clear.

## Shadows

- Directional shadows only, from the sun.
- Shadow softness: a fixed small percentage-closer filter kernel. Hard single-sample edges are rejected as aliasing; a large blur is rejected as fog.
- Shadowed surfaces keep ambient, so shadow darkens to roughly a quarter of the lit value and never to zero. Ambient is the floor; a hard lower bound of `0.12` of the lit value sits under it only so a material dark enough for ambient to vanish still reads as a surface.
- Shadow acne and peter-panning are both rejection criteria, checked in the screenshot review, not tuned by eye alone.

## Fog and atmosphere

- Fog is exponential in distance with a height component: density falls with altitude so valley air reads thicker than ridge air.
- Clear weather: fog colour linear RGB `0.588, 0.690, 0.784`, density such that terrain at 320 voxels retains about half its contrast.
- Fog colour must match the sky near the horizon. A visible seam between fog and sky is a bug.
- Fog is a composition tool for depth separation, not a way to hide the draw distance. If the render distance edge is visible as a wall, the fog is wrong or the radius is too small.

## Sky

- Procedural gradient, no texture assets.
- Three-stop vertical gradient: zenith linear RGB `0.243, 0.408, 0.667`, mid `0.435, 0.600, 0.796`, horizon `0.647, 0.741, 0.824`.
- A soft sun disc and a wide warm halo around the sun direction. No lens flare, no god rays, no clouds in this slice.
- The sky is always lighter than terrain at the horizon so silhouettes separate.

## Water

- Water is a distinct material with its own shading response, not tinted terrain.
- It must show a specular highlight from the sun that terrain does not, which is the primary cue that it is liquid.
- Water follows one continuous monotone downstream surface. A discontinuity introduced at a chunk boundary is a bug, not a style choice. The voxel top is `floor(surface)`, so a gently sloped river can legitimately change one top-water voxel between adjacent columns; this quantization is not a boundary discontinuity.
- Shoreline sediment is mandatory; it is what makes the water read as contained rather than painted on.
- This slice renders water opaque. Transparency, refraction, and motion are deliberately out of scope and are recorded as a limitation rather than pretended away.

## Weather states

Exactly two states in this slice.

| | clear | overcast |
|---|---|---|
| sun intensity | `1.00` | `0.42` |
| sun colour | `1.000, 0.945, 0.827` | `0.820, 0.843, 0.878` |
| ambient intensity | `0.38` | `0.62` |
| sky zenith | `0.243, 0.408, 0.667` | `0.404, 0.435, 0.482` |
| sky horizon | `0.647, 0.741, 0.824` | `0.639, 0.659, 0.690` |
| fog colour | `0.588, 0.690, 0.784` | `0.612, 0.635, 0.667` |
| fog density multiplier | `1.00` | `1.85` |
| water specular | full | reduced by half |

Overcast must remain readable: it raises ambient as it lowers the sun, so the landform still reads without a key light. An overcast frame that is flat grey mush fails.

## Visual noise limits

Rejection criteria, checked in the written screenshot review:

- No single-voxel material speckle on a surface. Material bands are at least two voxels thick in the direction they vary.
- At most thirty percent of canopy voxels use the highlight colour, and highlight voxels form clusters rather than salt-and-pepper.
- No more than three distinct materials visible in any four-by-four voxel patch of terrain surface.
- Vegetation covers at most thirty-five percent of the meadow surface. Denser than that and the landform disappears.
- Repetition visible as a grid at any distance is a failure. Placement grids must be jittered inside their cell.

## What this document does not cover

Creatures, equipment, architecture, settlements, roads, ruins, interiors, weather beyond the two states above, night or dawn lighting, seasons, particles, and colour grading. Those belong to later milestones and must not be invented here.

**Characters have their own contract.** [`CHARACTER_STYLE.md`](CHARACTER_STYLE.md) carries character scale, proportions, silhouette, joint overlap, palette slots, posture, animation timing, and joint ranges, under its own `CHARACTER_STYLE_VERSION`. The separation is not organisational: `STYLE_CONTRACT_VERSION` participates in the terrain generator fingerprint, so a character rule living here would invalidate every cached chunk whenever a proportion moved.

What stays here and applies to characters unchanged: the sunlight direction, colour, and intensity; ambient and shadow behaviour; fog, sky, and the two weather states; the relative-luminance and contrast rules; the HSV saturation ceilings; and the statements that voxel structure stays visible and intentional and that the target is neither photorealism nor Minecraft with a shader. What is per domain: shape language, proportions, detail frequency, and animation.
