//! The client's combat adapter: which encounter, driven by what, at which rate.
//!
//! `veldwake-combat` is headless and takes one player intent per tick. This
//! module is the whole boundary between it and the platform: it turns wall time
//! into a count of ticks, latched keys and a camera basis into intent, and the
//! events a tick published into something presentation can react to.
//!
//! **`VELDWAKE_ENCOUNTER=off` is the default and a contract.** With it unset the
//! client does exactly what it did before M6: the free-fly camera, the M5
//! character selections, no combat simulation, no second body on the GPU, no
//! weapon, and not one extra draw or upload.
//!
//! Two things here are worth stating because the obvious version is wrong.
//!
//! **The accumulator is integer.** [`CombatClock`] converts a `Duration` with
//! `u128` arithmetic and keeps its sub-tick remainder, so the same elapsed time
//! produces the same ticks whatever the frame rate. A float accumulator would
//! make that claim an approximation.
//!
//! **A press is latched, not sampled.** A frame can run no ticks or four, so a
//! key read as a held flag is either lost or repeated. The latch is set on the
//! key-down edge and consumed by the first tick of a frame.

use std::time::Duration;

use glam::Vec3;
use tracing::{info, warn};

use veldwake_character::GroundSampler;
use veldwake_combat::{
    CombatClock, CombatEvent, Encounter, EncounterError, Intent, MAX_EVENTS_PER_TICK,
    MAX_TICKS_PER_FRAME, NamedMoment, ScriptRunner, Side, Ticks, at_moment, fixture,
    script::GOLDEN_SCRIPT,
};
use veldwake_procedural::TerrainGenerator;

use crate::arena;
use crate::camera::Camera;
use crate::input::InputState;

/// Environment variable selecting what the encounter does.
const ENCOUNTER_VARIABLE: &str = "VELDWAKE_ENCOUNTER";

/// How many ticks a moment search may run before giving up.
///
/// Two passes of the reference script. A moment that does not occur in two passes
/// is a moment the script cannot produce, and the probe says so long before a
/// capture does.
/// Largest offset a named moment accepts, in ticks.
///
/// One second at the combat rate, which is longer than any action in the game.
/// A larger number is a mistyped command line rather than a request, and the
/// parser says so instead of searching for a frame that does not exist.
pub const MAX_MOMENT_OFFSET: u32 = veldwake_combat::COMBAT_TICK_HZ;

const MOMENT_SEARCH_TICKS: u64 = fixture::GOLDEN_RUN_TICKS * 2;

/// Most events one frame can carry, which is the tick bound times the tick cap.
pub const MAX_FRAME_EVENTS: usize = MAX_TICKS_PER_FRAME as usize * MAX_EVENTS_PER_TICK;

/// What the client does with the encounter.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum EncounterMode {
    /// No combat at all. This is what the M3, M4 and M5 regression smokes use.
    #[default]
    Off,
    /// Playable. Starts paused and arms itself on the first input, because the
    /// world takes about a minute to stream in and a fight that starts on its own
    /// is over before anybody sees it.
    Armed,
    /// Driven by the reference script, looping, for motion evidence.
    Script,
    /// Driven by the reference script and then **frozen** at a named moment.
    ///
    /// The only way to capture a hit window: it is a tenth of a second long, so
    /// the frame has to be held rather than caught. This is the combat version of
    /// M5's eight frozen gait phases, one run each.
    /// A named moment, frozen, optionally some whole ticks after it.
    ///
    /// The offset is what makes a motion strip possible. A frozen frame is the
    /// only way to photograph a `0.1`-second window, and one frozen frame says
    /// nothing about an arc; `moment:confirmed-hit+6` is the same fight, the
    /// same camera and the same tick arithmetic, six ticks later, so a sequence
    /// of runs reconstructs the swing exactly rather than at whatever interval a
    /// screen capture happened to land on.
    Moment(&'static NamedMoment, u32),
}

impl EncounterMode {
    /// Reads `VELDWAKE_ENCOUNTER`. Unset means off; an unparsable value warns and
    /// falls back rather than failing to start.
    #[must_use]
    pub fn from_environment() -> Self {
        match std::env::var(ENCOUNTER_VARIABLE) {
            Ok(value) => match Self::parse(&value) {
                Some(mode) => mode,
                None => {
                    warn!(%value, "unknown VELDWAKE_ENCOUNTER; using off");
                    Self::Off
                }
            },
            Err(_) => Self::Off,
        }
    }

