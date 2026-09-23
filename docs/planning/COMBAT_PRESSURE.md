# Combat Pressure

Status: **COMBAT PRESSURE: BLOCKED — the evasive half of the accepted capability only works with a reaction delay of 20 ticks (167 ms) or less.** Hard stop condition 1 fired during the first measurement, before any production rule was written. The branch `feat/combat-pressure` carries the repository oracle for the owner's strategy, the measurement that fired the stop, and this record. **No combat rule, spec, fixture value or lock changed.** No milestone number is assigned. Base: `main` at `ee35f62f97afbe3d001a27a576e9bae21e77c4d2`.

## Product question

> Can the adversary force the player to respond to pressure rather than win by holding forward and repeatedly attacking?

The question stands without the M9 reward. The adversary in `main` loses to "walk up to it, face it, attack whenever you can" every time, which is now asserted by a repository test (below).

**M6 did not fail and this is not a statement that it did.** M6 proved a readable telegraph, impact, a basic hit exchange, a deterministic fixed-step encounter, a basic adversary and a working attack-and-dodge loop, and its owner gates stay closed. Combat Pressure asks a stronger question than M6 did.

## Relationship to M9

M9 (`feat/m9-meaningful-reward`, unmerged, no pull request) failed its owner gate because both weapons supported the same approach-and-attack strategy; its redesign investigation closed with the conclusion that the adversary lacks a capability for managing pressure. Combat Pressure is that capability's own milestone, started from `main` and **independent of M9**: nothing from M9 was ported — no found weapon, no reward, no gate exchange, no ARM-001 or ARM-002, no `reach.rs`. M9 would consume the capability later and re-run its own gate. The M9 branch was not touched.

## Owner decisions

| decision | ruling |
|---|---|
| capability | **accepted**: *adversary commitment response* — a lateral evasive response through the existing `Action::Dodge`, plus a punish attack legal **only** against an authoritatively observable player whiff. Neither half is sufficient alone |
| tuning from the proposal | **not accepted**: reaction `18` ticks, punish damage `48` and a mandatory shared `DodgeSpec` were experimental points only |
| response dodge | a response-specific `DodgeSpec` may be measured — control `36` ticks / `2.2` u, candidates `24` / `~1.5` and `20` / `~1.2`, plus small corrections derived from measurement |
| fairness target | a robust result at roughly `24`–`30` ticks (`200`–`250` ms). `18` is a control, not an approval |
| punish | legal only when the player is in `Action::Attack`, its active window has ended, `hits[Adversary] == false`, the adversary is free and in range. Never after being hit, never on a predicted whiff, never from input or future geometry |
| second attack | allowed, as an `AttackKind` on `Action::Attack`, not a new action; two attacks are the whole universe |
| enablement | optional; `None` in every historical setup; an explicit setup and a QA opt-in only |
| audio / VFX | none new |
| perception | recorded as an invariant (COMBAT-005), not an ADR |
| hard stops | nine, listed below; any one of them stops the work as BLOCKED |

## What the branch adds

`crates/combat/src/oracle.rs`, and nothing else in code:

- **`OraclePolicy::OwnerSpam`** — the M9 owner's strategy as a policy: walk at the adversary, attack whenever the body can act and the adversary is within `AIM_ASSIST_RANGE` plus a per-seed misjudgement of `±0.25`, never dodge.
- **`OraclePolicy::Intentional`** — the fight M6 was built around: hold just outside the adversary's reach, dodge its telegraph once it has been visible for `OBSERVATION_LAG = 24` ticks, punish its recovery once that has lasted as long, punish its stagger, never swing into a ready opponent.
- **`run_fight`** over **`oracle_setup(seed)`** — the golden bodies, weapon and tuning on open ground, no arena, `PlayerVictoryPolicy::Remain`, six seeds (`ORACLE_SEEDS`, the six the M9 investigation used), sixty seconds at most.
- **`probe_reactive_evasion`** — whether the adversary's body, dodging sideways `R` ticks after the player's swing became visible, escapes that swing. It adds no adversary dodge to the rules. The rules are side-neutral, so it swaps the two roles instead: the adversary's body stands on the player side, where a dodge can be requested, and the player's body and attack spec stand on the adversary side, whose brain is tuned to commit at once. It uses the same sweep, aim assist and movement rules as the real fight.

Three tests use them; they are the evidence below.

## Evidence

Two kinds, kept apart. **Repository evidence** is the three tests in `oracle.rs`, reproducible from this branch. **Scratch evidence** was produced in the proposal session by a throwaway `git archive` copy of `main` with experimental hooks off by default; with every hook off it passed all `204` combat tests including the `GOLDEN_ENCOUNTER_SIGNATURE` lock. It is not reproducible from the repository and no rule it tried is approved. There is **no owner evidence** for Combat Pressure.

