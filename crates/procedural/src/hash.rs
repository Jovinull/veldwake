//! One deterministic non-cryptographic hash, shared by every identity and
//! spatial decision in this crate.
//!
//! Deliberately the same function the streaming cache already uses, byte for
//! byte, so that a fingerprint computed here and a fingerprint stored there
//! cannot drift apart through a subtle difference in mixing. It is duplicated
//! rather than shared because the dependency direction is
//! `voxel <- procedural <- streaming`: this crate must not depend on the
//! consumer that stores its output.

/// FNV-1a 64.
///
/// Chosen for identity derivation only: small, dependency free, and identical
/// on every platform because it consumes bytes rather than native integers. It
/// is not a cryptographic or anti-tamper measure and must never be used as one.
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::fnv1a64;

    #[test]
    fn the_hash_is_the_published_fnv1a_64_vector() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
        assert_ne!(fnv1a64(b"ab"), fnv1a64(b"ba"), "order must matter");
    }
}
