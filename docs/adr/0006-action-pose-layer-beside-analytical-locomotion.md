# ADR-0006: Action pose layer beside analytical locomotion

- Status: Accepted
- Date: 2026-09-18
- Owners: Veldwake maintainers
- Supersedes: None
- Superseded by: None

## Context

[ADR-0004](0004-rigid-voxel-character-and-analytical-locomotion.md) decided that a character is rigid voxel parts with no skinning, and that locomotion is analytical and driven by **distance travelled** rather than by time or by clips. It also named its own review trigger:

> The first behaviour that genuinely needs blended, interruptible, stateful animation — combat, hit reactions, ragdoll — is the review trigger. At that point the choice is between growing the analytical layer and adding an animation system beside it, and this ADR should be superseded rather than stretched.

M6 is that behaviour. An attack, a dodge and a hit reaction are interruptible, stateful, and — this is the part that matters — driven by **time**, not by distance. A swing takes the same number of ticks whether the attacker is standing still or running, so the property that makes locomotion correct (stride is specified, cadence follows) has nothing to say about it. Stretching ADR-0004 to cover both would make "driven by distance, not by time" false while leaving it written down as a decision.

A weapon adds a second constraint that the milestone discovered rather than assumed: a rigid weapon in a hand driven only by the gait's arm swing puts the blade tip through the ground at rest, because the arm hangs and the blade continues the hand's line. A combatant holding a weapon has to *carry* it, which is a pose, which means the weapon arm is action-driven at all times and not only while attacking.

## Decision

**Everything ADR-0004 decided stays decided.** Bodies are rigid voxel parts, one per bone, and nothing is skinned; animation writes transforms and never touches geometry. Locomotion — idle, walk, run and the blend between them — remains analytical, closed form, and driven by distance travelled, with stride as the specified quantity. Terrain contact remains a two-bone analytical IK solve against `GroundSampler`, which returns the top face of the topmost solid voxel and is never smoothed.

**A second animation category is added: a tick-driven action layer.** It lives in `veldwake-character`, because that crate owns what angle every joint is at, and it covers exactly five actions: `Carry`, `Attack`, `Dodge`, `Stagger` and `Defeated`. The fifth was not in the proposal and was added because a capture demanded it: a defeated body posed by `Carry` stood at the end of a fight with its sword out, indistinguishable from a body about to swing. An action is a set of closed-form keyed curves evaluated at a normalized progress derived from an integer tick count, composed over the locomotion angles as an override on the upper body, and clamped by the same joint-range table locomotion is clamped by. Leg IK still runs last, so contact is unaffected by what the upper body is doing.

The seam between the domains: **`veldwake-combat` says which action and how far through it is; `veldwake-character` owns every angle.** Combat never computes a joint angle, and character never knows what a hit is.

`pose()` keeps its M5 signature and behaviour exactly, and `pose_with(character, state, ground, overlay)` is the path that takes an action. With no overlay the two are the same function, which is what keeps every M5 fixture signature valid.

The boundary: this is not an animation system. There is no graph, no state machine inside the animation layer, no clip format, no sampler, no transition table, no additive stack, and no retargeting. Five named actions with keyed curves is the whole thing, and the number of hand-tuned constants grows with the number of actions exactly as ADR-0004 warned it would.

## Why ADR-0004 is not superseded

ADR-0004 named its own review trigger and expected to be replaced at it:

> this ADR should be superseded rather than stretched.

It was neither. The concern behind that sentence was that combat would force the analytical locomotion layer to grow into something it is not, leaving "driven by distance, not by time" written down as a decision while being false in the code. That did not happen, because the action layer is **not locomotion**. Locomotion is still every joint angle that comes from distance travelled, it is still closed form, and `pose()` is byte-for-byte the function M5 shipped — `GOLDEN_POSE_SIGNATURE` is unchanged at `0xfd1e1f61ba1737d2`, along with the geometry, skeleton, collision and behavioural signatures of all three fixtures.

