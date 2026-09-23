//! Combat tuning: authored in seconds, compiled to ticks, then immutable.
//!
//! The split this module exists for is the one the weapon module refuses to
//! blur. A [`crate::weapon::WeaponDescriptor`] says what the weapon *is*; an
//! [`AttackSpec`] says what a combatant *does* with it. M6 has one weapon and
//! two attack specs — the player's and the adversary's — and the adversary's
//! telegraph is two and a half times longer precisely because it is a property
//! of the decision rather than of the steel.
//!
//! Every duration is authored in seconds, because that is the unit a person
//! tunes in, and **compiled to ticks by a validating constructor** before it can
//! reach the runtime. After compilation there is no way to express a duration
//! the simulation cannot run.

use glam::Vec2;

use crate::tick::{
    DurationError, Ticks, optional_ticks_from_seconds, seconds_for, ticks_from_seconds,
};

/// Why a tuning cannot be compiled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpecError {
    /// An authored duration is unusable.
    Duration {
        field: &'static str,
        error: DurationError,
    },
    /// A distance or a speed is not a finite positive number.
    Distance { field: &'static str, value: f32 },
    /// A value has to be inside a range and is not.
    Range {
        field: &'static str,
        value: f32,
        low: f32,
        high: f32,
    },
    /// An attack that cannot hurt anybody is a configuration error.
    NoDamage,
    /// A body with no health cannot fight.
    NoHealth,
    /// The adversary's ranges contradict each other.
    RangesCross { min: f32, strike: f32, aggro: f32 },
    /// A pressure attack's selection band is empty, starts inside the primary
    /// attack's strike range, or reaches past the aggro radius.
    PressureBand {
        min: f32,
        max: f32,
        strike: f32,
        aggro: f32,
    },
    /// A pressure lunge's travel is longer than the windup and active window it
    /// has to end with.
    LungeTooLong { lunge: Ticks, available: Ticks },
    /// A lunge that connected would recover more slowly than one that missed,
    /// which inverts the opening the attack exists to create.
    ConnectOutlastsWhiff { connect: Ticks, whiff: Ticks },
}

impl std::fmt::Display for SpecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duration { field, error } => write!(formatter, "{field}: {error}"),
            Self::Distance { field, value } => {
                write!(
                    formatter,
                    "{field} is {value}, not a finite positive length"
                )
            }
            Self::Range {
                field,
                value,
                low,
                high,
            } => write!(formatter, "{field} is {value}, outside {low}..={high}"),
            Self::NoDamage => write!(formatter, "an attack must do damage"),
            Self::NoHealth => write!(formatter, "a combatant must have health"),
            Self::RangesCross { min, strike, aggro } => write!(
                formatter,
                "adversary ranges must satisfy min {min} < strike {strike} <= aggro {aggro}"
            ),
            Self::PressureBand {
                min,
                max,
                strike,
                aggro,
            } => write!(
                formatter,
                "pressure band must satisfy strike {strike} < min {min} < max {max} <= aggro {aggro}"
            ),
            Self::LungeTooLong { lunge, available } => write!(
                formatter,
                "a lunge of {lunge} ticks does not fit the {available} ticks of windup and active"
            ),
            Self::ConnectOutlastsWhiff { connect, whiff } => write!(
                formatter,
                "a connected lunge recovers in {connect} ticks, longer than a whiffed one's {whiff}"
            ),
        }
    }
}

impl std::error::Error for SpecError {}

fn duration(field: &'static str, seconds: f64) -> Result<Ticks, SpecError> {
    ticks_from_seconds(seconds).map_err(|error| SpecError::Duration { field, error })
}

fn optional_duration(field: &'static str, seconds: f64) -> Result<Ticks, SpecError> {
    optional_ticks_from_seconds(seconds).map_err(|error| SpecError::Duration { field, error })
}

fn positive(field: &'static str, value: f32) -> Result<f32, SpecError> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(SpecError::Distance { field, value })
    }
}

fn non_negative(field: &'static str, value: f32) -> Result<f32, SpecError> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(SpecError::Distance { field, value })
    }
}

fn within(field: &'static str, value: f32, low: f32, high: f32) -> Result<f32, SpecError> {
    if value.is_finite() && value >= low && value <= high {
        Ok(value)
    } else {
        Err(SpecError::Range {
            field,
            value,
            low,
            high,
        })
    }
}

/// One attack as a person tunes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AuthoredAttack {
    /// Anticipation. For the adversary this is the telegraph, and it is the only
    /// warning the player gets.
    pub windup_seconds: f64,
    /// The window the blade can connect in.
    pub active_seconds: f64,
    /// Commitment: the attacker can neither move properly nor act.
    pub recovery_seconds: f64,
    /// How long a victim cannot act after being hit.
    pub stagger_seconds: f64,
    /// How long both combatants' action timers freeze on a confirmed hit.
    pub hitstop_seconds: f64,
    pub damage: u16,
    /// World units the attacker steps forward across windup and active.
    pub step_in: f32,
    /// World units the victim is pushed away from the attacker.
    pub knockback: f32,
}

