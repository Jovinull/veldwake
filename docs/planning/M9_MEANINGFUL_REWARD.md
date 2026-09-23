# M9 — Meaningful Reward

Status (M9 revisit, 2026-09-23): **ported onto combat initiative on
`feat/m9-meaningful-reward-revisit`; technical gates and the headless pre-gate
PASS; ready for `OWNER PLAYTEST — WEAPON CHOICE MATTERS (REVISIT)`, not yet
run.** Not merged and not complete. See [*M9 REVISIT*](#m9-revisit) at the end;
everything between here and there is the original M9's record, unchanged.

Original status (2026-09-22, frozen branch): **M9 PRODUCT GATE: FAIL — mechanical sidegrade exists, but the current
encounter does not make weapon choice meaningfully affect play.** The
implementation stays on `feat/m9-meaningful-reward` and is not reverted, not
merged and has no pull request. It is **not ready for branch QA as a completed
milestone**, and it is **not being expanded further**: the redesign
investigation that followed the owner gate is closed with the conclusion that
**M9 needs a broader combat slice** (see *Redesign investigation — closed*
below). Base: `docs/post-m8-handoff` at
`658ebfbd618b1d7387eee7890d11af53b5f8e045`, which is `main` at merge commit
`ee35f62f97afbe3d001a27a576e9bae21e77c4d2` plus one documentation commit.

**The technical result and the product result are different results and both
stand.** Every implementation claim in this document was verified and remains
verified: the found weapon reaches further, commits longer, exposes a longer
whiff, opens a far wider standoff band, and the exchange, the armament, ARM-001,
the M6 locks and the untouched world all hold. What failed is the product
hypothesis the milestone existed to test, and it failed for a reason that is
about the *encounter* rather than about the weapons.

M8 proved a person will choose a direction because of what is standing at the
end of it. What it did not give that person was a reason to go beyond arriving.
M9 set out to be the first proof of **Mastery** in this project — reaching a
place gives you something that changes how you play — and built the whole of it.
The owner's session says the first half happened and the second did not: the
thing you are given is measurably and perceptibly different, and the way you
play is not.

It is not a progression system, not loot infrastructure, not an inventory and
not persistence.

## Scope, as accepted by the owner

> "Chegar a um lugar pode me dar algo que muda de verdade a maneira como eu
> jogo?"

From the discovery overlook the two directions now mean different kinds of
thing. One ends in a fight. The other ends in a weapon standing in the gate that
the player can take, carry into that fight, and put back.

## Owner decisions

| decision | ruling |
|---|---|
| reward form | a **found world object** |
| the object | a **second weapon**, because weapon and combat already exist and a semantic reward can be tested without building another system |
| host | the **Gate**. The adversary stays at the Spire; the Broken monolith stays M8's later reveal |
| carried count | **one**. No inventory, no slots, no equipment screen |
| interaction | an **explicit verb**, `E` |
| swap model | **fixed-site exchange**. The site holds one weapon, the player holds the other, interact swaps them, returning reverses the choice |
| defeat semantics | a combat defeat does **not** revoke the choice. The encounter resets; the armament does not |
| persistence | **out**. Nothing is written to disk |
| style version | `COMBAT_STYLE_VERSION` stays `1`. The M9 profile is additive under the same grammar |
| owner gate | **OWNER PLAYTEST — WEAPON CHOICE MATTERS**, and it may fail |

## What the branch implements

| part | where | what it is |
|---|---|---|
| attack envelope | `crates/combat/src/reach.rs` | **moved out of `fixture.rs` into production.** Where a swing's blade gets while it can connect, measured through the real pose path. The encounter's aim rule, the fixtures and `combat-probe` all call this one function |
| armament | `crates/combat/src/armament.rs` | `WeaponVariant::{Original, Found}`, `ArmamentState` (one value, the site derived), and `RewardSetup` |
| the exchange rule | `crates/combat/src/encounter.rs` | `exchange_weapons`: four refusals, one swap, one re-pose, one event |
| weapon selection | `crates/combat/src/encounter.rs` | `weapon_of(side)` and an armament-aware `attack_spec(side)`; every weapon-dependent path routes through them |
| derived aim range | `crates/combat/src/encounter.rs` | `aim_range_for`, floored at the historical `AIM_ASSIST_RANGE` |
| the world's contribution | `crates/combat/src/encounter.rs` | `WorldContact::weapon_exchange_site`, one position and one constructor |
| the verb | `apps/client/src/input.rs` | `CombatAction::Interact` and `CombatLatches`, latched on the key-down edge |
| the site | `apps/client/src/reward.rs` | the anchor derived from the gate's own opening, its validation, its world matrix and `REWARD_BEHAVIOR_SIGNATURE` |
| rendering | `apps/client/src/renderer.rs` | one placed-weapon slot: uploaded once, given a matrix per frame, drawn in the world and shadow passes |

**`crates/procedural` is not modified. Not one line.** `git diff main -- crates/procedural` is empty.

## The exchange, as the rules see it

An interact intent reaches `Encounter::step` after `start_action`, which is
where the input priority is decided: **attack outranks dodge outranks
interact**, and a lower-priority press is consumed for that press rather than
held to fire later. The rule then refuses, in order:

1. no reward configured — every M6, M7 and M8 encounter, and the reason none of
   them changed;
2. the world offered no exchange site;
3. the player cannot act — attacking, dodging, staggered, frozen or defeated;
4. the site is outside `interact_radius = 1.75` world units, by planar
   centre-to-site distance.

On success it swaps the armament, **re-poses the player** so the next tick's
sweep starts on the weapon now in the hand, counts it, and publishes exactly one
`CombatEvent::ArmamentSwapped`. A refusal is counted and silent: no prompt at
the boundary, no icon, no floating text, no HUD marker anywhere.

## The two weapons

Compiled by the existing compiler; every number measured rather than declared.

| | original (M6) | found |
|---|---|---|
| descriptor | blade `14×4×2`, guard `6×2`, grip `5×2`, pitch `0.90`, keen-steel | blade `20×6×2`, guard `8×2`, grip `6×2`, pitch `1.10`, dark-iron |
| identity | `0x084b_f386_500b_b0e4` | `0xa09f_cd9b_fa45_d87a` |
| geometry | `0x8a6b_18ed_d4a2_b879` | `0x0723_e9dd_aeff_d4d5` |
| solids / quads | `212` / `352` | `392` / `576` |
| blade length | `1.1667` u | `1.6667` u |
| reach from hand | `1.5833` u | `2.1667` u |
| carried tip height | `+0.2755` u | `+0.3593` u |
| swing active band | `0.7660`–`2.0561` u | `0.6776`–`2.6203` u |
| connects out to | `2.8835` u | `3.4477` u |
| windup / active / recovery | `22 / 12 / 41` ticks | `31 / 14 / 58` ticks |
| total lock | `75` ticks, `0.6250` s | `103` ticks, `0.8583` s |
| damage | `24`, four swings to fell `96` | `32`, three swings |
| step-in / knockback | `0.35` / `0.35` | `0.30` / `0.50` |

**It is a longblade, not a two-handed sword.** The weapon hangs off `HandR`
exactly as the original does, the free hand does nothing, and M9 adds no
second-hand contact and no two-handed pose.

**The grip pitch is a measured correction.** At the inherited `0.90` a
twenty-voxel blade's tip sits at `-0.029` world units in the carry pose — below
the terrain. The sweep over blade length and pitch:

| blade | 0.90 | 1.00 | 1.10 | 1.20 | 1.30 |
|---|---|---|---|---|---|
| 16 | `+0.145` | `+0.305` | `+0.473` | `+0.648` | `+0.827` |
| 18 | `+0.058` | `+0.233` | `+0.416` | `+0.607` | `+0.802` |
| **20** | **`-0.029`** | `+0.161` | **`+0.359`** | `+0.565` | `+0.777` |
| 22 | `-0.116` | `+0.088` | `+0.302` | `+0.524` | `+0.752` |

## The sidegrade, measured

Every number below comes from the repository's own code: `reach::attack_envelope`
over the real pose path, and the real `Encounter` tick loop.

**Reach, in the authoritative loop.** A sandbox drill stands the player at a
fixed separation and swings once; the sandbox adversary's aggro radius is `0.30`,
so it never wakes, never moves and never swings, and only the player's weapon is
in play. `the_found_weapon_connects_from_further_away_in_the_real_loop` asserts
the difference.

| | connects out to | furthest gap that actually hit |
|---|---|---|
| original | `2.8835` | `3.02` |
| found | `3.4477` | `3.58` |

**The standoff band — the decisive asymmetric number.** The adversary keeps the
original weapon whatever the player holds (ARM-002), so its reach is fixed:

```text
adversary connects out to                2.7439
adversary strike_range / min_range       2.2000 / 1.4000
player with original connects out to     2.8835   band 0.1396
player with found    connects out to     3.4477   band 0.7038
ratio                                                 5.04x
```

The found weapon multiplies by five the width of the ring in which the player can
land a blow the adversary cannot answer.

**And the price is exactly the reach.** The adversary approaches at `2.40` u/s
and commits at `2.20`:

| | swing from | ticks for the adversary to reach strike range | attack lock | margin |
|---|---|---|---|---|
| original | `2.8835` | `34` | `75` | **`-41`** |
| found | `3.4477` | `62` | `103` | **`-41`** |

The extra reach buys `28` ticks of closing time and the extra commitment costs
`28` ticks of lock. The exposure margin is identical: neither weapon is safer,
they are differently shaped.
`the_reach_the_found_weapon_buys_costs_about_what_the_commitment_does` asserts
that as a relation with an eight-tick tolerance, not as a bit-level lock.

**Commitment budget to fell a `96`-health adversary:**

```text
original  4 swings x  75 ticks = 300 ticks (2.500 s); one whiff exposes  75 ticks
found     3 swings x 103 ticks = 309 ticks (2.575 s); one whiff exposes 103 ticks
```

Total commitment differs by `3%`; per-mistake exposure by `+37%`.

**Timing cost alone, asymmetric-correct** — the original weapon on both sides,
so only the player's spec differs, run through `GOLDEN_SCRIPT`:

| player spec | swings | hits | whiffs | player lost | adversary lost | min player hp |
|---|---|---|---|---|---|---|
| original | 7 | 5 | 2 | 0 | 1 | `24` |
| found timing | 6 | 4 | 1 | 0 | 1 | `24` |

The chosen spec wins the reference fight with the same health left, landing one
fewer swing. A heavier candidate (`0.30 / 0.12 / 0.55`) cost `18` of `96` health
and was rejected; damage `36` was rejected because it also fells in three swings
and only makes the weapon stronger.

**Dodge interaction.** The player's weapon has no part in the adversary's sweep,
so dodging the adversary is unchanged by the player's choice, by construction.
What the choice changes is dodge *availability*: a dodge is refused for the whole
attack lock, so each found-weapon swing removes `103` ticks of it against the
original's `75`, measured against an adversary telegraph of `54` ticks.

## The site

Derived from the gate the world already built, with **no search of any kind**.
M8's own learning is that a gate's tallest column is the lintel over its opening,
so `LandmarkInstance::crown_column` already names the centre of the passage. M9
reads that field, steps one column along the gate's span axis, and asks the
client's own ground query for the height.

```text
gate        origin (-8, 52)  base_y 19  crown (-7, 58)  bounds x[-8,-6] z[52,64]
opening     z[55,61] x[-8,-6] = 21 columns, 24 voxels of clear air under the lintel
anchor      column (-7, 57)  world (-6.50, 57.50)  ground 19.0
terrain     face 18 across the footprint and a 3-column apron, spread 0, no water, no plants
```

Validated against the production adapters, not against a copy of them: dry,
level to a voxel over the interaction radius, clear of final vegetation, outside
every landmark keep-out, standable, and reachable on foot from the route start —
which is itself `62.7` world units away, so a session cannot begin inside its own
reward.

**Why it is one column off centre, and what that cost.** The first
implementation put the weapon at the exact centre of the opening. That is also
the line a body walks through a gate and the line the follow camera looks down,
and the driven captures were unambiguous: at three world units the planted
weapon was a grey slab between the camera and the body, occluding the torso and
both legs, and the only thing in the frame that read as a sword was its shadow.
One column to the side fixed it completely — still inside a seven-column
opening, still framed by both shafts, `1.0` world unit off the walking line,
which is outside the widest body's `0.86` keep-out radius and inside the `1.75`
interaction radius. The before and after captures are in the QA section.

**The object is not solid.** It has no keep-out, no collision and no entry in
the traversal veto, so `TRAVERSAL_RULE_VERSION` did not move and the gate is
exactly as walkable as M8 left it. A body walks through the weapon, which is the
honest consequence of adding no object collision system.

## Rendering: three weapon instances, not two

The exchange pair is the player's and the site's. **The adversary carries an
independent original weapon that is never part of any exchange**, so a running
session holds three instances and exactly two of them are exchangeable. Saying
"there are two weapons in the world" would be false and the client's own report
says so: `weapons=3`.

Nothing is re-uploaded when the player swaps. Both actors upload the original
weapon's mesh and one placed slot uploads the found one; each frame the armament
decides which of the two receives the player's hand matrix and which receives the
fixed site's. Geometry uploads once; a frame writes transforms.

```text
armament = Original:  original mesh <- player hand     found mesh <- site
armament = Found:     original mesh <- site            found mesh <- player hand
```

## Audio: measured, and deliberately untouched

The client derives the impact voice from the event:
`intensity = damage / victim_max_health * 4.0`, clamped to `[0, 1]`. With
`adversary_health = 96` the original's `24` gives exactly `1.000` and the found
weapon's `32` gives `1.333`, which clamps to `1.000`. **Both weapons produce an
identical hit voice**, and `weight` comes from the victim's body rather than the
weapon, so it does not move either. The whiff voice is a constant.

M9 records this and adds no audio work: no new system, no re-scaled mapping, no
weapon dimension on `VoiceParams`. It is KI-037.

## Evidence

### Gates, on the audited Windows 11 / D3D12 host

| gate | result |
|---|---|
| `cargo fmt --check` | **PASS** |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **PASS** |
| `cargo nextest run --workspace` | **PASS** — 799 tests, 796 by default and 3 `#[ignore]`d |
| `cargo nextest run --workspace --run-ignored all` | **PASS** — 799 of 799 |
| `cargo test --workspace --doc` | **PASS** — 0 doc tests, as before |
| `cargo doc --workspace --no-deps --all-features` | **PASS** — no warnings |
| `cargo metadata --locked` | **PASS** |
| `cargo deny check` | **PASS** — advisories, bans, licenses and sources ok |
| `cargo audit` | **PASS** — 226 crate dependencies scanned, exit `0` |
| driven client runs | **PASS** — eight sessions, every capture opened and read |

KI-008's bounded `LNK1104` retry was needed twice and is the linker lock, not a
test result; both times the next plain run passed.

### Locks

Unchanged, byte-identical, and re-measured on this branch:

```text
world fingerprint              0xf3c0377f402efbf8
TERRAIN_BEHAVIOR_SIGNATURE     0x2d593291f0529f23
GOLDEN_WORLD_BEHAVIOR_SIGNATURE 0x2d593291f0529f23
LANDMARK_BEHAVIOR_SIGNATURE    0xe97bee8f85737e95
GOLDEN_REGION_SIGNATURE        0x0cd95da61656b11b
TRAVERSAL_RULE_VERSION         2
GOLDEN_ROUTE_SIGNATURE         0xa06c9d72a8824848
ADVERSARY_COLUMN               (-135, 62)
GOLDEN_WEAPON_GEOMETRY_FINGERPRINT 0x8a6b18edd4a2b879
GOLDEN_WEAPON_IDENTITY_FINGERPRINT 0x084bf386500bb0e4
GOLDEN_ENCOUNTER_SIGNATURE     0x64157522d2535658
every M5 character lock
```

New, and locked here for the first time:

| lock | value | covers |
|---|---|---|
| `FOUND_WEAPON_GEOMETRY_FINGERPRINT` | `0x0723_e9dd_aeff_d4d5` | the found weapon's compiled surface, palette-independent |
| `FOUND_WEAPON_IDENTITY_FINGERPRINT` | `0xa09f_cd9b_fa45_d87a` | its descriptor plus both contract versions |
| `FOUND_ENCOUNTER_SIGNATURE` | `0x735a_9961_f661_8e7d` | a reference fight held with the found weapon |
| `REWARD_BEHAVIOR_SIGNATURE` | `0x0f08_fbf7_08e3_206d` | the anchor, its ground, the radius, both identities, both specs, the initial armament |

`FOUND_ENCOUNTER_SIGNATURE` uses **the same trace format** as
`GOLDEN_ENCOUNTER_SIGNATURE` and is not expected to equal it: it is a different
fixture doing different work with a different weapon, and the two are never
compared for equality.

`REWARD_BEHAVIOR_SIGNATURE` was re-locked once inside this milestone. **Old**
`0x0815_132f_1a6b_7572`, **new** `0x0f08_fbf7_08e3_206d`, **why**: the anchor
moved one column, from `(-7, 58)` to `(-7, 57)`, for the visual reason recorded
above. Nothing else in it changed.

**No M9 lock is folded into any chunk-cache key.** A cache entry answers whether
the source produces the same chunk bytes, and a weapon standing in a gate changes
no chunk byte.
`resolving_a_reward_writes_no_voxel_into_the_world` proves it by generating the
chunks around the gate from a generator the reward was resolved against and one
it was not, and comparing them.

### The authoritative loop

- an encounter with no reward cannot exchange, refuses nothing and counts
  nothing — two hundred presses, no event;
- an exchange needs a world that offers one, and is refused by range, by a
  running action, and by a defeated body;
- **attack outranks dodge outranks interact**, and a press consumed by a swing
  does not fire later as a stale latch — asserted over two hundred following
  ticks;
- the adversary holds the original weapon in every armament state, and its
  attack spec never moves;
- the player's weapon, spec and swept blade all follow the armament;
- **the M6 pair keeps the historical aim range exactly**: both sides resolve to
  `2.9000`, because both connect out to less than it and the derivation is
  `max(AIM_ASSIST_RANGE, connects_out)`. The found weapon resolves to `3.4477`;
- an exchange is its own inverse, and a new encounter starts over.

### ARM-001, in the real client

Driven session, found weapon taken at the gate, carried to the spire, player
defeated:

```text
php=96 ahp=96  w=found site=original   d=2.59
php=78 ...     w=found site=original
php=60 ...     w=found site=original
php=42 ...     w=found site=original
php=6  ...     w=found site=original   d=1.88
-- defeat hold, encounter reset --
php=96 px=-68.5 pz=49.5 facing=-101.14  w=found site=original
```

The body is back at exactly its configured start, its health and facing are
restored, the brain is dormant again — and the armament is untouched. The
victory case under `Remain` is headless evidence
(`a_victory_leaves_the_armament_alone_too`); the defeat case is both.

### The exchange in the real client

One session, four exchanges, zero refusals:

```text
walk to (-6.07, 58.59)  E -> player=found  site=original  interacts=1
walk through the gate to x=0.73, carrying the found weapon
walk back to (-6.69, 58.50)  E -> player=original site=found  interacts=2
E again -> player=found site=original  interacts=3
walk through the gate again to x=0.05
```

Every failure counter zero throughout: `upload_failures`,
`presentation_commit_failures`, `commit_invariant_failures`, `gaps_closed`,
`cache_read_failures`, `audio_device_errors`.

### Hits landed with the found weapon

A blind driver cannot aim — the finding M7 and M8 both recorded, and four driven
runs reproduced it exactly: `player_swings=23..30`, `player_hits=0`,
`player_aim_assists=0`, because the script's facing follows its own walk and the
adversary repositions out of the cone.

A **closed observe-decide-act loop** — the client's own report tailed from a file
between actions, the bearing to the adversary recomputed each iteration, the
camera turned to it — landed hits: `player_swings=9`, `player_hits=2`,
`player_aim_assists=3`, and the adversary's health went `96 → 64` on each, which
is the found weapon's `32` damage and not the original's `24`.

### What it looks like

Eight driven sessions on the audited host, `1920 x 991`, release client,
`m4-golden`, exit code `0` every time.

| capture | what it shows |
|---|---|
| the approach at 13 and 9 units | the planted weapon framed between the gate's two shafts, read as a sword with no prompt, icon or glow |
| **the centred anchor at 3 units** | **the visual failure**: a grey slab between camera and body, torso and both legs occluded, only the shadow reading as a sword |
| **the corrected anchor at 3 units** | pommel, grip, crossguard and blade going into the ground, clear of the body, framed by the left shaft |
| at the site, before and after `E` | the carried blade changes: the found weapon rises well past the shoulder and the original stands planted where the player is |
| through the gate, both armaments | the body walks through the opening with the object present, before and after the exchange |
| in the fight | both combatants, visibly different weapons, both health readouts |

### Performance, on the audited host

| measure | before M9 | after M9 |
|---|---|---|
| `LandmarkPlan` derivation, release | `506`–`586` ms over five runs | `262`–`286` ms over five runs |
| character GPU | `2` actors, `2` weapons, `985,648` bytes, `34`/`34` draws | `2` actors, **`3` weapons**, `1,091,712` bytes, `35`/`35` draws |
| dynamic upload per frame | `2,720` bytes | `2,800` bytes |
| FPS at the gate | — | `59.999`–`60.008`, vsync-bound |
| render wall | — | mean `10.56`–`10.84` ms, max `14.7`–`16.3` ms |
| time to idle | `63,032` ms (M7) | `66,805` ms |

The derivation numbers are the honest and slightly embarrassing kind:
`veldwake-procedural` was not modified, so the code is identical and the
difference is host load — the "before" set was taken immediately after a full
`cargo nextest`. It is KI-035's own warning about single quiet measurements,
reproduced by accident. The placed weapon costs exactly `106,064` bytes
(`92,160` vertex + `13,824` index + `80` uniform) and one draw in each pass.

## OWNER PLAYTEST — WEAPON CHOICE MATTERS: FAIL

2026-09-22, by the repository owner, who had read the M9 architecture report
beforehand and therefore knew what the reward was and where it stood. **No
discovery, visibility or composition claim in this milestone is owner evidence**
— M8 owns discovery; this gate asks only whether the choice matters.

The owner played both weapons in combat.

**What the owner did first, unprompted.** Approached the adversary, stood facing
it, attacked repeatedly, and defeated it — then did essentially the same thing
with the other weapon.

**What the owner reported after a second, deliberately difference-focused
session:**

- the **difference in distance and reach was perceptible**;
- the **difference in the attack's timing and motion was perceptible**;
- but the combat stays simple enough that neither changes the strategy
  significantly;
- in the owner's own words, *"sempre só acaba acertando ele de qualquer jeito"* —
  you just end up hitting him anyway.

**The reading, stated precisely.** The two weapons are mechanically and
perceptually distinguishable, and the owner distinguished them. The gate fails
because **both weapons support effectively the same natural combat strategy**:
the reward changes measurable weapon properties without yet changing the
player's meaningful combat decisions.

### What this result is not, and must never be written as

Four statements would be false and are recorded here so no later document
reaches for them:

- **not** "the weapons feel identical";
- **not** "the found weapon failed to communicate its longer reach";
- **not** "the found weapon is universally superior";
- **not** "the owner could not perceive the timing difference".

The owner perceived the reach difference and the timing difference. The failure
is downstream of perception.

### The product hypothesis that failed

> "Chegar a um lugar pode me dar algo que muda de verdade a maneira como eu
> jogo?"

**FAIL.** Reaching the place gives the player something measurably different to
hold. It does not yet give them a different way to play, because the encounter
does not ask them to trade anything.

### The finding this produces

**The current M6 encounter may be too permissive to expose a weapon sidegrade.**
The natural strategy the owner arrived at without being taught it is: stand in
front of the adversary, attack repeatedly, and the adversary dies. Against that
strategy, reach, commitment, whiff exposure and dodge availability all exist
technically and can be largely ignored.

This is **not** evidence that the combat system is bad. It is evidence that this
particular encounter does not demand the dimensions M9 traded. The distinction
matters: M6's own gates — readability, telegraph, impact, camera — were passed by
the same owner and are not reopened by this result. Recorded as KI-039, with no
solution prescribed and none chosen.

### What was deliberately not changed in response

Nothing. No descriptor, no attack spec, no damage, reach, windup or recovery, no
aim assist, no enemy tuning, no second verb, no second enemy, no stamina, no
combos and no difficulty. Making the found weapon artificially weaker to
manufacture a contrast would answer a different question than the one that
failed. The evidence was persisted first; the redesign analysis that followed
changed no production value either, and is recorded below.

## Redesign investigation — closed

**Conclusion: M9 needs a broader combat slice. The owner accepted this outcome
on 2026-09-22, and M9 is not being expanded to build it.** The current
encounter vocabulary cannot make the weapon sidegrade change a player's
decisions. The weapons, the exchange and every technical claim above are
unaffected, and nothing in the repository was changed by the investigation
except this documentation.

### Three kinds of evidence, kept apart

- **Owner evidence** is the playtest above and nothing else: the natural strategy
  was approach, face the adversary, attack repeatedly and win, with both weapons,
  and the owner perceived both the reach difference and the timing difference.
- **Repository evidence** is everything in *The sidegrade, measured* and
  *Evidence*: tests, probes, locks and driven client runs on this branch.
- **Scratch experimental evidence** is everything in this section below this
  list. It was produced by throwaway harnesses in a `git archive` copy of
  `6d3524065684f5f2f128b6d8df1cb9f55bd8e988` outside the repository, with
  experimental hooks that default to off. **None of it is a repository test, none
  of it is reproducible from the repository alone, and no rule it tried is
  approved for production.** With every hook off, the copy reproduced
  `GOLDEN_ENCOUNTER_SIGNATURE` `0x6415_7522_d253_5658`,
  `FOUND_ENCOUNTER_SIGNATURE` `0x735a_9961_f661_8e7d` and all four weapon
  fingerprints exactly.

The scratch method: the owner's behaviour turned into a policy (walk at the
adversary, attack whenever the body can act, never dodge), five intentional
policies for contrast (enter-hit-exit, edge-poke, hit-and-out, react-dodge,
edge-anticipate), a human approximation for all of them (perception lag of `18`
to `32` ticks, a distance misjudgement of `±0.25` world units, observable state
only), flat-ground runs as diagnosis, and the **real golden traversal as the
gate**: `TerrainGround`, `TerrainWalkability` and the real exchange site, walked
from the gate along three validated approaches — the overlook route, gate
direct (arriving from the north) and gate south — six seeds each, with any run
where the adversary stalled against landmark stone classified as KI-038 and
discarded. No run in the gate was discarded.

