//! A deterministic diagnostic path.
//!
//! Evidence for locomotion needs a character that moves the same way every
//! time, so two captures months apart are comparable. A course is the
//! character's counterpart to `veldwake-procedural`'s named camera poses: a
//! fixed list of legs, sampled by elapsed time, with no input, no controller,
//! and no gameplay.
//!
//! The type is world agnostic on purpose. Whether a particular course stays
//! inside a particular region, out of its water, and under a slope limit is a
//! property of that world, and is proved where both the course and the world
//! are visible — in the client, not here.

/// One segment of a course.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CourseLeg {
    pub name: &'static str,
    /// Seconds this leg lasts.
    pub seconds: f32,
    /// World units per second along the leg's facing.
    pub speed: f32,
    /// Yaw in radians; zero faces `-Z`.
    pub facing: f32,
}

/// Where a course puts the character at one instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CourseSample {
    pub x: f32,
    pub z: f32,
    pub facing: f32,
    pub speed: f32,
    /// Name of the leg that produced this sample.
    pub leg: &'static str,
    /// Whether the course has finished.
    pub finished: bool,
}

/// A named diagnostic path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterCourse {
    pub name: &'static str,
    pub start_x: f32,
    pub start_z: f32,
    pub legs: &'static [CourseLeg],
}

impl CharacterCourse {
    /// Total duration, in seconds.
    #[must_use]
    pub fn duration(&self) -> f32 {
        self.legs.iter().map(|leg| leg.seconds.max(0.0)).sum()
    }

    /// The course at an elapsed time.
    ///
    /// Positions integrate leg by leg in closed form, so sampling at any time
    /// costs the same and no accumulated state can drift between runs.
    #[must_use]
    pub fn sample(&self, seconds: f32) -> CourseSample {
        let mut x = self.start_x;
        let mut z = self.start_z;
        let mut remaining = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        let mut last = CourseSample {
            x,
            z,
            facing: self.legs.first().map_or(0.0, |leg| leg.facing),
            speed: 0.0,
            leg: self.legs.first().map_or("empty", |leg| leg.name),
            finished: true,
        };
        for leg in self.legs {
            let span = leg.seconds.max(0.0);
            let travelled = remaining.min(span);
            let distance = leg.speed.max(0.0) * travelled;
            let dx = leg.facing.sin() * distance;
            let dz = -leg.facing.cos() * distance;
            if remaining < span {
                return CourseSample {
                    x: x + dx,
                    z: z + dz,
                    facing: leg.facing,
                    speed: leg.speed,
                    leg: leg.name,
                    finished: false,
                };
            }
            x += dx;
            z += dz;
            remaining -= span;
            last = CourseSample {
                x,
                z,
                facing: leg.facing,
                speed: 0.0,
                leg: leg.name,
                finished: true,
            };
        }
        last
    }

    /// Samples the whole course at a fixed interval.
    #[must_use]
    pub fn walk(&self, step_seconds: f32) -> Vec<(f32, CourseSample)> {
        let step = if step_seconds.is_finite() && step_seconds > 0.0 {
            step_seconds
        } else {
            1.0 / 60.0
        };
        let duration = self.duration();
        let mut samples = Vec::new();
        let mut time = 0.0_f32;
        while time <= duration {
            samples.push((time, self.sample(time)));
            time += step;
        }
        samples
    }
}

#[cfg(test)]
mod tests {
    use super::{CharacterCourse, CourseLeg};

    const LEGS: &[CourseLeg] = &[
        CourseLeg {
            name: "stand",
            seconds: 2.0,
            speed: 0.0,
            facing: 0.0,
        },
        CourseLeg {
            name: "walk-north",
            seconds: 4.0,
            speed: 1.5,
            facing: 0.0,
        },
        CourseLeg {
            name: "run-east",
            seconds: 2.0,
            speed: 3.0,
            facing: std::f32::consts::FRAC_PI_2,
        },
    ];

    const COURSE: CharacterCourse = CharacterCourse {
        name: "test",
        start_x: 10.0,
        start_z: -4.0,
        legs: LEGS,
    };

    #[test]
    fn a_course_integrates_its_legs_in_closed_form() {
        assert_eq!(COURSE.duration(), 8.0);
        let start = COURSE.sample(0.0);
        assert_eq!((start.x, start.z), (10.0, -4.0));
        assert_eq!(start.leg, "stand");

        // Four seconds of walking at 1.5 toward -Z covers six units.
        let after_walk = COURSE.sample(6.0);
        assert!((after_walk.x - 10.0).abs() < 1.0e-4);
        assert!(
            (after_walk.z + 10.0).abs() < 1.0e-4,
            "z is {}",
            after_walk.z
        );
        assert_eq!(after_walk.leg, "run-east");

        // Two seconds of running at 3.0 toward +X covers six units.
        let end = COURSE.sample(8.0);
        assert!((end.x - 16.0).abs() < 1.0e-3, "x is {}", end.x);
        assert!((end.z + 10.0).abs() < 1.0e-3);
        assert!(end.finished);
    }

    #[test]
    fn sampling_is_reproducible_and_matches_a_fine_walk() {
        let coarse = COURSE.sample(5.0);
        assert_eq!(coarse, COURSE.sample(5.0));
        let samples = COURSE.walk(1.0 / 240.0);
        assert!(samples.len() > 1_900);
        let Some((_, near)) = samples.iter().find(|(time, _)| *time >= 5.0) else {
            panic!("the walk must reach five seconds");
        };
        assert!((near.x - coarse.x).abs() < 0.05);
        assert!((near.z - coarse.z).abs() < 0.05);
    }

    #[test]
    fn hostile_times_do_not_move_the_character_anywhere_strange() {
        for time in [f32::NAN, f32::INFINITY, -100.0, 1.0e9] {
            let sample = COURSE.sample(time);
            assert!(sample.x.is_finite() && sample.z.is_finite(), "{time}");
        }
        assert_eq!(COURSE.sample(-1.0), COURSE.sample(0.0));
    }

    #[test]
    fn an_empty_course_is_harmless() {
        let empty = CharacterCourse {
            name: "empty",
            start_x: 1.0,
            start_z: 2.0,
            legs: &[],
        };
        assert_eq!(empty.duration(), 0.0);
        let sample = empty.sample(3.0);
        assert_eq!((sample.x, sample.z), (1.0, 2.0));
        assert!(sample.finished);
        assert_eq!(empty.walk(0.0).len(), 1);
    }
}
