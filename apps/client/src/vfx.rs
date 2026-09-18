//! Two effects, one fixed pool, no engine.
//!
//! M6 needs exactly two things drawn that are not a body, a weapon or the
//! world: chips thrown off a confirmed hit, and an accent that makes the
//! adversary's telegraph readable. Both are voxel chips, because the game is
//! made of cubes and a soft round billboard would be the only thing in the
//! frame that is not.
//!
//! What this is not: there is no particle system here. No emitters, no curves,
//! no modules, no spawn descriptors, no sorting, no soft particles, no texture,
//! and no way to add a third effect without writing it. [`VfxKind`] has two
//! variants and adding a third is a deliberate edit to this file, which is the
//! whole design. A generic system is the thing to build when there is a third
//! and fourth effect to generalise from; inventing one for two is inventing the
//! wrong one.
//!
//! **Bounded by construction.** The pool is a fixed array of
//! [`MAX_PARTICLES`], so the memory is decided at compile time and a burst that
//! would exceed it is refused and counted rather than allocating. Nothing here
//! allocates, at any point, including when it draws: [`VfxPool::instances`]
//! writes into a buffer the caller owns.
//!
//! **Deterministic.** Ages are tick counts and the spread directions come from
//! a fixed table, so the same hit at the same tick throws the same chips on
//! every run and on every host. There is no random source, and a capture
//! fixture can therefore be compared against another capture rather than only
//! looked at.
//!
//! **Zero means zero.** With no live particles there is no instance written and
//! no draw issued, which is the same contract the debug views hold and is
//! asserted by `an_empty_pool_writes_nothing`.

use glam::Vec3;

/// How many particles can be alive at once.
///
/// Ninety-six. A hit throws [`IMPACT_CHIPS`] and a telegraph carries
/// [`TELEGRAPH_MOTES`], so the pool holds about six simultaneous hits' worth,
/// which is more than two combatants can produce: an attack is followed by a
/// recovery, so neither body can land hits faster than one per swing.
pub const MAX_PARTICLES: usize = 96;

/// How many chips one confirmed hit throws.
pub const IMPACT_CHIPS: usize = 12;

/// How many motes one telegraph accent carries.
pub const TELEGRAPH_MOTES: usize = 5;

/// How long an impact chip lives, in combat ticks.
const IMPACT_LIFE: u32 = 34;

/// How long a telegraph mote lives, in combat ticks.
const TELEGRAPH_LIFE: u32 = 26;

/// How fast a chip leaves the impact, in world units per second.
const IMPACT_SPEED: f32 = 3.4;

/// Downward acceleration on a chip, in world units per second squared.
///
/// Not the world's gravity and not pretending to be: chips are presentation and
/// never touch the ground query, the combat rules or anything else. This is the
/// number that makes them fall convincingly over a third of a second.
const CHIP_GRAVITY: f32 = 11.0;

/// Edge length of one chip at birth, in world units.
///
/// Two thirds of a character voxel. `0.028`, a third of a voxel, was the first
/// value and a capture rejected it: at contact range the chips came back as
/// two-pixel specks that had to be hunted for in a magnified crop, which is not
/// what an impact should take. Still well under a voxel, so a chip reads as
/// debris rather than as geometry that fell off.
const CHIP_SIZE: f32 = 0.055;

/// Edge length of one telegraph mote.
const MOTE_SIZE: f32 = 0.058;

/// Seconds in one combat tick, for integrating a chip's flight.
const TICK_SECONDS: f32 = 1.0 / 120.0;

/// Which of the two effects a particle belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VfxKind {
    /// Chips off a confirmed hit: fast, falling, warm.
    Impact,
    /// The accent on an adversary's windup: slow, rising, cold.
    Telegraph,
}

impl VfxKind {
    /// Colour at birth and at death, so a chip cools as it falls.
    const fn tint(self) -> ([f32; 3], [f32; 3]) {
        match self {
            // Struck metal: white hot at the edge, down to the blade's own grey.
            Self::Impact => ([0.98, 0.93, 0.72], [0.42, 0.40, 0.38]),
            // Cold and pale, so a telegraph never reads as damage.
            Self::Telegraph => ([0.62, 0.78, 0.96], [0.30, 0.42, 0.62]),
        }
    }

    const fn life(self) -> u32 {
        match self {
            Self::Impact => IMPACT_LIFE,
            Self::Telegraph => TELEGRAPH_LIFE,
        }
    }

