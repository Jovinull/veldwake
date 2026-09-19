//! What is left of each body, as pips above its head.
//!
//! The smallest readout that answers the only question M6 needs answered while
//! a fight is on: am I winning. A row of cubes over each combatant, bright for
//! health still held and dark for health lost.
//!
//! **Why pips and not a bar.** A bar wants a non-uniform scale, which the chip
//! instance does not carry — its placement is a centre and one edge length — so
//! a bar would mean a second instance layout, a second pipeline, or a
//! screen-space pass with its own projection. Pips need none of that: they are
//! the chip cube at a different size, so they ride the effect pipeline that
//! already exists and cost one more instance each. In a world made of cubes
//! they also simply look like they belong.
//!
//! **Why world space and not a corner of the screen.** The fight has two
//! bodies and either can be the one in trouble. A readout above each head says
//! which without a legend, and it needs no knowledge of the window size. The
//! cost is that a body behind a tree has its pips behind the tree too, which is
//! correct for a diegetic readout and is the reason this is a slice's readout
//! and not a game's.
//!
//! Bounded and deterministic like everything else here: [`READOUT_PIPS`] per
//! body, always, so the instance count is a constant and a full-health body
//! costs exactly what a nearly-dead one does.

use glam::Vec3;

use veldwake_combat::{Health, SIDES};

use crate::vfx::VfxInstance;

/// Pips per body.
///
/// Eight. Enough that one pip is a readable fraction of a fight — the reference
/// attack takes about an eighth of a body's health — and few enough that the
/// row stays narrower than the shoulders it floats over.
pub const READOUT_PIPS: usize = 8;

/// Instances the readout writes: one row per combatant.
pub const READOUT_INSTANCES: usize = READOUT_PIPS * SIDES.len();

/// Edge length of one pip, in world units.
const PIP_SIZE: f32 = 0.085;

/// Centre-to-centre spacing of the pips.
const PIP_SPACING: f32 = 0.115;

/// How far above the top of a body the row floats.
const PIP_RISE: f32 = 0.34;

/// Colour of a pip still held, and of one lost.
///
/// Held is warm and bright so it reads against grass and bark; lost is dark and
/// desaturated so the row still shows its own length and a viewer can see how
/// much has gone rather than only how much is left.
const PIP_HELD: [f32; 3] = [0.92, 0.82, 0.36];
const PIP_LOST: [f32; 3] = [0.16, 0.14, 0.12];

/// A pip about to be drawn, before it becomes an instance.
///
/// `right` is the direction the row runs in, which the caller takes from the
/// camera so the row always faces the viewer rather than the body's facing: a
/// readout that turns edge-on when its owner turns is not a readout.
pub struct Row {
    /// Centre of the row, already above the head.
    pub centre: Vec3,
    /// Unit direction the row runs along.
    pub right: Vec3,
    pub health: Health,
}

/// Writes every body's row into `out` and returns how many instances it used.
///
/// Nothing is allocated; the caller owns the buffer. A row whose placement is
/// not finite is skipped rather than written, because one `NaN` in a vertex
/// buffer takes the whole draw with it.
pub fn write(rows: &[Row], out: &mut [VfxInstance]) -> usize {
    let mut count = 0;
    for row in rows {
        if !row.centre.is_finite() {
            continue;
        }
        let right = row.right.normalize_or(Vec3::X);
        let fraction = row.health.fraction();
        // Ceiling rather than rounding: a body with any health left keeps at
        // least one bright pip, so "nearly dead" never looks like "dead".
        let held = if row.health.current() == 0 {
            0
        } else {
            let exact = fraction * READOUT_PIPS as f32;
            (exact.ceil() as usize).clamp(1, READOUT_PIPS)
        };
        // Centred on the head: the row's own width is fixed, so this is the
        // same offset for every body and every health.
        let span = PIP_SPACING * (READOUT_PIPS as f32 - 1.0);
        for index in 0..READOUT_PIPS {
            if count >= out.len() {
                return count;
            }
            let along = PIP_SPACING * index as f32 - span * 0.5;
            let position = row.centre + right * along;
            let color = if index < held { PIP_HELD } else { PIP_LOST };
            out[count] = VfxInstance {
                placement: [position.x, position.y, position.z, PIP_SIZE],
                color: [color[0], color[1], color[2], 1.0],
            };
            count += 1;
        }
    }
    count
}

/// Where a body's row floats: above the top of it.
#[must_use]
pub fn above(stand: Vec3, height: f32) -> Vec3 {
    stand + Vec3::Y * (height + PIP_RISE)
}

