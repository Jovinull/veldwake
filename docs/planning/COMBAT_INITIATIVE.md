# Combat Initiative / Spacing

Status: **implemented on `feat/combat-initiative-spacing`, technical and self-QA gates green, awaiting the owner playtest.** No pull request, nothing merged, no milestone number. Base: `main` at `ee35f62f97afbe3d001a27a576e9bae21e77c4d2` (M8 merged through PR #12). The owner playtest has **not** happened, and nothing below is owner evidence.

## Product question

> Can the adversary create a readable situation in which attacking immediately is sometimes the wrong decision?

More concretely: *do I have to watch the adversary and choose when to go in, instead of winning by running forward and pressing attack?* The question stands without M9: no found weapon, no reward, no gate, nothing from `feat/m9-meaningful-reward`.

## Where it came from

The M9 owner playtest found that the natural way to fight the M6 adversary — approach, face it, attack whenever you can — wins every time, with either weapon. The repository now carries that as an oracle: under six seeds owner-spam beats the historical encounter in `337`–`415` ticks at full health while the adversary lands nothing (`owner_spam_beats_the_historical_encounter_every_time_untouched`, exact per-seed ticks locked).

Two investigations then closed two directions:

- **Tuning** (the M9 redesign): damage and health, aim assist, stagger, telegraph, strike range, hold bands, pauses, a commit point, recovery punishment, threat-aware approach. None moved the result.
- **Reactive evasion** (`feat/combat-pressure`, blocked and not continued): the adversary sidestepping after the player's swing becomes visible escapes at `18` ticks and never at `24`–`30`, because the player's windup is `22`. A reaction that starts after the blade is live cannot be fair. That branch is not merged and nothing from it was merged here; only the owner-spam oracle's idea was re-derived.

The design session that followed measured why: at close range the player's `22`-tick windup, `3.02` reach and unconditional stagger pre-empt any adversary action with a fair telegraph, and after an exchange the adversary cannot reopen distance at the shared `3.4` u/s. Initiative has to act **before** the player commits, from outside the player's reach, and the adversary has to be able to get back there. No single capability did both; three coupled ones did. The owner accepted the larger slice.

## What M6 keeps

Nothing historical moved. `GOLDEN_ENCOUNTER_SIGNATURE` is `0x6415_7522_d253_5658`, byte-identical, as are both weapon fingerprints and every M5/M7/M8 lock. The player attack, the adversary's primary, the player dodge, the fixtures and the scripts are untouched; the capability exists only when a tuning authors `adversary_pressure`, which no historical setup does. `COMBAT_STYLE_VERSION` stays `1` (see *The lunge pose*).

## The three mechanisms

One product idea — *an adversary that controls distance and commitment, takes the initiative legibly, and gives a real opening when it misses* — built from three parts that only work together:

| mechanism | rule | where |
|---|---|---|
| **pressure lunge** | the adversary's second attack, `AttackKind::Pressure` on `Action::Attack`, chosen from middle distance; the body travels along a line locked at commit | `spec.rs` (`AuthoredPressure`, `PressureSpec`), `encounter.rs`, `adversary.rs` |
| **spacing dodge** | the existing `Action::Dodge`, requested by the brain **only** at its own first free tick after its own stagger, or after its own lunge connected, and only if its dodge cooldown has run out — otherwise it is dropped, never deferred; straight away from the player | `adversary.rs`, `encounter.rs` |
| **outcome-dependent recovery** | a lunge that connected recovers in `12` ticks; one that met nothing in `120` | `AttackSpec::recovery_for`, `encounter.rs` |

Two attacks are the whole universe. There is no moveset, attack list, registry, combo or ability system; `AttackKind` has exactly `Primary` and `Pressure`, and the player only ever swings `Primary`.

### The exact numbers

`fixture::pressure()`:

| | value | ticks |
|---|---|---|
| windup | `0.55` s | `66` |
| active | `0.12` s | `14` |
| recovery after a whiff | `1.00` s | `120` |
| recovery after a hit | `0.10` s | `12` |
| stagger / hitstop | `0.30` / `0.07` s | `36` / `8` |
| damage | `18` (the historical primary's) | |
| step-in / knockback | `0` / `0.30` | |
| lunge | `2.0` u over the last `32` ticks of windup + active (`7.5` u/s) | travel ticks `48`–`79` |
| selection band | `4.35`–`4.65` centre to centre | |
| spacing dodge | `3.0` u in `0.40` s (`7.5` u/s), cooldown `0.25` s | `48` + `30` |

Every speed the capability adds — the lunge and the spacing dodge — is `7.5` u/s, the order of the player's own `7.33` u/s dodge. Damage stays at the historical `18`: no result below depends on more.

### How the adversary chooses

The M6 four-state brain with two additions and two bits of memory, and nothing else:

- in `Approach` (and in `Reposition`), if the player is inside the band **and** the body faces within the swing-start aim cone (`AIM_ASSIST_CONE`, `35°`), commit the lunge; otherwise, inside `strike_range` swing the **primary**, otherwise approach — the historical order;
- the swing-start aim assist turns a lunge onto the player the way it turns every swing, with the band's far edge as its range; after that the line is locked for the whole action, recovery included;
- memory: `spacing_owed` (own stagger ended, or own lunge connected) and `lunge` (own lunge running, and whether it has connected, read from the brain's own `Action`).

It reads the player's **position** and whether the player is defeated. It never reads the player's action, input, latches, a future hit or position, the camera or a weapon identity. Two unit tests prove that structurally: the lunge decision and the spacing decision are identical whether the player stands free, winds up, swings live, recovers or dodges.

### The first aim rule was wrong

The first implementation committed a lunge only within six degrees of the bearing, to avoid a line that misses. It lost almost every lunge after a spacing dodge: facing is held through a stagger and a dodge by the side-neutral turn rule, the player moves meanwhile, and by the time the body had turned the distance had left the band. The measured failure was a body free at `4.38`, inside the band, turned `64°` away. Reusing the swing-start aim assist — already accepted by the M6 owner gate for the primary — fixed it without a snap larger than the one M6 already has.

## The selection band, derived

`oracle::probe_lunge` commits one lunge from a chosen distance and runs the real rules against a scripted answer. `measure_the_lunge_band` (ignored; prints the table) walks `3.40` to `5.00`:

- **far edge — no bluff.** A body that stands still and does nothing is hit from every distance up to `4.70`; from `4.75` the lunge falls short. The band ends at `4.65`. `every_distance_in_the_band_is_threatened` steps the whole band at `0.02`; `just_past_the_band_the_lunge_would_be_a_bluff` pins `4.85` as a miss.
- **near edge — no charging through.** A player that runs straight in and swings the moment it thinks it is in range, with the oracles' misjudgement of `-0.25` to `+0.25`, gets its blade there first below `4.35` (`LPP` at `4.10`, `LLP` from `4.15` to `4.30`) and never from `4.35` on. `inside_the_band_charging_in_and_swinging_is_hit_first` checks eleven misjudgements at every `0.05` of the band, and that `4.10` with `+0.25` does interrupt.
- **swinging on the spot** the moment the windup shows is hit everywhere in the band (`swinging_at_once_into_a_lunge_is_hit`).

The arithmetic behind the near edge: a spammer walking in at `0.0283` u/tick swings at `2.9 + m` and connects about `24` ticks later, so it pre-empts a commit made closer than `S + (W - 24)·0.0283` plus the lunge's own travel toward it.

### What the numbers were before, and why they moved

| value | first | now | evidence |
|---|---|---|---|
| lunge pose blade | rose `30°` above level as the arm drove | level at chest height | no stationary target hit at **any** distance; the blade angle from vertical is torso lean + `hand_pitch` + grip `0.90` |
| lunge travel | `1.6` u / `26` ticks | `2.0` u / `32` ticks | band widened from `0.10` to `0.40` u at the same speed |
| lateral sweep of the point | `~0.9` u (side-on guard) | `~0.25` u | stepping to the weapon side failed where the other side escaped; torso yaw is now held from guard to extension |
| spacing dodge | the shared `2.2` u | `3.0` u | with `2.2` the dodge landed below the band and the adversary fell back on the interruptible primary: owner-spam `6/0` |
| windup | `54` (the primary's) | `66` | a player **walking into** the lunge had to leave the line within `28` ticks (`233` ms); the real client reproduced the failure at about `280` ms (2 of 4 escaped) |
| whiff recovery | `72` → `90` → `108` | `120` | a `400` ms reader landed its answer `1` tick before `108` ran out |
| band | `4.20`–`4.60` | `4.35`–`4.65` | re-derived for the `66`-tick windup |

Each change moved one number against one measurement. No damage was changed and no armor, poise or interrupt immunity was added; a scratch test of armour turned the fight into a damage race, which the owner had already rejected.

## Fairness

The lunge is escaped by **leaving its line**, from either side, at a human reaction. From anywhere in the band, at every `0.05`, from both sides — standing asserted by `a_lunge_is_escaped_sideways_at_a_human_reaction_on_either_side`, walking in by `walking_into_a_lunge_still_leaves_time_to_step_off_its_line`:

| answer | standing when it starts | walking into it when it starts |
|---|---|---|
| dodge sideways | R18, R24, R32, R48 (`150`–`400` ms) | R12–R48 |
| walk sideways | R18, R24, R32, R48 | R12–R40 (`333` ms); R48 one side only |
| nothing | hit everywhere in the band | hit |

None of these needs the `18`–`20` ticks the reactive dodge did. What the numbers mean for a person: stepping aside works; **running straight at it** gives about a third of a second to step aside by walking.

**Backing away in a straight line does not work**, by design: the lunge travels `2.0` u along its line and the reach at the far edge of the band is `4.70`. The historical answer to the primary — dodge away — still works against the primary and is what the read policy does for it. The lesson the lunge teaches is *leave the line, do not retreat along it*. Whether that is legible to a person is the owner's call.

### The spacing dodge is not reactive

- **Trigger**: the brain's own state only — its stagger has ended, or its own lunge connected. The foe's action is not an input to the decision (structural tests above).
- **Audit**: `CombatCounters::dodges_during_unresolved_swing` counts every dodge that starts while the other body has a swing that has begun, is not past its active window and has not connected. For the adversary it is **`0`** in every oracle fight, flat and on the golden terrain, for every policy, and `0` in every real-client session.
- **Timing**: after its own stagger ends the adversary is free while the player is still locked in the recovery of the swing that staggered it (the design session's scratch audit measured `11`–`20` ticks of player lock left at every spacing dodge; the repository proves the property with the counter above). After a connected lunge the adversary is free `23` ticks after the hit, starts spacing at `24`, and the player can act only at `43`; separation after the dodge `5.74` (`a_connected_lunge_recovers_fast_and_spaces_before_the_player_can_act`).

That is the line between this and Combat Pressure: that branch dodged a swing in progress; this one moves only while the player cannot swing.

### The opening is real

A missed lunge holds the spent pose — low, forward, point down, head down — and recovers in `120` ticks. A player that stepped aside at `24` ticks and then waited until the recovery had been visible for `L` ticks still lands its answer inside it (`a_missed_lunge_leaves_an_opening_a_slow_reader_can_take`):

| wait `L` | answer lands at recovery tick | of |
|---|---|---|
| 18 | 75 | 120 |
| 24 | 81 | 120 |
| 32 | 89 | 120 |
| 48 | 107 | 120 |

## Oracles

`crates/combat/src/oracle.rs`. Three policies, each simple on purpose, reading only positions, the adversary's action, kind and elapsed ticks after an observation lag:

| policy | behaviour |
|---|---|
| `OwnerSpam` | approach, face, attack whenever in range (`2.9 + misjudgement`), never dodge |
| `SpamRead` | the same, but steps off a lunge's line with a sideways dodge once it has been visible `24` ticks |
| `Read` | hold at `3.0`; step off a lunge's line (dodge, or walk only) after `lag`; dodge away from a primary telegraph the historical way; punish a visible recovery after `lag`; punish a stagger; never swing into a ready opponent |

**Control, capability off** (the historical encounter): owner-spam `6/6` at `96/96` health, `0` adversary hits, ticks exactly `347, 344, 341, 412, 415, 337`; read (the M6 intentional player) `6/6`, `0` adversary hits. The oracle means what it meant.

### Flat ground, six seeds, capability on

| policy | W/L | mean health | lunges hit / whiffed | punishes | wasted swings | primaries |
|---|---|---|---|---|---|---|
| owner-spam | **2/4** | `26` | 26 / 0 | 0 | 30 | 13 |
| spam-read | 6/0 | 96 | 0 / 16 | 16 | 23 | 7 |
| read-dodge, lag 18 / 24 / 32 / 48 | 6/0 each | 96 | 0 / 24, 24, 18, 12 | = whiffs | 0 | 0–12 |
| read-walk, lag 18 / 24 / 32 | 6/0 each | 96 | 0 / 18 | 18 | 0 | 6 |
| read-walk, lag 48 | 3/3 | 48 | 18 / 12 | 12 | 0 | 0 |

No fight is a stalemate; the longest stretch without a hit in any of them is `316` ticks (`2.6` s); no body moves more than `0.355` u in one tick (the knockback) — asserted as no-teleport and no-kiting bounds in `with_initiative_reading_beats_spam_and_spam_loses_more_than_it_wins`.

**The relation read > spam-read > owner-spam is real, and it is not one number.** Owner-spam loses four fights in six. Spam-read — which learned exactly one thing, step off the line — wins every fight at full health, but wins them by interrupting the primary at close range after a whiffed lunge, wastes `23` swings, and takes longer. Read wins at full health with no wasted swing and punishes every whiff. Learning the line is enough to stop losing; learning the opening is what makes the fight clean.

## Real terrain

Flat ground is a diagnosis. `apps/client/src/initiative.rs` runs the same policies over `TerrainGround` and `TerrainWalkability` on the golden world. The open validation site is **M6's arena clearing at `(-69, 49)`** — the discovery overlook, level for eight units, open for twenty, no landmark near — where the opt-in session also stands. The M8 adversary was not moved; a QA-only site selector puts a session at its column for the witness.

| approach | owner-spam W/L | read W/L | spacing dodges truncated | stuck |
|---|---|---|---|---|
| clearing north / south / east / west | **2/4** each | 6/0 each (96) | 0 | 0 |
| spire, derived route's last leg | **2/4** | 6/0 | 3 of 21 (read) | 0 |
| spire north | **2/4** | 6/0 | 6 of 18 (read) | 0 |
| spire south | **2/4** | 6/0 | 9 of 21 (read) | 0 |
| **witness: spire west, stone behind the adversary** | 6/0 | 6/0 | 6 of 21 (spam), 406 blocked moves | 0 |
| **witness: spire east, stone between (KI-038 on the M9 branch)** | — | — | — | 12 of 12 |

Across every combat approach owner-spam won `14` and lost `28`; spam-read won all it played at the clearing; read won all `42`. The clearing's four approaches give identical results because it is level across the whole fight: there the terrain changes nothing.

**The capability works in the world and breaks against stone.** West of the spire the spacing dodge — straight away from the player — goes into the landmark, is refused or truncated, the adversary cannot get back into its band and the fight degrades to the historical one: owner-spam wins all six. East, the stone stands between the bodies and the adversary walks into it and freezes, the same no-navigation limit the M9 branch recorded as KI-038. Both are recorded as KI-041 and neither is fixed here: no navmesh, no steering, no world change. This is not a combat failure; it is an encounter-placement and navigation limit the capability makes more visible, because the adversary now moves backwards on purpose.

## The lunge pose

A sixth action, `ActionKind::Lunge`, in `veldwake-character` — its own procedural curves, no asset. ADR-0006 named the sixth action as the point to re-argue the keyed-curve layer; [ADR-0010](../adr/0010-sixth-action-keeps-the-keyed-layer.md) records why it is still keyed curves.

| phase | windup share | what it shows |
|---|---|---|
| prepare → guard | `0`–`0.6` | crouch, blade drawn back beside the hip and **level along the line**, free hand pointing down it |
| commit | `0.6`–`1.0` | deeper coil, torso leaning in; the body starts travelling at windup tick `48` |
| lunge | active | arm driven forward and straight, blade level at chest height, free arm thrown back |
| spent | recovery `0`–`0.6` | low, forward, point and head down — held; this is the opening |
| return | recovery `0.6`–`1.0` | back to the carry |

Every key is phase-local, so the recovery shortening on the tick a lunge connects moves no key on screen before the recovery. `the_lunge_blade_is_level_and_on_the_line_while_it_can_connect` pins what the no-bluff evidence rests on: during the active window the blade is level to `0.25` u and between `1.2` and `2.0` high, and at full extension both ends are within `0.3` of the line.

**`COMBAT_STYLE_VERSION` did not change.** It is folded into the weapon's identity fingerprint, a historical lock. The lunge adds a rule table and changes no existing rule, no weapon and no action, and every compiled weapon's identity — the thing the version is folded into — is byte-identical, which the weapon fingerprint tests check. Bumping the version for an addition would have moved a historical lock to record nothing that changed. `COMBAT_STYLE.md` lists the sixth action and its level-blade rule.

## Real-client self-QA

A Windows serial harness (operator tooling in the scratchpad, as `agents/EVIDENCE_HARNESS.md` prescribes): one process per run, the `Veldwake`-titled window of that process enumerated and required to be unique, foreground asserted before every key and capture, maximised to `1920x991`, the log tailed for the thing under test, captures opened and inspected, Escape, exit code read, process confirmed dead. Fifty-three runs on the audited host while the tuning moved, twenty-five of them with the final tuning — seventeen frozen, three live, one keyboard closed loop, four benchmarks — every one exit `0`, and no `ERROR`, `WARN` or panic line in the logs of the final runs.

How it drives the client:

- `VELDWAKE_ENCOUNTER=initiative:<driver>@<tick>` replays the fight headless from arming to a tick and holds it, which is how a pose inside a `0.1`-second window is photographed; framed with `VELDWAKE_POSE=combat-side` or `combat-plan`. Two captures three seconds apart were pixel-identical in every final run.
- `initiative:read` / `initiative:spam` play the oracle policies live; `RUST_LOG=combat_event=debug` turns on one line per combat event, which the harness tails to trigger bursts.
- `initiative` (a person) was driven with real keys in a closed loop: hold `W`; when the log shows a lunge start, wait `250` ms, release `W`, hold `D`; then walk in and attack.

| | evidence |
|---|---|
| A telegraph starting | frozen `t91` (windup `21`): blade level, pointing at the player; `t111`: crouched guard, free hand forward |
| B line locked | plan view `t137`/`t150`: the blade still points along the committed line after the player has left it; facing constant asserted headless |
| C player leaves laterally | plan `t95`; live read bursts; person-mode strafes |
| D lunge passes where the player was | plan `t137`/`t150`; live read log: the player goes from `x -69.00` to `-66.43` while the adversary travels exactly `2.00` along `z` (`46.38` to `48.38`) and whiffs |
| E long whiff recovery | frozen `t165`, `t185`: bent forward, point and head down |
| F entering on the opening | frozen `t185` (player winding up) → `t200` (hit, adversary staggered); live read: `4` of `4` whiffs punished |
| G lunge hits a player who does not react | frozen spam `t137`; live spam `3/3` lunges hit |
| H short hit recovery | spam `t171`: adversary dodging while the player is still staggered |
| I spacing dodge after stagger | frozen `t250`: adversary crouched back, player still in recovery; live read `3` |
| J primary at close range | spam `t284`: primary windup at `2.15`; live spam `4` primaries |
| K spacing refused by terrain | spire-west session: `3` spacing dodges, `54` blocked moves all inside dodges, `1` truncated; frames show the adversary pinned at the foot of the spire |
| L no teleport | headless bound `0.355` u/tick; live event positions continuous |
| M no absurd clipping | see *Visual findings* |
| N no endless kiting | headless longest quiet `316` ticks; live sessions reach a defeat and a reset |

**Person mode with keys**, final tuning, about `260` ms from the log line to the key: **`8` lunges, `8` whiffs**, `0` lunge hits on the player. With the `54`-tick windup the same run escaped `2` of `4`. The keyboard punish landed nothing: a blind driver cannot aim (`LEARNINGS.md`, M7–M9), and the punish was proved by the read driver instead.

### Visual findings

- **From the side the line reads before the lunge moves**: blade level and pointing, free hand down the same line, crouch deepening. In the first `~150` ms the prepare still resembles the carry.
- **From behind the player it reads less well.** A blade pointing at the camera is foreshortened; the line reads from the body — crouch, facing, pointing hand — rather than from the steel. Early in the windup the player's own body can hide part of the adversary. This is the view the owner plays in.
- The extension is a thrust with the arm and a lean, not a fencer's split lunge; the legs stay under the body.
- At body contact after a connected lunge, and during a spacing dodge started from contact, the adversary's blade overlaps the player's torso for about `0.1` s. It is not a hit — the hit already happened — and blades have never collided with bodies in this project (M6). It reads as a thrust being withdrawn, and it is recorded rather than fixed.
- The `combat-shoulder` frozen pose frames the fight from behind the **adversary**, not the player: `frame_the_fight` places the camera at `midpoint + (adversary - player) · 8`. Pre-existing M6 behaviour; recorded as KI-042 and not changed, because changing it moves every M6 capture.

## Determinism and performance

- Same seed, same fight: `the_same_seed_replays_the_same_initiative_fight`. No new random draw; the brain's streams are untouched. No wall-clock input in authority.
- **`COMBAT_INITIATIVE_SIGNATURE = 0x8238_2662_d859_8cf3`**, new, in `oracle.rs`: the tick-by-tick trace of the owner-spam and read reference fights. It is never compared with `GOLDEN_ENCOUNTER_SIGNATURE`. `combat-probe signature` prints both with the weapon fingerprints; all four match.
- Combat tick, release, alternating `main` and this branch on the audited host (`combat-probe bench 24000`, four rounds): historical `main` `6.35`–`7.46` µs, this branch `6.90`–`7.52` µs; initiative fight `6.12`–`7.08` µs. No regression beyond noise; the host measured slower today than M6's recorded `5.463` for both binaries.
- Client, no captures, alternating `armed` and `initiative:read` for 25 s: `60.0` FPS and `16.66` ms in both, vsync-bound; `2` actors, `2` weapons, `985,648` GPU bytes, `34`/`34` draws, `2,720` dynamic bytes a frame in both — identical to M6.

## Gates

On the audited Windows 11 host:

| gate | result |
|---|---|
| `cargo fmt --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo nextest run --workspace` | PASS — 793 run, 793 passed, 5 skipped (the 3 historical `#[ignore]` and 2 new measurements); the golden-world gates take `80` s (clearing) and `139` s (spire) in a debug build |
| `cargo test --workspace --doc` | PASS — 0 doc tests, as before |
| `cargo doc --workspace --no-deps --all-features` (`-D warnings`) | PASS |
| `cargo metadata --locked` | PASS |
| `cargo deny check` | PASS |
| `cargo audit` | PASS — 226 dependencies, exit `0` |
| historical locks | PASS — `combat-probe signature`: golden encounter `0x6415_7522_d253_5658`, weapon geometry `0x8a6b_18ed_d4a2_b879`, weapon identity `0x084b_f386_500b_b0e4`, all `ok`; every M5/M7/M8 lock asserted by its own test, unchanged |
| new lock | PASS — `COMBAT_INITIATIVE_SIGNATURE` `0x8238_2662_d859_8cf3`, measured identically in debug and release |
| real-client self-QA | PASS with findings (above) |
| owner playtest | **not run** |

KI-008's `LNK1104` linker lock appeared repeatedly and cleared on a plain re-run each time; the final `nextest` run needed a second attempt for it and no attempt failed a test.

## Owner playtest

`VELDWAKE_ENCOUNTER=initiative`. The session starts paused at the clearing, eight units from the adversary, and arms on the first input; a defeat either way starts another round. The owner knows the concept, so this is not a blind test. Instruction: **"lute normalmente."**

Afterwards:

- does chasing it and pressing attack still work?
- did you notice the lunge before it moved?
- did you understand you had to leave the line?
- did you notice the opening when it missed?
- did the backstep feel natural?
- did it feel like it was reading your input?
- were there stalled or frustrating moments?
- did the primary still appear?
- did the fight feel only harder, or did it ask for decisions?
- was it fun?
- what did you naturally start doing differently?

## Remaining risks

- **The player's own view.** The strongest evidence for the telegraph is from the side; from behind the player the lunge's blade is foreshortened. Whether a person reads it in time is the gate.
- **Running straight in leaves about a third of a second** to step aside by walking (`40` ticks), `48` by dodging. A player who habitually charges will be hit, which is the point, but it is close to the edge of a comfortable reaction.
- **Retreating in a straight line does not escape**, unlike the primary. It has to be legible that the lunge is a line.
- **Spam-read wins cleanly.** One learned rule — step off the line — turns losing into winning; the rest of the fight is still the M6 close range, where the player's swing interrupts the primary. If the owner finds the fight solved once the line is learned, that is the next question, not a hidden one.
- **Stone.** The capability degrades to the historical fight wherever the adversary has a landmark directly behind it (KI-041), and the spire adversary is placed next to one. The opt-in session avoids it; the M8 encounter does not.
- **Contact overlap** of the blade and torso after a connected lunge, about `0.1` s.
- **Non-reactivity against the player's swing is held by timing.** The spacing trigger never reads the player; what keeps the dodge out of a live swing is that the player is still locked when it starts. A spacing dodge owed while the adversary's dodge cooldown still runs is dropped, not deferred, so it can never start later over a swing it did not see; but a retune of the player's recovery, the stagger or the lunge's connected recovery could move the first free tick into a new player swing. `dodges_during_unresolved_swing` is the check, and it is `0` everywhere it was measured (COMBAT-005).
- Headless oracles are scripts, and the real client is one integrated-GPU host (KI-021).

## What this agent could not verify

- whether a person perceives the lunge before it moves, from the player's camera, at play speed;
- whether "leave the line" is learned without being told;
- whether the spacing dodge reads as a backstep or as the adversary running away;
- whether the fight is fun;
- a person landing a punish with the keyboard — the harness cannot aim.

## Non-goals

No player verb (no block, parry, heavy attack, combo, stamina, lock-on). No armor, poise or interrupt immunity. No damage tuning as a lever. No navmesh, steering or KI-038 fix. No second enemy, archetype, behaviour tree, utility AI or blackboard. No moveset or attack registry. No new `Action`. No VFX, trail, particle or audio. No change to the M6 player attack, primary, dodge, fixtures, scripts or locks. No M9 content, and nothing merged or cherry-picked from `feat/combat-pressure` or `feat/m9-meaningful-reward`.
