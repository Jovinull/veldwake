//! The Monolith descriptor: everything about one landmark, before a voxel
//! exists.
//!
//! Small on purpose. `LANDMARK_STYLE.md` allows one family with three
//! silhouette classes, so this describes exactly that and nothing more: there
//! is no universal architecture grammar here, and adding a field means adding
//! a rule to the style contract first.
//!
//! Canonicalisation rather than rejection, for the same reason
//! `veldwake-character` canonicalises a body: a descriptor that came out of a
//! seeded draw should be brought inside the style bands, not refused. The
//! typed [`MonolithDescriptor::validate`] exists for authored input and for
//! tests, and every canonical descriptor is guaranteed to compile.

use crate::hash::fnv1a64;
use crate::landmark::{LANDMARK_COMPILER_VERSION, LANDMARK_SCHEMA_VERSION, LANDMARK_STYLE_VERSION};

/// Which of the three silhouettes a landmark wears.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SilhouetteClass {
    /// One tall tapering shaft ending in a point.
    Spire,
    /// Two shafts carrying a lintel, with an opening a body can walk through.
    Gate,
    /// A frame that fell: two shafts of very different height and rubble.
    Broken,
}

impl SilhouetteClass {
    /// Every class, in declaration order.
    pub const ALL: [Self; 3] = [Self::Spire, Self::Gate, Self::Broken];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Spire => "spire",
            Self::Gate => "gate",
            Self::Broken => "broken",
        }
    }

    /// The style contract's height band for this class.
    #[must_use]
    pub const fn height_band(self) -> (i64, i64) {
        match self {
            Self::Spire => (28, 34),
            Self::Gate => (22, 28),
            Self::Broken => (22, 26),
        }
    }

    /// Whether this class is built from two shafts along a span axis.
    #[must_use]
    pub const fn is_framed(self) -> bool {
        matches!(self, Self::Gate | Self::Broken)
    }
}

/// Which horizontal axis a framed landmark spans.
///
/// Voxel structures are axis aligned: a diagonal frame would be a staircase of
/// half-columns at this scale, which the style contract rejects.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SpanAxis {
    X,
    Z,
}

impl SpanAxis {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Z => "z",
        }
    }
}

/// One landmark, described completely before any voxel is written.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MonolithDescriptor {
    pub class: SilhouetteClass,
    /// Total height in courses, base course included.
    pub height: i64,
    /// Base width of a spire, or the width of each shaft of a frame.
    pub shaft_width: i64,
    /// Footprint depth across the span axis.
    pub depth: i64,
    /// Columns between the two shafts of a frame.
    pub opening: i64,
    /// Courses of lintel, for a gate.
    pub lintel_thickness: i64,
    /// Courses the taller shaft of a gate rises above its lintel.
    pub rise: i64,
    /// Percent of the standing shaft the broken stump keeps.
    pub broken_percent: i64,
    /// How many fallen blocks a broken frame scatters.
    pub rubble: i64,
    /// Courses between strata bands.
    pub band_period: i64,
    /// A spire's single column of lean, applied above half height.
    pub lean: i64,
    pub axis: SpanAxis,
    /// Stream for erosion and rubble, drawn by the plan.
    pub seed: u64,
}

/// Why an authored descriptor is not valid input.
///
/// Canonicalisation cannot produce any of these; they exist so that a hand
/// written descriptor in a test or a future tool fails loudly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DescriptorError {
    OutOfBand {
        field: &'static str,
        found: i64,
        low: i64,
        high: i64,
    },
    EvenShaftWidth {
        found: i64,
    },
    TotalWidth {
        found: i64,
        low: i64,
        high: i64,
    },
    LintelClearance {
        found: i64,
        minimum: i64,
    },
    Proportion {
        found: f64,
        minimum: f64,
    },
}

impl std::fmt::Display for DescriptorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfBand {
                field,
                found,
                low,
                high,
            } => write!(
                formatter,
                "landmark {field} is {found}, outside {low}..={high}"
            ),
            Self::EvenShaftWidth { found } => {
                write!(formatter, "a spire's base width must be odd, found {found}")
            }
            Self::TotalWidth { found, low, high } => write!(
                formatter,
                "a framed landmark is {found} wide, outside {low}..={high}"
            ),
            Self::LintelClearance { found, minimum } => write!(
                formatter,
                "a gate leaves {found} voxels under its lintel, less than {minimum}"
            ),
            Self::Proportion { found, minimum } => write!(
                formatter,
                "a spire is {found:.2} times its base width, less than {minimum}"
            ),
        }
    }
}