### What the investigation found, in order

1. **Against the owner's strategy the adversary never reaches an active window.**
   Baseline, flat ground: sixteen of sixteen fights won with zero damage taken,
   with either weapon, and the adversary's swings all interrupted in windup or
   cut off by its defeat. A deliberate whiff at any distance from `3.0` to `5.0`
   units cost nothing. On the golden traversal the same strategy took zero
   damage with both weapons.
2. **Unconditional interruption closes the loop.** Every player hit replaces the
   adversary's action with a stagger at any windup tick. Its telegraph is `54`
   ticks; the player's blade connects about `24` ticks (original) or `33`
   (found) after a swing starts. For the original weapon the loop cannot be
   broken at all: its `75`-tick cycle is shorter than the `36` ticks of stagger
   it inflicts plus the `54`-tick telegraph (`90`). After every hit the brain
   repositions and then walks back into reach.
3. **The weapons already differ under pressure; the encounter rarely applies
   any.** Answering a telegraph with a swing wins if started within `36` ticks
   with the original weapon and `24` with the found one. The adversary commits
   at `2.2` units — inside both reaches (`2.8835` and `3.4477`) — so the
   standoff band measured above against its `2.7439` reach never occurs in play.
4. **A commit point on the adversary's telegraph (K) is necessary and
   insufficient.** Making the late windup uninterruptible breaks part of the
   loop. On the current windup curve tick `30` is mid-rise at close to peak
   angular speed and visually arbitrary; the legible points (`48`–`54`) come
   after the original weapon's re-hit and restore the loop. A rise-and-hold
   windup that makes the commit visible was modelled in scratch only and has not
   been shown to the owner.
