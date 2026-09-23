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

The two are meant to be a **sidegrade**: the found weapon connects out to
`3.4477` world units against `2.8835` against a standing body and commits for
`103` ticks against `75` (`108` since the owner's retune), and against the M6
adversary's approach the extra reach buys about as much time as the extra
commitment costs. Before the retune one needed three connected swings and the
other four; since it, both need four and the found weapon's hit is `28` against
`24`.

**That claim did not survive a person.** The M9 owner playtest on 2026-09-22
failed: both weapons were perceived as different and both supported the same
natural strategy — approach, face, attack repeatedly, win — because the M6
adversary never made the player trade anything. Combat initiative
(`COMBAT_INITIATIVE.md`) was built to answer that and is merged; the M9 revisit
measures the two weapons against it. Headless, the relation now looks like the
intended one — the original weapon is the more forgiving after a late read, the
found weapon the faster once the fight is read cleanly — and whether a person
plays them differently was `OWNER PLAYTEST — WEAPON CHOICE MATTERS (REVISIT)`:
**FAIL** on 2026-09-23 — the found weapon read as better overall. The owner then
approved one retune of the found weapon (damage `28`, windup `36` ticks), which
is measured and waits for REVISIT 2. See `M9_MEANINGFUL_REWARD.md`.

Still not built, and still not prejudged: combos, heavy attacks, a second attack
button, parries, blocks, stamina, status effects, elements, criticals, rarity, a
second enemy, target lock, limb hit volumes (KI-022), camera occlusion (KI-025)
and any two-handed grip or second-hand contact. The weapon in M9 hangs off the
right hand exactly as M6's did.