    const fn size(self) -> f32 {
        match self {
            Self::Impact => CHIP_SIZE,
            Self::Telegraph => MOTE_SIZE,
        }
    }
}

/// Twelve directions on the unit sphere, fixed.
///
/// The vertices of a regular icosahedron, normalised. A fixed table rather than
/// a random source because a capture has to be reproducible, and twelve
/// well-separated directions because a burst of chips that all go the same way
/// reads as one chip.
const SPREAD: [[f32; 3]; IMPACT_CHIPS] = [
    [0.0, 0.5257, 0.8507],
    [0.0, 0.5257, -0.8507],
    [0.0, -0.5257, 0.8507],
    [0.0, -0.5257, -0.8507],
    [0.5257, 0.8507, 0.0],
    [0.5257, -0.8507, 0.0],
    [-0.5257, 0.8507, 0.0],
    [-0.5257, -0.8507, 0.0],
    [0.8507, 0.0, 0.5257],
    [-0.8507, 0.0, 0.5257],
    [0.8507, 0.0, -0.5257],
    [-0.8507, 0.0, -0.5257],
];

/// One particle. Copy, so the pool is a plain array and nothing is boxed.
#[derive(Clone, Copy, Debug)]
struct Particle {
    kind: VfxKind,
    position: Vec3,
    velocity: Vec3,
    /// Ticks lived. At or past the kind's life the particle is gone.
    age: u32,
}

/// What the renderer needs to draw one particle.
///
/// Sixteen bytes of position and size, sixteen of colour: one instance, two
/// vectors, no padding to think about.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VfxInstance {
    /// `xyz` world centre, `w` edge length in world units.
    pub placement: [f32; 4],
    /// `rgb` colour, `a` unused.
    pub color: [f32; 4],
}

/// The fixed pool.
#[derive(Clone, Copy, Debug)]
pub struct VfxPool {
    items: [Option<Particle>; MAX_PARTICLES],
    live: usize,
    /// Particles that have ever been born.
    spawned: u64,
    /// Particles a full pool refused. Bounded work means this can happen, and
    /// silently allocating instead is the thing it exists to rule out.
    dropped: u64,
    /// Most ever alive at once, which is what says whether the pool is the
    /// right size.
    high_water: usize,
}

impl Default for VfxPool {
    fn default() -> Self {
        Self {
            items: [None; MAX_PARTICLES],
            live: 0,
            spawned: 0,
            dropped: 0,
            high_water: 0,
        }
    }
}

impl VfxPool {
    /// Chips off a confirmed hit at `at`, thrown back along `away`.
    ///
    /// `away` is the direction the blade was travelling; chips leave into that
    /// half-space so they read as coming off the strike rather than as an
    /// explosion. A zero or non-finite direction falls back to straight up,
    /// which is wrong-looking but never `NaN`.
    pub fn impact(&mut self, at: Vec3, away: Vec3) {
        if !at.is_finite() {
            return;
        }
        let away = away.normalize_or(Vec3::Y);
        for spread in SPREAD {
            let spread = Vec3::from_array(spread);
            // **Reflected** into the strike's half-space, not biased into it.
            // Adding `away * 0.9` to a unit spread was the first attempt and a
            // capture killed it: the four directions pointing back at the
            // attacker very nearly cancelled the bias, so their velocity came
            // out near zero, `normalize_or` had almost nothing to normalise, and
            // the twelve chips left as one clump instead of a spray. Reflecting
            // about the plane perpendicular to `away` keeps every direction a
            // unit vector and keeps all twelve distinct, while still sending
            // none of them backwards.
            let behind = spread.dot(away);
            let forward = if behind < 0.0 {
                spread - away * (2.0 * behind)
            } else {
                spread
            };
            // A gentle bias on top, so the spray leans the way the blade went.
            let direction = (forward + away * 0.35).normalize_or(Vec3::Y);
            self.push(Particle {
                kind: VfxKind::Impact,
                position: at,
                velocity: direction * IMPACT_SPEED,
                age: 0,
            });
        }
    }

    /// The accent on a telegraph: motes rising off the raised blade.
    pub fn telegraph(&mut self, at: Vec3) {
        if !at.is_finite() {
            return;
        }
        for spread in SPREAD.iter().take(TELEGRAPH_MOTES) {
            let spread = Vec3::from_array(*spread);
            self.push(Particle {
                kind: VfxKind::Telegraph,
                position: at + spread * 0.10,
                // Slow and upward: the opposite of an impact, on purpose, so
                // the two effects can never be mistaken for each other.
                velocity: Vec3::new(spread.x * 0.25, 0.85, spread.z * 0.25),
                age: 0,
            });
        }
    }

