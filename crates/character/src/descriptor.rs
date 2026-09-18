//! What a character *is*, before anything has been built.
//!
//! A descriptor is small, semantic, and public data: proportions as fractions
//! of body height, a palette choice, a seed, and the schema it was written
//! against. It is never compiled directly. [`CharacterDescriptor::validate`]
//! is the only way to obtain a [`ValidatedDescriptor`], and the compiler takes
//! nothing else, so an invalid character cannot exist far enough into the
//! pipeline to produce geometry.
//!
//! The bands every field is checked against are the character style contract's
//! (`docs/audiovisual/CHARACTER_STYLE.md`), not invented here.

use crate::hash::{fnv1a64, push_f64, push_u32, push_u64, signed_from_hash, sub_hash};
use crate::material::{GarmentScheme, HairTone, PaletteChoice, SkinTone};

/// Character voxels per world unit.
///
/// The one place the ratio between the terrain domain (one voxel per world
/// unit) and the character domain is declared. It is a style decision and it
/// is versioned with the style contract, not a physical constant.
///
/// **Twelve, decided by capture rather than by arithmetic.** Sixteen was the
/// first value, which made the reference humanoid `1.75` world units tall.
/// Two frames rejected it. The scale-reference capture showed a character the
/// M4 undergrowth dwarfed: low vegetation stands up to three world units, so
/// a bush was nearly twice the person's height. And a terrain voxel is a
/// whole world unit, which at that scale was taller than the character's
/// entire leg, so it could not step onto a single terrace of its own world.
/// At twelve the humanoid is `2.33` world units, its leg is `1.17`, a terrace
/// is a step it can take, and a tree still stands three to five times its
/// height.
pub const CHARACTER_VOXELS_PER_WORLD_UNIT: f64 = 12.0;

/// Edge of one character voxel, in world units.
pub const CHARACTER_VOXEL_SIZE: f32 = 1.0 / 12.0;

/// Version of `docs/audiovisual/CHARACTER_STYLE.md`.
///
/// It participates in [`CharacterIdentity`] and therefore in a compiled
/// character's fingerprint. It is deliberately **not** part of any chunk cache
/// key: a character rule must never invalidate the terrain a player already
/// has on disk.
pub const CHARACTER_STYLE_VERSION: u32 = 1;

/// Bumped by hand for an intentional compiler-contract revision that the
/// descriptor does not already capture.
pub const CHARACTER_COMPILER_VERSION: u32 = 1;

/// The descriptor schema this crate understands.
pub const CHARACTER_SCHEMA_VERSION: u32 = 1;

/// Smallest and largest body height the style contract allows, in character
/// voxels.
pub const MIN_HEIGHT_VOXELS: i32 = 20;
/// See [`MIN_HEIGHT_VOXELS`].
pub const MAX_HEIGHT_VOXELS: i32 = 40;

/// Thinnest feature the style contract allows, in character voxels.
pub const MIN_FEATURE_VOXELS: i32 = 2;

/// The seed a character's bounded variation is drawn from.
///
/// A newtype rather than a bare `u64` so a character seed cannot be confused
/// with a world seed, a fingerprint, or an index at a call site.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CharacterSeed(pub u64);

impl CharacterSeed {
    /// The named seed of the M5 golden humanoid.
    pub const GOLDEN: Self = Self(0x5645_4c44_4348_4152);

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Derives a stable child stream from a domain label.
    ///
    /// Named streams, exactly as `DETERMINISM.md` requires: adding a draw to
    /// one variation cannot perturb another, and nothing consumes sequential
    /// state.
    #[must_use]
    pub fn stream(self, label: VariationLabel) -> u64 {
        let mut bytes = Vec::with_capacity(24);
        push_u64(&mut bytes, self.0);
        push_u64(&mut bytes, label as u64);
        push_u32(&mut bytes, CHARACTER_COMPILER_VERSION);
        fnv1a64(&bytes)
    }
}

/// Named variation streams. One per thing the seed is allowed to move.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u64)]
pub enum VariationLabel {
    Stature = 1,
    LimbBuild = 2,
    HeadSize = 3,
    ShoulderBuild = 4,
}

/// The morphology families this crate can compile.
///
/// One variant. M5 proves a compiler path for a humanoid; a second family
/// would need its own grammar, its own skeleton, and its own style rules, and
/// inventing an enum with one real arm and one imagined one would be an
/// extension point with no user.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Archetype {
    #[default]
    Humanoid,
}

impl Archetype {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Humanoid => "humanoid",
        }
    }
}

/// Body proportions, as fractions of total body height unless stated.
///
/// Every field is an art control with a semantic name and a band in the
/// character style contract. The compiler reads these instead of scattering
/// magic constants, and the fingerprint hashes them so a changed control
/// produces a different character identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Proportions {
    /// Total body height, in world units.
    pub total_height_units: f64,
    /// Head height, from the chin to the crown.
    pub head_height_fraction: f64,
    /// Ground to hip joint.
    pub leg_length_fraction: f64,
    /// Shoulder joint to fingertip.
    pub arm_length_fraction: f64,
    /// Outer edge of one arm to the outer edge of the other, at rest.
    pub shoulder_span_fraction: f64,
    /// Width of the pelvis box.
    pub hip_width_fraction: f64,
    /// Width of the abdomen box; the waist the silhouette reads.
    pub waist_width_fraction: f64,
    /// Front-to-back depth of the pelvis and chest boxes.
    pub torso_depth_fraction: f64,
    /// Cross-section of an arm or a leg segment.
    pub limb_thickness_fraction: f64,
    /// Toe to heel.
    pub foot_length_fraction: f64,
    /// Front-to-back depth of a hand.
    pub hand_depth_fraction: f64,
    /// Thigh length over the distance from the ankle to the hip.
    pub thigh_share: f64,
    /// Upper-arm length over the distance from the shoulder to the wrist.
    pub upper_arm_share: f64,
    /// Hand length over the whole arm length.
    pub hand_share: f64,
}

