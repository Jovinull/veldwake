# Progression and traversal

Status: **Accepted principles; feature list Exploratory**.

Progression should combine vertical strength with horizontal capability. The player may gain power, but meaningful steps should also unlock ways to move, understand, craft, fight, influence, or reshape the world.

Rigid classes are not currently preferred. A build may emerge from equipment, weapon, abilities, magic, traits, and movement skills. This remains Proposed pending combat prototyping.

## Traversal as progression

Travel must not degrade into dead time. Candidate layers include running, climbing, swimming, mounts, boats, gliding, grappling, airships, flying creatures, and magic traversal. These are a design space, not a delivery checklist.

Each adopted mode should introduce decisions:

- climbing trades route directness for stamina/risk;
- gliding depends on height, terrain, and wind;
- boats make waterways and expeditions meaningful;
- mounts change speed and logistics without trivializing terrain;
- advanced flight requires late capability, constraints, or infrastructure.

Fast travel may exist, but should not be the first response to poor traversal. The design must measure useful decisions per travel time and preserve the scale that supports Wonder.

## Reward guardrails

- Avoid region changes that arbitrarily invalidate equipment.
- Avoid “same item, larger number” as the main progression.
- Avoid early access to advanced mobility that collapses world scale.
- Preserve reasons to revisit changed places and use old knowledge in new ways.

## The first implemented fragment

M9 implements the smallest honest version of "a meaningful step opens a way to
fight" and nothing beyond it.

**Real.** One found object at one fixed place in the world. One explicit verb to
take it. Exactly two weapons, exchanged one for the other, with the site keeping
what the player is not carrying. A difference expressed in dimensions the combat
system already understands — geometry, windup, active window, recovery, damage,
step-in, knockback — and chosen so that neither option dominates. State that
outlives a defeat and not a process.

**Not real, and deliberately not invented.** Vertical strength, levels,
experience, stats, skill trees, builds, equipment slots, an inventory of any
kind, loot, rarity, crafting, currency, respec, a death or failure loop beyond
M7's reset, and every traversal mode in the candidate list above. M9 adds no way
to move that did not exist in M7.

The guardrail this milestone was measured against is the one written above:
**avoid "same item, larger number"**. Damage `32` against `24` is part of the
found weapon's profile, but it was meant not to be the difference — the
difference was to be that `32` fells the adversary in three swings instead of
four while each swing is `37%` more expensive to miss with, and that the weapon
reaches into a band where the M6 adversary cannot answer at all.

**The owner's playtest said the guardrail was not met in play** (2026-09-22):
against the M6 adversary the natural strategy won with either weapon, so the
difference existed and did not matter. The "band where the adversary cannot
answer" is also gone against combat initiative, whose lunge reaches `4.70` —
further than either weapon. The M9 revisit re-measures the guardrail against
that adversary; its pre-gate found that with a clean read the difference
reduces to the number of openings needed, and that the found weapon's longer
commitment is what costs a late reader. Whether that is "same item, larger
number" to a person is the revisit's owner gate to decide.
