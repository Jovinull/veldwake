//! Deterministic spatial noise.
//!
//! Every function here is a pure function of a seed and a world position. No
//! process-global randomness, no sequential generator state, and no dependence
//! on the order in which chunks are generated: asking for the same position
//! twice always gives the same answer, and asking for a position never changes
//! what a neighbouring position will answer.
//!
//! Determinism here is the contract in `DETERMINISM.md`: identical results for
//! the same seed, generator version, and platform. Bitwise equality across CPU
//! vendors is explicitly not promised, which is why golden fixtures lock
//! observable structure rather than raw float bits.

/// 64-bit mix used for every spatial hash. This is the finaliser of
/// SplitMix64: cheap, well distributed, and dependency free.
const fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Hashes a two-dimensional lattice point under a named stream.
///
/// `stream` separates independent uses so that adding a draw to one system
/// cannot perturb another, which is the child-stream rule in `DETERMINISM.md`.
/// Coordinates are folded as two's-complement bit patterns, so negative
/// positions are ordinary inputs rather than a special case.
#[must_use]
pub fn hash_2d(seed: u64, stream: u64, x: i64, z: i64) -> u64 {
    let mut value = seed ^ mix64(stream);
    value = mix64(value ^ x.cast_unsigned());
    mix64(value ^ z.cast_unsigned().rotate_left(32))
}

/// Hashes a three-dimensional lattice point under a named stream.
#[must_use]
pub fn hash_3d(seed: u64, stream: u64, x: i64, y: i64, z: i64) -> u64 {
    let mut value = seed ^ mix64(stream);
    value = mix64(value ^ x.cast_unsigned());
    value = mix64(value ^ y.cast_unsigned().rotate_left(21));
    mix64(value ^ z.cast_unsigned().rotate_left(42))
}

/// A hash mapped to `[0, 1)`.
#[must_use]
pub fn unit_from_hash(hash: u64) -> f64 {
    // 53 bits is the mantissa width of f64, so this is exact and uniform.
    ((hash >> 11) as f64) * (1.0 / 9_007_199_254_740_992.0)
}

/// A hash mapped to `[-1, 1)`.
#[must_use]
pub fn signed_from_hash(hash: u64) -> f64 {
    unit_from_hash(hash).mul_add(2.0, -1.0)
}

/// Derives an independent value from an existing hash.
///
/// A placement decision usually needs several numbers from one lattice cell:
/// a jitter, an acceptance draw, a height, a shape variant. Re-hashing the
/// cell under a new stream for each would be wasteful, and consuming a
/// sequential generator would make the result depend on how many draws the
/// code happens to take. Indexing keeps every value addressable by name, so
/// adding a draw later cannot move an existing one.
#[must_use]
pub fn sub_hash(hash: u64, index: u64) -> u64 {
    mix64(hash ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15))
}

/// Quintic smoothstep, the interpolant used by every noise field here. Its
/// first and second derivatives vanish at the ends, so lattice cells join
/// without the visible creases a cubic leaves.
fn smooth(t: f64) -> f64 {
    t * t * t * t.mul_add(t.mul_add(6.0, -15.0), 10.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    (b - a).mul_add(t, a)
}

/// Value noise on the integer lattice, in `[-1, 1]`.
#[must_use]
pub fn value_2d(seed: u64, stream: u64, x: f64, z: f64) -> f64 {
    let x0 = x.floor();
    let z0 = z.floor();
    let tx = smooth(x - x0);
    let tz = smooth(z - z0);
    // `floor` of a finite f64 always fits i64 for the ranges this generator
    // uses; a non-finite input would be a caller bug and is clamped rather
    // than silently wrapping.
    let xi = clamp_to_i64(x0);
    let zi = clamp_to_i64(z0);

    // Saturating, so a clamped non-finite input cannot overflow the lattice
    // step. Inside the coordinate range this generator actually uses, the
    // saturation never triggers.
    let x1 = xi.saturating_add(1);
    let z1 = zi.saturating_add(1);

    let c00 = signed_from_hash(hash_2d(seed, stream, xi, zi));
    let c10 = signed_from_hash(hash_2d(seed, stream, x1, zi));
    let c01 = signed_from_hash(hash_2d(seed, stream, xi, z1));
    let c11 = signed_from_hash(hash_2d(seed, stream, x1, z1));

    lerp(lerp(c00, c10, tx), lerp(c01, c11, tx), tz)
}

fn clamp_to_i64(value: f64) -> i64 {
    if value >= i64::MAX as f64 {
        i64::MAX
    } else if value <= i64::MIN as f64 {
        i64::MIN
    } else if value.is_nan() {
        0
    } else {
        value as i64
    }
}

/// Parameters of a fractal sum. Named rather than positional because these
/// are art controls, not incidental constants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fbm {
    /// Size of the largest feature, in world voxels.
    pub feature_size: f64,
    pub octaves: u32,
    /// Frequency multiplier between octaves.
    pub lacunarity: f64,
    /// Amplitude multiplier between octaves.
    pub gain: f64,
}