    /// `off`, `armed`, `script`, `moment:<name>`, or `moment:<name>+<ticks>`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let trimmed = value.trim().to_ascii_lowercase();
        match trimmed.as_str() {
            "" | "off" | "none" => Some(Self::Off),
            "armed" | "play" | "playable" => Some(Self::Armed),
            "script" | "scripted" => Some(Self::Script),
            other => {
                let tail = other
                    .strip_prefix("moment:")
                    .or_else(|| other.strip_prefix("moment="))?;
                let (name, offset) = match tail.split_once('+') {
                    Some((name, ticks)) => (name, ticks.trim().parse::<u32>().ok()?),
                    None => (tail, 0),
                };
                // An offset past the end of an action is a typo, not a request.
                if offset > MAX_MOMENT_OFFSET {
                    return None;
                }
                veldwake_combat::script::moment(name).map(|moment| Self::Moment(moment, offset))
            }
        }
    }

    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Off => "off".to_owned(),
            Self::Armed => "armed".to_owned(),
            Self::Script => "script".to_owned(),
            Self::Moment(moment, 0) => format!("moment:{}", moment.name),
            Self::Moment(moment, offset) => format!("moment:{}+{offset}", moment.name),
        }
    }

    #[must_use]
    pub const fn is_off(self) -> bool {
        matches!(self, Self::Off)
    }

    /// Whether this mode wants a third-person camera following the player.
    #[must_use]
    pub const fn follows_the_player(self) -> bool {
        !matches!(self, Self::Off)
    }

    /// Whether the player drives the fight.
    #[must_use]
    pub const fn is_played(self) -> bool {
        matches!(self, Self::Armed)
    }
}

/// The events one frame's ticks published, bounded by construction.
///
/// The same discipline `StepEvents` uses a tick at a time, for the same reason: a
/// growable buffer makes "no per-frame allocation" a hope rather than a property.
#[derive(Clone, Copy, Debug)]
pub struct FrameEvents {
    items: [Option<CombatEvent>; MAX_FRAME_EVENTS],
    len: usize,
    dropped: u32,
}

impl Default for FrameEvents {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameEvents {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            items: [None; MAX_FRAME_EVENTS],
            len: 0,
            dropped: 0,
        }
    }

    fn clear(&mut self) {
        self.len = 0;
        self.dropped = 0;
    }

    fn push(&mut self, event: CombatEvent) {
        if self.len >= MAX_FRAME_EVENTS {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        self.items[self.len] = Some(event);
        self.len += 1;
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn dropped(&self) -> u32 {
        self.dropped
    }

    pub fn iter(&self) -> impl Iterator<Item = CombatEvent> + '_ {
        self.items[..self.len].iter().filter_map(|slot| *slot)
    }
}

/// How many ticks a frame ran, and what they did.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameOutcome {
    pub ticks: Ticks,
    pub dropped_ticks: u64,
}

/// The client's encounter, its clock, and what drives it.
pub struct EncounterScene {
    mode: EncounterMode,
    encounter: Encounter,
    clock: CombatClock,
    runner: Option<ScriptRunner>,
    /// True once a moment search has found its frame; nothing steps after that.
    frozen: bool,
    /// Which moment was searched for, and whether it was found.
    moment_found: Option<u64>,
    events: FrameEvents,
    ticks: u64,
}