impl Proportions {
    /// The reference humanoid of the M5 slice.
    ///
    /// The numbers are the character style contract's proportion bands
    /// expressed as a specific body: twenty-eight voxels tall, a head one
    /// fifth and a half of that, legs exactly half the height, and shoulders
    /// made by where the arms hang rather than by a wide chest box.
    #[must_use]
    pub const fn golden() -> Self {
        Self {
            total_height_units: 2.3333,
            head_height_fraction: 0.1786,
            leg_length_fraction: 0.5000,
            arm_length_fraction: 0.4286,
            // Twelve voxels across, not fourteen. Fourteen came from the
            // drawing board and fourteen is what the first capture rejected:
            // with a straight chest the arms no longer reached it, and with a
            // chest wide enough to reach them the torso became a slab. Twelve
            // puts each arm one voxel into the chest, which closes the
            // shoulder, and two voxels clear of it, which is the whole
            // silhouette a viewer gets of an arm at rest.
            shoulder_span_fraction: 0.4286,
            hip_width_fraction: 0.2857,
            waist_width_fraction: 0.2143,
            torso_depth_fraction: 0.2143,
            limb_thickness_fraction: 0.1071,
            foot_length_fraction: 0.1786,
            hand_depth_fraction: 0.1429,
            thigh_share: 0.4545,
            upper_arm_share: 0.5556,
            hand_share: 0.2500,
        }
    }

    /// A deliberately different body: shorter, heavier, and broader.
    ///
    /// Its whole purpose is to fail if the compiler is a hard-coded model
    /// rather than a compiler. Every number here differs from the golden one
    /// and the result must still satisfy every style rule.
    #[must_use]
    pub const fn sturdy() -> Self {
        Self {
            total_height_units: 2.0,
            head_height_fraction: 0.1700,
            leg_length_fraction: 0.4583,
            arm_length_fraction: 0.4167,
            shoulder_span_fraction: 0.5000,
            hip_width_fraction: 0.3333,
            waist_width_fraction: 0.2500,
            torso_depth_fraction: 0.2400,
            limb_thickness_fraction: 0.1250,
            foot_length_fraction: 0.2400,
            hand_depth_fraction: 0.2000,
            thigh_share: 0.5000,
            upper_arm_share: 0.5714,
            hand_share: 0.3000,
        }
    }

    fn fields(&self) -> [(&'static str, f64); 14] {
        [
            ("total_height_units", self.total_height_units),
            ("head_height_fraction", self.head_height_fraction),
            ("leg_length_fraction", self.leg_length_fraction),
            ("arm_length_fraction", self.arm_length_fraction),
            ("shoulder_span_fraction", self.shoulder_span_fraction),
            ("hip_width_fraction", self.hip_width_fraction),
            ("waist_width_fraction", self.waist_width_fraction),
            ("torso_depth_fraction", self.torso_depth_fraction),
            ("limb_thickness_fraction", self.limb_thickness_fraction),
            ("foot_length_fraction", self.foot_length_fraction),
            ("hand_depth_fraction", self.hand_depth_fraction),
            ("thigh_share", self.thigh_share),
            ("upper_arm_share", self.upper_arm_share),
            ("hand_share", self.hand_share),
        ]
    }
}

/// Why a descriptor cannot safely be compiled into a character.
///
/// Typed, with the offending field named, because a validation error that says
/// only "invalid" costs the next person the whole search.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CharacterDescriptorError {
    UnsupportedSchema {
        found: u32,
        supported: u32,
    },
    NonFinite {
        field: &'static str,
    },
    NonPositive {
        field: &'static str,
    },
    OutOfBand {
        field: &'static str,
        value: f64,
        low: f64,
        high: f64,
    },
    HeightOutOfRange {
        voxels: i32,
        low: i32,
        high: i32,
    },
    /// A derived body part rounds to fewer voxels than the style contract's
    /// minimum feature size.
    FeatureTooThin {
        feature: &'static str,
        voxels: i32,
        minimum: i32,
    },
    /// The head is too large a share of the shoulder span to read as a head.
    HeadDoesNotRead {
        head_width: i32,
        shoulder_span: i32,
    },
    /// The arms fall inside the chest box, so there are no shoulders.
    ArmsDoNotSeparate {
        arm_outer: i32,
        chest_outer: i32,
    },
    /// The fingertips do not fall between the hip and the knee.
    ReachIsImplausible {
        fingertip_y: i32,
        knee_y: i32,
        hip_y: i32,
    },
    /// A joint's two parts would not share a voxel, so flexing would open a
    /// hole.
    JointWouldOpen {
        joint: &'static str,
    },
}

