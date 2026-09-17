//! What a foot stands on.
//!
//! **The contract.** [`GroundSampler::surface`] returns the world-space height
//! of the walkable surface a viewer can see at a horizontal position: the top
//! face of the topmost solid voxel of that column. It is a step function, and
//! that is the point. The terrain the player looks at is made of blocks, so a
//! contact model that reports a smooth mathematical surface disagrees with the
//! picture, and a foot placed on it either floats above a block or sinks into
//! one. The image wins.
//!
//! The consequence is deliberate and handled elsewhere: because the surface
//! steps, the **feet** snap to it while the **pelvis** follows a smoothed
//! height, and two-bone leg IK absorbs the difference. That is what leg IK is
//! for, and it is why this trait does not smooth anything itself.
//!
//! `None` means "no answer here" — outside a finite region, for instance. It
//! is never silently turned into zero.
//!
//! This crate deliberately knows nothing about the world generator. The client
//! owns the adapter that answers this trait from a terrain field, which is what
//! keeps a headless character crate free of world-generation code.

/// A queryable walkable surface.
pub trait GroundSampler {
    /// Height of the walkable surface at `(x, z)`, in world units, or `None`
    /// where this sampler has no answer.
    fn surface(&self, x: f64, z: f64) -> Option<f64>;
}

impl<T: GroundSampler + ?Sized> GroundSampler for &T {
    fn surface(&self, x: f64, z: f64) -> Option<f64> {
        (**self).surface(x, z)
    }
}

/// A level floor. The simplest thing a contact test can be wrong against.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlatGround {
    pub height: f64,
}

impl FlatGround {
    #[must_use]
    pub const fn at(height: f64) -> Self {
        Self { height }
    }
}

impl GroundSampler for FlatGround {
    fn surface(&self, _x: f64, _z: f64) -> Option<f64> {
        Some(self.height)
    }
}

/// An ideal slope, continuous in `x`.
///
/// Not what terrain looks like, and that is why it is here: it isolates the
/// IK's behaviour on a gradient from the quantization the real world adds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RampGround {
    /// Rise per unit of `x`.
    pub slope: f64,
    pub height_at_origin: f64,
}

impl GroundSampler for RampGround {
    fn surface(&self, x: f64, _z: f64) -> Option<f64> {
        Some(self.height_at_origin + self.slope * x)
    }
}

/// The same slope, quantized the way voxel terrain actually is.
///
/// This is the shape every contact claim has to survive: a gentle gradient
/// that the world draws as a staircase of one-voxel terraces.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SteppedRamp {
    pub slope: f64,
    pub height_at_origin: f64,
    /// Edge of one terrain voxel, in world units.
    pub voxel: f64,
}

impl SteppedRamp {
    /// A stepped ramp on the terrain lattice, where one voxel is one world
    /// unit.
    #[must_use]
    pub const fn terrain(slope: f64, height_at_origin: f64) -> Self {
        Self {
            slope,
            height_at_origin,
            voxel: 1.0,
        }
    }
}

impl GroundSampler for SteppedRamp {
    fn surface(&self, x: f64, _z: f64) -> Option<f64> {
        if self.voxel <= 0.0 {
            return None;
        }
        let continuous = self.height_at_origin + self.slope * x;
        Some((continuous / self.voxel).floor() * self.voxel + self.voxel)
    }
}

/// A single vertical step, for the case a ramp smooths away.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepGround {
    pub edge_x: f64,
    pub low: f64,
    pub high: f64,
}

impl GroundSampler for StepGround {
    fn surface(&self, x: f64, _z: f64) -> Option<f64> {
        Some(if x < self.edge_x { self.low } else { self.high })
    }
}

/// A sampler that answers only inside a horizontal rectangle.
///
/// Models a finite region: outside it there is no answer, and the contact
/// stage has to say so rather than invent a floor at zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundedGround<G> {
    pub inner: G,
    pub min_x: f64,
    pub max_x: f64,
    pub min_z: f64,
    pub max_z: f64,
}

impl<G: GroundSampler> GroundSampler for BoundedGround<G> {
    fn surface(&self, x: f64, z: f64) -> Option<f64> {
        if x < self.min_x || x > self.max_x || z < self.min_z || z > self.max_z {
            return None;
        }
        self.inner.surface(x, z)
    }
}

#[cfg(test)]
mod tests {
    use super::{BoundedGround, FlatGround, GroundSampler, RampGround, StepGround, SteppedRamp};

    #[test]
    fn a_flat_floor_answers_the_same_everywhere_including_negatives() {
        let ground = FlatGround::at(18.0);
        for (x, z) in [(0.0, 0.0), (-512.5, 300.25), (1e6, -1e6)] {
            assert_eq!(ground.surface(x, z), Some(18.0));
        }
    }

    #[test]
    fn a_ramp_is_continuous_and_a_stepped_ramp_is_not() {
        let ramp = RampGround {
            slope: 0.25,
            height_at_origin: 4.0,
        };
        assert_eq!(ramp.surface(0.0, 0.0), Some(4.0));
        assert_eq!(ramp.surface(4.0, 0.0), Some(5.0));
        assert_eq!(ramp.surface(-4.0, 0.0), Some(3.0));

        let stepped = SteppedRamp::terrain(0.25, 4.0);
        // Within one terrace the answer does not move, and it jumps by exactly
        // one voxel at the terrace edge.
        assert_eq!(stepped.surface(0.0, 0.0), Some(5.0));
        assert_eq!(stepped.surface(3.9, 0.0), Some(5.0));
        assert_eq!(stepped.surface(4.1, 0.0), Some(6.0));
        assert_eq!(stepped.surface(-0.1, 0.0), Some(4.0));
    }

    #[test]
    fn a_stepped_ramp_never_answers_below_the_surface_it_quantizes() {
        let slope = 0.2;
        let stepped = SteppedRamp::terrain(slope, 10.0);
        let mut x = -40.0_f64;
        while x <= 40.0 {
            let Some(top) = stepped.surface(x, 0.0) else {
                panic!("the stepped ramp has an answer everywhere");
            };
            let continuous = 10.0 + slope * x;
            assert!(
                top >= continuous,
                "the block top {top} sits below the field {continuous} at {x}"
            );
            assert!(top - continuous <= 1.0);
            x += 0.37;
        }
    }

    #[test]
    fn a_step_is_a_step() {
        let ground = StepGround {
            edge_x: 2.0,
            low: 1.0,
            high: 3.0,
        };
        assert_eq!(ground.surface(1.999, 0.0), Some(1.0));
        assert_eq!(ground.surface(2.0, 0.0), Some(3.0));
    }

    #[test]
    fn a_bounded_sampler_reports_absence_rather_than_zero() {
        let ground = BoundedGround {
            inner: FlatGround::at(7.0),
            min_x: -10.0,
            max_x: 10.0,
            min_z: -10.0,
            max_z: 10.0,
        };
        assert_eq!(ground.surface(0.0, 0.0), Some(7.0));
        assert_eq!(ground.surface(-10.0, 10.0), Some(7.0));
        assert_eq!(ground.surface(10.01, 0.0), None);
        assert_eq!(ground.surface(0.0, -10.01), None);
    }

    #[test]
    fn a_reference_forwards_to_its_sampler() {
        let ground = FlatGround::at(2.0);
        let borrowed: &dyn GroundSampler = &ground;
        assert_eq!(borrowed.surface(0.0, 0.0), Some(2.0));
        assert_eq!(GroundSampler::surface(&&ground, 0.0, 0.0), Some(2.0));
    }
}