impl std::error::Error for DescriptorError {}

/// Total width of a framed landmark, in columns.
const FRAME_WIDTH_BAND: (i64, i64) = (13, 19);
/// Shortest clear height a gate may leave under its lintel.
const MIN_LINTEL_CLEARANCE: i64 = 14;
/// Fraction of a gate's height that must stay clear under the lintel.
const LINTEL_CLEARANCE_FRACTION: f64 = 0.6;
/// How tall a spire is relative to its base width, at least.
const SPIRE_SLENDERNESS: f64 = 4.0;
/// Columns of rubble apron a broken frame reserves beyond its fallen shaft.
pub(crate) const RUBBLE_APRON: i64 = 2;

impl MonolithDescriptor {
    /// A canonical descriptor of a class, from one seeded draw.
    ///
    /// Every field is placed inside its band by construction, so the result
    /// always validates and always compiles. This is the only way the plan
    /// makes a descriptor.
    #[must_use]
    pub fn from_seed(class: SilhouetteClass, axis: SpanAxis, seed: u64) -> Self {
        let pick = |index: u64, low: i64, high: i64| -> i64 {
            let span = (high - low + 1).cast_unsigned();
            low + (draw(seed, index) % span) as i64
        };
        let (low_height, high_height) = class.height_band();
        let height = pick(0, low_height, high_height);
        let band_period = pick(1, 3, 4);
        match class {
            SilhouetteClass::Spire => {
                let shaft_width = 5 + 2 * pick(2, 0, 1);
                Self {
                    class,
                    height,
                    shaft_width,
                    depth: shaft_width,
                    opening: 0,
                    lintel_thickness: 0,
                    rise: 0,
                    broken_percent: 0,
                    rubble: 0,
                    band_period,
                    // Always one column, never none: a spire with no lean and
                    // symmetric weathering would be its own mirror image, which
                    // the style contract rejects.
                    lean: 1 - 2 * pick(3, 0, 1),
                    axis,
                    seed,
                }
            }
            SilhouetteClass::Gate | SilhouetteClass::Broken => {
                let shaft_width = pick(2, 3, 5);
                // The opening is drawn and then pulled inside the total-width
                // band, so no draw can produce a frame the style rejects.
                let opening = pick(3, 5, 9)
                    .max(FRAME_WIDTH_BAND.0 - 2 * shaft_width)
                    .min(FRAME_WIDTH_BAND.1 - 2 * shaft_width)
                    .clamp(5, 9);
                let lintel_thickness = if class == SilhouetteClass::Gate {
                    pick(4, 2, 3)
                } else {
                    0
                };
                let rise = if class == SilhouetteClass::Gate {
                    pick(5, 1, 3)
                } else {
                    0
                };
                Self {
                    class,
                    height,
                    shaft_width,
                    depth: shaft_width,
                    opening,
                    lintel_thickness,
                    rise,
                    broken_percent: if class == SilhouetteClass::Broken {
                        pick(6, 35, 60)
                    } else {
                        0
                    },
                    rubble: if class == SilhouetteClass::Broken {
                        pick(7, 3, 8)
                    } else {
                        0
                    },
                    band_period,
                    lean: 0,
                    axis,
                    seed,
                }
            }
        }
    }

    /// Clear height under a gate's lintel, derived rather than stored so it
    /// cannot disagree with the height.
    #[must_use]
    pub const fn lintel_clearance(&self) -> i64 {
        self.height - self.lintel_thickness - self.rise
    }

    /// Height of the shorter mass: a gate's flush shaft or a broken stump.
    #[must_use]
    pub const fn short_shaft_height(&self) -> i64 {
        match self.class {
            SilhouetteClass::Spire => self.height,
            SilhouetteClass::Gate => self.height - self.rise,
            SilhouetteClass::Broken => {
                let stump = self.height * self.broken_percent / 100;
                if stump < 1 { 1 } else { stump }
            }
        }
    }