impl AuthoredAttack {
    /// Validates and compiles to ticks.
    pub fn compile(&self) -> Result<AttackSpec, SpecError> {
        if self.damage == 0 {
            return Err(SpecError::NoDamage);
        }
        let recovery = duration("recovery", self.recovery_seconds)?;
        Ok(AttackSpec {
            windup: duration("windup", self.windup_seconds)?,
            active: duration("active", self.active_seconds)?,
            recovery,
            // A swing's recovery does not depend on what it met. Only a
            // pressure lunge authors a different one; see `AuthoredPressure`.
            connect_recovery: recovery,
            stagger: duration("stagger", self.stagger_seconds)?,
            hitstop: optional_duration("hitstop", self.hitstop_seconds)?,
            damage: self.damage,
            step_in_bits: non_negative("step_in", self.step_in)?.to_bits(),
            knockback_bits: non_negative("knockback", self.knockback)?.to_bits(),
            lunge_bits: 0.0_f32.to_bits(),
            lunge_ticks: 0,
        })
    }
}

/// One attack, in ticks.
///
/// `recovery` is the recovery after a swing that met nothing. Every historical
/// attack recovers the same way whatever it met, so for them
/// `connect_recovery == recovery` and `lunge` is zero, and nothing about them
/// differs from before either field existed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttackSpec {
    windup: Ticks,
    active: Ticks,
    recovery: Ticks,
    connect_recovery: Ticks,
    stagger: Ticks,
    hitstop: Ticks,
    damage: u16,
    step_in_bits: u32,
    knockback_bits: u32,
    lunge_bits: u32,
    lunge_ticks: Ticks,
}

// `step_in` and `knockback` are stored as bit patterns so the whole spec can
// derive `Eq` and be compared and hashed exactly, which a locked fixture needs.
impl AttackSpec {
    #[must_use]
    pub const fn windup(&self) -> Ticks {
        self.windup
    }

    #[must_use]
    pub const fn active(&self) -> Ticks {
        self.active
    }

    #[must_use]
    pub const fn recovery(&self) -> Ticks {
        self.recovery
    }

    #[must_use]
    pub const fn stagger(&self) -> Ticks {
        self.stagger
    }

    #[must_use]
    pub const fn hitstop(&self) -> Ticks {
        self.hitstop
    }

    #[must_use]
    pub const fn damage(&self) -> u16 {
        self.damage
    }

    #[must_use]
    pub fn step_in(&self) -> f32 {
        f32::from_bits(self.step_in_bits)
    }

    #[must_use]
    pub fn knockback(&self) -> f32 {
        f32::from_bits(self.knockback_bits)
    }

    /// Recovery after a swing that connected.
    ///
    /// Equal to [`Self::recovery`] for every attack except a pressure lunge.
    #[must_use]
    pub const fn connect_recovery(&self) -> Ticks {
        self.connect_recovery
    }

    /// The recovery a swing actually runs, given whether it connected.
    #[must_use]
    pub const fn recovery_for(&self, connected: bool) -> Ticks {
        if connected {
            self.connect_recovery
        } else {
            self.recovery
        }
    }

    /// World units the body travels along its committed line, ending with the
    /// active window. Zero for every attack but a pressure lunge.
    #[must_use]
    pub fn lunge(&self) -> f32 {
        f32::from_bits(self.lunge_bits)
    }

    /// How many ticks the lunge's travel lasts.
    #[must_use]
    pub const fn lunge_ticks(&self) -> Ticks {
        self.lunge_ticks
    }

    /// Whether the body is travelling on its lunge at this elapsed tick.
    ///
    /// Half open like every other window: the travel is the last
    /// `lunge_ticks` before `active_end`, so it can start inside the windup —
    /// the committed body leaving — and always finishes when the blade stops
    /// being able to connect.
    #[must_use]
    pub const fn is_lunging(&self, elapsed: Ticks) -> bool {
        self.lunge_ticks > 0
            && elapsed < self.active_end()
            && elapsed + self.lunge_ticks >= self.active_end()
    }

    /// Ticks from the start of the swing to the end of recovery.
    ///
    /// The recovery counted is the whiffed one, which is the longest a swing
    /// can last; see [`Self::total_for`].
    #[must_use]
    pub const fn total(&self) -> Ticks {
        self.windup + self.active + self.recovery
    }

    /// Ticks from the start of the swing to the end of the recovery it
    /// actually runs.
    #[must_use]
    pub const fn total_for(&self, connected: bool) -> Ticks {
        self.windup + self.active + self.recovery_for(connected)
    }