5. **Punishing a visible recovery (P) creates the style divergence and is
   insufficient.** With K and P together the best intentional policy differs by
   weapon — enter-hit-exit for the original, edge-anticipate for the found — but
   the owner's strategy still wins.
6. **Damage and health cannot fix a fight the adversary does not land.**
   Adversary damage `48`, or `32` with `144` adversary health, made the owner's
   strategy lose on flat ground; on the golden traversal it still won six of
   six with the original weapon in every approach.
7. **A shorter brain `Recover` pause does not help.** After the adversary lands,
   the player is free at `+43` ticks and a repeated swing connects at `+67`
   (original) or `+76` (found), while the adversary is still inside its own
   `72`-tick attack recovery (free at `+86`) — before the brain's pause even
   starts. `recover_seconds` at its minimum of one tick changed nothing.
8. **A shorter adversary attack recovery does not help.** `0.43` s and `0.18` s,
   derived from that timeline, left the owner's strategy winning six of six in
   every approach and made the intentional policies worse.
9. **Threat-aware approach (T) does not help.** Stopping the adversary's
   approach when continuing would walk into a visible swing — after a
   `24`-tick reaction, with the threat reach derived from `attack_envelope` and
   no branch on weapon identity — fired `0.2`–`0.4` times per fight against the
   owner's strategy.