    fn push(&mut self, particle: Particle) {
        if let Some(slot) = self.items.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(particle);
            self.live += 1;
            self.spawned = self.spawned.saturating_add(1);
            self.high_water = self.high_water.max(self.live);
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    /// Ages every particle by however many combat ticks the frame ran.
    ///
    /// Tick-driven, like everything else about this fight: a frozen moment runs
    /// no ticks and the chips hang exactly where they were, which is what makes
    /// a frozen capture of an impact possible at all.
    pub fn advance(&mut self, ticks: u32) {
        if ticks == 0 {
            return;
        }
        let step = TICK_SECONDS * f32::from(u16::try_from(ticks).unwrap_or(u16::MAX));
        for slot in &mut self.items {
            let Some(particle) = slot.as_mut() else {
                continue;
            };
            particle.age = particle.age.saturating_add(ticks);
            if particle.age >= particle.kind.life() {
                *slot = None;
                self.live = self.live.saturating_sub(1);
                continue;
            }
            if particle.kind == VfxKind::Impact {
                particle.velocity.y -= CHIP_GRAVITY * step;
            }
            particle.position += particle.velocity * step;
        }
    }

    /// Drops every particle, for a reset.
    pub fn clear(&mut self) {
        self.items = [None; MAX_PARTICLES];
        self.live = 0;
    }

    #[must_use]
    pub const fn live(&self) -> usize {
        self.live
    }

    /// How many of one effect are alive. Reported separately because "the
    /// accent fired on a hit" and "chips flew on a telegraph" are two different
    /// bugs and one total hides both.
    #[must_use]
    pub fn live_of(&self, kind: VfxKind) -> usize {
        self.items
            .iter()
            .flatten()
            .filter(|particle| particle.kind == kind)
            .count()
    }

    #[must_use]
    pub const fn spawned(&self) -> u64 {
        self.spawned
    }

    #[must_use]
    pub const fn dropped(&self) -> u64 {
        self.dropped
    }

    #[must_use]
    pub const fn high_water(&self) -> usize {
        self.high_water
    }

    /// Writes one instance per live particle into `out` and returns how many.
    ///
    /// The caller owns the buffer, so drawing allocates nothing. A chip shrinks
    /// and cools as it ages, which is the whole of its animation.
    pub fn instances(&self, out: &mut [VfxInstance]) -> usize {
        let mut count = 0;
        for particle in self.items.iter().flatten() {
            if count >= out.len() {
                break;
            }
            let life = particle.kind.life().max(1);
            let t = f32::from(u16::try_from(particle.age.min(life)).unwrap_or(u16::MAX))
                / f32::from(u16::try_from(life).unwrap_or(u16::MAX));
            let (hot, cold) = particle.kind.tint();
            let mix = |a: f32, b: f32| a + (b - a) * t;
            out[count] = VfxInstance {
                placement: [
                    particle.position.x,
                    particle.position.y,
                    particle.position.z,
                    particle.kind.size() * (1.0 - t * 0.7),
                ],
                color: [
                    mix(hot[0], cold[0]),
                    mix(hot[1], cold[1]),
                    mix(hot[2], cold[2]),
                    1.0,
                ],
            };
            count += 1;
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::{IMPACT_CHIPS, MAX_PARTICLES, TELEGRAPH_MOTES, VfxInstance, VfxKind, VfxPool};
    use glam::Vec3;

    fn buffer() -> [VfxInstance; MAX_PARTICLES] {
        [VfxInstance::default(); MAX_PARTICLES]
    }

    #[test]
    fn an_empty_pool_writes_nothing() {
        // The zero contract. No hit has landed, so there is nothing to upload
        // and nothing to draw, and the renderer is told so by a count of zero
        // rather than by a buffer of transparent cubes.
        let pool = VfxPool::default();
        let mut out = buffer();
        assert_eq!(pool.instances(&mut out), 0);
        assert_eq!(pool.live(), 0);
        assert_eq!(pool.spawned(), 0);
        assert_eq!(pool.dropped(), 0);
        assert_eq!(out[0], VfxInstance::default());
        // Ageing an empty pool is also nothing.
        let mut pool = pool;
        pool.advance(500);
        assert_eq!(pool.instances(&mut out), 0);
    }

    #[test]
    fn one_hit_throws_chips_that_fall_and_expire() {
        let mut pool = VfxPool::default();
        pool.impact(Vec3::new(1.0, 2.0, 3.0), Vec3::X);
        assert_eq!(pool.live(), IMPACT_CHIPS);
        let mut out = buffer();
        assert_eq!(pool.instances(&mut out), IMPACT_CHIPS);
        let born: Vec<[f32; 4]> = out[..IMPACT_CHIPS].iter().map(|i| i.placement).collect();
        // Twelve chips, twelve directions: no two start the same way.
        for (index, chip) in born.iter().enumerate() {
            assert!(chip.iter().all(|value| value.is_finite()), "{chip:?}");
            assert!(chip[3] > 0.0, "chip {index} has no size");
        }
        pool.advance(12);
        let mut later = buffer();
        assert_eq!(pool.instances(&mut later), IMPACT_CHIPS);
        let moved = later[..IMPACT_CHIPS]
            .iter()
            .zip(&born)
            .filter(|(now, then)| now.placement[..3] != then[..3])
            .count();
        assert_eq!(moved, IMPACT_CHIPS, "chips did not fly");
        // Every chip is gone once its life is spent, without being told to.
        pool.advance(60);
        assert_eq!(pool.live(), 0);
        assert_eq!(pool.instances(&mut buffer()), 0);
    }

    #[test]
    fn a_hit_sprays_twelve_distinct_directions_forward() {
        // The defect a capture found: a bias strong enough to keep every chip
        // out of the attacker's half-space also cancelled four of the twelve
        // spread directions, and the spray came out as a clump. Both halves of
        // that are asserted here — nothing goes backwards, and no two chips go
        // the same way.
        let away = Vec3::new(0.0, 0.35, -1.0).normalize();
        let mut pool = VfxPool::default();
        pool.impact(Vec3::ZERO, away);
        let mut out = buffer();
        let count = pool.instances(&mut out);
        assert_eq!(count, IMPACT_CHIPS);

        // One tick of flight turns a direction into a displacement.
        pool.advance(1);
        let mut flown = buffer();
        assert_eq!(pool.instances(&mut flown), IMPACT_CHIPS);
        let mut directions = Vec::with_capacity(IMPACT_CHIPS);
        for (before, after) in out[..count].iter().zip(&flown[..count]) {
            let step = Vec3::new(
                after.placement[0] - before.placement[0],
                after.placement[1] - before.placement[1],
                after.placement[2] - before.placement[2],
            );
            assert!(
                step.length() > 1.0e-4,
                "a chip did not move: {step}, which is the clump this test exists for"
            );
            directions.push(step.normalize());
        }
        for (index, direction) in directions.iter().enumerate() {
            for other in directions.iter().skip(index + 1) {
                let together = direction.dot(*other);
                assert!(
                    together < 0.995,
                    "two chips left in the same direction, dot {together:.4}"
                );
            }
        }
    }

    #[test]
    fn the_two_effects_do_not_look_like_each_other() {
        // Impact chips fall and are warm; telegraph motes rise and are cold. If
        // these ever converged, the accent that warns of a swing would read as
        // damage already taken.
        let mut impact = VfxPool::default();
        impact.impact(Vec3::ZERO, Vec3::Z);
        let mut telegraph = VfxPool::default();
        telegraph.telegraph(Vec3::ZERO);
        assert_eq!(telegraph.live(), TELEGRAPH_MOTES);

        let mut a = buffer();
        let mut b = buffer();
        let na = impact.instances(&mut a);
        let nb = telegraph.instances(&mut b);
        let warmth = |instance: &VfxInstance| instance.color[0] - instance.color[2];
        let hottest = a[..na].iter().map(warmth).fold(f32::MIN, f32::max);
        let coldest = b[..nb].iter().map(warmth).fold(f32::MAX, f32::min);
        assert!(
            hottest > 0.0 && coldest < 0.0,
            "impact warmth {hottest}, telegraph warmth {coldest}"
        );

        impact.advance(10);
        telegraph.advance(10);
        let na = impact.instances(&mut a);
        let nb = telegraph.instances(&mut b);
        let mean_rise = |slice: &[VfxInstance]| -> f32 {
            let total: f32 = slice.iter().map(|i| i.placement[1]).sum();
            total / f32::from(u16::try_from(slice.len().max(1)).unwrap_or(1))
        };
        assert!(
            mean_rise(&b[..nb]) > mean_rise(&a[..na]),
            "the telegraph did not rise above the impact"
        );
    }

    #[test]
    fn a_full_pool_refuses_and_counts_rather_than_growing() {
        // Bounded work, as a property. The pool cannot be made to hold more than
        // it has room for however many hits land, and what it refused is
        // countable rather than silent.
        let mut pool = VfxPool::default();
        for _ in 0..40 {
            pool.impact(Vec3::ZERO, Vec3::Y);
        }
        assert_eq!(pool.live(), MAX_PARTICLES);
        assert_eq!(pool.high_water(), MAX_PARTICLES);
        assert!(pool.dropped() > 0, "a full pool refused nothing");
        assert_eq!(
            pool.spawned() + pool.dropped(),
            40 * IMPACT_CHIPS as u64,
            "every chip was either born or refused"
        );
        let mut out = buffer();
        assert_eq!(pool.instances(&mut out), MAX_PARTICLES);
        // A caller with a smaller buffer gets as many as fit, not a panic.
        let mut small = [VfxInstance::default(); 7];
        assert_eq!(pool.instances(&mut small), 7);
    }

    #[test]
    fn each_effect_is_counted_on_its_own() {
        // One total would hide both of the bugs worth watching for: the accent
        // firing on a hit, and chips flying on a telegraph.
        let mut pool = VfxPool::default();
        assert_eq!(pool.live_of(VfxKind::Impact), 0);
        assert_eq!(pool.live_of(VfxKind::Telegraph), 0);
        pool.impact(Vec3::ZERO, Vec3::Y);
        assert_eq!(pool.live_of(VfxKind::Impact), IMPACT_CHIPS);
        assert_eq!(pool.live_of(VfxKind::Telegraph), 0);
        pool.telegraph(Vec3::Y);
        assert_eq!(pool.live_of(VfxKind::Impact), IMPACT_CHIPS);
        assert_eq!(pool.live_of(VfxKind::Telegraph), TELEGRAPH_MOTES);
        assert_eq!(
            pool.live_of(VfxKind::Impact) + pool.live_of(VfxKind::Telegraph),
            pool.live()
        );
    }

    #[test]
    fn a_frozen_frame_holds_the_chips_where_they_were() {
        // A named moment runs no ticks. The chips have to hang, or a frozen
        // capture of an impact is a capture of an empty frame.
        let mut pool = VfxPool::default();
        pool.impact(Vec3::new(0.0, 1.0, 0.0), Vec3::X);
        pool.advance(5);
        let mut held = buffer();
        let count = pool.instances(&mut held);
        for _ in 0..200 {
            pool.advance(0);
            let mut now = buffer();
            assert_eq!(pool.instances(&mut now), count);
            assert_eq!(now[..count], held[..count]);
        }
    }

    #[test]
    fn the_same_ticks_give_the_same_chips() {
        let run = || {
            let mut pool = VfxPool::default();
            let mut trace = Vec::new();
            let mut out = buffer();
            for tick in 0..90_u32 {
                if tick % 31 == 0 {
                    pool.impact(Vec3::new(1.5, 1.2, -0.5), Vec3::new(0.3, 0.1, -0.9));
                }
                if tick % 17 == 0 {
                    pool.telegraph(Vec3::new(1.0, 2.0, 0.0));
                }
                let count = pool.instances(&mut out);
                trace.push(out[..count].to_vec());
                pool.advance(if tick % 4 == 0 { 3 } else { 1 });
            }
            trace
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn a_hit_at_an_impossible_place_throws_nothing() {
        let mut pool = VfxPool::default();
        pool.impact(Vec3::splat(f32::NAN), Vec3::X);
        pool.impact(Vec3::new(f32::INFINITY, 0.0, 0.0), Vec3::X);
        pool.telegraph(Vec3::splat(f32::NAN));
        assert_eq!(pool.live(), 0);
        // A direction that is no direction still produces finite chips.
        pool.impact(Vec3::ZERO, Vec3::ZERO);
        let mut out = buffer();
        let count = pool.instances(&mut out);
        assert_eq!(count, super::IMPACT_CHIPS);
        for instance in &out[..count] {
            assert!(instance.placement.iter().all(|v| v.is_finite()));
            assert!(instance.color.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn clearing_the_pool_leaves_the_counters_alone() {
        // A reset drops the chips in flight; it does not rewrite history.
        let mut pool = VfxPool::default();
        pool.impact(Vec3::ZERO, Vec3::Y);
        let spawned = pool.spawned();
        let high = pool.high_water();
        pool.clear();
        assert_eq!(pool.live(), 0);
        assert_eq!(pool.spawned(), spawned);
        assert_eq!(pool.high_water(), high);
        assert_eq!(pool.instances(&mut buffer()), 0);
    }
}