    /// First tick of the active window.
    #[must_use]
    pub const fn active_start(&self) -> Ticks {
        self.windup
    }

    /// One past the last tick of the active window.
    #[must_use]
    pub const fn active_end(&self) -> Ticks {
        self.windup + self.active
    }

    /// Whether the given elapsed tick is inside the active window.
    ///
    /// Half open, explicitly: the tick at `active_end` is already recovery.
    #[must_use]
    pub const fn is_active(&self, elapsed: Ticks) -> bool {
        elapsed >= self.active_start() && elapsed < self.active_end()
    }

    /// The windup as a fraction of the whole action, for the pose layer.
    #[must_use]
    pub fn windup_fraction(&self) -> f32 {
        self.windup as f32 / self.total().max(1) as f32
    }

    /// The active window as a fraction of the whole action.
    #[must_use]
    pub fn active_fraction(&self) -> f32 {
        self.active as f32 / self.total().max(1) as f32
    }

    /// Seconds the whole action lasts, for reporting.
    #[must_use]
    pub fn total_seconds(&self) -> f32 {
        seconds_for(self.total())
    }
}

/// A dodge as a person tunes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AuthoredDodge {
    pub duration_seconds: f64,
    pub cooldown_seconds: f64,
    /// World units travelled over the whole dodge.
    pub distance: f32,
}

impl AuthoredDodge {
    pub fn compile(&self) -> Result<DodgeSpec, SpecError> {
        Ok(DodgeSpec {
            duration: duration("dodge_duration", self.duration_seconds)?,
            cooldown: optional_duration("dodge_cooldown", self.cooldown_seconds)?,
            distance_bits: positive("dodge_distance", self.distance)?.to_bits(),
        })
    }
}

/// A dodge, in ticks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DodgeSpec {
    duration: Ticks,
    cooldown: Ticks,
    distance_bits: u32,
}

impl DodgeSpec {
    #[must_use]
    pub const fn duration(&self) -> Ticks {
        self.duration
    }

    #[must_use]
    pub const fn cooldown(&self) -> Ticks {
        self.cooldown
    }

    #[must_use]
    pub fn distance(&self) -> f32 {
        f32::from_bits(self.distance_bits)
    }

    /// Average speed of the dodge, in world units per second.
    #[must_use]
    pub fn speed(&self) -> f32 {
        self.distance() / seconds_for(self.duration).max(1.0e-4)
    }
}

/// How a body moves, as a person tunes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AuthoredMovement {
    /// World units per second at full intent.
    pub speed: f32,
    /// Radians per second a body may turn.
    pub turn_rate: f32,
    /// Largest rise a body may walk onto, in world units.
    pub max_step_up: f32,
    /// Largest drop a body may walk down, in world units.
    pub max_drop: f32,
    /// How much of `speed` is available during an attack's recovery.
    pub recovery_speed_scale: f32,
}

impl AuthoredMovement {
    pub fn compile(&self) -> Result<MovementSpec, SpecError> {
        Ok(MovementSpec {
            speed: positive("speed", self.speed)?,
            turn_rate: positive("turn_rate", self.turn_rate)?,
            max_step_up: positive("max_step_up", self.max_step_up)?,
            max_drop: positive("max_drop", self.max_drop)?,
            recovery_speed_scale: within(
                "recovery_speed_scale",
                self.recovery_speed_scale,
                0.0,
                1.0,
            )?,
        })
    }
}

/// How a body moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementSpec {
    speed: f32,
    turn_rate: f32,
    max_step_up: f32,
    max_drop: f32,
    recovery_speed_scale: f32,
}

impl MovementSpec {
    #[must_use]
    pub const fn speed(&self) -> f32 {
        self.speed
    }

    #[must_use]
    pub const fn turn_rate(&self) -> f32 {
        self.turn_rate
    }

    #[must_use]
    pub const fn max_step_up(&self) -> f32 {
        self.max_step_up
    }

    #[must_use]
    pub const fn max_drop(&self) -> f32 {
        self.max_drop
    }

    #[must_use]
    pub const fn recovery_speed_scale(&self) -> f32 {
        self.recovery_speed_scale
    }
}

/// The adversary's ranges and pauses, as a person tunes them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AuthoredAdversary {
    /// Centre-to-centre distance at which it notices the player.
    pub aggro_radius: f32,
    /// Centre-to-centre distance at which it commits to a swing.
    pub strike_range: f32,
    /// Closer than this it backs off instead of swinging.
    pub min_range: f32,
    pub approach_speed: f32,
    pub reposition_speed: f32,
    pub recover_seconds: f64,
    pub reposition_seconds: f64,
}

