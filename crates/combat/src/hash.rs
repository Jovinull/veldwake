//! The deterministic hashing this crate owns.
//!
//! The third copy of these thirty lines, and the reason is the same one
//! `DETERMINISM.md` already records for the second. `veldwake-character` keeps
//! its own FNV-1a and SplitMix64 rather than depending on the world generator
//! for them, and both crates keep them **private**. Reaching for either would
//! mean making a crate's internals public to serve a consumer, which buys code
//! reuse at the price of a boundary.
//!
//! What keeps "the same function" a checkable claim rather than a comment is
//! the test below: the published FNV-1a vectors and two SplitMix64 outputs are
//! pinned here exactly as they are pinned in the other two crates.
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
/// Indexing keeps every draw addressable by name, so adding one later cannot
/// move an existing one and nothing here consumes sequential state.
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

/// Folds one `f32` into a byte stream by its bit pattern.
///
/// The order in which a caller pushes its fields is what defines a fingerprint,
/// so nothing may reshuffle one casually.
pub fn push_f32(bytes: &mut Vec<u8>, value: f32) {
    bytes.extend_from_slice(&value.to_bits().to_le_bytes());
}

/// Folds one `u32` into a byte stream.
pub fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

/// Folds one `i32` into a byte stream.
pub fn push_i32(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

/// Folds one `u64` into a byte stream.
pub fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

/// Folds one `u16` into a byte stream.
pub fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::{
        fnv1a64, mix64, push_f32, push_i32, push_u16, push_u32, push_u64, sub_hash, unit_from_hash,
    };

    #[test]
    fn fnv1a_matches_the_published_vectors() {
        // The same three vectors the other two crates pin, which is what makes
        // "the same function" checkable across the duplication.
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
        assert_ne!(fnv1a64(b"ab"), fnv1a64(b"ba"), "order must matter");
    }

    #[test]
    fn the_mixer_matches_the_other_two_crates_byte_for_byte() {
        // Both `veldwake-procedural` and `veldwake-character` pin these exact
        // outputs. Duplicating the constant is only acceptable while "the same
        // function" stays a checkable statement, so check it.
        assert_eq!(mix64(0), 0);
        assert_eq!(mix64(1), 0x5692_161d_100b_05e5);
        assert_eq!(mix64(2), 0xdbd2_3897_3a2b_148a);
    }

    #[test]
    fn sub_hashes_are_addressable_and_distinct() {
        let base = fnv1a64(b"veldwake-combat");
        let draws: Vec<u64> = (0..8).map(|index| sub_hash(base, index)).collect();
        for (index, value) in draws.iter().enumerate() {
            for (other, compare) in draws.iter().enumerate() {
                if index != other {
                    assert_ne!(value, compare, "draw {index} collided with {other}");
                }
            }
        }
        // Adding a draw cannot move an existing one.
        assert_eq!(draws[3], sub_hash(base, 3));
    }

    #[test]
    fn the_unit_mapping_stays_inside_its_range() {
        for seed in 0..512_u64 {
            let value = unit_from_hash(mix64(seed));
            assert!((0.0..1.0).contains(&value), "{value} left [0, 1)");
        }
    }

    #[test]
    fn every_push_writes_little_endian_bytes() {
        let mut bytes = Vec::new();
        push_u16(&mut bytes, 0x0102);
        push_u32(&mut bytes, 0x0304_0506);
        push_i32(&mut bytes, -1);
        push_u64(&mut bytes, 0x0708_090a_0b0c_0d0e);
        push_f32(&mut bytes, 1.0);
        assert_eq!(&bytes[0..2], &[0x02, 0x01]);
        assert_eq!(&bytes[2..6], &[0x06, 0x05, 0x04, 0x03]);
        assert_eq!(&bytes[6..10], &[0xff, 0xff, 0xff, 0xff]);
        assert_eq!(
            &bytes[10..18],
            &[0x0e, 0x0d, 0x0c, 0x0b, 0x0a, 0x09, 0x08, 0x07]
        );
        assert_eq!(&bytes[18..22], &1.0_f32.to_bits().to_le_bytes());
    }
}