impl std::fmt::Display for CharacterDescriptorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSchema { found, supported } => write!(
                formatter,
                "descriptor schema {found} is not the supported schema {supported}"
            ),
            Self::NonFinite { field } => {
                write!(formatter, "character control {field} must be finite")
            }
            Self::NonPositive { field } => {
                write!(formatter, "character control {field} must be positive")
            }
            Self::OutOfBand {
                field,
                value,
                low,
                high,
            } => write!(
                formatter,
                "character control {field} is {value}, outside the style band [{low}, {high}]"
            ),
            Self::HeightOutOfRange { voxels, low, high } => write!(
                formatter,
                "body height {voxels} voxels is outside the style range [{low}, {high}]"
            ),
            Self::FeatureTooThin {
                feature,
                voxels,
                minimum,
            } => write!(
                formatter,
                "{feature} rounds to {voxels} voxels, below the {minimum}-voxel minimum"
            ),
            Self::HeadDoesNotRead {
                head_width,
                shoulder_span,
            } => write!(
                formatter,
                "head {head_width} is too wide for a shoulder span of {shoulder_span}"
            ),
            Self::ArmsDoNotSeparate {
                arm_outer,
                chest_outer,
            } => write!(
                formatter,
                "arms reach {arm_outer} but the chest already reaches {chest_outer}"
            ),
            Self::ReachIsImplausible {
                fingertip_y,
                knee_y,
                hip_y,
            } => write!(
                formatter,
                "fingertips at {fingertip_y} do not fall between the knee at {knee_y} and the hip at {hip_y}"
            ),
            Self::JointWouldOpen { joint } => {
                write!(formatter, "the {joint} joint's parts do not overlap")
            }
        }
    }
}

impl std::error::Error for CharacterDescriptorError {}

/// Everything that decides what a character is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterDescriptor {
    pub schema_version: u32,
    pub archetype: Archetype,
    pub seed: CharacterSeed,
    pub proportions: Proportions,
    pub palette: PaletteChoice,
    /// How far the seed may move a proportion, as a fraction of itself.
    ///
    /// Bounded variation, never structural variation: the seed can make a
    /// character a little taller or a little heavier, and can do nothing else.
    /// Zero is a reference body.
    pub build_variation: f64,
}

/// Largest variation the style contract allows.
pub const MAX_BUILD_VARIATION: f64 = 0.08;

impl CharacterDescriptor {
    /// The reference humanoid of the M5 slice, with no variation applied.
    #[must_use]
    pub fn golden() -> Self {
        Self {
            schema_version: CHARACTER_SCHEMA_VERSION,
            archetype: Archetype::Humanoid,
            seed: CharacterSeed::GOLDEN,
            proportions: Proportions::golden(),
            // Rust linen rather than moss wool: the reference humanoid stands
            // in a green meadow among green shrubs, and the first captures
            // showed a moss tunic dissolving into both. Value separation was
            // satisfied and hue separation was not, which is a reminder that
            // the value rule is a floor rather than the whole story.
            palette: PaletteChoice {
                skin: SkinTone::Tan,
                // Dark rather than auburn: auburn hair over a rust tunic has
                // almost the same value and almost the same hue, and the
                // close-up capture showed the cap and the torso reading as one
                // mass with a face floating in it.
                hair: HairTone::Dark,
                garment: GarmentScheme::RustLinen,
            },
            build_variation: 0.0,
        }
    }

    /// Serialises the descriptor for hashing.
    ///
    /// Floating-point controls are hashed by their bit pattern, which is exact.
    /// The order is fixed here and defines the fingerprint; it must never be
    /// reshuffled casually.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(160);
        push_u32(&mut bytes, self.schema_version);
        push_u32(&mut bytes, self.archetype as u32);
        push_u64(&mut bytes, self.seed.raw());
        for (_, value) in self.proportions.fields() {
            push_f64(&mut bytes, value);
        }
        push_f64(&mut bytes, self.build_variation);
        push_u32(&mut bytes, self.palette.skin as u32);
        push_u32(&mut bytes, self.palette.hair as u32);
        push_u32(&mut bytes, self.palette.garment as u32);
        bytes
    }

    /// Rejects anything that cannot become a character that satisfies the
    /// style contract.
    ///
    /// Validation is done on the **derived integer body**, not only on the
    /// input fractions, because rounding is where a plausible-looking ratio
    /// turns into a one-voxel limb.
    pub fn validate(&self) -> Result<ValidatedDescriptor, CharacterDescriptorError> {
        if self.schema_version != CHARACTER_SCHEMA_VERSION {
            return Err(CharacterDescriptorError::UnsupportedSchema {
                found: self.schema_version,
                supported: CHARACTER_SCHEMA_VERSION,
            });
        }
        for (field, value) in self.proportions.fields() {
            if !value.is_finite() {
                return Err(CharacterDescriptorError::NonFinite { field });
            }
            if value <= 0.0 {
                return Err(CharacterDescriptorError::NonPositive { field });
            }
        }
        if !self.build_variation.is_finite() {
            return Err(CharacterDescriptorError::NonFinite {
                field: "build_variation",
            });
        }
        if !(0.0..=MAX_BUILD_VARIATION).contains(&self.build_variation) {
            return Err(CharacterDescriptorError::OutOfBand {
                field: "build_variation",
                value: self.build_variation,
                low: 0.0,
                high: MAX_BUILD_VARIATION,
            });
        }

        let varied = self.varied_proportions();
        for (field, low, high) in STYLE_BANDS {
            let value = varied.field(field);
            if !(low..=high).contains(&value) {
                return Err(CharacterDescriptorError::OutOfBand {
                    field,
                    value,
                    low,
                    high,
                });
            }
        }

        let body = BodyMetrics::derive(&varied)?;
        Ok(ValidatedDescriptor {
            descriptor: *self,
            proportions: varied,
            body,
        })
    }

    /// The proportions after the seed's bounded variation.
    fn varied_proportions(&self) -> Proportions {
        let mut varied = self.proportions;
        if self.build_variation <= 0.0 {
            return varied;
        }
        let scale = |seed: CharacterSeed, label: VariationLabel, index: u64, amount: f64| {
            let draw = signed_from_hash(sub_hash(seed.stream(label), index));
            draw.mul_add(amount, 1.0)
        };
        let amount = self.build_variation;
        varied.total_height_units *= scale(self.seed, VariationLabel::Stature, 0, amount);
        varied.limb_thickness_fraction *=
            scale(self.seed, VariationLabel::LimbBuild, 0, amount * 0.5);
        varied.head_height_fraction *= scale(self.seed, VariationLabel::HeadSize, 0, amount * 0.5);
        varied.shoulder_span_fraction *=
            scale(self.seed, VariationLabel::ShoulderBuild, 0, amount * 0.5);
        varied
    }
}

