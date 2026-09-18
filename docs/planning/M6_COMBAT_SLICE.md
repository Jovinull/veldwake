# M6 — Combat Slice

Status: **in progress on `feat/m6-combat-slice`**
Base: `main` at merge commit `5bebc4fa5195b427216a296fff810a294b7bbd7b`

M5 proved a person can stand in the world. M6 has to prove that a person can *fight* in it — that a player approaches, reads an intent, decides, commits, connects, is hit, reacts, and does it again. The exit criterion is the same kind M4 and M5 used: a real run of the real client on the audited host, played by a person, judged against a written contract, backed by headless tests and measurements.

This document is written as the milestone lands, phase by phase. Anything not yet measured is not claimed.

## Scope, copied from the roadmap

> One weapon and one enemy with movement, attack/defense or dodge, telegraphs, hit reaction, camera response, procedural impact audio, VFX, animation, and encounter/readability playtest evidence.

Nothing else. This is a vertical slice of combat, not a combat system.

## Phases

| phase | contents | status |
|---|---|---|
| M6A | `veldwake-combat`: tick clock, weapon compiler, attack specs, combatants, actions, hit sweep, movement, adversary, encounter, fixtures, `combat-probe` | complete |
| M6B | client: input, encounter modes, accumulator, two actors, weapons, follow camera, armed gate, health readout, debug volumes, first real-game run | complete |
| M6C | action motion, reaction, knockback, hitstop, camera shake, named moments, capture cycle | complete |
| M6D | VFX and procedural impact audio, with measurements | complete |
| M6E | playtest evidence, regressions, performance, documentation close-out | in progress |

## The two corrections the design carried

The approved design had two defects that were corrected before any code was written, and both are worth keeping stated because the wrong version was plausible.

**An authoritative step must not accept an arbitrary `dt`.** `Encounter::step(input, ground)` advances exactly one tick. Every duration in the domain — attack phases, dodge, stagger, hitstop, adversary timers, defeat hold — is a tick count, not a float of seconds. `NaN`, negative, and infinite time are unrepresentable rather than rejected, and a phase boundary is an integer comparison rather than a float one. Seconds exist only in authored configuration, which is compiled to ticks by a validating constructor before it can reach the runtime.

**A weapon is an object; an attack is a decision.** The first design put `AttackProfile` inside `WeaponDescriptor` while giving the player and the adversary different windups — which would have meant two different weapons wearing one name. `WeaponDescriptor` now carries only what makes the weapon a physical object (dimensions, palette, derived blade segment, identity). Timing, damage, step-in and reach tuning live in `AttackSpec`, of which M6 has exactly two — `player` and `adversary` — both executing the *same compiled weapon*. That keeps "one weapon" literally true and lets the adversary telegraph for two and a half times as long without falsifying weapon identity.

## M6A — the combat domain, headless

### A new crate: `veldwake-combat`

The crate test in [`../engineering/ARCHITECTURE.md`](../engineering/ARCHITECTURE.md) was applied before the directory existed:

- **Ownership.** Combat rules are a distinct domain with their own identifier range, their own contract version, and their own fixtures — the same argument that qualified `veldwake-character`.
- **Dependency direction.** The rules need `character` (bones, poses, capsule, gait) and therefore sit above it. Putting them *inside* `character` would make the animation crate own gameplay rules, and `ARCHITECTURE.md` lists `gameplay` and `animation` as separate domains. Putting them in the client would bury testable authority in platform code, against [ADR-0002](../adr/0002-presentation-independent-authority.md).
- **Build isolation.** `character-probe` and the character suite must not compile hit sweeps, adversary logic and encounter scripts they never call.
- **Testability.** The whole domain runs with no GPU, no window, no device and no world generator.

```text
voxel <- character <- combat <- client
```

`veldwake-combat` depends on `veldwake-character`, `veldwake-voxel`, `glam` and `std`, and on nothing else. It contains no `wgpu`, `winit`, `cpal`, `veldwake-procedural` or `veldwake-streaming` types. What it needs from the world is the height under a foot, which is the `GroundSampler` trait `veldwake-character` already declares and the client already implements over `TerrainField`.

### The tick clock

`COMBAT_TICK_HZ = 120`. `Encounter::step` is one tick; there is no `dt` parameter anywhere in the crate.

The client converts wall time to a tick count with integer arithmetic only:

```text
scaled += elapsed.as_nanos() * COMBAT_TICK_HZ      (u128)
available = scaled / 1_000_000_000
scaled    = scaled % 1_000_000_000                 (sub-tick remainder preserved)
run       = min(available, MAX_TICKS_PER_FRAME)
dropped   = available - run                        (counted, reported, never merged into a big step)
```

Because the remainder carries, the total number of ticks after a given elapsed time is `floor(total_nanos * 120 / 1e9)` **whatever partition of frames delivered it**. That is the claim this representation supports, and it is the only claim made: for the same representable elapsed time, with no catch-up cap hit, the same number of ticks in the same order is produced. Human input timing relative to ticks is *not* claimed to be partition independent — which is exactly why the scripted encounter keys its input to tick indices, and is therefore bit-identical at 30, 60 and 144 Hz.