So every decision in ADR-0004 is still the current decision, and marking it superseded would say the opposite. What M6 adds is a second category beside it, which is what this ADR records. The review trigger has been honoured by answering it rather than by changing a status field.

If a later milestone does need blended, interruptible, clip-driven animation — a second archetype with its own moveset, a transition table, retargeting — then *both* of these ADRs are in scope for a superseding one, and the fact that the keyed-curve approach took five actions before it strained is the evidence that decision should start from.

## Alternatives considered

- **Stretching ADR-0004 to cover time-driven motion.** Rejected because ADR-0004 explicitly asked not to be stretched, and because it would leave "driven by distance, not by time" standing as a decision while being false.
- **A real animation graph or blend tree.** Rejected: one attack, one dodge and one reaction are three sets of curves, not a graph. `RISK_REGISTER.md` R-002 names a framework built before its consumers, and there is no second consumer.
- **Authored or imported animation clips.** Rejected by [ADR-0003](0003-procedural-first-audiovisual-production.md), and they would make combat the one part of the game that cannot be produced from code.
- **Putting the action curves in `veldwake-combat`.** Rejected: it splits pose mathematics across two crates, which is the drift that one shared lighting function exists to prevent for shading. Combat would then need the joint-limit table, the rest pose and the blending rules, all of which are `character`'s.
- **Distance-driven attacks, for consistency with locomotion.** Rejected: an attack that slows down when the attacker stops is not an attack. The categories are genuinely different and pretending otherwise was the temptation this ADR exists to close.
- **Driving the weapon arm from the gait and accepting the tip position.** Rejected by arithmetic and then by a capture: the blade reaches the ground at rest. A carry pose is required, which is why `Carry` is one of the four actions rather than an absence of action.
- **Letting the joint clamps shape the attack.** Rejected as a matter of test ethics: the clamps are a safety net for hostile input. A nominal action that needs the clamp to look valid is a curve that is wrong, and the curves are asserted to stay inside the ranges before clamping.

## Positive consequences

- Locomotion keeps the property that makes it correct, and the action layer keeps the property that makes *it* correct, without either compromising for the other.
- Every M5 fixture, pose signature and locomotion test remains valid and unchanged in meaning, because `pose()` is untouched.
- The action curves are pure functions of an integer tick count, so a pose is reproducible from an encounter's tick index, and the action poses get locked signatures of their own rather than making an M5 fixture pretend it always covered combat.
- Joint limits, the rest pose, the blend and the contact solve stay in one place, so an action cannot invent an elbow that hyperextends.

## Negative consequences

- Two animation categories exist and a reader has to know which one a motion belongs to. The naming carries that weight, and nothing enforces it.
- The constant count grows per action, exactly as ADR-0004 predicted for analytical motion. A fifth and sixth action would be the point to re-argue this decision rather than add two more curve tables.
- An action override on the upper body means locomotion's arm swing is simply not visible on the weapon arm while a weapon is held. That is correct for a carried weapon and would be wrong for an empty hand, and the rule is explicit rather than emergent.
- There is no transition blending between actions. An action ends and the next begins; the curves are authored so their endpoints agree, and the continuity is asserted rather than interpolated.

## Future implications

- This ADR supersedes ADR-0004 once the implementation and its evidence justify it; ADR-0004 then carries only the historical status metadata pointing here, and its decisions live on in this record.
- A fifth action, a second weapon archetype with its own swing, or any need to blend two actions at once is the trigger to re-argue whether an animation system belongs beside this layer. Adding curve tables past that point is the failure mode.
- Ragdoll and secondary motion remain out of scope and are not prejudged. Neither is expressible as a keyed curve over a tick count, so both would need their own decision.
- The action curves participate in a character's action-pose signature, not in its identity fingerprint, so tuning a swing does not invalidate a compiled body.
