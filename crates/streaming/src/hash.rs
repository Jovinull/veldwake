//! One deterministic non-cryptographic hash, shared so the cache format and
//! the source identity cannot drift apart.

/// FNV-1a 64.
///
/// Chosen for corruption detection and identity derivation only: small,
/// dependency free, and identical on every platform because it consumes bytes
/// rather than native integers. It is not a cryptographic or anti-tamper
/// measure and must never be used as one.
pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
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
        assert_eq!(fnv1a64(b"foobar"), 0x85944171f73967e8);
        assert_ne!(fnv1a64(b"ab"), fnv1a64(b"ba"), "order must matter");
    }
}