`MAX_TICKS_PER_FRAME = 4` (33.3 ms of catch-up). Beyond it the excess is discarded and counted; the simulation slows rather than teleporting.

### The canonical tick order

The order is part of the determinism contract, so it is stated and tested rather than left to emerge from loop order:

```text
1. hitstop      decrement; a frozen combatant advances no action timeline and no gait phase
2. intent       player from the latched input, adversary from its brain (brain decides only when the action is Free)
3. action       transitions, then timeline advance in ticks
4. movement     fixed order [Player, Adversary]: proposed planar move, step rule, ground, arena, realized speed
5. pose         character::pose_with(overlay); blade world segment recorded, previous kept
6. hit          fixed order [Player, Adversary]: sweep previous->current blade against the foe's hurt capsule
7. consequence  damage, stagger, knockback, hitstop, events
8. separation   capsule push-out through the same movement rules, then re-ground
9. outcome      defeat hold, reset
```

Two policies follow from that order and are documented rather than accidental:

- **Simultaneous hits resolve in the fixed order Player then Adversary, and a combatant defeated earlier in the same tick does not land its own swing.** Death cancels the swing. This deliberately gives the player the tie.
- **Hitstop set by a hit takes effect from the next tick**, so it cannot reorder anything inside the tick that produced it.

### Bounded events

`StepEvents` is a fixed array plus a length, not a `Vec`. A reserved-capacity `Vec` proves nothing about a bug that exceeds the capacity; a fixed array makes overflow representable, counted and asserted to be zero on every fixture.

### The weapon

One weapon, compiled from a descriptor by code, with no modelling tool and no new mesher.

- Voxels are written into one reused `32³` scratch grid and meshed by `veldwake-voxel`'s existing `mesh_exposed_faces`, at the character's `1/12` world unit so the two domains share a scale.
- `WeaponMaterial` owns the declared range `192..=223` — named for a weapon rather than for a general "equipment domain", because M6 compiles one weapon. Disjointness against air, the M2/M3 diagnostic identifiers, terrain and characters is proved in the crate and again in the client, where all four tables are visible at once.
- The blade's collision segment is **derived from the compiled voxels** by scanning for blade materials, never declared beside them, so it cannot drift from the geometry the viewer sees.
- The grip is one rigid `Transform` in `HandR` bone space. The weapon never moves relative to the hand; the arm moves. "Weapon detached from hand" is therefore impossible by construction rather than by vigilance.

### Hit detection

The blade segment is swept, not tested for overlap once a frame.

For each Active tick, both endpoints of the blade segment are kept from the previous tick and interpolated to the current one. The substep count comes from the *larger* of the two endpoint displacements, which is what makes a rotation about the hand safe:

```text
advance = max(|base_end - base_start|, |tip_end - tip_start|)
n       = clamp(ceil(advance / CHARACTER_VOXEL_SIZE), 1, MAX_SWEEP_SUBSTEPS)
```

Each substep is a segment-segment closest-distance test against the foe's hurt capsule axis, in closed form. `MAX_SWEEP_SUBSTEPS` is bounded and its high-water mark is reported.

The hurt volume is the compiled body capsule placed at the combatant's stand point. Its consequence is accepted and turned into an asserted property rather than a hidden limitation: for every named moment, the head, torso and pelvis cores are inside the capsule, while the arms, hands and the weapon may leave it. A swing that only grazes an outstretched arm does not register, and that is stated in [`../KNOWN_ISSUES.md`](../KNOWN_ISSUES.md) instead of being discovered later.

### Movement and world contact

No physics engine, and Rapier specifically remains rejected. The contact problem is "which block is under this foot", which `GroundSampler` answers exactly and a solver answers approximately — and CHAR-002 makes the exact answer an invariant. Knockback here is a bounded kinematic displacement, not a response to force, so the review trigger [ADR-0004](../adr/0004-rigid-voxel-character-and-analytical-locomotion.md) set for a physics engine has not fired.

The rules are: a proposed planar move is accepted only if the destination is inside the arena, has ground, and its surface is within the step-up and drop bounds; a rejected move is retried one axis at a time so a body slides instead of sticking; `surface == None` rejects. Separation between the two combatants reuses that same acceptance rule, so a push-out can never place a body outside the arena, off the ground, or over a forbidden step, and a blocked actor's share of the separation is deterministically given to the other.

### The adversary

A small explicit state machine — `Idle`, `Approach`, `Reposition`, `Recover` — that produces *intent* and nothing else. It does not carry a second attack timeline: while the authoritative action is not `Free` the brain holds. Six states and a behaviour-tree library for eight transitions is the overengineering R-002 names, and a utility system needs utility functions to choose between three options.