### The owner's strategy, as a repository oracle

`owner_spam_beats_the_historical_encounter_every_time_untouched`, flat ground, capability absent (it does not exist):

| seed | winner | ticks | player health | player swings / hits / whiffs | adversary swings / hits |
|---|---|---|---|---|---|
| golden | player | 347 | 96 | 4 / 4 / 0 | 2 / 0 |
| `0x1111…` | player | 344 | 96 | 4 / 4 / 0 | 2 / 0 |
| `0x9e37…` | player | 341 | 96 | 4 / 4 / 0 | 2 / 0 |
| `0xdead…` | player | 412 | 96 | 5 / 4 / 1 | 2 / 0 |
| `0x0123…` | player | 415 | 96 | 5 / 4 / 1 | 1 / 0 |
| `0x5555…` | player | 337 | 96 | 4 / 4 / 0 | 0 / 0 |

**Six of six, in under three and a half seconds each, without taking a hit.** This is the M9 owner finding turned into regression evidence. The test asserts it, so a change to the encounter that breaks the dominance fails the test by design and must update it deliberately.

`intentional_play_beats_the_historical_encounter_every_time`, the control: six of six, full health, `1,112`–`1,236` ticks (about ten seconds), four adversary swings each and none of them landed. The two policies both win and differ only in how long it takes: the historical encounter never forces a choice between them.

### The measurement that fired hard stop 1

`a_reactive_sidestep_escapes_the_players_swing_only_below_a_human_reaction_time`: escapes out of six separations (`1.5`, `1.7`, `2.0`, `2.3`, `2.6`, `2.85` world units) of one player swing, dodging perpendicular to the line between the bodies after `R` ticks:

| response dodge | speed | R 18 | R 20 | R 22 | **R 24** | **R 27** | **R 30** |
|---|---|---|---|---|---|---|---|
| `36` ticks / `2.2` u (control, the player's dodge) | `7.33` u/s | 6 | 2 | 1 | **0** | **0** | **0** |
| `24` ticks / `1.5` u | `7.50` u/s | 6 | 3 | 1 | **0** | **0** | **0** |
| `20` ticks / `1.2` u | `7.20` u/s | 6 | 2 | 1 | **0** | **0** | **0** |
| `20` ticks / `1.6` u (derived, faster) | `9.60` u/s | 6 | 6 | 3 | **1** | **0** | **0** |

Scratch, same measurement: `16` ticks / `1.6` u (`12.0` u/s, three and a half times the running speed) escapes `2` of `6` at `R 24` and `0` at `R 27`.

**Why, in one line of arithmetic.** The player's windup is `22` ticks (`0.18` s) and its blade connects in the first ticks of the active window. A reaction of `24` ticks starts after the blade is already live. A dodge that begins then has to outrun a sweep in progress, which no sidestep at a body's speed does. **The owner's hypothesis — that a shorter sidestep buys a longer reaction — is refuted: duration was never the constraint.** The three owner candidates have the same speed within `4%` and the same cliff. Speed moves the cliff by about two ticks per `2.4` u/s, so reaching `24`–`30` ticks would need a sidestep several times faster than anything else a body does in this game — the "absurd speed" the real-client QA was told to reject.

The cliff is structural: **no reactive defence against a `22`-tick windup can be fair at a human reaction time**, because a person could not do it either.

### Supporting scratch evidence (proposal session)

Full fights with evasion and whiff punish, flat ground, six seeds:

| configuration | owner-spam | intentional |
|---|---|---|
| no response (today) | 6/6, full health | 6/6, full health |
| evasion `R 18`, lateral, punish `18` ticks, damage `18` | 6/6, mean health `66` | 6/6, full |
| evasion `R 18`, lateral, punish damage `32` | 6/6, mean health `43` | 6/6, full |
| evasion `R 18`, lateral, punish damage `48` | **2/6** | 6/6, full |
| evasion `R 24`, `24` / `1.5` dodge, punish `24` ticks, damage `18` | 6/6, mean health `90` | 6/6, full |
| evasion `R 24`, `24` / `1.5` dodge, punish damage `48` | 5/6, mean health `72` | 6/6, full |
| evasion only, `R 18`, straight away | 2/6 won, **4/6 stalemates of 60 s** | — |

Evasion alone never let the adversary land a hit in any configuration: away or diagonal evasion leaves punish range and stalls, and lateral evasion does not change the outcome. The adversary's primary attack cannot punish a whiff either: it needs about `57`+ ticks from commit to contact, and a whiffing player is free `41` ticks after its active window ends.

**The only configuration in which owner-spam stopped winning reliably combined `R 18` with punish damage `48`, which is hard stops 1 and 2 at once.**

## Hard stop conditions

| # | condition | result |
|---|---|---|
| 1 | only reaction ≤ 20 ticks works | **FIRED.** Repository measurement above; no candidate escapes at `R ≥ 24` |
| 2 | only punish damage ~48 or more makes spam unreliable | **touched in scratch** (`2/6` only at damage `48` and `R 18`); not re-measured in the repository once 1 fired |
| 3 | away/diagonal evasion needed, and it stalemates | **scratch:** away and diagonal evasion stalemate (4/6 and 6/6 draws); lateral does not, but lateral depends on 1 |
| 4 | intentional play loses with spam | not observed in any configuration |
| 5 | evade rate approaches 100% | not observed (about half of spam swings evaded in scratch) |
| 6 | long periods with nobody able to attack | only in the away/diagonal stalemates of 3 |
| 7 | KI-038 increases materially | **not evaluated**: no response exists to run on the golden traversal |
| 8 | punish needs input or future reading | no: the rule reads `Action::Attack`, its phase and `hits`, all authoritative and visible |
| 9 | a historical lock moves | no: nothing that feeds a lock changed; the lock tests pass |

## What was deliberately not done, and why

Because hard stop 1 fired before any rule was written, the work stopped there. Forcing completion would have meant shipping a response that only works at a reaction the owner has declined in advance.

- **No adversary dodge intent**, no `AttackKind`, no punish `AttackSpec`, no response configuration, no response setup and no client opt-in. Building them would put production code for a blocked rule into the domain.
- **No golden-traversal oracle run.** The blocker is a timing fact between two numbers in the rules: a reaction delay against a `22`-tick windup. Terrain cannot change it. KI-038 was therefore not measured against a response that does not exist.
- **No real-client motion evidence.** There is no response to show. The owner gate **OWNER PLAYTEST — PRESSURE DEMANDS RESPONSE** was not reached, and no claim about it is made.
- **No damage tuning.** The owner ruled out hiding a reaction problem behind damage, and the scratch record shows damage `48` was the only lever that moved the result.
- **No change to the player's windup.** It is the one number that would move the cliff, and it is M6's contract: changing it reopens the M6 owner gates on attack feel and readability. That is an owner decision, not this branch's.

## What would unblock it — options for the owner, none chosen

1. **A non-reactive trigger.** The adversary reads a committed *approach* (the player entering its reach while it is ready) rather than a swing already in progress. That is a different capability with its own fairness question. It needs its own acceptance and measurement, and it risks the "reads my movement" feeling.
2. **A longer player windup.** Reactive evasion becomes fair when the player's windup is longer than a human reaction plus the sidestep. This moves `GOLDEN_ENCOUNTER_SIGNATURE`, the M6 attack feel and the M9 weapon measurements, so it would be a deliberate re-lock with an OLD/NEW/WHY paragraph and a fresh owner gate.
3. **Accept `R ≤ 20` as the design.** The owner has already declined this as the default. Recorded only for completeness.
4. **Pressure without evasion.** For example the adversary taking initiative so that its commitments, not the player's, set the tempo. This is unexplored.

The measurement narrows the choice: **whatever answers the product question cannot be a reaction to the player's swing at a human delay while that swing winds up in `22` ticks.**

## Perception contract

Recorded as **COMBAT-005** in [`../engineering/INVARIANTS.md`](../engineering/INVARIANTS.md): an adversary response may consume only authoritative state that corresponds to visible, committed player behaviour, after an authored reaction delay — never the input buffer, a future hit, a future position, the mouse, the camera, or a weapon-identity shortcut. The oracles in `oracle.rs` obey the same rule for the player side: positions, actions and elapsed counters only.

## Gates

On the audited Windows 11 host:

| gate | result |
|---|---|
| `cargo fmt --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo nextest run --workspace` | PASS — 758 run, 758 passed, 3 skipped (`#[ignore]`d, as on `main`); `main` had 755 + 3, and the difference is the three oracle tests |
| historical locks | `GOLDEN_ENCOUNTER_SIGNATURE 0x6415_7522_d253_5658` and every M5/M6/M7/M8 lock unchanged; no fixture value, spec or rule was edited |
| golden traversal | NOT YET APPLICABLE: no response exists to run |
| real-client motion evidence | NOT YET APPLICABLE: no response exists to show |
| owner gate | not reached |

## Non-goals

No second enemy, archetype, behaviour tree, navmesh, combo framework, stamina, block, parry, lock-on, poise, super armour, resource or difficulty system. No i-frames. No new audio or VFX. No change to M6 specs, telegraph or interruption rules. No M9 content. No fix for KI-038, KI-022 or KI-025.
