# ADR-0010: A sixth action keeps the keyed-curve layer

- Status: Accepted
- Date: 2026-09-23
- Owners: Veldwake maintainers
- Supersedes: None
- Superseded by: None

## Context

[ADR-0006](0006-action-pose-layer-beside-analytical-locomotion.md) added a tick-driven action layer beside analytical locomotion — named actions as keyed curves over a normalized progress, no graph, no clip, no transition table — and named its own review trigger:

> A fifth and sixth action would be the point to re-argue this decision rather than add two more curve tables.

The fifth, `Defeated`, arrived inside M6 because a capture demanded it. Combat initiative (`docs/planning/COMBAT_INITIATIVE.md`) needs a sixth: the adversary's pressure lunge. The owner ruled that it may not be the M6 cut run faster, because what it has to communicate is different — an overhead cut says *when*, a lunge has to say *where* before it goes — and the lunge's recovery length depends on whether it connected, which no existing action had to express.

So the trigger has been reached, and this record answers it rather than adding a curve table silently.

## Decision

**The lunge is a sixth keyed action, `ActionKind::Lunge`, in the same layer, and the layer is not replaced.** It is five poses — guard, coil, extension, spent, carry — interpolated over phase-local segments of one normalized progress, composed over locomotion by the same rules (weapon arm absolute, free arm weighted, torso additive), clamped by the same joint table, and asserted to need no clamp.

What made the re-argument come out this way, measured rather than assumed:

- **No blending between actions was needed.** The lunge's endpoints are the carry, like every other action's, and the continuity test that guards the cut guards it too.
- **No transition table was needed.** The one outcome dependence — a connected lunge recovers in twelve ticks, a missed one in a hundred and twenty — is carried by the caller choosing the total the progress is measured against. Every key before the recovery is phase-local, so the total changing on the tick of a hit moves nothing on screen; only the recovery is compressed, and a hit can never happen during it.
- **No new data format was needed.** The curves are constants beside the others and participate in the action-pose evidence, not in any body's identity fingerprint.

`COMBAT_STYLE_VERSION` does not change. It is folded into a compiled weapon's identity fingerprint, a locked historical value, and the lunge changes no existing rule, weapon or action.

## Alternatives considered

- **The lunge as the M6 `Attack` with a different timing.** Rejected by the owner before implementation, and by the geometry after it: the cut's blade sweeps from raised to low, which is the opposite of a thrust's level line, and a lunge posed that way would be read as the cut.
- **An animation graph or blend tree.** Rejected for the same reason ADR-0006 rejected it: six curve tables with shared endpoints are not a graph, and nothing here blends two actions at once.
- **A general "attack pose" parameterised by kind.** Rejected: two attacks are the whole universe in this slice, and a parameter space for poses is the moveset framework the slice forbids.

## Positive consequences

- The lunge's pose is as reproducible as every other action's: a pure function of an integer tick count.
- The evidence that makes the lunge fair — its blade level, at chest height and on its line while it can connect — is asserted next to the curves that produce it.

## Negative consequences

- The constant count grew again, exactly as ADR-0004 and ADR-0006 predicted. The first set of lunge keys raised the point thirty degrees above level as the arm drove forward, because the torso's lean adds to the arm's pitch, and no stationary target was hit; the second swept the point across the line from the weapon side. Both were found by measurement, and both are the kind of error hand-keyed curves invite.

## Future implications

- **The trigger moves to the seventh action**, or to the first need to blend two actions, to author a pose that is not a function of one progress, or to share poses between archetypes. Any of those re-argues this layer from the start, and this record is the evidence that six was still cheap.
- A second archetype with its own moveset is not a seventh action; it is a different decision about who owns poses, and belongs with the entity-model decision ADR-0005 reserved.
