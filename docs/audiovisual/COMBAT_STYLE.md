# Combat style contract

`COMBAT_STYLE_VERSION` is `1` and lives in [`crates/combat/src/weapon.rs`](../../crates/combat/src/weapon.rs). It is folded into a compiled weapon's identity fingerprint, so changing a rule below without bumping it is a failing test, which is the intent.

This document does for combat what [`CHARACTER_STYLE.md`](CHARACTER_STYLE.md) does for the body: it states the rules that make a fight readable, in a form a test can check, and it says which rules belong to it rather than to the style bible or the character contract.

## Why this is a separate version

`CHARACTER_STYLE_VERSION` is part of `CharacterIdentity`. Bumping it moves the identity and behavioural signature of every compiled body, which is right when a voxel or a proportion moves and wrong when the thing that changed is how a sword is held. M6's action-motion and weapon rules therefore answer to `COMBAT_STYLE_VERSION`, and the two are deliberately independent: a body is unchanged by a new attack curve, and an attack curve is unchanged by a wider hip.

The named action poses have their own signature, `GOLDEN_ACTION_POSE_SIGNATURE`, separate from M5's `GOLDEN_POSE_SIGNATURE` for the same reason. A change to an attack curve must move the first and leave the second alone, and the reverse.

## What a weapon is

A weapon is a physical object and nothing else. Dimensions, bands, palette, grip and seed are the descriptor; how fast it swings, how far it reaches and how much it hurts are an `AttackSpec`, of which M6 has two executing the same compiled weapon. One weapon means one weapon: the adversary telegraphs for two and a half times as long as the player without that being a second sword.

| rule | value | why |
|---|---|---|
| the blade is rigid voxel geometry | 212 solids, 352 quads | the same mesher, shader and rigid-part path the body uses ([ADR-0004](../adr/0004-rigid-voxel-character-and-analytical-locomotion.md)); a weapon that shaded differently from the hand holding it would be the only thing in frame that does |
| the grip is one transform | `Quat::from_rotation_x(0.90)` | a weapon is held, not skinned; the hand's matrix places it and nothing interpolates between them |
| identifiers | `192..=224` | terrain `64..128`, character `128..192`, diagnostics `1`, `2`, `7`. The client carries the global disjointness test |
| every material pair that shares a face differs in value | seven adjacent pairs, all checked | a silhouette made of one value is a silhouette with no blade in it |
| specular | none, on every weapon material | the style bible gives specular to water and rock; a sword that glinted would out-shine the world it is swung in |

## What an action is

Locomotion is driven by distance travelled, because stride is the thing that is specified and a sliding foot is then a bug rather than a setting. An action is the other category: a swing takes the same time whether the attacker is standing still or running, so its progress is a tick count. [ADR-0006](../adr/0006-action-pose-layer-beside-analytical-locomotion.md) records why those two live side by side rather than one inside the other.

Five actions, and adding a sixth is a deliberate edit rather than a configuration:

| action | what it has to read as |
|---|---|
| `Carry` | armed. A weapon arm driven only by the gait hangs straight down, and a blade long enough to reach then ends up below the ground |
| `Attack` | anticipation, the cut, and a follow through that is slower than the cut |
| `Dodge` | a committed crouching step. No roll, no spin, no invulnerability |
| `Stagger` | a recoil away from where the blow came from, which is what makes a hit read as a consequence rather than as a number |
| `Defeated` | beaten. Knees give, torso folds, blade drops. Not a death animation and not a ragdoll |

### Rules the curves answer to

- **The weapon arm is absolute; the free arm is weighted; torso, head and pelvis are additive.** A body can lean into a swing while still walking, and `Carry` can leave the gait's counter-swing alone.
- **A rigid weapon points where `shoulder_pitch + elbow_flex + wrist_pitch` plus the grip's own pitch says it points.** All three joints turn about the same axis, so the keys are chosen by what that sum does and not by what each joint looks like alone. Carry is `0.90`, the top of the windup `1.95`, the end of the cut `0.20`.
- **A swing sweeps from raised to low through horizontal**, and the phase boundaries are the authoritative tick counts, so the pose keys land exactly where the rules change phase and the blade cannot jump when it does.
- **No nominal action may depend on a joint clamp to look right.** The clamp is a safety net for hostile input. A nominal attack that reaches a joint limit is a curve to fix, and a test asserts the raw curves stay inside every range.
- **A collapse eases over the whole window it is given.** Easing over a quarter of it finished the sag in six ticks of the twenty-four the constant promised and read as a snap.

## What an effect is

Two effects, and a third is a deliberate edit:

| effect | reads as | rule |
|---|---|---|
| impact chips | struck metal | warm, fast, falling, thrown into the half-space the blade travelled |
| telegraph accent | a warning | cold, slow, rising |

They are opposites on purpose. An accent that read as damage would tell the player they had been hit before they had been.

Both are voxel chips, because the game is made of cubes and a soft round billboard would be the only thing in the frame that is not. A chip is a fraction of a character voxel — `0.055` world units, two thirds of one — and it shrinks and cools as it ages, which is the whole of its animation. A chip is emissive and takes no light: a spark that is shadowed is not a spark.

## What a readout is

Sixteen cubes in the world, eight over each head, bright for health held and dark for health lost. It is diegetic because the fight has two bodies and either can be the one in trouble, and a row above each head says which without a legend or a window size. Any health at all keeps one pip lit, so one hit from death never looks like death.

## What the camera may do

- **Only a confirmed hit moves it.** Not a miss, not a successful dodge, not the start of a swing. The type that moves it has no idea what those are.
- **The displacement is capped and decays**, at `0.055` world units and over sixteen ticks — a twelfth of the follow distance, visible in a motion strip and unable to put a body out of frame.
- **It is a function of a tick counter**, so a capture fixture reproduces it exactly and a frozen frame holds it.
- **No lock-on, no field-of-view pulse, no occlusion solving.** The camera clamps to a minimum height above the ground under it and offsets to one side; that is all (KI-025).

## What this document does not cover

Multiple weapons, weapon switching, sheathing, armour, equipment or any visual system for them; a second enemy or any creature that is not this adversary; blood, dismemberment or damage states; combos, parries, blocks or any attack this milestone does not have; music, reverb or ambience; and any HUD beyond the sixteen cubes above. Those are later work and must not be invented here.
