//! What one tick did, in a container whose bound is part of its type.
//!
//! The first design of this was a `Vec<CombatEvent>` with reserved capacity,
//! which proves nothing: a bug that produces one event too many reallocates
//! silently, and the "no per-frame allocation" claim quietly becomes false. A
//! fixed array plus a length makes the bound a property of the representation,
//! makes overflow representable and counted, and lets every fixture assert that
//! the overflow counter is zero.
//!
//! Presentation reads these and never invents one. A particle burst, an impact
//! sound and a camera shake all originate from the same [`CombatEvent::Hit`], so
//! a miss cannot shake the camera and a hit cannot be silent.

use glam::{Vec2, Vec3};

use crate::combatant::{SIDES, Side, SwingId};

/// Most events one tick can publish.
///
/// The worst real tick publishes three per side — one swing lifecycle event, one
/// hit, and one of stagger or defeat — plus a reset, so seven. Twelve leaves
/// room without pretending the bound does not exist.
pub const MAX_EVENTS_PER_TICK: usize = 12;

/// Something the rules did that presentation may react to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CombatEvent {
    /// A swing began. For the adversary this is the telegraph starting, and it
    /// is the cue a pre-swing sound and accent effect hang from.
    SwingStarted { side: Side, swing: SwingId },
    /// A swing's blade became able to connect.
    SwingActive { side: Side, swing: SwingId },
    /// A swing's active window closed without touching anything.
    SwingWhiffed { side: Side, swing: SwingId },
    /// A blade connected. The single origin of every reaction.
    Hit {
        attacker: Side,
        victim: Side,
        /// A point on the struck body's surface, in world space.
        point: Vec3,
        damage: u16,
        /// The victim's health after the hit.
        remaining: u16,
        /// Planar direction from the attacker to the victim.
        from: Vec2,
    },
    /// A dodge began.
    DodgeStarted { side: Side, direction: Vec2 },
    /// A body was staggered by a hit.
    Staggered { side: Side },
    /// A body ran out of health.
    Defeated { side: Side },
    /// The encounter returned to its starting state.
    EncounterReset,
}

impl CombatEvent {
    /// Which side this event is about, for a report line.
    #[must_use]
    pub const fn subject(&self) -> Option<Side> {
        match self {
            Self::SwingStarted { side, .. }
            | Self::SwingActive { side, .. }
            | Self::SwingWhiffed { side, .. }
            | Self::DodgeStarted { side, .. }
            | Self::Staggered { side }
            | Self::Defeated { side } => Some(*side),
            Self::Hit { attacker, .. } => Some(*attacker),
            Self::EncounterReset => None,
        }
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SwingStarted { .. } => "swing-started",
            Self::SwingActive { .. } => "swing-active",
            Self::SwingWhiffed { .. } => "swing-whiffed",
            Self::Hit { .. } => "hit",
            Self::DodgeStarted { .. } => "dodge-started",
            Self::Staggered { .. } => "staggered",
            Self::Defeated { .. } => "defeated",
            Self::EncounterReset => "encounter-reset",
        }
    }
}

/// The events of one tick, bounded by construction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepEvents {
    items: [Option<CombatEvent>; MAX_EVENTS_PER_TICK],
    len: u8,
    dropped: u8,
}

impl Default for StepEvents {
    fn default() -> Self {
        Self::new()
    }
}