impl AuthoredAdversary {
    pub fn compile(&self) -> Result<AdversarySpec, SpecError> {
        let aggro = positive("aggro_radius", self.aggro_radius)?;
        let strike = positive("strike_range", self.strike_range)?;
        let min = non_negative("min_range", self.min_range)?;
        if !(min < strike && strike <= aggro) {
            return Err(SpecError::RangesCross { min, strike, aggro });
        }
        Ok(AdversarySpec {
            aggro_radius: aggro,
            strike_range: strike,
            min_range: min,
            approach_speed: positive("approach_speed", self.approach_speed)?,
            reposition_speed: positive("reposition_speed", self.reposition_speed)?,
            recover: duration("recover", self.recover_seconds)?,
            reposition: duration("reposition", self.reposition_seconds)?,
        })
    }
}

/// The adversary's ranges and pauses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdversarySpec {
    aggro_radius: f32,
    strike_range: f32,
    min_range: f32,
    approach_speed: f32,
    reposition_speed: f32,
    recover: Ticks,
    reposition: Ticks,
}

impl AdversarySpec {
    #[must_use]
    pub const fn aggro_radius(&self) -> f32 {
        self.aggro_radius
    }

    #[must_use]
    pub const fn strike_range(&self) -> f32 {
        self.strike_range
    }

    #[must_use]
    pub const fn min_range(&self) -> f32 {
        self.min_range
    }

    #[must_use]
    pub const fn approach_speed(&self) -> f32 {
        self.approach_speed
    }

    #[must_use]
    pub const fn reposition_speed(&self) -> f32 {
        self.reposition_speed
    }

    #[must_use]
    pub const fn recover(&self) -> Ticks {
        self.recover
    }

    #[must_use]
    pub const fn reposition(&self) -> Ticks {
        self.reposition
    }
}

/// The adversary's second attack, as a person tunes it: the pressure lunge.
///
/// A committed thrust along a line that is fixed when the windup starts, taken
/// from middle distance. It exists so the adversary can act before a player
/// has closed to where the historical primary attack is interrupted every time
/// (`docs/planning/COMBAT_INITIATIVE.md`). Three things set it apart from the
/// primary, and each is a field here rather than a special case elsewhere:
///
/// - **travel**: the body covers `lunge_distance` over the last
///   `lunge_seconds` of the windup and active window, so the threat arrives with
///   the blade rather than walking into the opponent's reach ahead of it;
/// - **outcome-dependent recovery**: `attack.recovery_seconds` is the recovery
///   after a lunge that met nothing and `connect_recovery_seconds` the one after
///   a lunge that connected, so a miss leaves the body exposed and a hit does
///   not;
/// - **selection**: the brain may choose it only when the other body is between
///   `select_min` and `select_max`, centre to centre;
/// - **spacing**: the dodge the adversary takes after its own stagger or its
///   own connected lunge. It is an ordinary dodge — the same action, the same
///   movement rules, no invulnerability — authored for the adversary because
///   the distance it has to open is the distance back into the band, which the
///   player's dodge was never tuned for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AuthoredPressure {
    pub attack: AuthoredAttack,
    pub connect_recovery_seconds: f64,
    pub lunge_distance: f32,
    pub lunge_seconds: f64,
    pub select_min: f32,
    pub select_max: f32,
    pub spacing: AuthoredDodge,
}

impl AuthoredPressure {
    /// Validates and compiles to ticks.
    ///
    /// The band is checked against the primary attack's ranges by
    /// [`AuthoredTuning::compile`], which is the only place that sees both.
    pub fn compile(&self) -> Result<PressureSpec, SpecError> {
        let mut attack = self.attack.compile()?;
        let connect = duration("connect_recovery", self.connect_recovery_seconds)?;
        if connect > attack.recovery {
            return Err(SpecError::ConnectOutlastsWhiff {
                connect,
                whiff: attack.recovery,
            });
        }
        let lunge_ticks = duration("lunge", self.lunge_seconds)?;
        if lunge_ticks > attack.active_end() {
            return Err(SpecError::LungeTooLong {
                lunge: lunge_ticks,
                available: attack.active_end(),
            });
        }
        attack.connect_recovery = connect;
        attack.lunge_bits = positive("lunge_distance", self.lunge_distance)?.to_bits();
        attack.lunge_ticks = lunge_ticks;
        let min = positive("select_min", self.select_min)?;
        let max = positive("select_max", self.select_max)?;
        Ok(PressureSpec {
            attack,
            select_min: min,
            select_max: max,
            spacing: self.spacing.compile()?,
        })
    }
}

/// The pressure lunge, in ticks, and the distances it is chosen from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PressureSpec {
    attack: AttackSpec,
    select_min: f32,
    select_max: f32,
    spacing: DodgeSpec,
}

impl PressureSpec {
    #[must_use]
    pub const fn attack(&self) -> &AttackSpec {
        &self.attack
    }

    #[must_use]
    pub const fn select_min(&self) -> f32 {
        self.select_min
    }

