# M9 — Meaningful Reward

Status: **implemented on `feat/m9-meaningful-reward`, stopped for the owner
playtest.** No pull request, no merge. Base: `docs/post-m8-handoff` at
`658ebfbd618b1d7387eee7890d11af53b5f8e045`, which is `main` at merge commit
`ee35f62f97afbe3d001a27a576e9bae21e77c4d2` plus one documentation commit.

M8 proved a person will choose a direction because of what is standing at the
end of it. What it did not give that person was a reason to go beyond arriving.
M9 is the first proof of **Mastery** in this project: reaching a place gives you
something that changes how you play.

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

- **Whether the two weapons feel different to a person.** That is the owner
  gate and it has not been run. Everything above is measurement and self-QA.
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