10. **T-Hold and T-Reposition fail the same gate.** Holding still and falling
    back into the existing `Reposition` both left the owner's strategy a six of
    six win in every approach, created no stalemate and did not increase KI-038.
11. **The player is the body closing the distance.** The owner's strategy walks
    at `3.4` world units per second while attacking; half of its hits land on an
    adversary already retreating in `Reposition` at `1.8`.
12. **Ordinary adversary movement cannot escape.** The brain scales every
    movement intent against the shared `MovementSpec` speed and clamps it there,
    so the fastest an adversary can move with the existing intents is the
    player's own `3.4`. With reposition raised to that ceiling the owner's
    strategy still won every flat fight. The adversary has no action that can
    break the pressure of a pursuing attacker.

**The stop condition.** Across every bounded redesign tried, the owner's
strategy remained a reliable six-of-six victory in every ordinary golden
approach with both weapons.

### Families tested and rejected as the next step

Adversary damage and player or adversary health; aim-assist range; stagger
length; telegraph length; strike range on its own; a hold band; the brain's
`Recover` duration; the adversary's attack recovery; commit-point semantics
(K); recovery punishment (P); threat-aware approach (T), both as T-Hold and as
T-Reposition. K and P each produced a real effect and neither is approved: they
are findings, not rules.