    /// The dodge the adversary spaces with.
    #[must_use]
    pub const fn spacing(&self) -> &DodgeSpec {
        &self.spacing
    }

    #[must_use]
    pub const fn select_max(&self) -> f32 {
        self.select_max
    }

    /// Whether a centre-to-centre distance is one the lunge may be chosen from.
    ///
    /// Closed at both ends, so the band the fixtures author is exactly the band
    /// the no-bluff evidence walks.
    #[must_use]
    pub fn selects(&self, distance: f32) -> bool {
        distance.is_finite() && distance >= self.select_min && distance <= self.select_max
    }
}

/// Where the fight happens.
///
/// A fixture, not a game rule: the encounter is a bounded circle so a slice can
/// prove combat before anything can navigate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArenaSpec {
    centre: Vec2,
    radius: f32,
}

impl ArenaSpec {
    pub fn new(centre: Vec2, radius: f32) -> Result<Self, SpecError> {
        if !centre.is_finite() {
            return Err(SpecError::Distance {
                field: "arena_centre",
                value: centre.x,
            });
        }
        Ok(Self {
            centre,
            radius: positive("arena_radius", radius)?,
        })
    }

    #[must_use]
    pub const fn centre(&self) -> Vec2 {
        self.centre
    }

    #[must_use]
    pub const fn radius(&self) -> f32 {
        self.radius
    }

    /// Whether a planar position is inside the arena.
    #[must_use]
    pub fn contains(&self, position: Vec2) -> bool {
        position.is_finite()
            && (position - self.centre).length_squared() <= self.radius * self.radius
    }
}

/// The seed every deterministic choice in an encounter derives from.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CombatSeed(pub u64);

impl CombatSeed {
    /// The seed every recorded encounter refers to.
    pub const GOLDEN: Self = Self(0x5645_4c44_434f_4d42);

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Everything one encounter is tuned by, as a person authors it.
///
/// Combat numbers only. **Where in a world an encounter happens is not tuning**,
/// so the two bodies' initial positions and the optional arena live on
/// [`EncounterSetup`](crate::encounter::EncounterSetup) instead. Tuning that
/// carried an arena centre made one value mean the arena's middle, the enemy's
/// site, the player's respawn and the reset point at once, which stopped being
/// readable the moment the two bodies started a walk apart rather than a duel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AuthoredTuning {
    pub player_attack: AuthoredAttack,
    pub adversary_attack: AuthoredAttack,
    pub dodge: AuthoredDodge,
    pub movement: AuthoredMovement,
    pub adversary: AuthoredAdversary,
    /// The adversary's second attack and the spacing that goes with it.
    ///
    /// `None` in every historical fixture, which is why none of their
    /// signatures moved when it was added. Present only in the combat
    /// initiative setups, where it is the whole capability: the lunge, the
    /// outcome-dependent recovery, and the spacing dodge the brain takes after
    /// its own stagger or its own connected lunge.
    pub adversary_pressure: Option<AuthoredPressure>,
    pub player_health: u16,
    pub adversary_health: u16,
    pub defeat_hold_seconds: f64,
    pub seed: CombatSeed,
}

impl AuthoredTuning {
    pub fn compile(&self) -> Result<EncounterTuning, SpecError> {
        if self.player_health == 0 || self.adversary_health == 0 {
            return Err(SpecError::NoHealth);
        }
        let adversary = self.adversary.compile()?;
        let adversary_pressure = match self.adversary_pressure {
            Some(authored) => {
                let pressure = authored.compile()?;
                // Two attacks, two distances, and they may not overlap: inside
                // strike range is the primary's, and the band has to end
                // before the adversary stops noticing the player at all.
                if !(adversary.strike_range() < pressure.select_min()
                    && pressure.select_min() < pressure.select_max()
                    && pressure.select_max() <= adversary.aggro_radius())
                {
                    return Err(SpecError::PressureBand {
                        min: pressure.select_min(),
                        max: pressure.select_max(),
                        strike: adversary.strike_range(),
                        aggro: adversary.aggro_radius(),
                    });
                }
                Some(pressure)
            }
            None => None,
        };
        Ok(EncounterTuning {
            player_attack: self.player_attack.compile()?,
            adversary_attack: self.adversary_attack.compile()?,
            dodge: self.dodge.compile()?,
            movement: self.movement.compile()?,
            adversary,
            adversary_pressure,
            player_health: self.player_health,
            adversary_health: self.adversary_health,
            defeat_hold: duration("defeat_hold", self.defeat_hold_seconds)?,
            seed: self.seed,
        })
    }
}