impl Proportions {
    fn field(&self, name: &'static str) -> f64 {
        for (field, value) in self.fields() {
            if field == name {
                return value;
            }
        }
        // `STYLE_BANDS` only names fields `fields()` lists, and both live in
        // this module; a mismatch is a compile-time-adjacent programming error
        // rather than a runtime condition, so surface it loudly in tests.
        debug_assert!(false, "unknown proportion field {name}");
        f64::NAN
    }
}

/// The character style contract's proportion bands, as data.
const STYLE_BANDS: [(&str, f64, f64); 8] = [
    ("head_height_fraction", 0.150, 0.200),
    ("leg_length_fraction", 0.440, 0.560),
    ("arm_length_fraction", 0.340, 0.460),
    ("limb_thickness_fraction", 0.070, 0.160),
    ("thigh_share", 0.350, 0.650),
    ("upper_arm_share", 0.350, 0.650),
    ("hand_share", 0.150, 0.350),
    ("torso_depth_fraction", 0.100, 0.300),
];

/// The integer body a descriptor derives to, in character voxels.
///
/// This is where fractions stop and geometry starts. Every field is a voxel
/// count or a voxel-space coordinate, and every downstream stage reads these
/// rather than re-deriving them from fractions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BodyMetrics {
    /// Total height, in voxels. Ground is `y = 0`; the crown is `y = height`.
    pub height: i32,
    pub head_height: i32,
    pub head_width: i32,
    pub head_depth: i32,
    /// Ground to hip joint.
    pub leg_length: i32,
    pub thigh_length: i32,
    pub foot_height: i32,
    pub foot_length: i32,
    /// Shoulder joint to fingertip.
    pub arm_length: i32,
    pub upper_arm_length: i32,
    pub forearm_length: i32,
    pub hand_length: i32,
    pub hand_depth: i32,
    pub limb_thickness: i32,
    /// Outer edge of one arm to the outer edge of the other.
    pub shoulder_span: i32,
    pub hip_width: i32,
    pub waist_width: i32,
    pub torso_depth: i32,
    pub waist_depth: i32,
    /// Absolute voxel-space heights of the joints.
    pub ankle_y: i32,
    pub knee_y: i32,
    pub hip_y: i32,
    pub spine_y: i32,
    pub chest_y: i32,
    pub shoulder_y: i32,
    pub elbow_y: i32,
    pub wrist_y: i32,
    pub neck_y: i32,
    /// Lowest voxel row of the head box.
    pub head_bottom: i32,
    /// Half of the character's lateral extent at the pelvis.
    pub torso_half_width: i32,
    /// Width of the chest box, which is wider than the pelvis.
    pub chest_width: i32,
    /// Half of the chest box's width.
    pub chest_half_width: i32,
    /// Inner edge of a leg, on the positive side.
    pub leg_inner: i32,
    /// Inner edge of an arm, on the positive side.
    pub arm_inner: i32,
}

/// Fraction of the torso height at which the spine joint sits.
const SPINE_SHARE: f64 = 0.375;
/// Fraction of the torso height at which the chest joint sits.
const CHEST_SHARE: f64 = 0.625;
/// Height of the neck gap between the shoulder joint and the chin, as a
/// fraction of body height.
///
/// The torso height is derived from this rather than declared independently.
/// Two independent controls for "where the shoulders are" and "where the chin
/// is" can be set so that the head starts below the shoulders, which is not a
/// proportion a validator should have to catch after the fact.
const NECK_SHARE: f64 = 0.03;
/// Shortest torso, hip joint to shoulder joint, in voxels.
const MIN_TORSO_VOXELS: i32 = 4;

fn round_at_least(value: f64, minimum: i32) -> i32 {
    let rounded = value.round();
    if rounded.is_finite() {
        (rounded as i32).max(minimum)
    } else {
        minimum
    }
}

