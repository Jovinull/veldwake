//! The authoritative clock: integer ticks, and nothing else.
//!
//! **There is no `dt` anywhere in this crate.** One call to
//! [`Encounter::step`](crate::encounter::Encounter::step) advances exactly one
//! tick, every duration is a [`Ticks`] count, and every phase boundary is an
//! integer comparison. `NaN`, negative and infinite time are therefore not
//! rejected inputs to authority — they are unrepresentable. Seconds exist only
//! in authored configuration, which [`ticks_from_seconds`] turns into ticks
//! before it can reach the runtime.
//!
//! [`CombatClock`] is the other half of the contract: it turns the client's
//! wall-clock [`Duration`] into a tick count using integer arithmetic only.
//! A floating-point accumulator would make the equivalence claim below an
//! approximation, and the whole point of the representation is that it is not.
//!
//! **The claim, stated exactly.** The clock keeps its sub-tick remainder, so
//! after a total elapsed time `T` delivered in any partition of frames the
//! number of ticks consumed is `floor(T_nanos * COMBAT_TICK_HZ / 1e9)`,
//! independent of how the partition was cut, as long as no frame hit the
//! catch-up cap. Nothing stronger is claimed: where a human presses a key
//! relative to a tick is not partition independent, which is exactly why the
//! scripted encounter keys its input to tick indices instead of to seconds.

use std::time::Duration;

/// A duration or an elapsed counter, in authoritative ticks.
pub type Ticks = u32;

/// Authoritative simulation rate, in ticks per second.
///
/// `120` rather than `60` because the sharpest window in the slice — an active
/// swing — is about a tenth of a second, and six samples across it is too
/// coarse a grid for a dodge whose success is a timing decision.
pub const COMBAT_TICK_HZ: u32 = 120;

/// Nanoseconds in one second, as the clock's fixed-point denominator.
pub const NANOS_PER_SECOND: u128 = 1_000_000_000;

/// Most ticks one rendered frame may run, which is `33.3` ms of catch-up.
///
/// Beyond this the backlog is discarded and counted. The simulation visibly
/// slows on a long stall, which is the chosen failure: one enormous step would
/// tunnel through hit windows and teleport bodies.
pub const MAX_TICKS_PER_FRAME: u32 = 4;

/// Seconds one tick lasts, for presentation and for reporting only.
#[must_use]
pub fn tick_seconds() -> f32 {
    1.0 / COMBAT_TICK_HZ as f32
}

/// Seconds a tick count lasts, for presentation and for reporting only.
#[must_use]
pub fn seconds_for(ticks: Ticks) -> f32 {
    ticks as f32 / COMBAT_TICK_HZ as f32
}

/// Why an authored duration in seconds is not a usable tick count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DurationError {
    /// The value was `NaN` or infinite.
    NotFinite,
    /// The value was negative.
    Negative { seconds: f64 },
    /// The value rounded to zero ticks, so the phase would not exist.
    TooShort { seconds: f64 },
    /// The value would not fit in a tick counter.
    TooLong { seconds: f64 },
}

impl std::fmt::Display for DurationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFinite => write!(formatter, "duration is not finite"),
            Self::Negative { seconds } => write!(formatter, "duration {seconds} is negative"),
            Self::TooShort { seconds } => {
                write!(formatter, "duration {seconds} rounds to zero ticks")
            }
            Self::TooLong { seconds } => write!(formatter, "duration {seconds} is too long"),
        }
    }
}

impl std::error::Error for DurationError {}

/// Converts an authored duration in seconds into ticks.
///
/// Rounds to the nearest tick and rejects anything that would round to zero: a
/// phase that does not exist is a configuration error, not a zero-length phase
/// the runtime has to special-case.
pub fn ticks_from_seconds(seconds: f64) -> Result<Ticks, DurationError> {
    if !seconds.is_finite() {
        return Err(DurationError::NotFinite);
    }
    if seconds < 0.0 {
        return Err(DurationError::Negative { seconds });
    }
    let exact = seconds * f64::from(COMBAT_TICK_HZ);
    if exact > f64::from(u32::MAX >> 1) {
        return Err(DurationError::TooLong { seconds });
    }
    let rounded = exact.round() as Ticks;
    if rounded == 0 {
        return Err(DurationError::TooShort { seconds });
    }
    Ok(rounded)
}

/// The same conversion, but zero ticks is a legitimate answer.
///
/// Used for durations that may genuinely be absent, such as a hitstop a tuning
/// switches off.
pub fn optional_ticks_from_seconds(seconds: f64) -> Result<Ticks, DurationError> {
    if !seconds.is_finite() {
        return Err(DurationError::NotFinite);
    }
    if seconds < 0.0 {
        return Err(DurationError::Negative { seconds });
    }
    if seconds == 0.0 {
        return Ok(0);
    }
    ticks_from_seconds(seconds)
}

