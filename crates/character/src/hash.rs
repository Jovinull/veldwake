//! The deterministic hashing this crate owns.
//!
//! Small on purpose. A character needs exactly three things from a hash: a
//! stable identity fingerprint over a descriptor, a named child stream so one
//! variation cannot perturb another, and an addressable draw inside a stream.
//!
//! This is duplicated rather than borrowed from `veldwake-procedural`. Taking a
//! dependency on the world generator so a character can hash three numbers
//! would invert nothing, isolate nothing, and make a headless character crate
//! compile terrain. The functions are the published FNV-1a and SplitMix64
//! finalizer, so "the same function" is a testable claim rather than a promise.
//!
//! None of this is cryptographic and none of it may ever be used as if it were.

/// FNV-1a 64, byte oriented and therefore identical on every platform.
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The SplitMix64 finalizer: cheap, well distributed, dependency free.
#[must_use]
pub const fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Derives an independent value from an existing hash.
///
/// A compilation step usually needs several numbers from one decision point.
/// Indexing keeps every value addressable by name, so adding a draw later
/// cannot move an existing one, and nothing here consumes sequential state.
#[must_use]
pub const fn sub_hash(hash: u64, index: u64) -> u64 {
    mix64(hash ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15))
}

/// A hash mapped to `[0, 1)`.
///
/// Fifty-three bits is the mantissa width of `f64`, so the mapping is exact.
#[must_use]
pub fn unit_from_hash(hash: u64) -> f64 {
    ((hash >> 11) as f64) * (1.0 / 9_007_199_254_740_992.0)
}

/// A hash mapped to `[-1, 1)`.
#[must_use]
pub fn signed_from_hash(hash: u64) -> f64 {
    unit_from_hash(hash).mul_add(2.0, -1.0)
}

/// Folds one `f64` into a byte stream by its bit pattern.
///
/// Exact, and free of the formatting round trip a decimal rendering would
/// need. The order in which a descriptor pushes its fields is what defines a
/// fingerprint, so a caller must never reshuffle one casually.
pub fn push_f64(bytes: &mut Vec<u8>, value: f64) {
    bytes.extend_from_slice(&value.to_bits().to_le_bytes());
}

/// Folds one `f32` into a byte stream by its bit pattern.
pub fn push_f32(bytes: &mut Vec<u8>, value: f32) {
    bytes.extend_from_slice(&value.to_bits().to_le_bytes());
}

/// Folds one `u64` into a byte stream.
pub fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

/// Folds one `u32` into a byte stream.
pub fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

/// Folds one `i32` into a byte stream.
pub fn push_i32(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::{fnv1a64, mix64, signed_from_hash, sub_hash, unit_from_hash};

    #[test]
    fn the_hash_is_the_published_fnv1a_64_vector() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
        assert_ne!(fnv1a64(b"ab"), fnv1a64(b"ba"), "order must matter");
    }

    #[test]
    fn the_mixer_matches_the_world_generator_byte_for_byte() {
        // `veldwake-procedural` uses the same SplitMix64 finalizer. Duplicating
        // the constant is only acceptable while "the same function" stays a
        // checkable statement, so check it.
        assert_eq!(mix64(0), 0);
        assert_eq!(mix64(1), 0x5692_161d_100b_05e5);
        assert_eq!(mix64(2), 0xdbd2_3897_3a2b_148a);
    }

    #[test]
    fn sub_hashes_are_addressable_and_independent() {
        let base = fnv1a64(b"character");
        let draws: Vec<u64> = (0..8).map(|index| sub_hash(base, index)).collect();
        for (index, value) in draws.iter().enumerate() {
            assert_eq!(*value, sub_hash(base, index as u64), "not reproducible");
            for (other_index, other) in draws.iter().enumerate() {
                if index != other_index {
                    assert_ne!(value, other, "draw {index} collides with {other_index}");
                }
            }
        }
    }

    #[test]
    fn unit_and_signed_mappings_stay_inside_their_ranges() {
        for seed in 0..512_u64 {
            let hash = mix64(seed);
            let unit = unit_from_hash(hash);
            assert!((0.0..1.0).contains(&unit), "unit {unit} out of range");
            let signed = signed_from_hash(hash);
            assert!(
                (-1.0..1.0).contains(&signed),
                "signed {signed} out of range"
            );
        }
        assert_eq!(unit_from_hash(0), 0.0);
        assert_eq!(signed_from_hash(0), -1.0);
    }
}