#[cfg(test)]
mod tests {
    use super::{PIP_HELD, PIP_LOST, READOUT_INSTANCES, READOUT_PIPS, Row, above, write};
    use crate::vfx::VfxInstance;
    use glam::Vec3;
    use veldwake_combat::Health;

    fn row(current: u16, max: u16) -> Row {
        Row {
            centre: Vec3::new(1.0, 2.5, -3.0),
            right: Vec3::X,
            health: Health::new(current, max),
        }
    }

    fn held_count(out: &[VfxInstance]) -> usize {
        out.iter()
            .filter(|instance| instance.color[..3] == PIP_HELD)
            .count()
    }

    #[test]
    fn a_full_body_shows_every_pip_and_an_empty_one_shows_none() {
        let mut out = [VfxInstance::default(); READOUT_INSTANCES];
        let count = write(&[row(96, 96)], &mut out);
        assert_eq!(count, READOUT_PIPS);
        assert_eq!(held_count(&out[..count]), READOUT_PIPS);

        let count = write(&[row(0, 96)], &mut out);
        assert_eq!(count, READOUT_PIPS, "a dead body still shows its row");
        assert_eq!(held_count(&out[..count]), 0);
        assert!(
            out[..count]
                .iter()
                .all(|instance| instance.color[..3] == PIP_LOST)
        );
    }

    #[test]
    fn any_health_at_all_keeps_a_pip_lit() {
        // The property that stops "one hit from death" from looking like death.
        let mut out = [VfxInstance::default(); READOUT_INSTANCES];
        for current in 1..=96_u16 {
            let count = write(&[row(current, 96)], &mut out);
            let held = held_count(&out[..count]);
            assert!(held >= 1, "{current} of 96 health showed no pip at all");
            assert!(held <= READOUT_PIPS);
        }
    }

    #[test]
    fn the_row_is_monotonic_in_health() {
        // More health never shows fewer pips. Obvious, and the kind of thing a
        // rounding change breaks silently.
        let mut out = [VfxInstance::default(); READOUT_INSTANCES];
        let mut previous = 0;
        for current in 0..=96_u16 {
            let count = write(&[row(current, 96)], &mut out);
            let held = held_count(&out[..count]);
            assert!(
                held >= previous,
                "{current} health showed {held} pips after {previous}"
            );
            previous = held;
        }
        assert_eq!(previous, READOUT_PIPS);
    }

    #[test]
    fn two_rows_fit_and_a_short_buffer_is_respected() {
        let mut out = [VfxInstance::default(); READOUT_INSTANCES];
        let count = write(&[row(96, 96), row(40, 96)], &mut out);
        assert_eq!(count, READOUT_INSTANCES);
        // A caller with less room gets what fits rather than a panic.
        let mut small = [VfxInstance::default(); 5];
        assert_eq!(write(&[row(96, 96), row(40, 96)], &mut small), 5);
    }

    #[test]
    fn an_impossible_placement_writes_nothing() {
        let mut out = [VfxInstance::default(); READOUT_INSTANCES];
        let bad = Row {
            centre: Vec3::splat(f32::NAN),
            right: Vec3::X,
            health: Health::new(50, 96),
        };
        assert_eq!(write(&[bad], &mut out), 0);
        // A direction that is no direction still produces a finite row.
        let flat = Row {
            centre: Vec3::ZERO,
            right: Vec3::ZERO,
            health: Health::new(50, 96),
        };
        let count = write(&[flat], &mut out);
        assert_eq!(count, READOUT_PIPS);
        for instance in &out[..count] {
            assert!(instance.placement.iter().all(|value| value.is_finite()));
        }
    }

    #[test]
    fn the_row_floats_above_the_body_rather_than_inside_it() {
        let stand = Vec3::new(4.0, 11.0, -2.0);
        let head = above(stand, 2.75);
        assert!(head.y > stand.y + 2.75, "the row is inside the head");
        assert_eq!(head.x, stand.x);
        assert_eq!(head.z, stand.z);
    }

    #[test]
    fn a_row_runs_along_the_direction_it_is_given() {
        // Taken from the camera, so it faces the viewer whichever way the body
        // is turned.
        let mut out = [VfxInstance::default(); READOUT_INSTANCES];
        let along_z = Row {
            centre: Vec3::ZERO,
            right: Vec3::Z,
            health: Health::new(96, 96),
        };
        let count = write(&[along_z], &mut out);
        let xs: Vec<f32> = out[..count].iter().map(|i| i.placement[0]).collect();
        let zs: Vec<f32> = out[..count].iter().map(|i| i.placement[2]).collect();
        assert!(xs.iter().all(|x| x.abs() < 1.0e-6), "the row ran along x");
        assert!(
            zs.first() < zs.last(),
            "the row did not run along z: {zs:?}"
        );
    }
}