impl Fbm {
    /// Fractal sum of value noise, normalised to roughly `[-1, 1]`.
    #[must_use]
    pub fn sample(&self, seed: u64, stream: u64, x: f64, z: f64) -> f64 {
        let mut frequency = 1.0 / self.feature_size.max(f64::EPSILON);
        let mut amplitude = 1.0;
        let mut total = 0.0;
        let mut normaliser = 0.0;
        for octave in 0..self.octaves {
            total += amplitude
                * value_2d(
                    seed,
                    stream ^ u64::from(octave).wrapping_mul(0x9e37_79b9_7f4a_7c15),
                    x * frequency,
                    z * frequency,
                );
            normaliser += amplitude;
            frequency *= self.lacunarity;
            amplitude *= self.gain;
        }
        if normaliser <= f64::EPSILON {
            0.0
        } else {
            total / normaliser
        }
    }

    /// Ridged variant: folds the field about zero and inverts it, which turns
    /// smooth hills into creased ridges with a steep and a gentle face.
    #[must_use]
    pub fn sample_ridged(&self, seed: u64, stream: u64, x: f64, z: f64) -> f64 {
        let mut frequency = 1.0 / self.feature_size.max(f64::EPSILON);
        let mut amplitude = 1.0;
        let mut total = 0.0;
        let mut normaliser = 0.0;
        for octave in 0..self.octaves {
            let sample = value_2d(
                seed,
                stream ^ u64::from(octave).wrapping_mul(0xc2b2_ae3d_27d4_eb4f),
                x * frequency,
                z * frequency,
            );
            // 1 - |n| peaks where the underlying field crosses zero, which is
            // what produces a crest line rather than a dome.
            total += amplitude * (1.0 - sample.abs());
            normaliser += amplitude;
            frequency *= self.lacunarity;
            amplitude *= self.gain;
        }
        if normaliser <= f64::EPSILON {
            0.0
        } else {
            (total / normaliser).mul_add(2.0, -1.0)
        }
    }
}

/// Offsets a sample position by a low-frequency field, which breaks the axis
/// alignment that makes plain fractal noise look like a grid.
#[must_use]
pub fn domain_warp(
    seed: u64,
    stream: u64,
    x: f64,
    z: f64,
    field: Fbm,
    strength: f64,
) -> (f64, f64) {
    let warp_x = field.sample(seed, stream, x, z);
    let warp_z = field.sample(seed, stream ^ 0x5bf0_3635_ca62_1c4d, x + 133.7, z - 71.3);
    (warp_x.mul_add(strength, x), warp_z.mul_add(strength, z))
}

