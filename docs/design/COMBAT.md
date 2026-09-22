# Combat

Status: **Accepted as an early validation priority; mechanics remain Proposed**.

Combat is a core system, not content to add after an “engine” is complete. A combat slice must validate movement, attack, dodge or defense, hit reaction, camera response, animation, impact audio, VFX, enemy telegraphing, and damage readability as one experience.

## Direction

- Action combat takes broad inspiration from Cube World, Zelda, and light Monster Hunter structure without copying any one system.
- Candidate weapon families include sword, greatsword, dagger, spear, bow, staff, shield, gauntlets, axe, and hammer; none are promised for the first slice.
- Weapon geometry may inform reach, mass impression, blocking profile, animation, and resonance, but physical derivation must remain legible and balanceable.
- A single excellent enemy is more valuable than many procedural enemies with poor encounters.
- Procedural creature generation cannot bypass encounter design, silhouette, timing, locomotion, and hitbox validation.

## Early acceptance questions

- Can the player read intent before impact?
- Do animation, camera, VFX, and sound agree on timing and force?
- Are movement and recovery responsive without eliminating commitment?
- Does one weapon create a distinct decision pattern?
- Does the enemy remain interesting after learning its pattern?

Exact controls, stamina, lock-on, damage formulas, classes, PvP, and difficulty modes are TBD.

## What M9 added, and what it deliberately did not

M9 gives the player a **choice of weapon** and nothing else about combat moves.
There are two accepted weapons, both named fixtures: the original longsword M6
built and a found longblade standing at a fixed site in the world. An explicit
verb exchanges them, one at a time, with the site keeping whichever one the
player is not carrying.

The two are a **sidegrade**, which is the design claim worth keeping: the found
weapon connects out to `3.4477` world units against `2.8835` and commits for
`103` ticks against `75`, and against the adversary's approach the extra reach
buys as much time as the extra commitment costs. Neither weapon is safer; one
needs three connected swings and the other four.

Still not built, and still not prejudged: combos, heavy attacks, a second attack
button, parries, blocks, stamina, status effects, elements, criticals, rarity, a
second enemy, target lock, limb hit volumes (KI-022), camera occlusion (KI-025)
and any two-handed grip or second-hand contact. The weapon in M9 hangs off the
right hand exactly as M6's did.