Variation comes from a named deterministic stream (`CombatSeed::stream(label)` plus an explicit decision index), never from ambient or sequential randomness.

### Headless evidence, M6A

The workspace has **586 tests** (38 voxel, 69 procedural, 97 streaming, 126 character, 168 combat, 88 client). Every M5 signature is unchanged, which is the point: `pose()` is untouched, so `GOLDEN_POSE_SIGNATURE`, the geometry, skeleton and collision fingerprints and all three behaviour signatures are byte for byte what M5 locked. The character crate gained one new locked constant, `GOLDEN_ACTION_POSE_SIGNATURE`, for the eleven named action poses.

**The weapon**, from `combat-probe weapon`:

| measure | value |
|---|---|
| volume | `6 x 23 x 4` character voxels, origin `[-3, -19, -2]`, hand row `19` |
| bands (grid `y`) | blade `[0, 14)`, guard `[14, 16)`, grip `[16, 21)`, pommel `[21, 23)` |
| solid voxels / quads | 212 / 352 |
| CPU mesh payload | 47,872 bytes |
| transient scratch | 65,536 bytes, one reused `32³` grid |
| blade segment, derived | base `(3, 14, 2)`, tip `(3, 0, 2)`, radius `1` voxel |
| blade length / radius | `1.1667` / `0.0833` world units |
| reach from the wrist | `1.5833` world units |
| identity / geometry | `0x084bf386500bb0e4` / `0x8a6b18edd4a2b879` |

**The two combatants**, from `combat-probe bodies`. The adversary got its own descriptor because a test refused the first design: reusing M5's `sturdy` fixture as the enemy gave a body that is *shorter* than the player and shares its absolute hip width of eight voxels and its limb thickness of three.

| measure | player | adversary |
|---|---|---|
| height | 28 voxels (`2.3333` units) | 33 voxels (`2.7500` units) |
| shoulder span / hip / waist | 12 / 8 / 6 | 16 / 10 / 8 |
| limb thickness | 3 | 4 |
| arm / leg / foot | 12 / 14 / 5 | 14 / 15 / 8 |
| hurt capsule radius | `0.6274` | `0.8563` |

The palette carries a second finding. **M5's three garment schemes share one luminance ladder on purpose**, so their primary tunics sit within a ten-thousandth of each other and no choice of scheme can separate two characters by value on the garment. The two are separated by size, by hue on the tunic — rust linen is led by red, slate wool by blue — and by value on the skin, where the declared tones do differ.

**The timelines**, from `combat-probe spec`, at `120` Hz:

| | windup | active | recovery | total | stagger | hitstop | damage | step-in | knockback |
|---|---|---|---|---|---|---|---|---|---|
| player | 22 (`0.183` s) | 12 (`0.100` s) | 41 (`0.342` s) | 75 (`0.625` s) | 36 | 8 | 24 | `0.35` | `0.35` |
| adversary | 54 (`0.450` s) | 14 (`0.117` s) | 72 (`0.600` s) | 140 (`1.167` s) | 36 | 8 | 18 | `0.45` | `0.30` |

Active windows are the half-open tick ranges `[22, 34)` and `[54, 68)`. The dodge is 36 ticks over `2.2` world units, so `7.33` u/s against a walk of `3.4`; its cooldown is 30 further ticks. Both bodies have 96 health, so the player falls in six of the adversary's hits and the adversary in four of the player's.

**The swing envelope**, from `combat-probe reach`, which is what the ranges were chosen against rather than from arithmetic:

| | max tip reach | active reach | active height | connects out to |
|---|---|---|---|---|
| player | `2.0561` | `0.7660` to `2.0561` | `0.9356` to `2.9037` | `2.9124` |
| adversary | `2.1165` | `0.7352` to `2.1165` | `1.1775` to `3.1654` | `2.7439` |

The adversary commits at `2.2` centre to centre. **The first draft of that number was `2.6`, which is outside its own reach**: the swing only landed because of the step-in, so the telegraph was a bluff. It is now comfortably inside.

**The dodge window**, from `combat-probe dodge`. The adversary's swing is 140 ticks; a dodge pressed on each of them, in a fresh encounter each time, either escapes or does not:

| dodge direction | escapes when pressed | last escaping press | of a telegraph of |
|---|---|---|---|
| away | ticks 1 to 51 | 51 | 54 |
| lateral | ticks 1 to 45 | 45 | 54 |

So a player has about four tenths of a second to read the swing, and the window closes three ticks — twenty-five milliseconds — before the blade goes live. Dodging away buys distance; dodging sideways leaves the plane the blade sweeps in, and the two have different margins for different reasons. **Hand arithmetic over reach, radius and step-in got this wrong**, because the blade's height matters as much as its reach: the first active ticks pass above a body and the last ones pass through it.

**The reference encounter**, from `combat-probe script` and `combat-probe moments`: 4,500 ticks (37.5 s) of `arena-exchange` on flat ground.