impl BodyMetrics {
    fn derive(p: &Proportions) -> Result<Self, CharacterDescriptorError> {
        let height_f = p.total_height_units * CHARACTER_VOXELS_PER_WORLD_UNIT;
        if !height_f.is_finite() {
            return Err(CharacterDescriptorError::NonFinite {
                field: "total_height_units",
            });
        }
        let height = round_at_least(height_f, 0);
        if !(MIN_HEIGHT_VOXELS..=MAX_HEIGHT_VOXELS).contains(&height) {
            return Err(CharacterDescriptorError::HeightOutOfRange {
                voxels: height,
                low: MIN_HEIGHT_VOXELS,
                high: MAX_HEIGHT_VOXELS,
            });
        }
        let scale = f64::from(height);

        let limb_thickness = round_at_least(p.limb_thickness_fraction * scale, 0);
        let head_height = round_at_least(p.head_height_fraction * scale, 0);
        let leg_length = round_at_least(p.leg_length_fraction * scale, 0);
        let arm_length = round_at_least(p.arm_length_fraction * scale, 0);
        let foot_length = round_at_least(p.foot_length_fraction * scale, 0);
        let hand_depth = round_at_least(p.hand_depth_fraction * scale, 0);
        let torso_depth = round_at_least(p.torso_depth_fraction * scale, 0);
        // Even widths so the midline plane at `x = 0` is a voxel boundary and
        // the left and right sides mirror exactly.
        let hip_width = round_at_least(p.hip_width_fraction * scale, 0) & !1;
        // The waist reads by construction rather than by rejection: rounding
        // two independent fractions to even widths can make them collide at
        // some heights, and narrowing is the answer a generator should give.
        let waist_width =
            (round_at_least(p.waist_width_fraction * scale, 0) & !1).min(hip_width - 2);
        let shoulder_span = round_at_least(p.shoulder_span_fraction * scale, 0) & !1;
        // The head box follows the head's own height rather than a separate
        // control: a head whose width and height are independent is a way to
        // produce a brick or a pole, and neither is a face. Slightly wider
        // than tall, because the first captures showed a head that read as
        // too small against the shoulders at conversational distance.
        let head_width = round_at_least(f64::from(head_height) * 1.1, 0) & !1;
        // Strictly shallower than the torso, so the head's bottom row sits
        // inside the chest instead of sharing a face plane with it.
        let head_depth = (head_width - 1)
            .min(torso_depth - 1)
            .max(MIN_FEATURE_VOXELS);
        let waist_depth = (torso_depth - 2).max(0);

        for (feature, voxels) in [
            ("limb thickness", limb_thickness),
            ("head height", head_height),
            ("head width", head_width),
            ("head depth", head_depth),
            ("torso depth", torso_depth),
            ("waist depth", waist_depth),
            ("foot length", foot_length),
            ("hand depth", hand_depth),
            ("hip width", hip_width),
            ("waist width", waist_width),
        ] {
            if voxels < MIN_FEATURE_VOXELS {
                return Err(CharacterDescriptorError::FeatureTooThin {
                    feature,
                    voxels,
                    minimum: MIN_FEATURE_VOXELS,
                });
            }
        }

        let foot_height = limb_thickness;
        let ankle_y = foot_height;
        let hip_y = leg_length;
        let above_ankle = hip_y - ankle_y;
        let thigh_length = round_at_least(f64::from(above_ankle) * p.thigh_share, 0);
        let shin_length = above_ankle - thigh_length;
        for (feature, voxels) in [("thigh", thigh_length), ("shin", shin_length)] {
            if voxels < MIN_FEATURE_VOXELS {
                return Err(CharacterDescriptorError::FeatureTooThin {
                    feature,
                    voxels,
                    minimum: MIN_FEATURE_VOXELS,
                });
            }
        }
        let knee_y = hip_y - thigh_length;

        let hand_length = round_at_least(f64::from(arm_length) * p.hand_share, 0);
        let above_wrist = arm_length - hand_length;
        let upper_arm_length = round_at_least(f64::from(above_wrist) * p.upper_arm_share, 0);
        let forearm_length = above_wrist - upper_arm_length;
        for (feature, voxels) in [
            ("hand", hand_length),
            ("upper arm", upper_arm_length),
            ("forearm", forearm_length),
        ] {
            if voxels < MIN_FEATURE_VOXELS {
                return Err(CharacterDescriptorError::FeatureTooThin {
                    feature,
                    voxels,
                    minimum: MIN_FEATURE_VOXELS,
                });
            }
        }

        let head_bottom = height - head_height;
        let neck_gap = round_at_least(NECK_SHARE * scale, 1);
        let shoulder_y = head_bottom - neck_gap;
        let torso_height = shoulder_y - hip_y;
        if torso_height < MIN_TORSO_VOXELS {
            return Err(CharacterDescriptorError::JointWouldOpen { joint: "torso" });
        }
        let elbow_y = shoulder_y - upper_arm_length;
        let wrist_y = elbow_y - forearm_length;
        let fingertip_y = wrist_y - hand_length;
        let spine_y = hip_y + round_at_least(f64::from(torso_height) * SPINE_SHARE, 0);
        let chest_y = hip_y + round_at_least(f64::from(torso_height) * CHEST_SHARE, 0);
        let neck_y = head_bottom + 1;

        let torso_half_width = hip_width / 2;
        // The chest is exactly as wide as the pelvis, and the capture that
        // settled it is worth stating. A chest wider than the pelvis was tried
        // first, on the theory that the step would read as a torso. In the
        // game it did the opposite: the chest overhung the waist as a ledge,
        // it reached past the inner edge of the arms and swallowed the top of
        // each sleeve, and daylight came through the notch left between the
        // ledge, the waist and the arm. What reads as a torso is not a wider
        // chest — it is the arms standing clear of a straight one.
        let chest_width = hip_width;
        let chest_half_width = chest_width / 2;
        // Like the waist, the gap between the legs is a construction rule
        // rather than a rejection: a thick limb on a narrow hip would close it
        // at some heights, and the silhouette rule says the legs must read.
        //
        // Two voxels of inset rather than one, so the gap is four voxels wide.
        // The first captures showed a two-voxel gap disappearing at any
        // distance where the whole body fits the frame, which is exactly the
        // distance the silhouette rule is about. A leg may end up wider than
        // the pelvis, which reads as a stance rather than as an error.
        let leg_inner = (torso_half_width - limb_thickness).max(2);
        // The shoulder span is what places the arms, so the control the style
        // contract measures is the control the geometry reads.
        let arm_outer = shoulder_span / 2;
        let arm_inner = arm_outer - limb_thickness;

        if arm_outer <= chest_half_width {
            return Err(CharacterDescriptorError::ArmsDoNotSeparate {
                arm_outer,
                chest_outer: chest_half_width,
            });
        }
        if f64::from(head_width) > f64::from(shoulder_span) * 0.60 {
            return Err(CharacterDescriptorError::HeadDoesNotRead {
                head_width,
                shoulder_span,
            });
        }
        // The arm must also reach back inside the chest, or the shoulder joint
        // has nothing to hide it.
        if arm_inner >= chest_half_width {
            return Err(CharacterDescriptorError::JointWouldOpen { joint: "shoulder" });
        }
        if fingertip_y >= hip_y || fingertip_y < knee_y {
            return Err(CharacterDescriptorError::ReachIsImplausible {
                fingertip_y,
                knee_y,
                hip_y,
            });
        }

        // The stack has to leave room for a torso: every joint must sit
        // strictly above the one below it, or the boxes invert.
        for (joint, lower, upper) in [
            ("spine", hip_y, spine_y),
            ("chest", spine_y, chest_y),
            ("shoulder", chest_y, shoulder_y),
        ] {
            if upper <= lower {
                return Err(CharacterDescriptorError::JointWouldOpen { joint });
            }
        }

        Ok(Self {
            height,
            head_height,
            head_width,
            head_depth,
            leg_length,
            thigh_length,
            foot_height,
            foot_length,
            arm_length,
            upper_arm_length,
            forearm_length,
            hand_length,
            hand_depth,
            limb_thickness,
            shoulder_span,
            hip_width,
            waist_width,
            torso_depth,
            waist_depth,
            ankle_y,
            knee_y,
            hip_y,
            spine_y,
            chest_y,
            shoulder_y,
            elbow_y,
            wrist_y,
            neck_y,
            head_bottom,
            torso_half_width,
            chest_width,
            chest_half_width,
            leg_inner,
            arm_inner,
        })
    }