impl StepEvents {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            items: [None; MAX_EVENTS_PER_TICK],
            len: 0,
            dropped: 0,
        }
    }

    /// Adds an event, or counts a drop when the tick is already full.
    ///
    /// Dropping rather than growing is deliberate: a bounded container that
    /// silently allocates is not bounded, and a counter that says an event was
    /// lost is more useful than a heap allocation nobody sees.
    pub fn push(&mut self, event: CombatEvent) -> bool {
        let index = self.len as usize;
        if index >= MAX_EVENTS_PER_TICK {
            self.dropped = self.dropped.saturating_add(1);
            return false;
        }
        self.items[index] = Some(event);
        self.len += 1;
        true
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// How many events this tick could not hold. Must be zero.
    #[must_use]
    pub const fn dropped(&self) -> u8 {
        self.dropped
    }

    /// The events, in the order the tick produced them.
    pub fn iter(&self) -> impl Iterator<Item = CombatEvent> + '_ {
        self.items[..self.len as usize]
            .iter()
            .filter_map(|slot| *slot)
    }

    /// Whether any event satisfies a predicate, for the tests and the probe.
    pub fn any(&self, predicate: impl Fn(&CombatEvent) -> bool) -> bool {
        self.iter().any(|event| predicate(&event))
    }

    /// The first hit this tick produced, if any.
    #[must_use]
    pub fn first_hit(&self) -> Option<CombatEvent> {
        self.iter()
            .find(|event| matches!(event, CombatEvent::Hit { .. }))
    }

    /// Counts the hits landed on each side this tick.
    #[must_use]
    pub fn hits_taken(&self) -> [u8; SIDES.len()] {
        let mut counts = [0_u8; SIDES.len()];
        for event in self.iter() {
            if let CombatEvent::Hit { victim, .. } = event {
                counts[victim.index()] = counts[victim.index()].saturating_add(1);
            }
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::{CombatEvent, MAX_EVENTS_PER_TICK, StepEvents};
    use crate::combatant::{Side, SwingId};
    use glam::{Vec2, Vec3};

    fn hit(attacker: Side, victim: Side) -> CombatEvent {
        CombatEvent::Hit {
            attacker,
            victim,
            point: Vec3::ZERO,
            damage: 10,
            remaining: 90,
            from: Vec2::Y,
        }
    }

    fn sample_events() -> Vec<CombatEvent> {
        vec![
            CombatEvent::SwingStarted {
                side: Side::Player,
                swing: SwingId(7),
            },
            CombatEvent::SwingActive {
                side: Side::Player,
                swing: SwingId(7),
            },
            CombatEvent::SwingWhiffed {
                side: Side::Adversary,
                swing: SwingId(7),
            },
            hit(Side::Player, Side::Adversary),
            CombatEvent::DodgeStarted {
                side: Side::Player,
                direction: Vec2::Y,
            },
            CombatEvent::Staggered {
                side: Side::Adversary,
            },
            CombatEvent::Defeated {
                side: Side::Adversary,
            },
            CombatEvent::EncounterReset,
        ]
    }

    #[test]
    fn an_empty_tick_holds_nothing_and_drops_nothing() {
        let events = StepEvents::new();
        assert!(events.is_empty());
        assert_eq!(events.len(), 0);
        assert_eq!(events.dropped(), 0);
        assert_eq!(events.iter().count(), 0);
        assert!(events.first_hit().is_none());
        assert_eq!(events.hits_taken(), [0, 0]);
        assert_eq!(StepEvents::default(), events);
    }

    #[test]
    fn events_come_back_in_the_order_they_were_published() {
        let mut events = StepEvents::new();
        let pushed = sample_events();
        for event in &pushed {
            assert!(events.push(*event));
        }
        assert_eq!(events.len(), pushed.len());
        let seen: Vec<CombatEvent> = events.iter().collect();
        assert_eq!(seen, pushed);
        assert_eq!(events.dropped(), 0);
    }

    #[test]
    fn a_full_tick_drops_and_counts_instead_of_allocating() {
        let mut events = StepEvents::new();
        for _ in 0..MAX_EVENTS_PER_TICK {
            assert!(events.push(CombatEvent::EncounterReset));
        }
        assert_eq!(events.len(), MAX_EVENTS_PER_TICK);
        assert!(!events.push(CombatEvent::EncounterReset));
        assert_eq!(events.dropped(), 1);
        assert_eq!(
            events.len(),
            MAX_EVENTS_PER_TICK,
            "the container never grows"
        );
        assert!(!events.push(CombatEvent::EncounterReset));
        assert_eq!(events.dropped(), 2);
    }

    #[test]
    fn hits_are_findable_and_countable_by_victim() {
        let mut events = StepEvents::new();
        assert!(events.push(CombatEvent::SwingStarted {
            side: Side::Player,
            swing: SwingId(7),
        }));
        assert!(events.push(hit(Side::Player, Side::Adversary)));
        assert!(events.push(hit(Side::Adversary, Side::Player)));
        match events.first_hit() {
            Some(CombatEvent::Hit { attacker, .. }) => assert_eq!(attacker, Side::Player),
            other => panic!("expected the player's hit first, got {other:?}"),
        }
        assert_eq!(events.hits_taken(), [1, 1]);
        assert!(events.any(|event| matches!(event, CombatEvent::Hit { .. })));
        assert!(!events.any(|event| matches!(event, CombatEvent::Defeated { .. })));
    }

    #[test]
    fn every_event_names_itself_and_its_subject() {
        for event in sample_events() {
            assert!(!event.name().is_empty());
            match event {
                CombatEvent::EncounterReset => assert!(event.subject().is_none()),
                _ => assert!(event.subject().is_some()),
            }
        }
    }
}