/// Everything one encounter is tuned by, in ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EncounterTuning {
    player_attack: AttackSpec,
    adversary_attack: AttackSpec,
    dodge: DodgeSpec,
    movement: MovementSpec,
    adversary: AdversarySpec,
    adversary_pressure: Option<PressureSpec>,
    player_health: u16,
    adversary_health: u16,
    defeat_hold: Ticks,
    seed: CombatSeed,
}

impl EncounterTuning {
    #[must_use]
    pub const fn player_attack(&self) -> &AttackSpec {
        &self.player_attack
    }

    #[must_use]
    pub const fn adversary_attack(&self) -> &AttackSpec {
        &self.adversary_attack
    }

    #[must_use]
    pub const fn dodge(&self) -> &DodgeSpec {
        &self.dodge
    }

    #[must_use]
    pub const fn movement(&self) -> &MovementSpec {
        &self.movement
    }

    #[must_use]
    pub const fn adversary(&self) -> &AdversarySpec {
        &self.adversary
    }

    /// The adversary's pressure lunge, when this encounter has one.
    #[must_use]
    pub const fn adversary_pressure(&self) -> Option<&PressureSpec> {
        self.adversary_pressure.as_ref()
    }

    #[must_use]
    pub const fn player_health(&self) -> u16 {
        self.player_health
    }

    #[must_use]
    pub const fn adversary_health(&self) -> u16 {
        self.adversary_health
    }

    #[must_use]
    pub const fn defeat_hold(&self) -> Ticks {
        self.defeat_hold
    }

    #[must_use]
    pub const fn seed(&self) -> CombatSeed {
        self.seed
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ArenaSpec, AuthoredAdversary, AuthoredAttack, AuthoredDodge, AuthoredMovement,
        AuthoredPressure, CombatSeed, SpecError,
    };
    use crate::tick::{COMBAT_TICK_HZ, DurationError};
    use glam::Vec2;

    fn attack() -> AuthoredAttack {
        AuthoredAttack {
            windup_seconds: 0.18,
            active_seconds: 0.10,
            recovery_seconds: 0.34,
            stagger_seconds: 0.30,
            hitstop_seconds: 0.07,
            damage: 24,
            step_in: 0.35,
            knockback: 0.35,
        }
    }