| counter | player | adversary |
|---|---|---|
| swings | 8 | 14 |
| hits landed | 6 | 6 |
| whiffs | 2 | 5 |
| dodges | 7 | 0 |
| staggers taken | 6 | 5 |
| defeats | 0 | 1 |
| lowest health | 6 | 0 |

One reset, 693 separations, 160 hit queries, worst sweep 4 substeps of a bound of 16, 91 multi-hit contacts suppressed, **zero events dropped**, and a worst body overlap after separation of `0.0000` world units. The player wins, and with six health left out of ninety-six — a reference fight that is a fight. All eleven named moments occur, the earliest at tick 1 and the last, the defeat, at tick 3,054.

**Partition equivalence**, from `combat-probe partition`: 20 s of wall time delivered at 30, 60 and 144 frames a second produced **2,399 ticks and the identical trace `0x0807fcad37689f9f`** in all three, with no capped frames and no dropped ticks. 144 does not divide a second exactly in nanoseconds, so the agreement is the clock's carried remainder working rather than an accident of the numbers.

**Cost**, release build on the audited Windows 11 / Intel Iris Xe host, as observations on that host:

| measure | value |
|---|---|
| combat tick, mean over 12,000 ticks after a warm-up | `5.1`–`8.9` µs across four runs |
| implied cost of a second of play at 120 Hz | `0.61`–`1.07` ms |
| combat tick, worst | `0.59`–`0.65` ms |
| ground queries per tick | `14.96` |
| one hit query at 16 substeps | `0.168` µs |
| weapon compile, median of 64 | `95.2` µs (min `84.1`, max `223.3`) |

The worst-case tick is two orders of magnitude above the mean and does not move with the work, so it is desktop scheduling rather than a tick: the figure to read is the mean, and a second of simulation costs about a millisecond of one core.

## M6B — the playable loop in the client

`VELDWAKE_ENCOUNTER` selects the mode, and `off` is the default and a regression contract: with it unset the client behaves exactly as it did at `5bebc4f` — free-fly camera, `VELDWAKE_CHARACTER` untouched, no combat simulation, no second actor on the GPU, no weapon, no particles, no audio device, and no extra draw or upload.

### Modes

| value | what it does |
|---|---|
| `off` | the default, and the regression contract above |
| `armed` | playable: `WASD` relative to the camera, `J` or left mouse attacks, `K` or `Space` dodges, `F4` detaches the camera, `F1` cycles to a combat view that draws the volumes a hit is decided by |
| `script` | the reference script drives the player, so a fight runs unattended |
| `moment:<name>` | replays the script to a named moment and freezes there |
| `moment:<name>+<ticks>` | the same, some whole ticks later |

The offset exists because of a defect. A frozen frame is the only way to photograph a `0.1`-second window, and one frozen frame says nothing about an arc; `moment:confirmed-hit+6` is the same fight, the same camera and the same tick arithmetic, six ticks on, so a sequence of runs reconstructs a swing exactly rather than at whatever interval a screen grab happened to land on. It also had to be built twice. The first version ran the offset inside the moment search, which happens before the frame loop exists — so the events of the moment's own tick reached nothing, and the first frozen capture of a hit came back with zero chips and zero camera strikes. The search now finds *which* tick and a second pass replays to the tick before it, handing the rest to the frame loop, which runs them through exactly the path a played tick takes.

### The arena

The fight happens in a scanned clearing of the M4 golden region at `(-69, 49)`, chosen by a full-region scan for a level, vegetation-free, walkable column. `ARENA_RADIUS` is `5.5` rather than the `7.0` it started at: that clearing is free of vegetation only out to seven world units, and at `5.5` the widest body plus the arena radius is `6.36`, comfortably inside it. There is no vegetation collision, so an arena that let a body reach the boundary would let it stand inside a shrub.

### What one encounter costs the GPU

Measured on the audited host from the `encounter ready` line:

| measure | value |
|---|---|
| actors | 2 |
| weapons | 2 (the same compiled weapon, drawn twice) |
| rigid parts uploaded | 32 |
| quads | 5,342 |
| static GPU bytes | 985,648 |
| dynamic upload per frame | 2,720 bytes |
| world draws | 34 |
| shadow draws | 34 |

Thirty-four is sixteen bones plus one weapon, twice. Geometry is uploaded once; a frame writes transforms only, which is what the `2,720` is.

### The real-game gate

A played run on the audited host, driven through the evidence harness: the encounter armed on the first input, the player swung thirteen times, was hit ten times, dodged four times, was defeated once and the encounter reset once. Zero errors, zero validation messages, exit code `0`, `events_dropped=0`, `frame_events_dropped=0`.

That run also found two real defects, both fixed:

- **The input latch was never consumed on the frozen path**, so `input_latched` was permanently true in the report. A frozen encounter now takes the latches and drops them, and the report splits attack from dodge so a stuck latch names itself.
- **The follow camera sat at chest height directly behind the player**, which hid the adversary, and the adversary circling behind filled the frame. The camera is now at `2.10` eye height, `6.2` back, `1.15` to one side, pitched `-0.24`, yawing back toward its anchor. Occlusion solving remains an explicit non-goal; `F4` exists for the cases it would solve.