    /// Total width along the span axis, in columns.
    #[must_use]
    pub const fn span_width(&self) -> i64 {
        match self.class {
            SilhouetteClass::Spire => self.shaft_width,
            SilhouetteClass::Gate => 2 * self.shaft_width + self.opening,
            SilhouetteClass::Broken => 2 * self.shaft_width + self.opening + RUBBLE_APRON,
        }
    }

    /// Footprint in world columns: `(along x, along z)`.
    #[must_use]
    pub const fn footprint(&self) -> (i64, i64) {
        match self.axis {
            SpanAxis::X => (self.span_width(), self.depth),
            SpanAxis::Z => (self.depth, self.span_width()),
        }
    }

    /// Rejects anything outside the style contract.
    pub fn validate(&self) -> Result<(), DescriptorError> {
        let band = |field, found, (low, high): (i64, i64)| {
            if (low..=high).contains(&found) {
                Ok(())
            } else {
                Err(DescriptorError::OutOfBand {
                    field,
                    found,
                    low,
                    high,
                })
            }
        };
        band("height", self.height, self.class.height_band())?;
        band("band_period", self.band_period, (3, 4))?;
        band("depth", self.depth, (3, 7))?;
        match self.class {
            SilhouetteClass::Spire => {
                band("shaft_width", self.shaft_width, (5, 7))?;
                if self.shaft_width % 2 == 0 {
                    return Err(DescriptorError::EvenShaftWidth {
                        found: self.shaft_width,
                    });
                }
                band("lean", self.lean, (-1, 1))?;
                if self.lean == 0 {
                    return Err(DescriptorError::OutOfBand {
                        field: "lean",
                        found: 0,
                        low: -1,
                        high: 1,
                    });
                }
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "landmark dimensions are tens of voxels"
                )]
                let slenderness = self.height as f64 / self.shaft_width as f64;
                if slenderness < SPIRE_SLENDERNESS {
                    return Err(DescriptorError::Proportion {
                        found: slenderness,
                        minimum: SPIRE_SLENDERNESS,
                    });
                }
            }
            SilhouetteClass::Gate | SilhouetteClass::Broken => {
                band("shaft_width", self.shaft_width, (3, 5))?;
                band("opening", self.opening, (5, 9))?;
                let frame = 2 * self.shaft_width + self.opening;
                if !(FRAME_WIDTH_BAND.0..=FRAME_WIDTH_BAND.1).contains(&frame) {
                    return Err(DescriptorError::TotalWidth {
                        found: frame,
                        low: FRAME_WIDTH_BAND.0,
                        high: FRAME_WIDTH_BAND.1,
                    });
                }
                if self.class == SilhouetteClass::Gate {
                    band("lintel_thickness", self.lintel_thickness, (2, 3))?;
                    band("rise", self.rise, (1, 3))?;
                    let clearance = self.lintel_clearance();
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "landmark dimensions are tens of voxels"
                    )]
                    let minimum = MIN_LINTEL_CLEARANCE
                        .max((self.height as f64 * LINTEL_CLEARANCE_FRACTION).ceil() as i64);
                    if clearance < minimum {
                        return Err(DescriptorError::LintelClearance {
                            found: clearance,
                            minimum,
                        });
                    }
                } else {
                    band("broken_percent", self.broken_percent, (35, 60))?;
                    band("rubble", self.rubble, (3, 8))?;
                }
            }
        }
        Ok(())
    }

    /// Canonical bytes, in a fixed order, for the identity fingerprint.
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(128);
        bytes.push(match self.class {
            SilhouetteClass::Spire => 0,
            SilhouetteClass::Gate => 1,
            SilhouetteClass::Broken => 2,
        });
        bytes.push(match self.axis {
            SpanAxis::X => 0,
            SpanAxis::Z => 1,
        });
        for value in [
            self.height,
            self.shaft_width,
            self.depth,
            self.opening,
            self.lintel_thickness,
            self.rise,
            self.broken_percent,
            self.rubble,
            self.band_period,
            self.lean,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&self.seed.to_le_bytes());
        bytes
    }

    /// Identity of this descriptor under the current schema, compiler and
    /// style versions.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut bytes = Vec::with_capacity(160);
        bytes.extend_from_slice(b"veldwake.landmark.descriptor");
        bytes.extend_from_slice(&LANDMARK_SCHEMA_VERSION.to_le_bytes());
        bytes.extend_from_slice(&LANDMARK_COMPILER_VERSION.to_le_bytes());
        bytes.extend_from_slice(&LANDMARK_STYLE_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.canonical_bytes());
        fnv1a64(&bytes)
    }
}