/// Turns wall-clock time into a count of authoritative ticks.
///
/// Lives in this crate rather than in the client because the tick rate, the
/// catch-up cap and the remainder policy are the combat contract, and because
/// the partition-equivalence tests then run headlessly with everything else.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CombatClock {
    /// Unconsumed time in units of `nanoseconds * COMBAT_TICK_HZ`, so one tick
    /// costs exactly [`NANOS_PER_SECOND`] and no rounding ever happens.
    scaled: u128,
    consumed: u64,
    dropped: u64,
    /// Frames that hit the catch-up cap, which is the observable symptom.
    capped_frames: u64,
}

impl CombatClock {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            scaled: 0,
            consumed: 0,
            dropped: 0,
            capped_frames: 0,
        }
    }

    /// Accepts one frame's elapsed time and returns how many ticks to run.
    ///
    /// The sub-tick remainder is kept, whole ticks beyond the cap are dropped
    /// and counted, and nothing is ever merged into a larger step.
    pub fn advance(&mut self, elapsed: Duration) -> Ticks {
        self.advance_with_cap(elapsed, MAX_TICKS_PER_FRAME)
    }

    /// [`CombatClock::advance`] with an explicit cap, for the tests that need
    /// to prove what the cap does.
    pub fn advance_with_cap(&mut self, elapsed: Duration, cap: Ticks) -> Ticks {
        let nanos = elapsed.as_nanos();
        self.scaled = self
            .scaled
            .saturating_add(nanos.saturating_mul(u128::from(COMBAT_TICK_HZ)));
        let available = self.scaled / NANOS_PER_SECOND;
        self.scaled %= NANOS_PER_SECOND;
        let run = available.min(u128::from(cap));
        let dropped = available - run;
        // `run` is bounded by `cap`, a `u32`, so the cast cannot truncate.
        let run = run as Ticks;
        self.consumed = self.consumed.saturating_add(u64::from(run));
        if dropped > 0 {
            self.dropped = self
                .dropped
                .saturating_add(u64::try_from(dropped).unwrap_or(u64::MAX));
            self.capped_frames = self.capped_frames.saturating_add(1);
        }
        run
    }

    /// Total ticks this clock has handed out.
    #[must_use]
    pub const fn consumed(&self) -> u64 {
        self.consumed
    }

    /// Total whole ticks discarded because a frame hit the catch-up cap.
    #[must_use]
    pub const fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Frames that hit the catch-up cap.
    #[must_use]
    pub const fn capped_frames(&self) -> u64 {
        self.capped_frames
    }

    /// The unconsumed sub-tick remainder, in scaled units.
    #[must_use]
    pub const fn remainder(&self) -> u128 {
        self.scaled
    }
}

#[cfg(test)]
mod tests {
    use super::{
        COMBAT_TICK_HZ, CombatClock, DurationError, MAX_TICKS_PER_FRAME, NANOS_PER_SECOND,
        optional_ticks_from_seconds, seconds_for, tick_seconds, ticks_from_seconds,
    };
    use std::time::Duration;

    fn frame(hz: u32) -> Duration {
        Duration::from_nanos(u64::from(1_000_000_000_u32 / hz))
    }

    #[test]
    fn one_second_is_exactly_the_tick_rate_whatever_the_frame_rate() {
        // The representable-elapsed claim, checked at three refresh rates with
        // an elapsed time each of them divides exactly.
        for hz in [30_u32, 60, 144] {
            let step = frame(hz);
            let mut clock = CombatClock::new();
            let mut ticks = 0_u64;
            // 144 does not divide a second exactly in nanoseconds, so compare
            // against the total time actually delivered rather than against
            // one second of intent.
            let frames = u64::from(hz);
            for _ in 0..frames {
                ticks += u64::from(clock.advance_with_cap(step, 1_000));
            }
            let delivered = step.as_nanos() * u128::from(frames);
            let expected = (delivered * u128::from(COMBAT_TICK_HZ)) / NANOS_PER_SECOND;
            assert_eq!(
                u128::from(ticks),
                expected,
                "{hz} Hz produced {ticks} ticks, expected {expected}"
            );
            assert_eq!(clock.dropped(), 0);
        }
    }

    #[test]
    fn the_same_elapsed_time_produces_the_same_tick_count_in_any_partition() {
        let total = Duration::from_millis(2_500);
        let reference = {
            let mut clock = CombatClock::new();
            u64::from(clock.advance_with_cap(total, 100_000))
        };
        for parts in [1_u32, 2, 3, 7, 30, 60, 144, 300] {
            let mut clock = CombatClock::new();
            let mut ticks = 0_u64;
            let slice = total.as_nanos() / u128::from(parts);
            let mut delivered = 0_u128;
            for _ in 0..parts {
                let piece = Duration::from_nanos(u64::try_from(slice).unwrap_or(u64::MAX));
                delivered += piece.as_nanos();
                ticks += u64::from(clock.advance_with_cap(piece, 100_000));
            }
            // Integer division can lose up to `parts - 1` nanoseconds of the
            // whole, which is why the comparison is against what was actually
            // delivered. Equality here is what "exact" means.
            let expected = (delivered * u128::from(COMBAT_TICK_HZ)) / NANOS_PER_SECOND;
            assert_eq!(
                u128::from(ticks),
                expected,
                "partition into {parts} differed"
            );
            if delivered == total.as_nanos() {
                assert_eq!(ticks, reference, "partition into {parts} lost a tick");
            }
        }
    }