The same run's blind key pattern landed only one of thirteen swings, which looked like a design problem and was not. `combat-probe aim` settles it by standing a passive body at a bearing and swinging: a swing aimed at the body connects at **every** range from `1.60` to `2.80`, and the window is roughly `-45°` to `+14°` at contact range narrowing to `-15°` to `+10°` at `2.80`. It is off-centre by about fifteen degrees because the swing is right-handed. A driver that presses `W` and `J` without seeing the screen aims no better than it walks; a player who aims, hits. No aim assist was added, and the measurement rather than an opinion is why.

## M6C — action motion

## M6C — action motion

The action layer lives in `veldwake-character` because that crate owns "what angle is every joint at". Combat says *which action and how far through*; the curves, the blending and the joint clamps stay in one place. Locomotion remains distance driven; the action layer is tick driven, and that second category is what [ADR-0006](../adr/0006-action-pose-layer-beside-analytical-locomotion.md) records.

Five actions: `Carry`, `Attack`, `Dodge`, `Stagger`, `Defeated`. `pose()` is unchanged and `pose_with()` is the new path, which is why every M5 signature is byte-identical — see *What moved and what did not* below.

### What the captures found

The capture cycle is the point of M6C, and it earned its place: every one of the following was found by opening an image, not by reading code.

**The hit was being decided against the wrong volume.** The contact frame showed the player's sword raised over its shoulder with its tip in the air above the adversary's head, and the rules said a hit had landed. The volume being swept was M5's `BodyCapsule`, which contains every voxel of the rest pose — arms included — so it came out `0.8274` wide on a body `0.50` through the chest and reached from `0.52` below the ground to above the crown. The blade was inside it for the whole windup. [`veldwake_combat::hurt`](../../crates/combat/src/hurt.rs) replaces it with a torso-column volume refitted every tick, and the hit moved from the fourth of twelve active ticks to the eighth, with the blade at chest height and through the body.

Choosing that volume took three rounds of measurement and each one removed something:

| core | radius, adversary | why it did not survive |
|---|---|---|
| whole rest pose (M5) | `0.8274` | as wide as the hands reach; took hits in empty air |
| torso, thighs and shins | `0.6609` | a striding body throws a shin `0.30` clear of it |
| torso and thighs | `0.6470` | a striding body throws a thigh `0.35` clear of it |
| **torso column** | **`0.6335`** | kept |

Refitting to the current pose does not rescue a leg: the width is the stride's, not the fit's. So everything that swings — both arms, both legs below the hip, the feet, the weapon — is outside the volume, and that limitation is asserted rather than hoped for. The blade's active window sweeps a height band of `1.14` to `2.84` above the ground, which is chest and head, so nothing the two attacks can do is lost by it.

**A defeated body held the carry pose.** The last frame of a fight showed the loser standing with its sword out, indistinguishable from a body about to swing. `ActionKind::Defeated` sags the knees, folds the torso and drops the blade over a fixed `DEFEAT_SAG_TICKS` window. That window was then corrected in the same session: it eased over a quarter of itself, so the sag finished in six ticks of the twenty-four the constant promised and read as a snap.

**A fixed camera cannot frame a moment.** The `defeat` capture came back showing one figure standing alone. Nothing was wrong with the renderer: the defeat landed three units off the arena centre with both bodies almost exactly in line with a camera that looks along `x`, so one stood in front of the other. `arena::frame_the_fight` reinterprets a named pose's offset, for a frozen moment, as a distance in the *fight's* own frame — across the line between the bodies, above the ground they stand on, along that line from the midpoint. Every moment is framed the same way whatever the fight did to get there, which is what makes two captures comparable.

**The adversary's hands were two slabs.** At `0.20` of body height a thirty-three-voxel body gets a hand seven voxels deep, and the close contact frame came back with two brown slabs and two brass guards piled where the blades meet. `0.15` gives five, still inside the style contract's `1.20`–`1.80` hand-to-forearm band.

**The impact chips left as a clump.** Twelve spread directions biased into the strike's half-space by adding `away * 0.9` very nearly cancelled the four pointing back at the attacker, so their velocity came out near zero and the spray was one blob. Reflecting about the plane perpendicular to the blow keeps twelve distinct unit directions and still sends none of them backwards.

**Two measurement tools were wrong, and were corrected rather than trusted.** `combat-probe contact` printed `closest_points`' **squared** distance as a distance, and its bearing column had the `x` sign flipped against `movement::facing_of`, which made a swing that connected look like one aimed forty-one degrees wide. Both are fixed and the three independent measurements — the aim table, the contact table and the fight itself — now agree: the reference hit lands at a `-9.6°` facing error, inside the measured window.