impl EncounterScene {
    /// Builds the encounter for a mode, on the terrain it will be fought on.
    ///
    /// For a moment mode this also *runs* the search, so the frozen frame exists
    /// before the first rendered one and any settle lands on the same picture.
    pub fn new(
        mode: EncounterMode,
        generator: &TerrainGenerator,
        ground: Option<&dyn GroundSampler>,
    ) -> Result<Self, EncounterError> {
        let setup = fixture::setup(arena::centre(), arena::ARENA_RADIUS);
        let encounter = Encounter::new(&setup, ground)?;
        let _ = generator;
        let mut scene = Self {
            mode,
            encounter,
            clock: CombatClock::new(),
            runner: None,
            frozen: false,
            moment_found: None,
            events: FrameEvents::new(),
            ticks: 0,
        };
        match mode {
            EncounterMode::Off | EncounterMode::Armed => {}
            EncounterMode::Script => {
                scene.encounter.arm();
                scene.runner = Some(ScriptRunner::new(
                    GOLDEN_SCRIPT,
                    fixture::reach_of(&scene.encounter),
                ));
            }
            EncounterMode::Moment(moment, offset) => {
                scene.encounter.arm();
                let mut runner =
                    ScriptRunner::new(GOLDEN_SCRIPT, fixture::reach_of(&scene.encounter));
                let mut after = None;
                for _ in 0..MOMENT_SEARCH_TICKS {
                    let intent = runner.next_intent(&scene.encounter);
                    let events = scene.encounter.step(intent, ground);
                    scene.ticks += 1;
                    if let Some(remaining) = after {
                        // Past the moment, running out the offset. The script
                        // keeps driving, so these are the ticks the fight would
                        // have run anyway.
                        if remaining == 0 {
                            scene.moment_found = Some(scene.encounter.tick_index());
                            break;
                        }
                        after = Some(remaining - 1);
                        continue;
                    }
                    if at_moment(moment.kind, &scene.encounter, &events) {
                        if offset == 0 {
                            scene.moment_found = Some(scene.encounter.tick_index());
                            break;
                        }
                        after = Some(offset - 1);
                        continue;
                    }
                    if runner.finished() {
                        runner.restart();
                    }
                }
                scene.frozen = true;
                match scene.moment_found {
                    Some(tick) => info!(
                        moment = moment.name,
                        tick,
                        intent = moment.intent,
                        "encounter frozen at a named moment"
                    ),
                    None => warn!(
                        moment = moment.name,
                        searched = MOMENT_SEARCH_TICKS,
                        "the reference script never reached this moment; showing where it stopped"
                    ),
                }
            }
        }
        Ok(scene)
    }

    #[must_use]
    pub const fn mode(&self) -> EncounterMode {
        self.mode
    }

    #[must_use]
    pub const fn encounter(&self) -> &Encounter {
        &self.encounter
    }

    #[must_use]
    pub const fn events(&self) -> &FrameEvents {
        &self.events
    }

    /// Total ticks this scene has run.
    #[must_use]
    pub const fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Whether the encounter is held at a named moment.
    #[must_use]
    pub const fn is_frozen(&self) -> bool {
        self.frozen
    }

    /// Which tick the named moment was found on, if it was.
    #[must_use]
    pub const fn moment_tick(&self) -> Option<u64> {
        self.moment_found
    }

    #[must_use]
    pub const fn clock(&self) -> &CombatClock {
        &self.clock
    }

    /// The point the camera should follow: the player's stand point.
    #[must_use]
    pub fn camera_target(&self) -> Vec3 {
        self.encounter.combatant(Side::Player).stand_point()
    }

    /// Advances the fight by however many whole ticks this frame is worth.
    pub fn update(
        &mut self,
        elapsed: Duration,
        input: &mut InputState,
        camera: &Camera,
        ground: Option<&dyn GroundSampler>,
    ) -> FrameOutcome {
        self.events.clear();
        if self.frozen {
            // A frozen encounter still consumes the latches. Leaving them set
            // would mean a press made while a moment is held is queued for ever
            // and fires the instant anything unfreezes — and it would make the
            // report line's `input_latched` permanently true, which is how this
            // was noticed in the first real run.
            let _ = input.take_combat_latches();
            return FrameOutcome::default();
        }
        let before = self.clock.dropped();
        let due = self.clock.advance(elapsed);
        let mut latches = input.take_combat_latches();

        for _ in 0..due {
            let intent = match self.runner.as_mut() {
                Some(runner) => {
                    let intent = runner.next_intent(&self.encounter);
                    if runner.finished() {
                        runner.restart();
                    }
                    intent
                }
                None => {
                    // The played path. The latch belongs to the first tick of the
                    // frame: a press is one swing, not one per catch-up tick.
                    let axes = input.movement_axes();
                    let planar =
                        camera.planar_forward() * axes.forward + camera.planar_right() * axes.right;
                    let intent = Intent::player(planar, latches.0, latches.1);
                    latches = (false, false);
                    // A playable encounter arms itself the moment the player does
                    // something, so the settle is not a fight nobody watched.
                    if !self.encounter.is_armed()
                        && (planar.length_squared() > 0.0 || intent.attack() || intent.dodge())
                    {
                        self.encounter.arm();
                        info!("encounter armed by the first input");
                    }
                    intent
                }
            };
            let events = self.encounter.step(intent, ground);
            self.ticks += 1;
            for event in events.iter() {
                self.events.push(event);
            }
        }
        FrameOutcome {
            ticks: due,
            dropped_ticks: self.clock.dropped() - before,
        }
    }

    /// Whether the two bodies are close enough for a swing to matter, for the
    /// report line.
    #[must_use]
    pub fn distance(&self) -> f32 {
        self.encounter.separation_distance()
    }
}