/// Smoothstep between two edges, clamped. The shaping primitive used to turn
/// a distance or a height into a mask.
#[must_use]
pub fn smoothstep(edge0: f64, edge1: f64, value: f64) -> f64 {
    if (edge1 - edge0).abs() <= f64::EPSILON {
        return if value < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    smooth(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashing_is_deterministic_and_separates_streams() {
        assert_eq!(hash_2d(1, 0, 5, -7), hash_2d(1, 0, 5, -7));
        assert_ne!(hash_2d(1, 0, 5, -7), hash_2d(2, 0, 5, -7));
        assert_ne!(hash_2d(1, 0, 5, -7), hash_2d(1, 1, 5, -7));
        assert_ne!(hash_2d(1, 0, 5, -7), hash_2d(1, 0, -7, 5), "axes differ");
        assert_ne!(hash_3d(1, 0, 1, 2, 3), hash_3d(1, 0, 3, 2, 1));
    }

    #[test]
    fn unit_and_signed_conversions_stay_in_range() {
        for raw in [0, 1, u64::MAX, 0x8000_0000_0000_0000, 0x1234_5678_9abc_def0] {
            let unit = unit_from_hash(raw);
            assert!((0.0..1.0).contains(&unit), "{unit} out of range");
            let signed = signed_from_hash(raw);
            assert!((-1.0..1.0).contains(&signed), "{signed} out of range");
        }
    }

    #[test]
    fn value_noise_is_continuous_across_lattice_and_sign_boundaries() {
        let seed = 0xfeed;
        for base in [-64.0, -1.0, 0.0, 1.0, 64.0] {
            let step = 1e-6;
            let left = value_2d(seed, 0, base - step, 3.25);
            let right = value_2d(seed, 0, base + step, 3.25);
            assert!(
                (left - right).abs() < 1e-3,
                "discontinuity at x={base}: {left} vs {right}"
            );
        }
    }

    #[test]
    fn value_noise_is_bounded_and_varies() {
        let mut min = f64::MAX;
        let mut max = f64::MIN;
        for i in -200..200 {
            let value = value_2d(7, 0, f64::from(i) * 0.37, f64::from(i) * -0.21);
            assert!((-1.0..=1.0).contains(&value));
            min = min.min(value);
            max = max.max(value);
        }
        assert!(max - min > 0.5, "field is too flat to be useful");
    }

    #[test]
    fn fractal_sums_stay_bounded_in_both_variants() {
        let field = Fbm {
            feature_size: 40.0,
            octaves: 5,
            lacunarity: 2.0,
            gain: 0.5,
        };
        for i in -150..150 {
            let x = f64::from(i) * 1.7;
            let z = f64::from(i) * -0.9;
            let smooth_sample = field.sample(11, 1, x, z);
            let ridged_sample = field.sample_ridged(11, 2, x, z);
            assert!((-1.05..=1.05).contains(&smooth_sample), "{smooth_sample}");
            assert!((-1.05..=1.05).contains(&ridged_sample), "{ridged_sample}");
        }
    }

    #[test]
    fn zero_octaves_and_degenerate_feature_sizes_do_not_divide_by_zero() {
        let degenerate = Fbm {
            feature_size: 0.0,
            octaves: 0,
            lacunarity: 2.0,
            gain: 0.5,
        };
        assert_eq!(degenerate.sample(1, 0, 3.0, 4.0), 0.0);
        assert_eq!(degenerate.sample_ridged(1, 0, 3.0, 4.0), 0.0);
    }

    #[test]
    fn smoothstep_clamps_and_handles_a_degenerate_edge() {
        assert_eq!(smoothstep(0.0, 1.0, -5.0), 0.0);
        assert_eq!(smoothstep(0.0, 1.0, 5.0), 1.0);
        assert!((smoothstep(0.0, 1.0, 0.5) - 0.5).abs() < 1e-12);
        assert_eq!(smoothstep(2.0, 2.0, 1.0), 0.0);
        assert_eq!(smoothstep(2.0, 2.0, 3.0), 1.0);
    }

    #[test]
    fn domain_warp_moves_the_sample_without_exploding_it() {
        let field = Fbm {
            feature_size: 90.0,
            octaves: 3,
            lacunarity: 2.0,
            gain: 0.5,
        };
        let (wx, wz) = domain_warp(3, 0, 10.0, -20.0, field, 12.0);
        assert!((wx - 10.0).abs() <= 12.0 + 1e-9);
        assert!((wz + 20.0).abs() <= 12.0 + 1e-9);
        assert_eq!(domain_warp(3, 0, 10.0, -20.0, field, 12.0), (wx, wz));
    }

    #[test]
    fn sub_hashes_are_independent_and_addressable_by_index() {
        let base = hash_2d(7, 3, -11, 42);
        let mut seen = std::collections::BTreeSet::new();
        for index in 0..8 {
            let value = sub_hash(base, index);
            assert_eq!(value, sub_hash(base, index), "must be stable");
            assert!(seen.insert(value), "index {index} collided");
        }
        assert_ne!(sub_hash(base, 0), sub_hash(hash_2d(7, 3, -11, 43), 0));
    }

    #[test]
    fn non_finite_input_does_not_panic() {
        // A caller bug, but external-facing robustness matters more than a
        // crash: the clamp keeps the lattice index defined.
        let _ = value_2d(1, 0, f64::NAN, 0.0);
        let _ = value_2d(1, 0, f64::INFINITY, 0.0);
        let _ = value_2d(1, 0, f64::NEG_INFINITY, 0.0);
    }
}
