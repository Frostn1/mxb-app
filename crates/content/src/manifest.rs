//! Pure beta21e content-checksum primitives.
//!
//! These functions accept bytes already supplied by a lawful caller. They do not open packages,
//! decrypt containers, or provide an extraction surface.

/// Reproduce beta21e's per-file checksum: two modulo-255 accumulators packed as `s2:s1`.
pub fn checksum_bytes(bytes: &[u8]) -> u16 {
    let mut s1 = 0u32;
    let mut s2 = 0u32;
    for byte in bytes {
        s1 = (s1 + u32::from(*byte)) % 255;
        s2 = (s2 + s1) % 255;
    }
    ((s2 << 8) | s1) as u16
}

/// Add named-file checksum words exactly as beta21e's aggregate builders do.
pub fn aggregate_checksums(words: impl IntoIterator<Item = u16>) -> u32 {
    words
        .into_iter()
        .fold(0u32, |sum, word| sum.wrapping_add(u32::from(word)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_matches_the_decoded_modulo_255_algorithm() {
        assert_eq!(checksum_bytes(b""), 0x0000);
        assert_eq!(checksum_bytes(&[1, 2, 3]), 0x0A06);
        assert_eq!(checksum_bytes(&[255, 1]), 0x0101);
    }

    #[test]
    fn aggregate_is_zero_extended_and_wrapping() {
        assert_eq!(aggregate_checksums([0x0102, 0x0304]), 0x0406);
        assert_eq!(u32::MAX.wrapping_add(1), 0);
    }
}