    /// Outer edge of an arm, on the positive side.
    #[must_use]
    pub const fn arm_outer(&self) -> i32 {
        self.arm_inner + self.limb_thickness
    }

    /// Lowest voxel row of a hand.
    #[must_use]
    pub const fn fingertip_y(&self) -> i32 {
        self.wrist_y - self.hand_length
    }

    /// Body height in world units, as the renderer sees it.
    #[must_use]
    pub fn height_units(&self) -> f32 {
        self.height as f32 * CHARACTER_VOXEL_SIZE
    }
}

/// A descriptor whose invariants have been proved.
///
/// The only input the compiler accepts. It carries the derived integer body so
/// no later stage repeats the rounding, which is what keeps the skeleton, the
/// geometry, and the collision representation talking about the same character.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ValidatedDescriptor {
    descriptor: CharacterDescriptor,
    proportions: Proportions,
    body: BodyMetrics,
}

impl ValidatedDescriptor {
    #[must_use]
    pub const fn descriptor(&self) -> &CharacterDescriptor {
        &self.descriptor
    }

    /// Proportions after the seed's bounded variation.
    #[must_use]
    pub const fn proportions(&self) -> &Proportions {
        &self.proportions
    }

    #[must_use]
    pub const fn body(&self) -> &BodyMetrics {
        &self.body
    }

    #[must_use]
    pub const fn palette(&self) -> PaletteChoice {
        self.descriptor.palette
    }

    #[must_use]
    pub const fn seed(&self) -> CharacterSeed {
        self.descriptor.seed
    }
}

/// Which character this is, and under which rules.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CharacterIdentity {
    pub descriptor_fingerprint: u64,
    pub compiler_version: u32,
    pub style_version: u32,
}

impl CharacterIdentity {
    #[must_use]
    pub fn of(descriptor: &CharacterDescriptor) -> Self {
        Self {
            descriptor_fingerprint: fnv1a64(&descriptor.canonical_bytes()),
            compiler_version: CHARACTER_COMPILER_VERSION,
            style_version: CHARACTER_STYLE_VERSION,
        }
    }

    /// One cheap value covering the descriptor and both contract versions.
    ///
    /// This is a character identity key. It is deliberately not connected to
    /// the streaming chunk cache: a character is not a chunk and must never
    /// invalidate one.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut bytes = Vec::with_capacity(16);
        push_u64(&mut bytes, self.descriptor_fingerprint);
        push_u32(&mut bytes, self.compiler_version);
        push_u32(&mut bytes, self.style_version);
        fnv1a64(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Archetype, CHARACTER_SCHEMA_VERSION, CharacterDescriptor, CharacterDescriptorError,
        CharacterIdentity, CharacterSeed, MAX_BUILD_VARIATION, MAX_HEIGHT_VOXELS,
        MIN_FEATURE_VOXELS, MIN_HEIGHT_VOXELS, Proportions,
    };

    fn golden() -> CharacterDescriptor {
        CharacterDescriptor::golden()
    }