### Camera response

Only a confirmed hit moves the camera. `HitShake` has no idea what a miss, a dodge or the start of a swing is: the only way to move it is `strike()`, and the only caller is a `CombatEvent::Hit`. It is capped at `SHAKE_MAX_OFFSET = 0.055` world units — a twelfth of the follow distance — decays over `SHAKE_TICKS = 16`, restarts rather than accumulates, and is a function of an integer tick counter, so the same hit displaces the camera identically on every run. A frozen moment runs no ticks and holds.

## M6D — VFX and procedural audio

## M6D — VFX and procedural audio

Deliberately last. If the combat does not feel right with the presentation extras off, particles and sound will not fix it; they are added after the core is judged, and the judgement is repeated with them on.

### Two effects, one fixed pool

No particle system: no emitters, no curves, no modules, no spawn descriptors, no sorting, no texture, and no way to add a third effect without writing it. `VfxKind` has two variants — impact chips off a confirmed hit, and a cold accent on the adversary's windup — and they are deliberately opposites, so neither can be mistaken for the other: chips are warm, fast and fall; motes are cold, slow and rise.

The pool is a fixed array of `MAX_PARTICLES = 96`. Nothing allocates, including when it draws: `instances()` writes into a buffer the caller owns. Ages are tick counts and spread directions come from a fixed icosahedron table, so the same hit throws the same chips on every run — and a frozen moment holds them exactly where they were, which is what makes a frozen capture of an impact possible.

The rendering is one instanced pass: one static unit cube, one instance buffer allocated once at its maximum, one draw call.

### What one hit costs, measured

From the frozen captures:

| moment | impact chips | telegraph motes | camera strikes | vfx draws | instance bytes |
|---|---|---|---|---|---|
| `confirmed-hit+3` | 12 | 0 | 1 | 1 | 384 |
| `telegraph-early+6` | 0 | 5 | 0 | 1 | 160 |
| a settled frame, nothing in flight | 0 | 0 | 0 | 1 | 512 |

One event, the right response, never the wrong one: the telegraph fires no chips and moves no camera; the hit fires no motes. The `512` bytes in the last row are the readout's sixteen pips, which are always present during an encounter — so **the "nothing in flight costs nothing" contract is about particles, and during an encounter there is always one effect draw for the readout**. With the encounter off there is no draw at all.

### The readout

A row of `READOUT_PIPS = 8` cubes above each head, bright for health held and dark for health lost. It shares the effect pipeline because a pip is the chip cube at a different size, so it costs one instance each and no second pipeline. A bar would want a non-uniform scale the instance does not carry. Any health at all keeps one pip lit, so "one hit from death" never looks like death, and a dead body still shows its row, all dark. Captured at `24` of `96` health: two bright, six dark.

### The sound

Generated, never recorded: no samples, no assets, no files. A hit is a noise transient through a fast envelope with a low body under it and a short metallic ring over it; a whiff is noise through a one-pole filter whose corner rises and falls. Both are a few hundred bytes of arithmetic.

Three layers, and the separation is the design:

- [`synth`](../../apps/client/src/synth.rs) is pure DSP with no cpal, no threads, no clock and no I/O — a function from voice requests and a sample rate to a buffer of `f32`. That is what makes every claim about it testable headlessly, against the same code the device plays.
- `SoundQueue` is a lock-free single-producer single-consumer ring of packed requests. Atomics only, no `unsafe` — the workspace forbids it — and no `std::sync::mpsc`, because an `mpsc` receiver is not documented to be allocation-free and a channel that *probably* does not allocate is not a real-time channel.
- `AudioDevice` is the only code in the repository that knows cpal exists.

The real-time callback never allocates, never locks, never logs, never blocks and never panics, and each of those is a property rather than an intention. Everything it wants to report is an atomic the game thread reads and logs on its own time.

A host with no sound card, no default output, or a device that does not want `f32` still plays the game: the failure is warned once, counted, and the fight is silent. CI has no audio device and must not fail for it.

### Audio, measured on the audited host

A `37.5`-second scripted fight, `VELDWAKE_ENCOUNTER=script`:

| measure | value |
|---|---|
| device | `Alto-falantes`, WASAPI |
| sample rate / channels / format | `48,000` Hz, `2`, `f32` |
| callbacks | 10,412 |
| frames rendered | 4,998,336 |
| buffer high-water | 1,056 frames (`22.0` ms) |
| voices started | 53 |
| voices displaced by the voice limit | 0 |
| voice high-water | 1 |
| peak sample | `0.2323` against a `0.92` limit |
| device errors | 0 |
| queue consumed / dropped / waiting | 53 / 0 / 0 |

The cross-check is the one that matters: the same run recorded 15 player hits, 15 adversary hits, 7 player whiffs and 16 adversary whiffs — **30 + 23 = 53, exactly the voices started**. Every combat event produced one sound, none were invented, and none were lost.

