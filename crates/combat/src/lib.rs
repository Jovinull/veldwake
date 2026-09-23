//! Deterministic headless combat rules for Veldwake: one weapon, two
//! combatants, one encounter.
//!
//! This crate answers one question: what is happening in this fight right now?
//! It knows nothing about GPUs, windows, cameras, frames, audio devices, files,
//! scheduling or world generation. Advancing a fight is a pure function of the
//! encounter, one player intent, and a ground query.
//!
//! The dependency direction is `voxel <- character <- combat <- client`:
//!
//! - `veldwake-voxel` supplies the dense grid and the exposed-face mesher;
//! - `veldwake-character` owns bodies, poses, gaits and the `GroundSampler`
//!   contract, and this crate consumes all four rather than reimplementing any;
//! - this crate owns combat meaning: the weapon as an object, the attack as a
//!   decision, the hit as a swept query, the consequence, and the one enemy;
//! - the client renders what it is given, answers `GroundSampler` from the
//!   terrain field, submits intent, and reacts to events it never invents.
//!
//! It deliberately does **not** depend on `veldwake-procedural`. What combat
//! needs from the world is the height under a foot, which is the two-method trait
//! `veldwake-character` already declares and the client already implements in
//! eleven lines. [ADR-0005] records the whole boundary argument.
//!
//! # Time
//!
//! **There is no `dt` in this crate.** [`Encounter::step`](encounter::Encounter::step)
//! advances exactly one tick of [`tick::COMBAT_TICK_HZ`], every
//! duration is a [`tick::Ticks`] count, and every window boundary is an
//! integer comparison. `NaN`, negative and infinite time are unrepresentable
//! rather than rejected. [`tick::CombatClock`] is how a client turns
//! wall time into a count of ticks, with integer arithmetic and a preserved
//! sub-tick remainder.
//!
//! The modules read in the order a fight is built:
//!
//! | module | question it answers |
//! | --- | --- |
//! | [`tick`] | what is a unit of authoritative time? |
//! | [`material`] | what does a weapon voxel mean? |
//! | [`weapon`] | what *is* the weapon, as an object? |
//! | [`spec`] | what does a combatant do with it, and for how many ticks? |
//! | [`combatant`] | who is fighting, and what state are they in? |
//! | [`hit`] | did the blade touch the body? |
//! | [`movement`] | may this body be where it is trying to go? |
//! | [`reach`] | how far does this body's blade get while it can connect? |
//! | [`armament`] | which of the two exchangeable weapons is the player holding? |
//! | [`adversary`] | what does the enemy want to do? |
//! | [`event`] | what did this tick do that presentation may react to? |
//! | [`encounter`] | all of the above, once per tick, in a stated order |
//! | [`script`] | a reproducible fight, and the frames worth capturing |
//! | [`fixture`] | which bodies, weapon, tuning and values are locked |
//!
//! [ADR-0005]: https://github.com/Jovinull/veldwake/blob/main/docs/adr/0005-fixed-step-headless-combat-domain.md

mod hash;

pub mod adversary;
pub mod armament;
pub mod combatant;
pub mod encounter;
pub mod event;
pub mod fixture;
pub mod hit;
pub mod hurt;
pub mod material;
pub mod movement;
pub mod oracle;
pub mod reach;
pub mod script;
pub mod spec;
pub mod tick;
pub mod weapon;

pub use adversary::{AdversaryBrain, AdversaryState};
pub use armament::{ArmamentState, RewardSetup, WeaponVariant};
pub use combatant::{
    Action, AttackKind, AttackPhase, Combatant, Health, Intent, SIDES, Side, SwingId,
};
pub use encounter::{
    CombatCounters, CountingGround, Encounter, EncounterError, EncounterSetup, PlayerVictoryPolicy,
    WorldContact,
};
pub use event::{CombatEvent, MAX_EVENTS_PER_TICK, StepEvents};
pub use hit::{Capsule, MAX_SWEEP_SUBSTEPS, Segment, Sweep, SweepHit};
pub use hurt::{HURT_CORE, HURT_MARGIN, HurtVolume};
pub use material::{
    ALL_WEAPON_MATERIALS, CompiledWeaponPalette, FIRST_WEAPON_ID, WEAPON_ID_END, WeaponMaterial,
    WeaponScheme,
};
pub use movement::{MoveBlockReason, MoveRules, TraversalLegality, check_move};
pub use reach::{AttackEnvelope, attack_envelope};
pub use script::{
    EncounterScript, MomentKind, NAMED_MOMENTS, NamedMoment, ScriptRunner, at_moment,
};
pub use spec::{
    ArenaSpec, AttackSpec, AuthoredPressure, AuthoredTuning, CombatSeed, DodgeSpec,
    EncounterTuning, MovementSpec, PressureSpec, SpecError,
};
pub use tick::{COMBAT_TICK_HZ, CombatClock, MAX_TICKS_PER_FRAME, Ticks};
pub use weapon::{
    COMBAT_STYLE_VERSION, CompiledWeapon, WeaponCompiler, WeaponDescriptor, WeaponError,
    WeaponIdentity, WeaponSeed,
};