/// One deterministic draw from a descriptor seed.
pub(crate) fn draw(seed: u64, index: u64) -> u64 {
    fnv1a64(&[seed.to_le_bytes(), index.to_le_bytes()].concat())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_seeded_descriptor_of_every_class_validates() {
        for class in SilhouetteClass::ALL {
            for axis in [SpanAxis::X, SpanAxis::Z] {
                for seed in 0..512_u64 {
                    let descriptor = MonolithDescriptor::from_seed(class, axis, seed);
                    if let Err(error) = descriptor.validate() {
                        panic!("{class:?} seed {seed} produced {error}: {descriptor:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn canonical_draws_are_deterministic_and_the_fingerprint_reacts_to_every_field() {
        let base = MonolithDescriptor::from_seed(SilhouetteClass::Gate, SpanAxis::X, 7);
        assert_eq!(
            base,
            MonolithDescriptor::from_seed(SilhouetteClass::Gate, SpanAxis::X, 7)
        );
        assert_eq!(base.fingerprint(), base.fingerprint());
        let mut seen = std::collections::BTreeSet::new();
        seen.insert(base.fingerprint());
        for mutated in [
            MonolithDescriptor {
                height: base.height + 1,
                ..base
            },
            MonolithDescriptor {
                shaft_width: base.shaft_width + 1,
                ..base
            },
            MonolithDescriptor {
                depth: base.depth + 1,
                ..base
            },
            MonolithDescriptor {
                opening: base.opening + 1,
                ..base
            },
            MonolithDescriptor {
                lintel_thickness: base.lintel_thickness + 1,
                ..base
            },
            MonolithDescriptor {
                rise: base.rise + 1,
                ..base
            },
            MonolithDescriptor {
                broken_percent: base.broken_percent + 1,
                ..base
            },
            MonolithDescriptor {
                rubble: base.rubble + 1,
                ..base
            },
            MonolithDescriptor {
                band_period: base.band_period + 1,
                ..base
            },
            MonolithDescriptor {
                lean: base.lean + 1,
                ..base
            },
            MonolithDescriptor {
                axis: SpanAxis::Z,
                ..base
            },
            MonolithDescriptor {
                seed: base.seed + 1,
                ..base
            },
            MonolithDescriptor {
                class: SilhouetteClass::Broken,
                ..base
            },
        ] {
            assert!(
                seen.insert(mutated.fingerprint()),
                "the fingerprint ignored a field: {mutated:?}"
            );
        }
    }

    #[test]
    fn validation_rejects_what_canonicalisation_cannot_produce() {
        let spire = MonolithDescriptor::from_seed(SilhouetteClass::Spire, SpanAxis::X, 3);
        assert!(matches!(
            MonolithDescriptor {
                shaft_width: 6,
                ..spire
            }
            .validate(),
            Err(DescriptorError::EvenShaftWidth { found: 6 })
        ));
        assert!(matches!(
            MonolithDescriptor {
                height: 12,
                ..spire
            }
            .validate(),
            Err(DescriptorError::OutOfBand {
                field: "height",
                ..
            })
        ));
        let gate = MonolithDescriptor::from_seed(SilhouetteClass::Gate, SpanAxis::X, 5);
        assert!(matches!(
            MonolithDescriptor { rise: 12, ..gate }.validate(),
            Err(DescriptorError::OutOfBand { field: "rise", .. })
        ));
        assert!(matches!(
            MonolithDescriptor {
                opening: 5,
                shaft_width: 3,
                ..gate
            }
            .validate(),
            Err(DescriptorError::TotalWidth { found: 11, .. })
        ));
    }

    #[test]
    fn a_frame_is_never_narrower_than_the_style_band_however_it_is_drawn() {
        for class in [SilhouetteClass::Gate, SilhouetteClass::Broken] {
            for seed in 0..512_u64 {
                let descriptor = MonolithDescriptor::from_seed(class, SpanAxis::X, seed);
                let frame = 2 * descriptor.shaft_width + descriptor.opening;
                assert!(
                    (FRAME_WIDTH_BAND.0..=FRAME_WIDTH_BAND.1).contains(&frame),
                    "{class:?} seed {seed} is {frame} wide"
                );
            }
        }
    }
}