The offline listening fixture (`cargo nextest run -p veldwake-client --run-ignored all the_listening_fixture`) renders six seconds of every sound the fight makes to a `.wav` in the system temporary directory. Measured: `48` kHz, 16-bit mono, peak `0.7712` on the eight-simultaneous-hits burst, **zero clipped samples**, and per-half-second RMS that tracks the script — whiffs at `0.001`–`0.004`, the exchange at `0.010`–`0.017`, the burst at `0.052`.

**What none of that establishes is whether an impact *sounds* like an impact.** That judgement needs ears and this agent has none, so it is not claimed. See the owner check in the milestone's closing status.

### The cpal dependency, audited before it was added

`cpal = { version = "=0.18.2", default-features = false }`, pinned exactly like every other dependency here.

| check | result |
|---|---|
| licence | Apache-2.0; `dasp_sample` is MIT OR Apache-2.0. Both already in `deny.toml`'s allow list |
| `rust-version` | `1.85`, against the toolchain's `1.98.1` |
| default features | empty upstream, and written out anyway so the day a default appears is the day this line stops it |
| features enabled | none. No `asio`, no `jack`, no `pipewire`, no `pulseaudio`, no `realtime`. WASAPI is compiled into the Windows backend with no feature flag |
| `cargo tree` on `x86_64-pc-windows-msvc` | grows by exactly two crates: `cpal v0.18.2` and `dasp_sample v0.11.0`. The existing `windows v0.62.2` is reused, so no duplicate version |
| `Cargo.lock` | 17 new packages, of which 15 (`alsa`, `coreaudio-rs`, `objc2-*`, …) are other platforms' backends that this target never compiles |
| `cargo deny check` | advisories ok, bans ok, licenses ok, sources ok |
| `cargo audit` | 226 crate dependencies scanned, no vulnerabilities |

### Cost

| measure | value |
|---|---|
| combat tick, mean | `14.161` µs over 12,000 ticks |
| a second of simulation at 120 Hz | `1.699` ms of one core |
| ground queries per tick | `14.96` |
| one hit query at 16 substeps | `0.3551` µs |
| worst sweep substeps in a fight | 4 of 16 |
| weapon compile, median | `125.5` µs |
| `renderer_render_wall`, mean / max | `9,216` / `14,879` µs |

The bench's worst single tick is `44,021` µs, two orders of magnitude above the mean and unrelated to the work: it is desktop scheduling, and the figure to read is the mean. `renderer_render_wall` is **wall time around the render call**, not GPU time; there are no GPU timestamps here and none is claimed.

## What moved and what did not

Every locked value that changed, and why. Nothing was re-locked to make a test pass; each entry names the semantic change that required it.

### Unchanged, and the point of the exercise

| signature | value |
|---|---|
| `GOLDEN_GEOMETRY_FINGERPRINT` | `0x3ebe8c822f549151` |
| `GOLDEN_SKELETON_FINGERPRINT` | `0x2628aeb41d2979ed` |
| `GOLDEN_COLLISION_FINGERPRINT` | `0x818156de8637d934` |
| `GOLDEN_BEHAVIOUR_SIGNATURE` | `0x6ca752c70a5bf919` |
| `GOLDEN_POSE_SIGNATURE` | `0xfd1e1f61ba1737d2` |
| `STURDY_BEHAVIOUR_SIGNATURE` | `0xb7d67addb9845142` |
| `VARIED_BEHAVIOUR_SIGNATURE` | `0xa931d3c2ede0d752` |

`CHARACTER_STYLE_VERSION` was deliberately **not** bumped. It is part of `CharacterIdentity`, so bumping it would move the identity and behavioural signature of three compiled bodies to record a rule that moves no voxel. The action-motion rules answer to their own version in `docs/audiovisual/COMBAT_STYLE.md` instead, and `pose()` is untouched, which is why `GOLDEN_POSE_SIGNATURE` above is byte-identical.

### Moved, with reasons

| signature | old | new | why |
|---|---|---|---|
| `GOLDEN_ACTION_POSE_SIGNATURE` | `0xd736e07231cbd790` | `0xce4233533b66e60c` | a fifth action, `Defeated`, and the two named poses covering it. Carry, attack, dodge and stagger curves unchanged |
| `GOLDEN_ACTION_POSE_SIGNATURE` | `0xce4233533b66e60c` | `0xd86daa4a8d870882` | the collapse eased over a quarter of its window instead of all of it, so it snapped |
| `GOLDEN_ENCOUNTER_SIGNATURE` | `0x38e207fc7ead6c47` | `0xc2fc91e36fbd0ec4` | the adversary's hand depth came down from seven voxels to five after the close contact capture, which moves its capsule radius and so every separation |
| `GOLDEN_ENCOUNTER_SIGNATURE` | `0xc2fc91e36fbd0ec4` | `0x008e8bd64f62f267` | the hurt volume became the torso column; every position in the fight follows from where a hit lands |

