//! Deterministic PRNG for corpus generation (SplitMix64).
//!
//! The corpus must be byte-identical on every machine (docs/05 §6: "fixed,
//! versioned, documented so numbers are reproducible"), so we implement the
//! generator here instead of taking a `rand` dependency whose stream is not
//! a stability contract across crate versions.

/// SplitMix64: fast, stable, seedable — the stream for a given seed never
/// changes, which is exactly the property the corpus needs.
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform value in `0..n` (`n` must be > 0).
    pub fn below(&mut self, n: u64) -> u64 {
        debug_assert!(n > 0);
        self.next_u64() % n
    }

    /// Fill a buffer with pseudo-random bytes (incompressible content).
    pub fn fill_bytes(&mut self, buf: &mut [u8]) {
        let mut chunks = buf.chunks_exact_mut(8);
        for chunk in &mut chunks {
            chunk.copy_from_slice(&self.next_u64().to_le_bytes());
        }
        let rem = chunks.into_remainder();
        if !rem.is_empty() {
            rem.copy_from_slice(&self.next_u64().to_le_bytes()[..rem.len()]);
        }
    }

    /// Derive an independent stream for a (set, file) pair: hash the seed
    /// with two tags so generation order doesn't couple the streams.
    pub fn derive(seed: u64, tag_a: u64, tag_b: u64) -> Self {
        let mut h = Self::new(seed);
        let mixed = h.next_u64() ^ tag_a.wrapping_mul(0xA24B_AED4_963E_E407);
        let mut h2 = Self::new(mixed);
        Self::new(h2.next_u64() ^ tag_b.wrapping_mul(0x9FB2_1C65_1E98_DF25))
    }
}

/// FNV-1a 64-bit, used for corpus manifest checksums (no hash dependency).
pub struct Fnv1a(u64);

impl Fnv1a {
    pub fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    pub fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

impl Default for Fnv1a {
    fn default() -> Self {
        Self::new()
    }
}

/// Median of a duration sample set (average of the two middle values for
/// even counts). Warmup runs are excluded by the callers.
pub fn median_ms(samples: &mut [u128]) -> u64 {
    assert!(!samples.is_empty());
    samples.sort_unstable();
    let n = samples.len();
    let median = if n % 2 == 1 {
        samples[n / 2]
    } else {
        (samples[n / 2 - 1] + samples[n / 2]) / 2
    };
    median as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = SplitMix64::new(1);
        let mut b = SplitMix64::new(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn fill_bytes_is_deterministic_and_nontrivial() {
        let mut a = SplitMix64::new(7);
        let mut b = SplitMix64::new(7);
        let mut x = [0u8; 37]; // not a multiple of 8: exercises the remainder
        let mut y = [0u8; 37];
        a.fill_bytes(&mut x);
        b.fill_bytes(&mut y);
        assert_eq!(x, y);
        assert!(x.iter().any(|&b| b != 0));
    }

    #[test]
    fn derived_streams_are_independent() {
        let mut a = SplitMix64::derive(42, 1, 1);
        let mut b = SplitMix64::derive(42, 1, 2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn median_odd_and_even() {
        assert_eq!(median_ms(&mut [30, 10, 20]), 20);
        assert_eq!(median_ms(&mut [40, 10, 20, 30]), 25);
        assert_eq!(median_ms(&mut [7]), 7);
    }

    #[test]
    fn fnv1a_known_vector() {
        // FNV-1a 64 of "" is the offset basis; of "a" is a published value.
        assert_eq!(Fnv1a::new().finish(), 0xcbf2_9ce4_8422_2325);
        let mut h = Fnv1a::new();
        h.update(b"a");
        assert_eq!(h.finish(), 0xaf63_dc4c_8601_ec8c);
    }
}
