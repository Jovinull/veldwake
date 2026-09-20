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
    AdversaryState, CombatClock, CombatEvent, Encounter, EncounterError, Intent,
    MAX_EVENTS_PER_TICK, MAX_TICKS_PER_FRAME, MomentKind, NamedMoment, ScriptRunner, Side, Ticks,
    WorldContact, at_moment, fixture, script::GOLDEN_SCRIPT,
};
use veldwake_procedural::TerrainGenerator;

use crate::arena;
use crate::camera::Camera;
use crate::input::InputState;
use crate::traversal;

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
    /// **The M7 traversal session.** The player starts at the named route's
    /// start, the adversary stands dormant at its far end, there is no arena
    /// disc, water blocks, and the session continues past the fight.
    Traverse,
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
            "traverse" | "traversal" | "walk" => Some(Self::Traverse),
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
            Self::Traverse => "traverse".to_owned(),
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
        matches!(self, Self::Armed | Self::Traverse)
    }

    /// Whether this mode is a walk across the region rather than a duel in a
    /// clearing.
    ///
    /// Two things follow from it and nothing else does: the streaming demand is
    /// anchored on the body, and a dodge is refused while the adversary is
    /// dormant.
    #[must_use]
    pub const fn is_traversal(self) -> bool {
        matches!(self, Self::Traverse)
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
    /// Ticks a named moment's offset still owes the frame loop.
    pending: u32,
    /// A press that has not reached a tick yet.
    ///
    /// A frame can run no ticks at all — at 144 Hz most of them do — and taking
    /// the input latches on such a frame and then not stepping would throw the
    /// press away. Holding it here until a tick consumes it is what makes "a
    /// press between two ticks is not lost" true rather than intended.
    held: (bool, bool),
    events: FrameEvents,
    ticks: u64,
    /// Dodge requests refused because the adversary was still dormant.
    ///
    /// A dodge outside combat is an explicit M7 non-goal, so the request is
    /// gated at the intent rather than by a new rule in the domain — the client
    /// may choose what to ask for and may never write the encounter. Counted
    /// because a gate that fires on every press, or on none, is a defect either
    /// way and only the number says which.
    dodges_suppressed: u32,
}

impl EncounterScene {
    /// Replays the reference script until a moment happens, and says when.
    ///
    /// Throwaway: the encounter it steps is discarded, because the only thing
    /// wanted from it is the tick index.
    fn search(
        encounter: &mut Encounter,
        kind: MomentKind,
        ground: Option<&dyn GroundSampler>,
    ) -> Option<u64> {
        let mut runner = ScriptRunner::new(GOLDEN_SCRIPT, fixture::reach_of(encounter));
        for _ in 0..MOMENT_SEARCH_TICKS {
            let intent = runner.next_intent(encounter);
            let events = encounter.step(intent, WorldContact::from_ground(ground));
            if at_moment(kind, encounter, &events) {
                return Some(encounter.tick_index());
            }
            if runner.finished() {
                runner.restart();
            }
        }
        None
    }