The weapon's two fingerprints — `0x8a6b18edd4a2b879` geometry and `0x084bf386500bb0e4` identity — have not moved since they were first locked.

The second encounter re-lock also made it a better fight rather than only a more legible one. The adversary now has to aim, so it whiffs seven of thirteen swings instead of connecting almost every time, and the reference run ends with the player on `24` health rather than `6`.

## M6E — evidence

The exit gate is the real game on the audited host, with fresh captures opened and inspected.

### Played runs

Three runs of `VELDWAKE_ENCOUNTER=armed`, driven through the evidence harness on the audited host, each settled for 75 s and then played:

| run | swings | hits | whiffs | hit by | dodges | refused | staggers | defeat | reset |
|---|---|---|---|---|---|---|---|---|---|
| 1 | 13 | 1 | 11 | 10 | 4 | 10 | 9 | player | 1 |
| 2 | 14 | 4 | 9 | 9 | 3 | 10 | 8 | player | 1 |
| 3 | 14 | 4 | 9 | 10 | 3 | 12 | 9 | player | 1 |

Every run: zero errors, zero validation messages, zero dropped combat events, zero dropped frame events, zero dropped particles, zero dropped sound requests, exit code `0`. The audio cross-check is exact in each: run 2's `4 + 9` hits and `9 + 2` whiffs are `24` events, and the device started `24` voices.

**No played run produced a victory, and that is stated rather than worked around.** The harness presses keys without seeing the screen, so it aims no better than it walks; `combat-probe aim` establishes separately that a swing aimed at the body connects at every range the attack reaches. Victory is demonstrated in `script` mode, where the reference fight defeats the adversary once, and headlessly by `the_reference_script_reaches_every_named_moment`. Defeat, reset, hits in both directions, misses, dodges, refused dodges and staggers are all demonstrated by the played runs themselves.

### The `off` contract, by observation rather than by eye

Two runs, same world, same camera pose, same weather, same 75-second settle, no captures taken during the interval:

| observation | `ENCOUNTER=off` | `ENCOUNTER=script` |
|---|---|---|
| `combat state` / `combat work` lines | 0 | 40 |
| `encounter ready` | 0 | 1 |
| `audio device open` | 0 | 1 |
| debug draws / slots / allocations / uniform writes | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 |
| ` ERROR ` lines | 0 | 0 |

With the encounter off no device is opened at all, so there is no audio thread and no handle on the sound card — not a muted one.

`renderer_render_wall` came back at a mean of `12,091` µs with the encounter off and `10,697` µs with it on. **The run with more work in it measured faster, which is noise and is reported as noise.** That wall time is dominated by present and vsync; it is not a measure of what combat costs. What combat costs is the headless figure: `1.699` ms of one core per second of simulation.

### Regressions at this HEAD

| regression | configuration | result |
|---|---|---|
| M3 diagnostic | `ENCOUNTER=off`, `CHARACTER=off`, `WORLD=diagnostic`, `PROFILE=m3-diagnostic` | 0 errors, 0 validation, 0 device lost, 0 panics, 0 gaps of any kind, 0 upload failures, 0 commit-invariant failures |
| M4 golden | `ENCOUNTER=off`, `CHARACTER=off`, `WORLD=golden` | 0 errors, 0 validation, 0 device lost, 0 panics |
| M5 character | `ENCOUNTER=off`, `CHARACTER=course`, pose `character-portrait` | 0 errors, walking at `speed=2.0` and grounded; identity `0xb21b87d0a3ce9078`, geometry `0x3ebe8c822f549151`, 16 parts, 1,814 quads, 16 world draws, 16 shadow draws, 1,280 dynamic bytes per frame — the M5 figures unchanged |

All five M5 locked signatures are byte-identical; see *What moved and what did not*. `character-probe`, `terrain-probe`, `streaming-probe`, `voxel-probe` and `combat-probe` all exit `0` in release at this HEAD.

KI-018 and KI-019 are unchanged: nothing in M6 touches the gait solver or the shadow pass. KI-020 is **more visible and not worse in kind**: an attack flexes the wrist further than locomotion ever does, so the dark seam where a parent's end cap meets its child is wider in an attack frame than in a walk frame. It is the same accepted consequence of rigid parts, at a larger angle.

## Scope, and what this milestone deliberately does not build

No weapon system, second weapon, weapon switching or sheathing; no second enemy; no loot, inventory, equipment, stats, skill tree, crafting or quests; no multiplayer; no AI framework, combat framework or ability system; no animation graph; no physics engine; no ECS; no navmesh or pathfinding; no boss; no procedural creature ecosystem. Specifically also: no lock-on, no stamina or posture, no invulnerability frames, no combos, no attack cancelling, no parry or block, no jumping or falling, no animated strafe or backpedal, no dodge roll, no ragdoll, no vegetation collision, no camera occlusion solving, no line-of-sight raycast, no HUD or UI framework, no pause or death screen, no encounter persistence, no music, no reverb, and no automated visual regression.
