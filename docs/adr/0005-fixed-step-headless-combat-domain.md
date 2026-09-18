# ADR-0005: Fixed-step headless combat domain

- Status: Accepted
- Date: 2026-09-18
- Owners: Veldwake maintainers
- Supersedes: None
- Superseded by: None

## Context

M6 has to produce a combat encounter that a person can play and judge, on a repository that has no gameplay of any kind: no entity model, no collision response, no player control, no timing contract. Three decisions in that space outlive the milestone, because everything built on combat later inherits them: where the rules live, how time is represented, and whether a physics engine enters.

The constraints that force each one are already written down. [ADR-0002](0002-presentation-independent-authority.md) requires authoritative gameplay to be independent of presentation. `ARCHITECTURE.md` lists `gameplay`, `animation` and `physics` as separate logical domains and carries the test a new crate must pass. `INVARIANTS.md` CHAR-002 makes the visible block top the contact contract. `AGENTS.md` forbids speculative dependencies and abstractions. The audited host is an integrated GPU already spending its frame budget on terrain, and `PERFORMANCE.md` defines every recorded number in wall time.

Combat timing is the sharp edge. The client today measures wall time per frame and passes it, clamped, to the camera and the character. A combat rule set driven that way produces different outcomes at 30, 60 and 144 Hz, cannot be reproduced by a headless script, and makes `NaN`, negative and infinite time representable inputs to authority.

## Decision

**Combat rules live in a new headless crate, `veldwake-combat`, depending only on `veldwake-character`, `veldwake-voxel`, `glam` and `std`.** It contains no GPU, window, device, scheduling, world-generation or streaming types. What it needs from the world is the height under a foot, through the `GroundSampler` trait `veldwake-character` already declares and the client already implements over `TerrainField`.

**Authoritative time is an integer tick, and one call advances exactly one tick.** `Encounter::step(input, ground)` takes no duration. `COMBAT_TICK_HZ` is 120. Every duration inside the domain — attack phases, dodge, stagger, hitstop, adversary timers, defeat hold — is a tick count. Seconds appear only in authored configuration, which a validating constructor compiles to ticks before it can reach the runtime. Phase windows are half-open integer ranges. `NaN`, negative and infinite time are therefore unrepresentable rather than rejected.

The client converts wall time to ticks with integer arithmetic over `Duration::as_nanos`, carrying the sub-tick remainder, capped at four ticks per rendered frame, with the excess discarded and counted. The claim this supports, and the only one made, is that the same representable elapsed time produces the same number of ticks in the same order whatever partition of frames delivered it, as long as the cap is not hit.

**The actor model is exactly two combatants.** `enum Side { Player, Adversary }` indexes `[Combatant; 2]`. No entity allocator, no identifier space, no map, no ECS. Extending to N actors is a later decision that needs an entity model, and pre-building one for two actors is the overengineering the risk register names as R-002.

**No physics engine, and Rapier specifically stays rejected.** Movement is bounded kinematic displacement against the ground query, with a step rule, arena bounds, and a capsule push-out that reuses the same acceptance rule. Hit detection is a swept segment against a capsule in closed form. Knockback is a bounded kinematic displacement, not a response to force.

The boundary: this covers one encounter between two combatants with one weapon. It is not a gameplay framework, an ability system, or an authority server. Networking, prediction, more actors, and persistence are not prejudged.

## Alternatives considered

- **Combat rules inside `veldwake-character`.** Rejected: it would make the animation crate own gameplay rules against `ARCHITECTURE.md`'s domain split, and would force `character-probe` and the 111 character tests to compile hit sweeps and adversary logic they never call.
- **Combat rules inside `apps/client`.** Rejected: it buries testable authority in platform code, against ADR-0002 and ARCH-003, and would make the deterministic encounter script impossible to run without a window.
- **`Encounter::step(dt: f32)`.** Rejected, and this was in the approved design before correction. It makes `NaN`, negative and infinite time representable inputs to authority, makes phase boundaries float comparisons, and reduces partition equivalence to an approximation. An integer tick makes the whole class unrepresentable.
- **A floating-point accumulator in the client.** Rejected for the same reason: if the claim is exact equivalence between partitions, the representation has to be exact. `u128` nanoseconds scaled by the tick rate is.
- **Unbounded catch-up, or one large step when behind.** Rejected: a large step tunnels through hit windows and teleports bodies. A hard cap that discards and counts the excess makes the slowdown visible instead of making the simulation wrong.
- **An ECS.** Rejected for two actors. `RISK_REGISTER.md` R-002 names exactly this pattern, and the fixed pair makes every state exhaustively matchable.
- **Rapier or another physics engine.** Rejected: the contact problem is a height query that CHAR-002 requires to be exact and a solver answers approximately; hit detection is closed form; a solver adds a second source of truth for where a body is, its own fixed step, and a large dependency for zero simulation. ADR-0004 set the review trigger at "something must respond dynamically to force", and a bounded kinematic knockback is not that.
- **A `Vec<CombatEvent>` with reserved capacity for events.** Rejected: reserving capacity does not prove an allocation cannot happen. A fixed array plus a length makes the bound part of the type.

## Positive consequences

- The entire combat domain is testable without a GPU, a window or an audio device, and a scripted encounter is bit-identical across frame rates, which makes it usable as a locked fixture signature.
- Timing defects that are endemic to variable-step gameplay — refresh-rate-dependent windows, tunnelling on a frame spike, lost inputs between steps — are addressed by representation rather than by tuning.
- The dependency surface does not grow at all for the rules: no new crate outside the workspace enters for combat.
- Presentation cannot invent a hit. Every reaction in the client originates from one event the authority produced.

## Negative consequences

- Two clocks exist in the client: wall time for presentation and ticks for authority. The relationship has to be understood to read any measurement, and the report has to name which clock a number came from.
- A capped catch-up means the simulation genuinely slows on a long frame spike. That is the chosen failure, and it is observable rather than silent.
- The fixed pair of combatants will not extend to a third actor without a real entity model. That is deliberate, and the cost is a known future refactor.
- Bounded event arrays mean a tick that would produce more events than the bound loses them. The bound is asserted against every fixture and the overflow is counted, but the guarantee is a bound, not infinity.

## Future implications

- A third combatant, or any actor that is not one of the two, is the trigger to design an entity model. Do not grow `[Combatant; 2]` by adding arrays.
- A local authoritative server would host `Encounter` unchanged; the client already only submits intent and consumes events, which is what ADR-0002 asks for.
- A physics engine becomes worth reconsidering when something must respond dynamically to force — a thrown body, a ragdoll, stacked dynamic objects. Kinematic knockback and a swept blade are not that thing.
- The tick rate is part of the contract. Changing it changes every compiled duration and every locked encounter signature, and requires the same deliberate re-lock a style version does.