    /// Builds the encounter for a mode, on the terrain it will be fought on.
    ///
    /// For a moment mode this also *runs* the search, so the frozen frame exists
    /// before the first rendered one and any settle lands on the same picture.
    pub fn new(
        mode: EncounterMode,
        generator: &TerrainGenerator,
        ground: Option<&dyn GroundSampler>,
    ) -> Result<Self, EncounterError> {
        // Where the fight is, and under what bounds, is the one thing the mode
        // changes about the setup.
        let setup = if mode.is_traversal() {
            traversal::traversal_setup()
        } else {
            fixture::setup(arena::centre(), arena::ARENA_RADIUS)
        };
        let encounter = Encounter::new(&setup, ground)?;
        let _ = generator;
        let mut scene = Self {
            mode,
            encounter,
            clock: CombatClock::new(),
            runner: None,
            frozen: false,
            pending: 0,
            held: (false, false),
            moment_found: None,
            events: FrameEvents::new(),
            ticks: 0,
            dodges_suppressed: 0,
        };
        match mode {
            EncounterMode::Off | EncounterMode::Armed | EncounterMode::Traverse => {}
            EncounterMode::Script => {
                scene.encounter.arm();
                scene.runner = Some(ScriptRunner::new(
                    GOLDEN_SCRIPT,
                    fixture::reach_of(&scene.encounter),
                ));
            }
            EncounterMode::Moment(moment, offset) => {
                // Two passes over the same deterministic script, and the second
                // one stops one tick short.
                //
                // A moment is only recognisable from the tick that produced it,
                // so a single pass necessarily consumes that tick's events
                // before the frame loop exists — and the first frozen capture of
                // a hit proved what that costs: zero chips, zero camera
                // strikes, a contact frame with no impact in it. So the first
                // pass finds *which* tick, and the second replays to the tick
                // before it and hands the rest to the frame loop, which runs
                // them through exactly the path a played tick takes. The
                // presentation then sees the hit as a hit.
                //
                // Both passes are headless and the script is deterministic, so
                // the second arrives at the same fight as the first.
                scene.encounter.arm();
                let found = Self::search(&mut scene.encounter, moment.kind, ground);
                scene.encounter = Encounter::new(&setup, ground)?;
                scene.encounter.arm();
                let mut runner =
                    ScriptRunner::new(GOLDEN_SCRIPT, fixture::reach_of(&scene.encounter));
                if let Some(tick) = found {
                    for _ in 0..tick.saturating_sub(1) {
                        let intent = runner.next_intent(&scene.encounter);
                        let _ = scene
                            .encounter
                            .step(intent, WorldContact::from_ground(ground));
                        scene.ticks += 1;
                        if runner.finished() {
                            runner.restart();
                        }
                    }
                    scene.moment_found = Some(tick);
                    // The moment's own tick, then the offset.
                    scene.pending = offset.saturating_add(1);
                }
                scene.runner = Some(runner);
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

    /// Whether a press is waiting for a tick to consume it.
    ///
    /// Reported rather than inspected: a latch that is still set several report
    /// intervals later is a stuck input, and the report is how that is seen.
    #[must_use]
    pub const fn held_input(&self) -> (bool, bool) {
        self.held
    }

    /// Ticks still owed to a named moment's offset.
    ///
    /// Run by the frame loop, a frame's worth at a time, before the scene
    /// freezes. Thirty frames at the largest offset the parser accepts, which is
    /// invisible inside a settle measured in seconds.
    #[must_use]
    pub const fn pending(&self) -> u32 {
        self.pending
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

    /// Where the player's body is standing.
    ///
    /// One source for everything that follows the body: the third-person
    /// camera, and in a traversal session the streaming demand anchor.
    #[must_use]
    pub fn player_stand_point(&self) -> Vec3 {
        self.encounter.combatant(Side::Player).stand_point()
    }

    /// The point the camera should follow: the player's stand point.
    #[must_use]
    pub fn camera_target(&self) -> Vec3 {
        self.player_stand_point()
    }

    /// Dodge requests refused because the adversary was dormant.
    #[must_use]
    pub const fn dodges_suppressed(&self) -> u32 {
        self.dodges_suppressed
    }

    /// Whether the adversary is dormant right now.
    #[must_use]
    pub fn adversary_is_dormant(&self) -> bool {
        self.encounter.brain().state() == AdversaryState::Idle
    }

    /// Advances the fight by however many whole ticks this frame is worth.
    pub fn update(
        &mut self,
        elapsed: Duration,
        input: &mut InputState,
        camera: &Camera,
        world: WorldContact<'_>,
    ) -> FrameOutcome {
        self.events.clear();
        if self.frozen && self.pending > 0 {
            // The offset a named moment asked for, run through the ordinary
            // tick path so that the events, the chips and the camera all see it.
            let _ = input.take_combat_latches();
            let due = self.pending.min(MAX_TICKS_PER_FRAME);
            for _ in 0..due {
                let intent = match self.runner.as_mut() {
                    Some(runner) => {
                        let intent = runner.next_intent(&self.encounter);
                        if runner.finished() {
                            runner.restart();
                        }
                        intent
                    }
                    None => Intent::player(glam::Vec2::ZERO, false, false),
                };
                let events = self.encounter.step(intent, world);
                self.ticks += 1;
                for event in events.iter() {
                    self.events.push(event);
                }
            }
            self.pending -= due;
            return FrameOutcome {
                ticks: due,
                dropped_ticks: 0,
            };
        }
        if self.frozen {
            // A frozen encounter still consumes the latches. Leaving them set
            // would mean a press made while a moment is held is queued for ever
            // and fires the instant anything unfreezes — and it would make the
            // report line's `input_latched` permanently true, which is how this
            // was noticed in the first real run.
            let _ = input.take_combat_latches();
            self.held = (false, false);
            return FrameOutcome::default();
        }
        let before = self.clock.dropped();
        let due = self.clock.advance(elapsed);
        // Whatever was pressed since the last tick, plus whatever is still held
        // over from frames that ran none.
        let taken = input.take_combat_latches();
        self.held = (self.held.0 || taken.0, self.held.1 || taken.1);
        let mut latches = self.held;
        if due > 0 {
            self.held = (false, false);
        }

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
                    // **Read inside the loop, never above it.** A frame can run
                    // four ticks, and the first of them can be the one that
                    // crosses the aggro radius; the three after it have to see
                    // the adversary awake. Hoisting this would make a dodge
                    // depend on how the frames happened to be cut, which is the
                    // one thing the integer clock exists to prevent.
                    let dodge = if self.mode.is_traversal() && self.adversary_is_dormant() {
                        if latches.1 {
                            // Consumed, not held: a press kept here would fire
                            // the instant the adversary woke, which is the stuck
                            // latch the first played M6 run produced.
                            self.dodges_suppressed = self.dodges_suppressed.saturating_add(1);
                        }
                        false
                    } else {
                        latches.1
                    };
                    let intent = Intent::player(planar, latches.0, dodge);
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
            let events = self.encounter.step(intent, world);
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
    use super::{EncounterMode, EncounterScene, FrameEvents, MAX_FRAME_EVENTS};
    use crate::camera::Camera;
    use crate::character::TerrainGround;
    use crate::input::{CameraAction, CombatAction, InputState};
    use std::time::Duration;
    use veldwake_character::ground::FlatGround;
    use veldwake_combat::MAX_TICKS_PER_FRAME;
    use veldwake_combat::{CombatEvent, MomentKind, Side, WorldContact};
    use veldwake_procedural::TerrainGenerator;

    // -----------------------------------------------------------------------
    // M7 traversal
    // -----------------------------------------------------------------------

    /// A camera whose forward axis points along a planar direction, so a test
    /// can walk a body somewhere specific through the ordinary intent path
    /// rather than by writing a position.
    fn camera_facing(direction: glam::Vec2) -> Camera {
        let yaw = direction.x.atan2(-direction.y);
        Camera::at(glam::Vec3::ZERO, yaw.to_degrees(), 0.0)
    }

    /// A traversal scene on the golden region, ready to be driven.
    fn traversal_scene(generator: &TerrainGenerator) -> EncounterScene {
        let ground = TerrainGround::new(generator);
        match EncounterScene::new(EncounterMode::Traverse, generator, Some(&ground)) {
            Ok(scene) => scene,
            Err(error) => panic!("the traversal encounter did not build: {error}"),
        }
    }

    #[test]
    fn the_traversal_mode_parses_and_names_itself() {
        for spelling in ["traverse", "traversal", "walk", " TRAVERSE "] {
            assert_eq!(
                EncounterMode::parse(spelling),
                Some(EncounterMode::Traverse),
                "{spelling} did not select traversal"
            );
        }
        assert_eq!(EncounterMode::Traverse.name(), "traverse");
        assert!(EncounterMode::Traverse.is_traversal());
        assert!(EncounterMode::Traverse.is_played());
        assert!(EncounterMode::Traverse.follows_the_player());
        assert!(!EncounterMode::Traverse.is_off());
        // And no other mode claims to be one.
        for mode in [
            EncounterMode::Off,
            EncounterMode::Armed,
            EncounterMode::Script,
        ] {
            assert!(!mode.is_traversal(), "{} claimed traversal", mode.name());
        }
    }

    #[test]
    fn a_traversal_session_starts_a_route_apart_with_no_arena() {
        let setup = crate::traversal::traversal_setup();
        assert!(
            setup.arena.is_none(),
            "a walk across a valley must not be fenced by a disc"
        );
        assert_eq!(
            setup.player_victory,
            veldwake_combat::PlayerVictoryPolicy::Remain,
            "a session that resets on victory is not a session"
        );
        let separation = (setup.starts[0] - setup.starts[1]).length();
        assert!(
            separation > 100.0,
            "the two bodies start {separation} apart, which is not a traversal"
        );
        // And the armed fixture is untouched: M6 keeps its disc and its reset.
        let armed = veldwake_combat::fixture::golden_setup();
        assert!(armed.arena.is_some());
        assert_eq!(
            armed.player_victory,
            veldwake_combat::PlayerVictoryPolicy::ResetEncounter
        );
    }

    #[test]
    fn a_traversal_session_places_the_bodies_where_the_route_says() {
        let generator = TerrainGenerator::golden();
        let scene = traversal_scene(&generator);
        let encounter = scene.encounter();
        let player = encounter.combatant(Side::Player).position();
        let adversary = encounter.combatant(Side::Adversary).position();
        let expected_player = crate::traversal::column_centre(
            crate::traversal::ROUTE_START_X,
            crate::traversal::ROUTE_START_Z,
        );
        let expected_adversary = crate::traversal::column_centre(
            crate::traversal::ADVERSARY_COLUMN.0,
            crate::traversal::ADVERSARY_COLUMN.1,
        );
        assert!((player - expected_player).length() < 1.0e-3);
        assert!((adversary - expected_adversary).length() < 1.0e-3);
        // Far apart means dormant, which is the point of placing them so.
        assert!(scene.adversary_is_dormant());
        assert_eq!(scene.dodges_suppressed(), 0);
    }

    #[test]
    fn a_dodge_is_refused_while_the_adversary_sleeps_and_allowed_once_it_wakes() {
        let generator = TerrainGenerator::golden();
        let ground = TerrainGround::new(&generator);
        let veto = crate::traversal::TerrainWalkability::new(&generator);
        let world = WorldContact::terrain(&ground, &veto);
        let mut scene = traversal_scene(&generator);
        let mut input = InputState::default();
        let camera = Camera::default();

        // Dormant: the press is consumed and refused, not held for later.
        input.set_combat_action(CombatAction::Dodge, true);
        input.set_combat_action(CombatAction::Dodge, false);
        let outcome = scene.update(Duration::from_millis(20), &mut input, &camera, world);
        assert!(outcome.ticks > 0);
        assert_eq!(scene.dodges_suppressed(), 1, "the dodge was not refused");
        assert_eq!(
            scene.held_input(),
            (false, false),
            "a refused press must be consumed, or it fires the moment the enemy wakes"
        );
        assert!(
            !scene
                .encounter()
                .combatant(Side::Player)
                .action()
                .is_dodging(),
            "a dodge happened outside combat"
        );

        // The armed arena mode never gates: the adversary is always awake there
        // and M6's behaviour is unchanged.
        let mut armed = match EncounterScene::new(EncounterMode::Armed, &generator, Some(&ground)) {
            Ok(scene) => scene,
            Err(error) => panic!("{error}"),
        };
        let mut input = InputState::default();
        input.set_combat_action(CombatAction::Dodge, true);
        input.set_combat_action(CombatAction::Dodge, false);
        let _ = armed.update(Duration::from_millis(20), &mut input, &camera, world);
        assert_eq!(
            armed.dodges_suppressed(),
            0,
            "the arena mode must not gate anything"
        );
        assert!(
            armed
                .encounter()
                .combatant(Side::Player)
                .action()
                .is_dodging(),
            "the arena dodge stopped working"
        );
    }

    #[test]
    fn the_dodge_gate_is_read_inside_the_tick_loop_not_above_it() {
        // The defect this test exists to prevent: hoisting `brain.state()` out
        // of the loop would make a frame that runs four ticks judge all four
        // against the state before the first. A frame can cross the aggro
        // radius on its first tick, and the other three have to see the
        // adversary awake — otherwise a dodge depends on how the frames
        // happened to be cut, which is exactly what the integer clock exists to
        // prevent.
        let generator = TerrainGenerator::golden();
        let ground = TerrainGround::new(&generator);
        let veto = crate::traversal::TerrainWalkability::new(&generator);
        let world = WorldContact::terrain(&ground, &veto);
        let mut scene = traversal_scene(&generator);
        let mut input = InputState::default();

        // Walk the player up to the adversary **along the derived route**, in
        // frames of one tick. A straight line at it does not work and the
        // reason is the milestone's own: the river is in the way, and water
        // blocks. The route is the path the rules say exists.
        let grid = crate::traversal::SurfaceGrid::sample(&generator);
        let movement = *scene.encounter().tuning().movement();
        let report = crate::traversal::audit(
            &grid,
            &movement,
            (
                crate::traversal::ROUTE_START_X,
                crate::traversal::ROUTE_START_Z,
            ),
        );
        let Some(route) =
            crate::traversal::derive_route(&grid, &report, crate::traversal::ADVERSARY_COLUMN)
        else {
            panic!("the locked adversary column is not reachable");
        };

        let aggro = veldwake_combat::fixture::adversary().aggro_radius;
        let goal = scene.encounter().combatant(Side::Adversary).position();
        let mut waypoint = 1_usize;
        let mut crossed_in_a_multi_tick_frame = false;
        let mut suppressed_before = 0;
        for _ in 0..200_000_u32 {
            let player = scene.encounter().combatant(Side::Player).position();
            let distance = (goal - player).length();
            // The brain decides *before* movement inside a tick, so a frame
            // that only closes the last hair of the gap wakes nothing: the
            // decision it makes is the one from before the step. Standing
            // within about two ticks of the boundary means the wake lands on
            // the second or third tick of the frame, with the first still
            // dormant — which is the arrangement this test needs.
            if distance <= aggro + 0.06 && distance > aggro {
                // The frame that will cross: four ticks at once, with a dodge
                // pressed. The first tick is still dormant, so the press is
                // refused exactly once; the ticks after it must not be able to
                // refuse anything, because by then the adversary is awake.
                suppressed_before = scene.dodges_suppressed();
                input.set_combat_action(CombatAction::Dodge, true);
                input.set_combat_action(CombatAction::Dodge, false);
                let camera = camera_facing((goal - player).normalize_or_zero());
                let outcome =
                    scene.update(Duration::from_micros(33_400), &mut input, &camera, world);
                assert_eq!(
                    outcome.ticks,
                    veldwake_combat::MAX_TICKS_PER_FRAME,
                    "the crossing frame did not run the whole catch-up cap"
                );
                crossed_in_a_multi_tick_frame = true;
                break;
            }
            let target = match route.waypoints.get(waypoint) {
                Some(target) => *target,
                // Past the last waypoint, head straight at the body.
                None => goal,
            };
            if (target - player).length() < 0.35 && waypoint < route.waypoints.len() {
                waypoint += 1;
                continue;
            }
            let camera = camera_facing((target - player).normalize_or_zero());
            input.set_action(CameraAction::Forward, true);
            let _ = scene.update(Duration::from_micros(8_400), &mut input, &camera, world);
        }
        assert!(
            crossed_in_a_multi_tick_frame,
            "the player never reached the aggro boundary"
        );
        assert!(
            !scene.adversary_is_dormant(),
            "the four-tick frame did not wake the adversary"
        );
        assert_eq!(
            scene.dodges_suppressed(),
            suppressed_before + 1,
            "the press was judged more than once, or judged against a stale brain state"
        );
    }

    /// Everything a traversal tick decides, compared bit for bit.
    ///
    /// Streaming, the camera and the renderer are deliberately absent: the
    /// anchor is sampled once per rendered frame, so its sequence is a function
    /// of the partition and is not authoritative. `DETERMINISM.md` says so.
    #[derive(Debug, Eq, PartialEq)]
    struct AuthoritativeState {
        scene_ticks: u64,
        tick_index: u64,
        domain_ticks: u64,
        player_x: u32,
        player_z: u32,
        player_facing: u32,
        player_phase: u32,
        player_base_height: u32,
        player_health: u16,
        player_action: &'static str,
        adversary_x: u32,
        adversary_z: u32,
        adversary_health: u16,
        brain: &'static str,
        decisions: u32,
    }

    fn snapshot(scene: &EncounterScene) -> AuthoritativeState {
        let encounter = scene.encounter();
        let player = encounter.combatant(Side::Player);
        let adversary = encounter.combatant(Side::Adversary);
        // Bit patterns, not tolerances: the claim is that two runs are the same
        // run, and a tolerance would be admitting they are not.
        AuthoritativeState {
            scene_ticks: scene.ticks(),
            tick_index: encounter.tick_index(),
            domain_ticks: encounter.counters().ticks,
            player_x: player.position().x.to_bits(),
            player_z: player.position().y.to_bits(),
            player_facing: player.state().facing.to_bits(),
            player_phase: player.state().phase.to_bits(),
            player_base_height: player.state().base_height.to_bits(),
            player_health: player.health().current(),
            player_action: player.action().label(encounter.attack_spec(Side::Player)),
            adversary_x: adversary.position().x.to_bits(),
            adversary_z: adversary.position().y.to_bits(),
            adversary_health: adversary.health().current(),
            brain: encounter.brain().state().name(),
            decisions: encounter.brain().decisions(),
        }
    }

    /// Walks a traversal session for an exact total, cut into `rate` frames a
    /// second, delivering every nanosecond of it.
    ///
    /// The remainder is distributed across the frames rather than truncated,
    /// so the run really is the stated duration. M6's partition probe truncated
    /// and reported `19.99` seconds as twenty; this does not repeat that.
    fn walk_partition(
        generator: &TerrainGenerator,
        world: WorldContact<'_>,
        rate: u32,
        seconds: u64,
    ) -> EncounterScene {
        let mut scene = traversal_scene(generator);
        let mut input = InputState::default();
        let goal = scene.encounter().combatant(Side::Adversary).position();
        let player = scene.encounter().combatant(Side::Player).position();
        let camera = camera_facing((goal - player).normalize_or_zero());
        input.set_action(CameraAction::Forward, true);

        let total = Duration::from_secs(seconds).as_nanos();
        let frames = u128::from(rate) * u128::from(seconds);
        let mut delivered = 0_u128;
        for frame in 1..=frames {
            let until = total * frame / frames;
            let step = until - delivered;
            delivered = until;
            let Ok(step) = u64::try_from(step) else {
                panic!("a frame longer than a u64 of nanoseconds");
            };
            let _ = scene.update(Duration::from_nanos(step), &mut input, &camera, world);
        }
        assert_eq!(
            delivered, total,
            "partition {rate} did not deliver {seconds} s"
        );
        scene
    }

    #[test]
    fn traversal_is_the_same_walk_at_every_rate_the_catch_up_cap_allows() {
        // "It uses the M6 clock" is an argument. This is the evidence: the same
        // exact elapsed time and the same input trace, cut into four different
        // frame partitions, must leave the authoritative state identical.
        //
        // Thirty hertz is not in this list and its absence is deliberate: a
        // thirtieth of a second is exactly `MAX_TICKS_PER_FRAME` ticks, so the
        // clock's remainder carry periodically asks for a fifth and the cap
        // discards it. That is a real property of the cap rather than of
        // traversal, and `a_thirty_hertz_frame_sits_exactly_on_the_catch_up_cap`
        // is where it is stated.
        let generator = TerrainGenerator::golden();
        let ground = TerrainGround::new(&generator);
        let veto = crate::traversal::TerrainWalkability::new(&generator);
        let world = WorldContact::terrain(&ground, &veto);

        let mut results = Vec::new();
        for rate in [50_u32, 60, 144, 240] {
            let scene = walk_partition(&generator, world, rate, 20);
            assert_eq!(
                scene.clock().dropped(),
                0,
                "partition {rate} hit the catch-up cap, so it is not a clean comparison"
            );
            results.push((rate, snapshot(&scene)));
        }
        let Some((_, reference)) = results.first() else {
            panic!("no partitions ran");
        };
        assert_eq!(
            reference.scene_ticks, 2_400,
            "twenty seconds is 2,400 ticks at 120 Hz"
        );
        for (rate, state) in &results {
            assert_eq!(
                state, reference,
                "partition {rate} reached a different authoritative state"
            );
        }
        // And the walk actually happened, so the comparison is not of two
        // bodies standing still.
        let start = crate::traversal::column_centre(
            crate::traversal::ROUTE_START_X,
            crate::traversal::ROUTE_START_Z,
        );
        let walked = (f32::from_bits(reference.player_x) - start.x).abs()
            + (f32::from_bits(reference.player_z) - start.y).abs();
        assert!(
            walked > 30.0,
            "the partition test compared a body that only moved {walked}"
        );
    }

    #[test]
    fn a_thirty_hertz_frame_sits_exactly_on_the_catch_up_cap() {
        // A thirtieth of a second is `120 / 30 = 4` ticks, and
        // `MAX_TICKS_PER_FRAME` is `4`. A frame is a whole number of
        // nanoseconds, so an exact partition of twenty seconds into six hundred
        // frames cannot give every frame exactly four ticks: the clock carries a
        // remainder, and about one frame in twenty-five asks for five. The cap
        // discards the fifth and counts it.
        //
        // The consequence is the one M6 chose deliberately — the simulation
        // slows rather than teleporting — and this states it with a number
        // instead of leaving it to be discovered in a playtest.
        let generator = TerrainGenerator::golden();
        let ground = TerrainGround::new(&generator);
        let veto = crate::traversal::TerrainWalkability::new(&generator);
        let world = WorldContact::terrain(&ground, &veto);

        let thirty = walk_partition(&generator, world, 30, 20);
        let dropped = thirty.clock().dropped();
        let ran = thirty.ticks();
        assert!(
            dropped > 0,
            "thirty hertz no longer hits the cap; this test is stale"
        );
        assert_eq!(
            ran + dropped,
            2_400,
            "the ticks that ran plus the ticks the cap discarded must be the ticks the time was worth"
        );

        // The ticks it *did* run are the same walk. A clean partition stopped
        // at the same tick count reaches the same authoritative state, so the
        // cap slows the simulation without changing it.
        let mut clean = traversal_scene(&generator);
        let mut input = InputState::default();
        let goal = clean.encounter().combatant(Side::Adversary).position();
        let player = clean.encounter().combatant(Side::Player).position();
        let camera = camera_facing((goal - player).normalize_or_zero());
        input.set_action(CameraAction::Forward, true);
        while clean.ticks() < ran {
            let _ = clean.update(
                Duration::from_nanos(1_000_000_000 / 240),
                &mut input,
                &camera,
                world,
            );
        }
        assert_eq!(clean.ticks(), ran, "the clean partition overshot");
        assert_eq!(clean.clock().dropped(), 0);
        assert_eq!(
            snapshot(&clean),
            snapshot(&thirty),
            "the catch-up cap changed the walk rather than only slowing it"
        );
    }

    #[test]
    fn a_press_between_two_ticks_is_not_lost() {
        // The property §9 asks for, and the defect that hid behind it. A frame
        // can run no ticks — at 144 Hz most do not — and the first version took
        // the input latches before checking, so a press on such a frame went
        // straight in the bin. It is held until a tick consumes it.
        let ground = FlatGround::at(0.0);
        let generator = TerrainGenerator::golden();
        let mut scene = match EncounterScene::new(EncounterMode::Armed, &generator, Some(&ground)) {
            Ok(scene) => scene,
            Err(error) => panic!("the encounter did not build: {error}"),
        };
        let mut input = InputState::default();
        let camera = Camera::default();

        // A frame far too short to produce a tick, with an attack pressed in it.
        input.set_combat_action(CombatAction::Attack, true);
        let outcome = scene.update(
            Duration::from_micros(100),
            &mut input,
            &camera,
            WorldContact::ground_only(&ground),
        );
        assert_eq!(outcome.ticks, 0, "a tenth of a millisecond produced a tick");
        assert_eq!(
            scene.held_input(),
            (true, false),
            "the press was thrown away on a frame that ran no ticks"
        );

        // Several more such frames: it is still waiting, not multiplied.
        for _ in 0..20 {
            let outcome = scene.update(
                Duration::from_micros(100),
                &mut input,
                &camera,
                WorldContact::ground_only(&ground),
            );
            assert_eq!(outcome.ticks, 0);
        }
        assert_eq!(scene.held_input(), (true, false));

        // Now a frame long enough to tick. The press is consumed exactly once,
        // and the swing it started is the only one.
        let outcome = scene.update(
            Duration::from_millis(40),
            &mut input,
            &camera,
            WorldContact::ground_only(&ground),
        );
        assert!(outcome.ticks > 0, "forty milliseconds produced no tick");
        assert_eq!(
            scene.held_input(),
            (false, false),
            "the press was not consumed"
        );
        let swings: usize = scene
            .events()
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    CombatEvent::SwingStarted {
                        side: Side::Player,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(swings, 1, "one press produced {swings} swings");
    }

    #[test]
    fn one_press_with_four_catch_up_ticks_is_still_one_swing() {
        // The other half: a frame that runs the whole catch-up cap must not
        // turn one press into four.
        let ground = FlatGround::at(0.0);
        let generator = TerrainGenerator::golden();
        let mut scene = match EncounterScene::new(EncounterMode::Armed, &generator, Some(&ground)) {
            Ok(scene) => scene,
            Err(error) => panic!("the encounter did not build: {error}"),
        };
        let mut input = InputState::default();
        let camera = Camera::default();

        input.set_combat_action(CombatAction::Attack, true);
        // Long enough to owe far more ticks than the cap allows.
        let outcome = scene.update(
            Duration::from_millis(250),
            &mut input,
            &camera,
            WorldContact::ground_only(&ground),
        );
        assert_eq!(
            outcome.ticks, MAX_TICKS_PER_FRAME,
            "a 250 ms frame did not hit the catch-up cap"
        );
        assert!(outcome.dropped_ticks > 0, "the excess was not counted");
        let swings: usize = scene
            .events()
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    CombatEvent::SwingStarted {
                        side: Side::Player,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(
            swings, 1,
            "one press produced {swings} swings across four ticks"
        );
    }

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
