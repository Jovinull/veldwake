# ADR-0010: Session-acquired state in the authoritative encounter

- Status: Accepted
- Date: 2026-09-22
- Owners: Veldwake maintainers
- Supersedes: None
- Superseded by: None

## Context

M9 gives the player a choice of weapon: a longblade stands at a fixed site in
the gate, the player carries a different one, and an exchange swaps them. That
introduces the first state in this repository whose lifetime is **longer than an
encounter and shorter than a process**, and the question of where it lives has
to be answered once rather than per consumer.

The constraints are already written down. [ADR-0002](0002-presentation-independent-authority.md)
requires authoritative gameplay to be independent of presentation. `INVARIANTS.md`
COMBAT-002 says presentation reads events and never infers a consequence.
[ADR-0005](0005-fixed-step-headless-combat-domain.md) fixes the actor model at
`[Combatant; 2]` and names a third actor as the trigger to design an entity
model. `AGENTS.md` forbids speculative abstractions. `cache/mod.rs` states in its
own header that the disk cache is not a save and that authoritative persistence
is a separate problem with separate guarantees.

The sharp edge is what the armament *decides*. The weapon in the player's hand
selects the blade segment the hit sweep follows, the attack spec that times the
action, and — since M9 derives it from the wielded geometry — the aim-assist
range. All three are rules. A client that owned the armament would be
presentation choosing what the rules hit.

The second edge is lifetime. M7 gave a defeated player a reset: both bodies
return to their configured starts, health is restored, actions clear and the
adversary's brain goes back to dormant. If that reset also took the weapon back,
the reward would be a loan rather than a reward, and the milestone's product
question — does reaching a place give me something that changes how I play —
would be answered "only until you lose once".

## Decision

**The armament is authoritative state owned by `Encounter` in
`veldwake-combat`.** `armament::ArmamentState` is one enum value naming which of
two weapons the player holds; the site holds the complement. Presentation may
read it and may not write it: `ArmamentState::exchange` is `pub(crate)` and the
encounter's exchange rule is its only caller.

**Its lifetime is the session, not the encounter.** An encounter reset restores
bodies, health, actions, the adversary's brain and the resolution, and
deliberately does not touch the armament. That is **ARM-001** in
`INVARIANTS.md`, asserted by driving a real defeat through the authoritative
loop rather than by reading the code.

**Its lifetime ends with the process.** Nothing is written to disk. A new
process builds a new encounter and the authored initial armament applies again:
the player carries the original weapon and the found one stands at the site.

**The exchange is a rule, not a setter.** An interact intent enters `step`, the
domain checks in order that a reward is configured, that the world offered an
exchange site, that the player can act, and that the site is inside the tuned
interaction radius — then swaps, re-poses the player so the next sweep starts on
the weapon now held, counts it, and publishes exactly one
`CombatEvent::ArmamentSwapped`. A refusal is counted and silent.

**The world's only contribution is a position.** `WorldContact` gains
`weapon_exchange_site: Option<Vec2>` and one constructor that supplies it. The
world says *where*; the domain owns the radius and every other clause.

**The exchange pair is the player's and the site's, and nothing else.** The
adversary carries an independent original weapon that is never part of any
exchange, so a running session holds three weapon instances and exactly two of
them are exchangeable.

The boundary: this covers one weapon exchange at one fixed place. It is not an
inventory, not an item system, not an interaction framework and not persistence.

## Alternatives considered

- **Client-owned armament.** Rejected: the held weapon decides the hit sweep, so
  presentation would be choosing what the rules hit, against ADR-0002,
  COMBAT-002 and ARCH-003. It would also split "what resets" across two crates,
  which is where a reset rule would eventually disagree with itself.
- **Storing the pair `{ player, site }`.** Rejected: with exactly two weapons
  the site is always the complement, so a stored pair makes "both hold the found
  weapon" representable. One value plus a derived accessor cannot disagree with
  itself.
- **An item identifier, a slot, or any collection.** Rejected as the
  overengineering `RISK_REGISTER.md` R-002 names. There are two weapons; a `Vec`
  or a map would be a framework built before its second consumer exists.
- **A generic `Interactable` trait, an interaction provider, or an entity
  query.** Rejected for the same reason and for a sharper one: a generic seam
  invites reuse, and the next thing reused through it would be an entity model
  arriving without the decision ADR-0005 reserves.
- **`WorldContact { site: Option<Vec2> }` under a generic name.** Rejected on
  naming alone. A vague `site` is an invitation to make it mean the next thing
  too; `weapon_exchange_site` cannot be reused without renaming it, which is a
  visible decision.
- **Two optional fields for the found weapon and its attack.** Rejected: it
  makes "a weapon with no swing" and "a swing with no weapon" representable and
  validated. One optional `RewardSetup` group makes both unrepresentable.
- **Making the armament durable now.** Rejected as out of M9's scope and
  premature: persistence needs a versioned format, a migration policy and a
  decision about what else belongs in it. M9 deliberately produces the fact and
  stops. See **Future implications**.
- **Extending `Intent::player` with a fourth parameter.** Rejected: every M6, M7
  and M8 call site means "no interact", and widening the constructor would make
  sixty of them say so for no gain. `Intent::interacting` adds the verb where it
  is used.

## Positive consequences

- The rules own everything the rules depend on, so a weapon choice cannot be
  made by a renderer, a camera or an input handler.
- The whole model is one enum value, and the shape makes an inventory a visible
  decision rather than a drift.
- "What a reset restores" is one function with one deliberate omission and a
  test that drives a real defeat to prove it.
- Every M6, M7 and M8 fixture configures no reward, so the exchange is
  unreachable from them and their locked signatures did not move.
- The distinction between encounter state, session state and durable state is
  now demonstrated rather than theorised, which is the evidence a persistence
  milestone needs before it designs a format.

## Negative consequences

- `Encounter` grew a second weapon, a second attack spec and an armament, so
  "the encounter has one weapon" is no longer true and several accessors now
  take a side.
- Session state that does not survive a restart is a lifetime a player can feel
  as a loss. That is accepted for M9 and is the exact thing a later milestone
  may choose to change.
- The armament is read by presentation every frame to decide which uploaded mesh
  receives which matrix. That is a read-only coupling, but it is a coupling.

## Future implications

- **A persistence milestone starts here.** The fact worth saving is "which
  weapon is the player carrying, and which one is at the site". It is one enum
  value, it is authoritative, and it already has a locked identity in
  `REWARD_BEHAVIOR_SIGNATURE`. Making it durable is a format decision, not a
  model decision.
- **A second exchange site, or a second reward, is the trigger to reconsider the
  model.** Two weapons and one site fit one enum; three sites do not, and that
  is where an item identifier becomes a decision worth making rather than an
  abstraction taken on credit.
- **ADR-0005's third-actor trigger is untouched.** A placed weapon has no tick,
  no brain, no health and no `Side`, and `[Combatant; 2]` is unchanged.
- **An object with collision is a different decision.** M9's weapon is
  non-colliding by design, which is why `TRAVERSAL_RULE_VERSION` did not move.
  The first object that refuses a body needs its own reasoning and probably its
  own invariant.