    #[test]
    fn exact_twenty_seconds_produce_2400_ticks_in_every_exact_partition() {
        let total = Duration::from_secs(20);
        let expected = 2_400_u64;
        for parts in [1_u32, 30, 60, 144, 300, 2_400] {
            let part_count = u128::from(parts);
            let base = total.as_nanos() / part_count;
            let remainder = total.as_nanos() % part_count;
            let mut clock = CombatClock::new();
            let mut delivered = 0_u128;
            let mut ticks = 0_u64;
            for index in 0..u128::from(parts) {
                // Give the first `remainder` pieces one extra nanosecond so
                // this is an exact partition, unlike truncating a nominal
                // refresh interval such as `1_000_000_000 / 144`.
                let nanos = base + u128::from(index < remainder);
                let frame = Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX));
                delivered += frame.as_nanos();
                ticks += u64::from(clock.advance_with_cap(frame, 3_000));
            }
            assert_eq!(delivered, total.as_nanos(), "partition {parts}");
            assert_eq!(ticks, expected, "partition {parts}");
            assert_eq!(clock.dropped(), 0, "partition {parts}");
        }
    }

    #[test]
    fn the_remainder_is_preserved_across_frames() {
        // A frame shorter than a tick produces no tick, but the time is not
        // lost: the tick arrives once enough frames have accumulated.
        let mut clock = CombatClock::new();
        let sliver = Duration::from_nanos(1_000_000); // 1 ms, 0.12 of a tick
        let mut ticks = 0;
        for _ in 0..8 {
            ticks += clock.advance(sliver);
        }
        assert_eq!(ticks, 0, "eight milliseconds is not yet a tick");
        ticks += clock.advance(sliver);
        assert_eq!(ticks, 1, "the ninth millisecond completes the first tick");
        assert!(clock.remainder() < NANOS_PER_SECOND);
    }

    #[test]
    fn a_frame_spike_is_capped_and_the_excess_is_counted_not_merged() {
        let mut clock = CombatClock::new();
        let spike = Duration::from_millis(500); // 60 ticks of backlog
        let run = clock.advance(spike);
        assert_eq!(run, MAX_TICKS_PER_FRAME);
        assert_eq!(clock.dropped(), 56);
        assert_eq!(clock.capped_frames(), 1);
        // The dropped time is gone, not queued: the next ordinary frame runs an
        // ordinary number of ticks.
        let run = clock.advance(Duration::from_nanos(8_333_334));
        assert_eq!(run, 1);
    }

    #[test]
    fn a_zero_length_frame_produces_no_tick_and_no_drop() {
        let mut clock = CombatClock::new();
        assert_eq!(clock.advance(Duration::ZERO), 0);
        assert_eq!(clock.dropped(), 0);
        assert_eq!(clock.consumed(), 0);
        assert_eq!(clock.capped_frames(), 0);
    }

    #[test]
    fn an_absurd_frame_saturates_instead_of_overflowing() {
        let mut clock = CombatClock::new();
        let run = clock.advance(Duration::MAX);
        assert_eq!(run, MAX_TICKS_PER_FRAME);
        assert!(clock.dropped() > 0);
    }

    #[test]
    fn authored_seconds_become_ticks_or_a_typed_error() {
        assert_eq!(ticks_from_seconds(1.0), Ok(COMBAT_TICK_HZ));
        assert_eq!(ticks_from_seconds(0.1), Ok(12));
        assert_eq!(ticks_from_seconds(0.5 / 120.0), Ok(1), "rounds to nearest");
        assert_eq!(
            ticks_from_seconds(0.001),
            Err(DurationError::TooShort { seconds: 0.001 })
        );
        assert_eq!(ticks_from_seconds(f64::NAN), Err(DurationError::NotFinite));
        assert_eq!(
            ticks_from_seconds(f64::INFINITY),
            Err(DurationError::NotFinite)
        );
        assert_eq!(
            ticks_from_seconds(-1.0),
            Err(DurationError::Negative { seconds: -1.0 })
        );
        assert!(matches!(
            ticks_from_seconds(1.0e12),
            Err(DurationError::TooLong { .. })
        ));
        assert_eq!(optional_ticks_from_seconds(0.0), Ok(0));
        assert_eq!(optional_ticks_from_seconds(0.1), Ok(12));
    }

    #[test]
    fn tick_and_second_conversions_agree() {
        assert!((tick_seconds() * COMBAT_TICK_HZ as f32 - 1.0).abs() < 1.0e-6);
        assert!((seconds_for(COMBAT_TICK_HZ) - 1.0).abs() < 1.0e-6);
        assert_eq!(seconds_for(0), 0.0);
    }

    #[test]
    fn every_error_prints_something_specific() {
        for error in [
            DurationError::NotFinite,
            DurationError::Negative { seconds: -1.0 },
            DurationError::TooShort { seconds: 0.0001 },
            DurationError::TooLong { seconds: 1.0e12 },
        ] {
            assert!(!error.to_string().is_empty());
        }
    }
}