    #[test]
    fn the_golden_descriptor_derives_the_reference_body() -> Result<(), CharacterDescriptorError> {
        let body = *golden().validate()?.body();
        assert_eq!(body.height, 28, "the reference body is twenty-eight voxels");
        assert_eq!(body.head_height, 5);
        assert_eq!(body.head_width, 6);
        assert_eq!(body.leg_length, 14);
        assert_eq!(body.hip_y, 14);
        assert_eq!(body.knee_y, 9);
        assert_eq!(body.ankle_y, 3);
        assert_eq!(body.shoulder_y, 22);
        assert_eq!(body.elbow_y, 17);
        assert_eq!(body.wrist_y, 13);
        assert_eq!(body.fingertip_y(), 10);
        assert_eq!(body.spine_y, 17);
        assert_eq!(body.chest_y, 19);
        assert_eq!(body.head_bottom, 23);
        assert_eq!(body.hip_width, 8);
        assert_eq!(body.waist_width, 6);
        assert_eq!(body.shoulder_span, 12);
        assert_eq!(body.limb_thickness, 3);
        assert_eq!(body.foot_length, 5);
        assert_eq!(body.torso_depth, 6);
        assert_eq!(body.leg_inner, 2);
        assert_eq!(body.arm_inner, 3);
        assert_eq!(body.arm_outer(), 6);
        assert_eq!(body.chest_width, 8);
        // One voxel of arm inside the chest closes the shoulder; two voxels
        // outside it are the whole silhouette of an arm at rest.
        assert_eq!(body.chest_half_width - body.arm_inner, 1);
        assert_eq!(body.arm_outer() - body.chest_half_width, 2);
        Ok(())
    }

    #[test]
    fn the_sturdy_descriptor_derives_a_different_body() -> Result<(), CharacterDescriptorError> {
        let sturdy = CharacterDescriptor {
            proportions: Proportions::sturdy(),
            ..golden()
        };
        let golden_body = *golden().validate()?.body();
        let sturdy_body = *sturdy.validate()?.body();
        assert_ne!(golden_body, sturdy_body);
        assert!(
            sturdy_body.height < golden_body.height,
            "the sturdy body is shorter"
        );
        // Relatively broader: more hip per unit of height, and a shorter leg.
        assert!(
            sturdy_body.hip_width * golden_body.height > golden_body.hip_width * sturdy_body.height,
            "the sturdy body is not relatively broader"
        );
        assert!(
            sturdy_body.leg_length * golden_body.height
                < golden_body.leg_length * sturdy_body.height,
            "the sturdy body is not relatively shorter legged"
        );
        Ok(())
    }

    #[test]
    fn an_unsupported_schema_is_rejected() {
        let descriptor = CharacterDescriptor {
            schema_version: CHARACTER_SCHEMA_VERSION + 1,
            ..golden()
        };
        assert_eq!(
            descriptor.validate().err(),
            Some(CharacterDescriptorError::UnsupportedSchema {
                found: CHARACTER_SCHEMA_VERSION + 1,
                supported: CHARACTER_SCHEMA_VERSION,
            })
        );
    }