#[cfg(test)]
mod tests {
    use super::{EncounterMode, FrameEvents, MAX_FRAME_EVENTS};
    use veldwake_combat::{CombatEvent, MomentKind, Side};

    #[test]
    fn the_mode_parses_every_name_it_documents_and_refuses_the_rest() {
        assert_eq!(EncounterMode::parse(""), Some(EncounterMode::Off));
        assert_eq!(EncounterMode::parse("off"), Some(EncounterMode::Off));
        assert_eq!(EncounterMode::parse("  NONE "), Some(EncounterMode::Off));
        assert_eq!(EncounterMode::parse("armed"), Some(EncounterMode::Armed));
        assert_eq!(EncounterMode::parse("play"), Some(EncounterMode::Armed));
        assert_eq!(EncounterMode::parse("script"), Some(EncounterMode::Script));
        match EncounterMode::parse("moment:player-active") {
            Some(EncounterMode::Moment(moment, offset)) => {
                assert_eq!(moment.kind, MomentKind::PlayerActive);
                assert_eq!(offset, 0);
            }
            other => panic!("expected a moment, got {other:?}"),
        }
        match EncounterMode::parse("MOMENT=Defeat") {
            Some(EncounterMode::Moment(moment, 0)) => assert_eq!(moment.kind, MomentKind::Defeat),
            other => panic!("expected a moment, got {other:?}"),
        }
        assert!(EncounterMode::parse("moment:not-a-moment").is_none());
        assert!(EncounterMode::parse("fight").is_none());
        // The offset a motion strip is built from, and the ways it can be wrong.
        match EncounterMode::parse("moment:confirmed-hit+6") {
            Some(EncounterMode::Moment(moment, offset)) => {
                assert_eq!(moment.kind, MomentKind::ConfirmedHit);
                assert_eq!(offset, 6);
            }
            other => panic!("expected an offset moment, got {other:?}"),
        }
        assert_eq!(
            EncounterMode::parse("moment:confirmed-hit+0").map(EncounterMode::name),
            Some("moment:confirmed-hit".to_owned()),
            "a zero offset is the moment itself and names itself that way"
        );
        assert_eq!(
            EncounterMode::parse("moment:confirmed-hit+6").map(EncounterMode::name),
            Some("moment:confirmed-hit+6".to_owned())
        );
        assert!(
            EncounterMode::parse("moment:confirmed-hit+99999").is_none(),
            "an offset past any action is a typo"
        );
        assert!(EncounterMode::parse("moment:confirmed-hit+").is_none());
        assert!(EncounterMode::parse("moment:confirmed-hit+-3").is_none());
        assert!(EncounterMode::parse("moment:confirmed-hit+two").is_none());
    }

    #[test]
    fn every_mode_names_itself_and_says_what_it_needs() {
        assert!(EncounterMode::Off.is_off());
        assert!(!EncounterMode::Off.follows_the_player());
        assert!(!EncounterMode::Off.is_played());
        assert!(EncounterMode::Armed.is_played());
        assert!(EncounterMode::Armed.follows_the_player());
        assert!(!EncounterMode::Script.is_played());
        assert!(EncounterMode::Script.follows_the_player());
        for mode in [
            EncounterMode::Off,
            EncounterMode::Armed,
            EncounterMode::Script,
        ] {
            assert!(!mode.name().is_empty());
        }
        assert_eq!(EncounterMode::default(), EncounterMode::Off);
    }

    #[test]
    fn a_frames_events_are_bounded_and_a_drop_is_counted() {
        let mut events = FrameEvents::new();
        assert_eq!(events.len(), 0);
        for _ in 0..MAX_FRAME_EVENTS {
            events.push(CombatEvent::EncounterReset);
        }
        assert_eq!(events.len(), MAX_FRAME_EVENTS);
        assert_eq!(events.dropped(), 0);
        events.push(CombatEvent::Defeated { side: Side::Player });
        assert_eq!(events.len(), MAX_FRAME_EVENTS, "the buffer never grows");
        assert_eq!(events.dropped(), 1);
        assert_eq!(events.iter().count(), MAX_FRAME_EVENTS);
        events.clear();
        assert_eq!(events.len(), 0);
        assert_eq!(events.dropped(), 0);
    }

    #[test]
    fn the_frame_bound_is_the_tick_bound_times_the_tick_cap() {
        // If either bound moves, this is the line that has to be read again.
        assert_eq!(
            MAX_FRAME_EVENTS,
            veldwake_combat::MAX_TICKS_PER_FRAME as usize * veldwake_combat::MAX_EVENTS_PER_TICK
        );
    }
}