### The capability that is missing, and what is not decided

The missing category is **a broader adversary capability for defending against
and managing pressure**. One plausible first experiment for a future milestone
is a burst evasive action — possibly the existing `Dodge` action used by the
adversary — because ordinary movement cannot open separation from a pursuer.
**That is a hypothesis, not a selection.** No enemy dodge has been tested, and
nothing here chooses one.

### What happens next

**Not M9 implementation.** The next step is a separate combat milestone
proposal, started from `main` rather than by growing this branch, with a product
question independent of the reward: *can the adversary force the player to
respond to pressure rather than win by holding forward and repeatedly
attacking?* This branch would later consume that capability and run its own
owner gate again. No milestone number is assigned.

## Accepted behaviour

- The planted weapon is **non-colliding**: a body walks through it. There is no
  object collision system and adding one is a decision, not an extension.
- The weapon stands **one column off the centre** of the gate's opening, for the
  measured reason above.
- Both weapon meshes are resident and drawn every frame, in two of three places.
- A world that composed no gate has no exchange at all, exactly as a world
  without landmarks has no discovery (KI-033). Nothing is placed where the
  composition does not hold.
- The exchange is reversible by returning to the site, and the choice does not
  survive a process restart.
- The two weapons sound identical (KI-037).

## Non-goals

Recorded once so the rest of the document can refer to them. No persistence or
save; no inventory, slots, bag or stash; no equipment screen, stats UI, XP,
levels, skill tree, currency, crafting, durability, rarity or loot tables; no
enemy drops; no second enemy, second reward site or third weapon; no third
actor, entity model or ECS; no object physics or dropped items; no world editing
or voxel mutation; no pickup prompt, item-name UI or navigation UI; no quest,
dialogue or NPC; no traversal verb, day/night, ecology, history or economy; no
camera collision fix; no audio redesign; no streaming, LOD or render-radius
change; no generalized interaction system, universal item framework or
generalized equipment system; **and no two-handed pose or second-hand contact.**

## What this agent could not verify

- ~~**Whether the two weapons feel different to a person.**~~ **Answered by the
  owner gate above**: they do, in both reach and timing. What the gate found
  instead is that the difference does not change how the encounter is played.
- **Whether the found weapon reads as belonging in this world or as generic
  loot.** A capture can show the silhouette; it cannot answer the question.
- **Whether the longblade's scale is imposing or absurd** at `2.50` world units
  against a `2.44`-unit body. The captures are in the record and the judgement
  is not this agent's.
- **A player victory in the real client.** The closed loop took the adversary to
  `64` twice and never to `0`; the victory-preserves-armament case is headless
  evidence only.
- **The overlook clearing, the third landmark, or anything M8 left unsettled.**
  Untouched here.

## Findings recorded rather than fixed

- **The adversary has no navigation, and M8 gave it a wall.** Approached from
  the east, the adversary walks straight into the spire's keep-out and stops:
  `brain="approach"` with its position frozen and the distance stuck at `8.72`
  units for the rest of the session. That is M6's explicit no-navmesh non-goal
  meeting M8's landmark keep-out; nothing in M9 caused it and nothing in M9
  addresses it. KI-038.
- **A blind driver still cannot aim.** Four runs, `0` hits, `0` aim assists.
  Only the closed loop landed anything. The same finding M7 and M8 recorded.
- **The camera meets the planted weapon.** The follow camera passes through the
  object at close range, as it does through gate stone. KI-025, unchanged.
- **KI-039: the encounter does not demand what the sidegrade trades.** The
  owner's own session is the evidence, and it is the reason the product gate
  failed. The bounded redesigns tried afterwards are exhausted as a next step;
  a broader adversary capability is required and none is chosen.

## M9 REVISIT

Status: **M9 REVISIT PRE-GATE: PASS — ready for `OWNER PLAYTEST — WEAPON
CHOICE MATTERS (REVISIT)`, which has not run.** Branch
`feat/m9-meaningful-reward-revisit`, cut from `docs/post-combat-initiative-handoff`
at `0aeac9c849a27e71d249d5d013376d50e1896e9d` (`main` at
`af475efc18139dfc4b86b3c41e165dcfd0d7393c` plus the post-merge handoff), on
2026-09-23. Not merged, no pull request, no independent QA. Everything in this
section is technical, headless and self-QA evidence; none of it is owner
judgement, and **an owner PASS in the laboratory would not by itself authorise
merging M9** (KI-043).

Nothing above this section was rewritten. The original M9 — technically
complete, **OWNER PLAYTEST — WEAPON CHOICE MATTERS: FAIL** on 2026-09-22,
frozen — stands as recorded; it is why combat initiative exists.

### Why a revisit, and what it asks