    #[test]
    fn non_finite_and_non_positive_controls_are_rejected() {
        for hostile in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let descriptor = CharacterDescriptor {
                proportions: Proportions {
                    leg_length_fraction: hostile,
                    ..Proportions::golden()
                },
                ..golden()
            };
            assert!(
                matches!(
                    descriptor.validate(),
                    Err(CharacterDescriptorError::NonFinite { .. })
                ),
                "{hostile} was accepted"
            );
        }
        for hostile in [0.0, -0.5] {
            let descriptor = CharacterDescriptor {
                proportions: Proportions {
                    limb_thickness_fraction: hostile,
                    ..Proportions::golden()
                },
                ..golden()
            };
            assert!(
                matches!(
                    descriptor.validate(),
                    Err(CharacterDescriptorError::NonPositive { .. })
                ),
                "{hostile} was accepted"
            );
        }
    }

    #[test]
    fn hostile_build_variation_is_rejected() {
        for hostile in [f64::NAN, -0.01, MAX_BUILD_VARIATION + 0.01] {
            let descriptor = CharacterDescriptor {
                build_variation: hostile,
                ..golden()
            };
            assert!(descriptor.validate().is_err(), "{hostile} was accepted");
        }
        let ok = CharacterDescriptor {
            build_variation: MAX_BUILD_VARIATION,
            ..golden()
        };
        assert!(ok.validate().is_ok());
    }

    #[test]
    fn a_body_outside_the_height_range_is_rejected() {
        for units in [0.9_f64, 5.0] {
            let descriptor = CharacterDescriptor {
                proportions: Proportions {
                    total_height_units: units,
                    ..Proportions::golden()
                },
                ..golden()
            };
            assert!(
                matches!(
                    descriptor.validate(),
                    Err(CharacterDescriptorError::HeightOutOfRange { .. })
                ),
                "{units} world units was accepted"
            );
        }
        const { assert!(MIN_HEIGHT_VOXELS < MAX_HEIGHT_VOXELS) };
    }

    #[test]
    fn a_proportion_outside_its_style_band_is_rejected() {
        let descriptor = CharacterDescriptor {
            proportions: Proportions {
                head_height_fraction: 0.33,
                ..Proportions::golden()
            },
            ..golden()
        };
        assert!(matches!(
            descriptor.validate(),
            Err(CharacterDescriptorError::OutOfBand { .. })
        ));
    }

    #[test]
    fn a_limb_that_rounds_too_thin_is_rejected() {
        // A tall body with the thinnest allowed limb fraction still rounds to
        // a usable limb; a short one does not, and that is the case rounding
        // hides from a purely fractional check.
        let descriptor = CharacterDescriptor {
            proportions: Proportions {
                total_height_units: 1.75,
                limb_thickness_fraction: 0.070,
                ..Proportions::golden()
            },
            ..golden()
        };
        assert!(
            matches!(
                descriptor.validate(),
                Err(CharacterDescriptorError::FeatureTooThin { .. })
            ),
            "a one-voxel limb was accepted: {:?}",
            descriptor.validate().map(|v| *v.body())
        );
        assert_eq!(MIN_FEATURE_VOXELS, 2);
    }

    /// The waist and the leg gap are theorems rather than hopes: whatever the
    /// descriptor asks for, a compiled body's waist is narrower than its hips
    /// and its legs are separated. Sweeping heights matters because rounding
    /// two even widths is where a plausible ratio collapses.
    #[test]
    fn the_waist_and_the_leg_gap_read_for_every_body_that_validates() {
        let mut checked = 0;
        for height_units in [1.80_f64, 2.00, 2.15, 2.33, 2.55, 2.80, 3.20] {
            for waist in [0.150_f64, 0.2143, 0.2800, 0.3400] {
                for limb in [0.070_f64, 0.1071, 0.1400, 0.1600] {
                    let descriptor = CharacterDescriptor {
                        proportions: Proportions {
                            total_height_units: height_units,
                            waist_width_fraction: waist,
                            limb_thickness_fraction: limb,
                            ..Proportions::golden()
                        },
                        ..golden()
                    };
                    let Ok(validated) = descriptor.validate() else {
                        continue;
                    };
                    let body = validated.body();
                    assert!(
                        body.waist_width < body.hip_width,
                        "waist {} does not read against hips {} at {height_units}",
                        body.waist_width,
                        body.hip_width
                    );
                    assert!(
                        2 * body.leg_inner >= 2,
                        "the legs touch at {height_units} with limb {limb}"
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 40, "only {checked} bodies validated");
    }

    #[test]
    fn arms_that_hide_inside_the_chest_are_rejected() {
        // Shoulders narrower than the hips put the arms inside the chest box,
        // which leaves the silhouette with no shoulders at all.
        let descriptor = CharacterDescriptor {
            proportions: Proportions {
                shoulder_span_fraction: 0.2857,
                hip_width_fraction: 0.2857,
                ..Proportions::golden()
            },
            ..golden()
        };
        assert!(
            matches!(
                descriptor.validate(),
                Err(CharacterDescriptorError::ArmsDoNotSeparate { .. })
            ),
            "unexpected: {:?}",
            descriptor.validate()
        );
    }

    #[test]
    fn a_shoulder_that_cannot_hide_its_joint_is_rejected() {
        // Very wide shoulders put the arm entirely outside the chest, so the
        // shoulder joint has nothing covering it and flexing opens a hole.
        let descriptor = CharacterDescriptor {
            proportions: Proportions {
                shoulder_span_fraction: 0.5800,
                hip_width_fraction: 0.2857,
                ..Proportions::golden()
            },
            ..golden()
        };
        assert!(
            matches!(
                descriptor.validate(),
                Err(CharacterDescriptorError::JointWouldOpen { joint: "shoulder" })
            ),
            "unexpected: {:?}",
            descriptor.validate()
        );
    }

    #[test]
    fn implausible_reach_is_rejected() {
        // Long legs and long arms are each inside their own band, and together
        // they put the fingertips below the knee. This is the case a
        // per-field check cannot see.
        let descriptor = CharacterDescriptor {
            proportions: Proportions {
                leg_length_fraction: 0.5600,
                arm_length_fraction: 0.4600,
                ..Proportions::golden()
            },
            ..golden()
        };
        assert!(
            matches!(
                descriptor.validate(),
                Err(CharacterDescriptorError::ReachIsImplausible { .. })
            ),
            "unexpected: {:?}",
            descriptor.validate()
        );
    }

    #[test]
    fn identity_tracks_every_descriptor_field() {
        let base = CharacterIdentity::of(&golden()).fingerprint();
        let mut moved = golden();
        moved.seed = CharacterSeed(1);
        assert_ne!(base, CharacterIdentity::of(&moved).fingerprint());
        let mut taller = golden();
        taller.proportions.total_height_units += 0.01;
        assert_ne!(base, CharacterIdentity::of(&taller).fingerprint());
        let mut repainted = golden();
        repainted.palette.hair = crate::material::HairTone::Flaxen;
        assert_ne!(base, CharacterIdentity::of(&repainted).fingerprint());
        assert_eq!(base, CharacterIdentity::of(&golden()).fingerprint());
        assert_eq!(golden().archetype, Archetype::Humanoid);
    }

    #[test]
    fn variation_is_bounded_deterministic_and_seed_addressed() {
        let mut varied = golden();
        varied.build_variation = 0.06;
        let first = varied.validate().map(|v| *v.body());
        let second = varied.validate().map(|v| *v.body());
        assert_eq!(first, second, "variation is not reproducible");

        let mut different_seed = varied;
        different_seed.seed = CharacterSeed(0x1234_5678_9abc_def0);
        let other = different_seed.validate().map(|v| *v.body());
        assert!(other.is_ok(), "a varied body failed validation: {other:?}");

        // Bounded: no seed may push the height outside the declared range.
        for raw in 0..64_u64 {
            let mut probe = varied;
            probe.seed = CharacterSeed(raw.wrapping_mul(0x9e37_79b9_7f4a_7c15));
            let body = probe.validate();
            assert!(body.is_ok(), "seed {raw} produced {body:?}");
        }
    }

    #[test]
    fn zero_variation_leaves_the_authored_proportions_untouched()
    -> Result<(), CharacterDescriptorError> {
        let validated = golden().validate()?;
        assert_eq!(validated.proportions(), &Proportions::golden());
        Ok(())
    }
}