    #[test]
    fn an_authored_attack_compiles_to_exact_tick_counts() {
        let spec = match attack().compile() {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(spec.windup(), 22, "0.18 s at 120 Hz");
        assert_eq!(spec.active(), 12, "0.10 s at 120 Hz");
        assert_eq!(spec.recovery(), 41, "0.34 s at 120 Hz");
        assert_eq!(spec.stagger(), 36);
        assert_eq!(spec.hitstop(), 8);
        assert_eq!(spec.total(), 75);
        assert_eq!(spec.damage(), 24);
        assert!((spec.step_in() - 0.35).abs() < 1.0e-6);
        assert!((spec.knockback() - 0.35).abs() < 1.0e-6);
        assert!((spec.total_seconds() - 75.0 / 120.0).abs() < 1.0e-6);
    }

    #[test]
    fn the_active_window_is_half_open_and_its_boundaries_are_exact() {
        let spec = match attack().compile() {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(spec.active_start(), 22);
        assert_eq!(spec.active_end(), 34);
        assert!(!spec.is_active(21), "the last windup tick is not active");
        assert!(spec.is_active(22), "the first active tick is active");
        assert!(spec.is_active(33), "the last active tick is active");
        assert!(!spec.is_active(34), "the first recovery tick is not active");
        assert!(!spec.is_active(0));
        assert!(!spec.is_active(spec.total()));
        // Exactly `active` ticks are active, counted rather than asserted.
        let counted = (0..spec.total())
            .filter(|tick| spec.is_active(*tick))
            .count();
        assert_eq!(counted as u32, spec.active());
    }

    #[test]
    fn the_phase_fractions_land_on_the_tick_boundaries_the_rules_use() {
        let spec = match attack().compile() {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        };
        let total = spec.total() as f32;
        assert!((spec.windup_fraction() - spec.windup() as f32 / total).abs() < 1.0e-6);
        assert!((spec.active_fraction() - spec.active() as f32 / total).abs() < 1.0e-6);
        assert!(spec.windup_fraction() + spec.active_fraction() < 1.0);
    }

    #[test]
    fn an_attack_with_no_damage_or_a_bad_duration_is_rejected() {
        let mut authored = attack();
        authored.damage = 0;
        assert_eq!(authored.compile(), Err(SpecError::NoDamage));
        let mut authored = attack();
        authored.windup_seconds = 0.0;
        assert_eq!(
            authored.compile(),
            Err(SpecError::Duration {
                field: "windup",
                error: DurationError::TooShort { seconds: 0.0 }
            })
        );
        let mut authored = attack();
        authored.active_seconds = f64::NAN;
        assert!(matches!(
            authored.compile(),
            Err(SpecError::Duration {
                field: "active",
                ..
            })
        ));
        let mut authored = attack();
        authored.step_in = f32::NAN;
        assert!(matches!(
            authored.compile(),
            Err(SpecError::Distance {
                field: "step_in",
                ..
            })
        ));
        let mut authored = attack();
        authored.knockback = -1.0;
        assert!(matches!(
            authored.compile(),
            Err(SpecError::Distance {
                field: "knockback",
                ..
            })
        ));
    }

    #[test]
    fn a_hitstop_may_be_switched_off_but_a_phase_may_not() {
        let mut authored = attack();
        authored.hitstop_seconds = 0.0;
        match authored.compile() {
            Ok(spec) => assert_eq!(spec.hitstop(), 0),
            Err(error) => panic!("{error}"),
        }
        let mut authored = attack();
        authored.recovery_seconds = 0.0;
        assert!(matches!(
            authored.compile(),
            Err(SpecError::Duration {
                field: "recovery",
                ..
            })
        ));
    }

    #[test]
    fn the_adversary_telegraphs_for_longer_than_the_player_does() {
        // Not a coincidence to be preserved by luck: the whole readability claim
        // rests on it, so it is asserted where the two specs are built.
        let player = match attack().compile() {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        };
        let adversary = match (AuthoredAttack {
            windup_seconds: 0.45,
            ..attack()
        })
        .compile()
        {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        };
        assert!(adversary.windup() > player.windup() * 2);
    }

    #[test]
    fn a_dodge_compiles_and_reports_its_own_speed() {
        let spec = match (AuthoredDodge {
            duration_seconds: 0.30,
            cooldown_seconds: 0.25,
            distance: 2.2,
        })
        .compile()
        {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(spec.duration(), 36);
        assert_eq!(spec.cooldown(), 30);
        assert!((spec.distance() - 2.2).abs() < 1.0e-6);
        assert!(
            (spec.speed() - 2.2 / 0.3).abs() < 0.05,
            "speed {}",
            spec.speed()
        );
        assert!(matches!(
            (AuthoredDodge {
                duration_seconds: 0.3,
                cooldown_seconds: 0.0,
                distance: 0.0,
            })
            .compile(),
            Err(SpecError::Distance { .. })
        ));
    }

    #[test]
    fn movement_rejects_impossible_tuning() {
        let good = AuthoredMovement {
            speed: 3.4,
            turn_rate: 9.0,
            max_step_up: 1.0,
            max_drop: 2.0,
            recovery_speed_scale: 0.25,
        };
        match good.compile() {
            Ok(spec) => {
                assert!((spec.speed() - 3.4).abs() < 1.0e-6);
                assert!((spec.turn_rate() - 9.0).abs() < 1.0e-6);
                assert!((spec.max_step_up() - 1.0).abs() < 1.0e-6);
                assert!((spec.max_drop() - 2.0).abs() < 1.0e-6);
                assert!((spec.recovery_speed_scale() - 0.25).abs() < 1.0e-6);
            }
            Err(error) => panic!("{error}"),
        }
        assert!(matches!(
            AuthoredMovement { speed: 0.0, ..good }.compile(),
            Err(SpecError::Distance { field: "speed", .. })
        ));
        assert!(matches!(
            AuthoredMovement {
                recovery_speed_scale: 1.5,
                ..good
            }
            .compile(),
            Err(SpecError::Range { .. })
        ));
    }

    #[test]
    fn the_adversary_ranges_must_not_cross() {
        let good = AuthoredAdversary {
            aggro_radius: 14.0,
            strike_range: 2.6,
            min_range: 1.6,
            approach_speed: 2.4,
            reposition_speed: 1.8,
            recover_seconds: 0.5,
            reposition_seconds: 0.8,
        };
        assert!(good.compile().is_ok());
        assert!(matches!(
            AuthoredAdversary {
                min_range: 3.0,
                ..good
            }
            .compile(),
            Err(SpecError::RangesCross { .. })
        ));
        assert!(matches!(
            AuthoredAdversary {
                strike_range: 20.0,
                ..good
            }
            .compile(),
            Err(SpecError::RangesCross { .. })
        ));
    }

    #[test]
    fn an_arena_knows_what_is_inside_it() {
        let arena = match ArenaSpec::new(Vec2::new(-69.0, 49.0), 7.0) {
            Ok(arena) => arena,
            Err(error) => panic!("{error}"),
        };
        assert!(arena.contains(Vec2::new(-69.0, 49.0)));
        assert!(arena.contains(Vec2::new(-63.0, 49.0)));
        assert!(!arena.contains(Vec2::new(-61.0, 49.0)));
        assert!(!arena.contains(Vec2::new(f32::NAN, 49.0)));
        assert!((arena.radius() - 7.0).abs() < 1.0e-6);
        assert_eq!(arena.centre(), Vec2::new(-69.0, 49.0));
        assert!(ArenaSpec::new(Vec2::ZERO, 0.0).is_err());
        assert!(ArenaSpec::new(Vec2::splat(f32::NAN), 1.0).is_err());
    }

    #[test]
    fn a_seed_is_explicit_and_the_golden_one_is_named() {
        assert_eq!(CombatSeed::GOLDEN.raw(), 0x5645_4c44_434f_4d42);
        assert_ne!(CombatSeed::GOLDEN, CombatSeed(0));
    }

    #[test]
    fn a_tick_count_cannot_be_invented_at_a_call_site() {
        // `AttackSpec` has no public constructor and no public fields, so the
        // only way to obtain one is through the validating compile above. This
        // test exists to state the intent; the compiler enforces it.
        let spec = match attack().compile() {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(
            spec.total(),
            spec.windup() + spec.active() + spec.recovery()
        );
        assert_eq!(COMBAT_TICK_HZ, 120);
    }

    #[test]
    fn every_error_prints_something_specific() {
        for error in [
            SpecError::Duration {
                field: "windup",
                error: DurationError::NotFinite,
            },
            SpecError::Distance {
                field: "speed",
                value: -1.0,
            },
            SpecError::Range {
                field: "scale",
                value: 2.0,
                low: 0.0,
                high: 1.0,
            },
            SpecError::NoDamage,
            SpecError::NoHealth,
            SpecError::RangesCross {
                min: 3.0,
                strike: 2.0,
                aggro: 1.0,
            },
        ] {
            assert!(!error.to_string().is_empty());
        }
    }

    #[test]
    fn specs_compare_exactly_so_a_fixture_can_lock_one() {
        let first = match attack().compile() {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        };
        let second = match attack().compile() {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(first, second);
        let different = match (AuthoredAttack {
            damage: 25,
            ..attack()
        })
        .compile()
        {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        };
        assert_ne!(first, different);
    }

    #[test]
    fn every_historical_attack_recovers_the_same_whatever_it_meets() {
        for authored in [
            crate::fixture::player_attack(),
            crate::fixture::adversary_attack(),
        ] {
            let spec = match authored.compile() {
                Ok(spec) => spec,
                Err(error) => panic!("{error}"),
            };
            assert_eq!(spec.connect_recovery(), spec.recovery());
            assert_eq!(spec.total_for(true), spec.total());
            assert_eq!(spec.total_for(false), spec.total());
            assert_eq!(spec.lunge_ticks(), 0);
            assert!(spec.lunge().abs() < f32::EPSILON);
            assert!((0..=spec.total()).all(|tick| !spec.is_lunging(tick)));
        }
    }

    #[test]
    fn a_pressure_lunge_compiles_and_refuses_what_would_invert_it() {
        let authored = crate::fixture::pressure();
        let pressure = match authored.compile() {
            Ok(pressure) => pressure,
            Err(error) => panic!("{error}"),
        };
        let attack = pressure.attack();
        assert!(attack.connect_recovery() < attack.recovery());
        assert!(attack.total_for(true) < attack.total_for(false));
        // The travel is the last ticks of the windup and the whole active
        // window, and nothing before.
        let lunging: Vec<_> = (0..=attack.total())
            .filter(|tick| attack.is_lunging(*tick))
            .collect();
        assert_eq!(lunging.len() as u32, attack.lunge_ticks());
        assert_eq!(lunging.last().copied(), Some(attack.active_end() - 1));
        assert!(pressure.selects(pressure.select_min()));
        assert!(pressure.selects(pressure.select_max()));
        assert!(!pressure.selects(pressure.select_min() - 0.01));
        assert!(!pressure.selects(pressure.select_max() + 0.01));
        assert!(!pressure.selects(f32::NAN));

        let inverted = AuthoredPressure {
            connect_recovery_seconds: 2.0,
            ..authored
        };
        assert!(matches!(
            inverted.compile(),
            Err(SpecError::ConnectOutlastsWhiff { .. })
        ));
        let too_long = AuthoredPressure {
            lunge_seconds: 5.0,
            ..authored
        };
        assert!(matches!(
            too_long.compile(),
            Err(SpecError::LungeTooLong { .. })
        ));

        // The band belongs between the primary's strike range and the aggro
        // radius, and only the tuning sees both.
        let mut tuning = crate::fixture::initiative_tuning();
        assert!(tuning.compile().is_ok());
        tuning.adversary_pressure = Some(AuthoredPressure {
            select_min: 1.0,
            ..authored
        });
        assert!(matches!(
            tuning.compile(),
            Err(SpecError::PressureBand { .. })
        ));
        tuning.adversary_pressure = Some(AuthoredPressure {
            select_max: 99.0,
            ..authored
        });
        assert!(matches!(
            tuning.compile(),
            Err(SpecError::PressureBand { .. })
        ));
    }
}