The original gate failed because approaching and attacking won with either
weapon: the M6 adversary never made a player trade anything. Combat initiative
(`COMBAT_INITIATIVE.md`, merged through PR #13) is the capability that
finding asked for — a pressure lunge from middle distance, a non-reactive
spacing dodge and an outcome-dependent recovery — and the owner judged that
"just running and attacking does not work" any more. The revisit asks the
original question against that adversary:

> *Against an adversary that really demands decisions, do the original and the
> found weapon make the player take different decisions?*

It does not assume the answer. The found weapon could create a new way of
facing the lunge, or it could break the lunge's band and bring back spam. That
was the pre-gate's question, and it was answered by measurement before any
owner session was prepared.

### How it was ported

**Fresh port, as the owner approved** — no rebase, no merge of the old branch,
no cherry-pick. `feat/m9-meaningful-reward` at
`61cb43e76af9f307bb388cb1bdebff84e4425b2b` is untouched and is the evidence of
the original M9 and its FAIL. Its commits, for reference:

| SHA | what it carried |
|---|---|
| `8a1c14c` | the weapon exchange in the combat domain |
| `be2599f` | the gate site, input, rendering and client |
| `acf3cdc` | the M9 record — **and code**: the renderer's placed-weapon byte accounting and the anchor's re-lock in `reward.rs`, despite its `docs:` subject |
| `6d35240` | the owner FAIL |
| `61cb43e` | the redesign investigation, closed |

Files only M9 had changed — `armament.rs`, `reach.rs`, `event.rs`, `input.rs`,
`renderer.rs`, `reward.rs`, `traversal.rs` and the M9-only documents — were
taken whole from `61cb43e`, so `acf3cdc`'s code arrived with them and was
verified byte-identical. Every file both branches changed was reconciled by
hand against combat initiative, not trusted to an automatic merge:

| file | what had to be decided |
|---|---|
| `crates/combat/src/encounter.rs` | spec selection, aim range per kind, counters, the exchange's place in the tick |
| `crates/combat/src/combatant.rs` | `Intent` gained both `attack_kind` and `interact`; combat initiative's two adversary constructors would not have compiled |
| `crates/combat/src/reach.rs` | `Action::Attack` now needs its kind: a weapon's envelope is its `Primary` |
| `crates/combat/src/fixture.rs` | the lock table, now seven entries; `swing_envelope` stays in `reach.rs` |
| `crates/combat/src/oracle.rs` | made armament-aware for the player only (below) |
| `apps/client/src/encounter.rs` | `CombatLatches` against the initiative driver's tuple; the site per mode |
| `apps/client/src/app.rs` | the placed weapon's site per mode; the eager world build removed |

The ADR the frozen branch wrote as ADR-0010 is
[ADR-0011](../adr/0011-session-acquired-state-in-the-authoritative-encounter.md):
`main` had since accepted ADR-0010 (a sixth action keeps the keyed layer). Its
decision is unchanged.

### Attack kind and armament are orthogonal

The found weapon is **armament, not a third attack kind**. There is no
`FoundAttackKind`, no weapon-specific adversary behaviour and no moveset:

| side | kind | spec |
|---|---|---|
| player, original | `Primary` | the historical player attack |
| player, found | `Primary` | the found attack (`31` / `14` / `58` ticks, `32` damage) |
| adversary | `Primary` | the historical adversary primary, whatever the player holds |
| adversary | `Pressure` | the `PressureSpec`, whatever the player holds |

`Encounter::attack_spec_for(side, kind)` is still the one place a kind becomes a
spec, and the armament reaches exactly one arm of it, `primary_spec(Player)`.
The running `Action::Attack` still carries the authoritative kind.

**Aim assist keeps both works' intent.** A primary is aimed within
`Encounter::aim_range_of(side)` = `max(2.90, connects out to)`: exactly `2.90`
for both M6 bodies and `3.4477` for the found weapon, as the original assertion
requires. A lunge is aimed within the band's far edge (`4.65`), whatever the
player holds — the player's weapon range never reaches the pressure rule.

### Locks

Measured on the revisit branch with `combat-probe signature` and the locks'
own tests; every value is the frozen branch's or `main`'s, **exactly**. No lock
was re-locked.

| lock | value |
|---|---|
| `GOLDEN_WEAPON_GEOMETRY_FINGERPRINT` | `0x8a6b_18ed_d4a2_b879` |
| `GOLDEN_WEAPON_IDENTITY_FINGERPRINT` | `0x084b_f386_500b_b0e4` |
| `GOLDEN_ENCOUNTER_SIGNATURE` | `0x6415_7522_d253_5658` |
| `COMBAT_INITIATIVE_SIGNATURE` | `0x8238_2662_d859_8cf3` |
| `FOUND_WEAPON_GEOMETRY_FINGERPRINT` | `0x0723_e9dd_aeff_d4d5` |
| `FOUND_WEAPON_IDENTITY_FINGERPRINT` | `0xa09f_cd9b_fa45_d87a` |
| `FOUND_ENCOUNTER_SIGNATURE` | `0x735a_9961_f661_8e7d` |
| `REWARD_BEHAVIOR_SIGNATURE` | `0x0f08_fbf7_08e3_206d` (client test) |
| `FOUND_WEAPON_PROFILE_VERSION` / anchor / radius | `1` / `(-7, 57)` / `1.75` |
| every M3, M4, M5, M7 and M8 lock | unchanged, each asserted by its own test |

`locked_values` now has **seven** entries — the three M6 locks,
`COMBAT_INITIATIVE_SIGNATURE` and the three M9 locks — the count read from the
code, not predicted. `REWARD_BEHAVIOR_SIGNATURE` depends on the golden world and
is asserted by the client, not printed by the headless probe. No durable lock
of the found weapon against initiative was created, on purpose: that waits for
the owner's verdict.

### The oracle knows what the player holds

A player knows what it is holding, so every distance an oracle policy judges by
its **own** reach — when to swing, where to hold, from where to punish — takes
`aim_range_of(Player)`. For the original weapon that is exactly `2.90` and every
offset is exactly `0.0`, so **`COMBAT_INITIATIVE_SIGNATURE` did not move**, and
`holding_the_original_weapon_is_the_historical_initiative_fight` requires the
original weapon, with the exchange merely on offer, to reproduce every
historical initiative fight report for report. The adversary learns none of it.

### Found × Pressure: the band

`measure_the_found_weapon_against_the_lunge_band`: a player **charging** a
lunge committed from each distance, swinging when its own reach plus the
oracle's misjudgement says it is in range. `P` = the player's blade first (the
lunge is cut), `L` = the lunge first; eleven misjudgements from `-0.25` to
`+0.25`.

| committed from | original | found |
|---|---|---|
| `4.10` | `LLLLLPPPPPP` | `LPPPPPPPPPP` |
| `4.20` | `LLLLLLLPPPP` | `LLLPPPPPPPP` |
| `4.30` | `LLLLLLLLLLP` | `LLLLLLPPPPP` |
| **`4.35`** (band) | `LLLLLLLLLLL` | `LLLLLLLPPPP` |
| **`4.40`** | `LLLLLLLLLLL` | `LLLLLLLLPPP` |
| **`4.45`** | `LLLLLLLLLLL` | `LLLLLLLLLPP` |
| **`4.50`** | `LLLLLLLLLLL` | `LLLLLLLLLLP` |
| **`4.55`–`4.65`** | `L` throughout | `L` throughout |

**Where the original loses the race:** everywhere in the band, under every
misjudgement — the band was derived for it. **Where the found weapon wins it:**
`10` of the band's `77` cells, only in its near half (`4.35`–`4.50`) and only
when it swings early (misjudgement `+0.10` or more). The planning session's
analytic estimate — that the found weapon would cut the lunge across most of
the band — was wrong, which is why it was measured.

The other answers do not depend on the weapon: a player standing still is hit
at every distance in the band with either, swinging at once on the windup is hit
with either, and stepping off the line — dodging at `18`/`24`/`32`/`48` ticks,
walking at the same lags, advancing and then leaving — escapes to both sides
with either, at every distance from `4.10` to `4.80`.

**Punishing a missed lunge**, by the distance the answer starts from — the tick
of the `120`-tick recovery it lands on, `--` where it misses:

| weapon, reaction | 2.4 | 2.6 | 2.8 | 2.9 | 3.0 | 3.1 | 3.2 | 3.3 |
|---|---|---|---|---|---|---|---|---|
| original, 18 | 88 | 82 | 75 | 72 | 69 | -- | -- | -- |
| original, 24 | 95 | 88 | 81 | 78 | 75 | -- | -- | -- |
| original, 48 | 119 | 113 | 107 | 104 | 101 | -- | -- | -- |
| found, 18 | 100 | 93 | 86 | 83 | 79 | 76 | 73 | -- |
| found, 24 | 106 | 99 | 92 | 89 | 85 | 83 | 79 | -- |
| found, 48 | -- | -- | 116 | 113 | 110 | 107 | 103 | -- |

The found weapon punishes from `0.2` further (`3.20` against `3.00`), not the
`0.55` its standing-target reach suggests — the lunge's spent pose is low and
forward — and from any given distance it lands about eleven ticks later. A slow
reader with the found weapon must answer from `2.8` or further or the recovery
ends first; the original lands from anywhere inside `3.0`. The oracle's read
policy, punishing from its nominal reach, therefore misses the opening probe with
the found weapon at every lag and takes it with the original.

### Found × Pressure: the fights

`measure_the_found_weapon_against_initiative`, flat ground, the six oracle seeds
and misjudgements, identical positions, tuning and policies; totals over six
fights, health out of `576`:

| policy | weapon | W/L | health | ticks | player swings / hits / whiffs | lunges / hit / whiff / cut | primary / hit / cut | punishes | spacing | hit while committed | longest quiet |
|---|---|---|---|---|---|---|---|---|---|---|---|
| owner-spam | original | **2/4** | 156 | 6,189 | 73 / 17 / 30 | 26 / 26 / 0 / 0 | 13 / 0 / 11 | 0 | 37 | 26 | 168 |
| owner-spam | found | **3/3** | 180 | 5,269 | 55 / 10 / 22 | 24 / 24 / 0 / 0 | 8 / 0 / 4 | 0 | 28 | 24 | 182 |
| spam-read 18 | original | 6/0 | 576 | 5,071 | 44 / 24 / 20 | 15 / 0 / 15 / 0 | 7 / 0 / 4 | 15 | 18 | 0 | 297 |
| spam-read 18 | found | 6/0 | 576 | 5,328 | 36 / 18 / 18 | 18 / 0 / 18 / 0 | 18 / 0 / 12 | 0 | 12 | 0 | 318 |
| spam-read 32 | original | 6/0 | 576 | 4,975 | 44 / 24 / 20 | 14 / 0 / 14 / 0 | 8 / 0 / 5 | 14 | 18 | 0 | 292 |
| spam-read 32 | found | 6/0 | **540** | 5,092 | 36 / 18 / 18 | 16 / 2 / 14 / 0 | 15 / 0 / 8 | 0 | 14 | 0 | 354 |
| spam-read 48 | original | 6/0 | 576 | 5,034 | 49 / 24 / 25 | 14 / 0 / 14 / 0 | 8 / 0 / 5 | 14 | 18 | 0 | 286 |
| spam-read 48 | found | 6/0 | **324** | 6,320 | 53 / 18 / 26 | 22 / 14 / 8 / 0 | 16 / 0 / 9 | 0 | 26 | **12** | 354 |
| read-dodge 24 | original | 6/0 | 576 | 5,124 | 24 / 24 / 0 | 24 / 0 / 24 / 0 | 0 / 0 / 0 | 24 | 18 | 0 | 231 |
| read-dodge 24 | found | 6/0 | 576 | **4,056** | 18 / 18 / 0 | 18 / 0 / 18 / 0 | 0 / 0 / 0 | 18 | 12 | 0 | 252 |
| read-dodge 48 | original | 6/0 | 576 | 6,462 | 24 / 24 / 0 | 12 / 0 / 12 / 0 | 12 / 0 / 0 | 12 | 18 | 0 | 316 |
| read-dodge 48 | found | 6/0 | 576 | **4,332** | 18 / 18 / 0 | 18 / 0 / 18 / 0 | 0 / 0 / 0 | 18 | 12 | 0 | 265 |
| read-walk 24 | original | 6/0 | 576 | 5,460 | 24 / 24 / 0 | 18 / 0 / 18 / 0 | 6 / 0 / 0 | 18 | 18 | 0 | 292 |
| read-walk 24 | found | 6/0 | 576 | **3,870** | 18 / 18 / 0 | 18 / 0 / 18 / 0 | 0 / 0 / 0 | 18 | 12 | 0 | 238 |
| read-walk 48 | original | 3/3 | 288 | 5,262 | 12 / 12 / 0 | 30 / 18 / 12 / 0 | 0 | 12 | 24 | 0 | 245 |
| read-walk 48 | found | 3/3 | 288 | 5,499 | 12 / 12 / 0 | 30 / 18 / 12 / 0 | 0 | 12 | 24 | 0 | 261 |

The rows not shown — spam-read 24, read-dodge 18 and 32, read-walk 18 and 32 —
follow their neighbours and are printed by the measurement. In every fight with
either weapon `dodges_during_unresolved_swing` for the adversary is `0`, no fight
is a stalemate, and no body moves more than `0.503` u in a tick (the found
weapon's `0.50` knockback).

Owner-spam by seed, found weapon: misjudgements `-0.25`, `-0.15` and `0.00` win
at `60` health each, taking two lunges; `+0.10`, `+0.20` and `+0.25` lose, and
the last two never land a hit (the adversary ends on `96`), because a found swing
started early is a `103`-tick commitment the lunge arrives inside. Original:
`-0.25` and `-0.15` win at `78`; the other four lose, the adversary ending on
`24` or `48`.

**In the laboratory**, on the golden world exactly as the owner's session stands
(`measure_both_weapons_in_the_weapon_choice_laboratory`), the same relations
hold: owner-spam `2/4` original and `3/3` found; spam-read at lag `48` `576`
health original and `360` found, `11` hits taken while committed; read fights
`4,038`–`4,572` ticks with the found weapon against `5,370`–`6,708` with the
original, both at full health; no stall, no truncated spacing dodge, and COMBAT-005's
counter `0` throughout.

### Original's measured advantage

Not cosmetic, and not manufactured:

- **More forgiving after a late read.** Spam-read — a player who has learned to
  leave the lunge's line but still swings whenever in range — keeps full health
  with the original at every reaction lag; with the found weapon it loses `36`
  at `32` ticks and `252` at `48`, twelve of those hits landing while its own
  `103`-tick swing had it locked. The original's `75` ticks leave the dodge
  available in time.
- **Lower whiff exposure.** A spammer swinging too early with the found weapon
  (`+0.20`, `+0.25`) never lands a hit; the original, same misjudgement, lands
  two before losing.
- **Punishes from anywhere inside its reach, sooner.** From the same distance
  the original's answer to a missed lunge lands about eleven ticks earlier, and
  at a `400` ms reaction it still lands from close range where the found weapon
  runs out of recovery.

### Found's measured advantage

- **Fewer openings needed.** Three connected swings against four: a clean
  reader wins in `3,792`–`4,332` ticks with the found weapon against
  `5,124`–`6,462` with the original, both at full health.
- **A second answer to the lunge, as a gamble.** Swinging early into a lunge
  committed from the band's near half cuts it (`10` of `77` cells); the
  original never can. In the oracle fights this never happened (`0` of `24`
  lunges) and a mistimed attempt is exactly the committed hit above.
- **A slightly longer punish** (`3.20` against `3.00`), with no timing
  advantage.
- **Owner-spam `3/3` against `2/4`** — the found weapon's fewer-hits advantage
  showing up in the worst strategy. It does not restore it: owner-spam still
  loses half, and loses at every early misjudgement.

### Sidegrade assessment against the hard stops

| stop | measured | result |
|---|---|---|
| A. found owner-spam `5/6` or `6/6` | `3/3` flat, `3/3` laboratory | not triggered |
| B. found owner-spam wins more than it loses, restoring approach-and-mash | wins exactly as many as it loses; loses at every early misjudgement, two lunges taken per win | not triggered — **recorded as the first risk** |
| C. found reliably interrupts Pressure across most of the band at no cost | `10` of `77` cells, near half, early swings only; `0` of `24` in fights; a miss is a committed hit | not triggered |
| D. found dominates in win rate, survivability, fight time and tactical safety at once | owner-spam: found ahead on the first three, equal on safety; read: equal but faster; spam-read: original ahead on survivability and safety at `32`–`48`, faster at every lag | not triggered |
| E. no regime shows a meaningful original advantage | spam-read at `32`–`48` ticks, early whiffs, close-range late punish | not triggered |
| F. the only difference is three hits instead of four | commitment exposure, punish distance and timing, the lunge gamble and a different punish pattern (found spam-read wins by interrupting primaries, original by punishing lunges) all differ | not triggered — **but for a clean reader it is nearly true** |
| G. COMBAT-005 broken indirectly | structural test equal tick for tick; counter `0` everywhere | not triggered |
| H. `COMBAT_INITIATIVE_SIGNATURE` moved | `0x8238_2662_d859_8cf3` | not triggered |
| I. an M9 lock moved | all exact | not triggered |

**Pre-gate: PASS.** The measured shape is the hypothesis the owner wrote down —
the original more reactive and forgiving, the found weapon more reach, more
commitment and higher-risk timing — and it lives in the player's own timing, not
in anything the adversary knows. **Its weakest point is stated plainly:** a
player who reads the lunge cleanly takes no damage with either weapon and simply
finishes sooner with the found one. Whether a person experiences that as a
choice or as "the found weapon is better" is exactly what the owner gate asks;
no headless number can answer it, and none is offered as an answer.

### COMBAT-005 with a weapon in the hand

`the_adversary_decides_the_same_whichever_weapon_the_player_holds` holds the
seed, the starts and the adversary's state equal, changes only whether the first
tick's press is an interact (the real exchange), and runs three thousand ticks
under two player scripts — one standing, one walking a fixed pattern with
dodges. The brain's state, the adversary's action, position and facing bits and
the player's health are equal on every tick, with lunges, primaries and spacing
dodges all running. The brain does not read `WeaponVariant`, the weapon's reach
or identity, the player's action or its attack kind; ARM-002 now cross-references
COMBAT-005.

### The weapon-choice laboratory (G1)

`VELDWAKE_ENCOUNTER=weapon-choice` is **test infrastructure**, approved as G1:
combat initiative's fight at the open clearing — same starts, same adversary,
same lunge — plus the M9 exchange at a QA point `1.25` u to the player's left at
the round start. Left, because the follow camera sits behind and to the right
and the carried blade is on the right; `1.25` is inside the `1.75` radius and
outside the widest body's `0.86`, and off the line the fight is fought along.
It uses the real `ArmamentState` and the real exchange rule, needs an explicit
`E`, marks nothing but the planted weapon, and is not the gate. The owner's
`initiative` session is unchanged and offers no exchange.

**One thing the real client forced.** Combat initiative re-arms each round the
moment it resets, and the first lunge arrived about a second and a half after a
reset: in a self-QA run the press meant as a weapon change met a stagger and was
refused. In the laboratory, played by a person, **every round now starts paused
beside the point**, as the first one always did, and the first input — `E`
included — arms it (`Encounter::pause`, `fix:` commit `ba62242`). Driven QA
sessions never pause. The owner's `initiative` session is untouched.

### Real-client self-QA

Serial Windows harness per `agents/EVIDENCE_HARNESS.md`, release client,
`m4-golden`, `1920 x 991`, one process per run, the `Veldwake` window of that
process required to be unique, foreground asserted before every key and
capture. Nineteen runs, every one exit `0`, no `WARN`, `ERROR` or panic line in
any log. Every capture was opened and read.

| evidence | run | what it showed |
|---|---|---|
| original in initiative | `02`, `04`, `06`, `10` | lunge active against a player that left the line; the punish landing; the lunge landing on a committed spammer; the primary |
| found in initiative | `03`, `05`, `07`, `09` | the same four moments holding the found weapon, the long dark-iron blade in the hand |
| exchange original → found → original | `01` | `armament-swapped` `weapon="found"`, then `"original"`; the carried blade and the planted one change places |
| placed weapon | `01`, `16` | planted to the left of the body, pommel up, clear of the body and of the line to the adversary |
| no stale blade after a swap | `01`, `12`, `13` | no hit on any swap tick; domain tests re-pose and clear the blade on exchange |
| found whiff | `08` | the found swing in recovery at `4.05` u, the lunge's opening missed |
| found punished for a bad commitment | `07`, `13` | frozen: the lunge landing while the found swing started at tick `113` still had the body locked; live `spam+found`: `10` adversary hits |
| found punish | `05`, `12` | frozen punish at tick `211`; live `read+found`: `16` player hits, `16` lunge whiffs, full health for `45` s |
| primary against both | `09`, `10`, `12`, `13` | primary windups against each weapon; live primaries interrupted |
| spacing after a hit | `11` | plan view: the adversary's dodge away while the found swing recovers |
| reset preserving the armament | `12`, `13`, `14`, `16` | `player_weapon="found"` in every report across five won rounds (`read+found`), five won rounds (`spam+found`) and a lost one (person) |
| paused round start and swap back | `16` | `round reset; paused…`, `armed=false`, full health, found in hand; `E` → `"original"` at once |
| process restart | `15` | first report of a new process: `player_weapon="original"`, `site_weapon="found"` |

**Frozen captures.** Of ten frozen pairs taken three seconds apart, one was
pixel-identical; the other nine differed only above row `370` — the horizon and
distant terrain still streaming sixteen seconds after start — and not at all
where the bodies are. The QA `combat-side` pose puts its camera near the
laboratory point, so the planted original weapon is large in the foreground of
those frames; the played view does not.

**What the harness could not show.** A person landing a punish with the
keyboard — the harness cannot aim, as M7, M8 and the frozen M9 recorded — so
every "found punish" above is the read driver's; and anything about how the
two weapons feel.

### Performance

In `engineering/PERFORMANCE.md`: vsync-bound `60` FPS and `16.66` ms in
`armed`, `initiative:read` and `weapon-choice:read+found`; the laboratory costs
the planted weapon's `106,064` bytes and one draw per pass, as on the frozen
branch; combat tick `6.86`–`7.29` µs with either weapon against initiative.

### OWNER PLAYTEST — WEAPON CHOICE MATTERS (REVISIT): prepared, not run

Not blind: the owner knows combat initiative, the lunge and the found weapon's
concept. Session:

```text
VELDWAKE_ENCOUNTER=weapon-choice cargo run --release -p veldwake-client
```

The session starts paused at the clearing, the adversary eight units ahead and
the found weapon planted to the left. `E` beside it swaps weapons; every round
starts paused beside it again.

**Instruction, and nothing else:** *"Lute normalmente. Entre as lutas, você pode
trocar de arma no ponto de início quando quiser."* Not said: that the found
weapon is longer, that it can interrupt the lunge, that the original is safer,
or that either is better for anything.

1. **Phase A** — the original weapon, two or three fights.
2. **Phase B** — the found weapon, two or three fights.
3. **Phase C** — free choice, at least two swaps if the owner naturally accepts
   doing so.

Then, first and alone: *"o que você achou?"* — recorded verbatim. Then, neutral:

1. Você lutou do mesmo jeito com as duas? O que mudou?
2. A que distância você ficava dele com cada uma? Por quê?
3. Você entrava para atacar em momentos diferentes?
4. Quando ele errava o lunge, você punia do mesmo jeito?
5. Alguma delas te colocou em perigo de um jeito que a outra não colocava? Quando?
6. Uma delas é simplesmente melhor? Existe situação em que você prefere a outra?
7. Na fase livre, por que você trocou (ou não trocou)?
8. Com alguma das duas dá para só correr e atacar?
9. Foi divertido com as duas?

**PASS** needs owner evidence that the two weapons lead to meaningfully
different play; that the difference includes a decision — spacing, entry timing
or punish timing; that there is a reason to prefer the original in some
situation or style and a reason to prefer the found weapon in another; and that
approach-and-mash does not become reliable with either. **FAIL** on anything
equivalent to "the same thing with a longer sword", "the found weapon is always
better", "there is no reason to use the original", or no changed decision the
owner can name. The verdict is the owner's; no agent awards it. Counters in the
log are QA evidence and never owner judgement.

**After an owner PASS: stop.** No independent QA, no pull request and no merge
follow directly: the behaviour will have been shown in the laboratory, and the
product encounter at the spire still cannot host combat initiative (KI-041,
KI-038, KI-043). Integrating it into the product world is the owner's next
decision.

### Remaining risks

- **A clean reader finishes sooner with the found weapon at no extra risk.** If
  the owner plays the way combat initiative taught — dodge off the line, punish
  the whiff — the found weapon may read as simply better. The original's
  advantage is real but lives in imperfect play: late reads, early swings,
  close-range late punishes.
- **Found owner-spam `3/3`** rather than `2/4`. Not a restoration, but the
  direction is the wrong one.
- **The laboratory is not the product** (KI-043), and its paused round start is
  a laboratory convenience that the owner's `initiative` session does not have.
- **The owner is not blind** and asked for the great sword during combat
  initiative's gate; novelty can favour the found weapon.
- KI-037 (identical impact sound) is unchanged and may hide a difference a
  person would otherwise hear.
- The oracles are scripts; the real client is one integrated-GPU host (KI-021).

### Non-goals, unchanged

No retune of the found weapon — windup, active, recovery, damage, step-in,
knockback and reach are the frozen branch's — and none of the adversary, the
lunge, the band, the spacing dodge or the recovery. No inventory, equipment UI,
loot, rarity, stats screen, crafting, persistence, new enemy, third weapon,
combo framework, stamina, poise, block, parry, traversal verb, worldgen change,
new landmark or reward system. No combat initiative at the spire, no navigation,
no move of `ADVERSARY_COLUMN`, no KI-038 or KI-041 fix, and no durable lock of the
found weapon against initiative.
