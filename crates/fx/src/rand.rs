//! Deterministic seeded randomness for fx components.
//!
//! A splitmix64-flavored PRNG: same seed and same call sequence always
//! produce the same values, on every platform, so fx output is
//! reproducible byte for byte (the golden galleries depend on it).

/// Small deterministic PRNG (splitmix64).
#[derive(Clone, Copy, Debug)]
pub struct FxRand {
    state: u64,
}

impl FxRand {
    /// Seed from any integer (component seeds are typically FNV-1a of
    /// a stable identifier).
    #[must_use]
    pub fn new(seed: u64) -> Self {
        FxRand {
            state: seed.wrapping_add(0x9E37_79B9_7F4A_7C15),
        }
    }

    /// Next raw 64-bit value.
    #[must_use]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Next value in `[0, 1)`.
    #[must_use]
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1_u64 << 53) as f64
    }

    /// Next value in `[lo, hi)`.
    #[must_use]
    pub fn next_range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next_f64() * (hi - lo)
    }

    /// Next integer in `[0, n)`; `n == 0` yields 0.
    #[must_use]
    pub fn next_index(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    /// Pick from a slice; empty slices yield `fallback`.
    #[must_use]
    pub fn pick<'a, T>(&mut self, items: &'a [T], fallback: &'a T) -> &'a T {
        match items.get(self.next_index(items.len())) {
            Some(item) => item,
            None => fallback,
        }
    }
}

/// Stable per-cell hash: combines a base seed with cell coordinates so
/// grid-based components (flicker, dots) get per-cell randomness
/// without sequential draws.
#[must_use]
pub fn cell_hash(base: u64, x: u64, y: u64) -> u64 {
    let mut h =
        base ^ x.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ y.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = FxRand::new(42);
        let mut b = FxRand::new(42);
        for _ in 0..16 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = FxRand::new(1);
        let mut b = FxRand::new(2);
        let same = (0..8).all(|_| a.next_u64() == b.next_u64());
        assert!(!same);
    }

    #[test]
    fn ranges_are_bounded() {
        let mut r = FxRand::new(7);
        for _ in 0..200 {
            let v = r.next_f64();
            assert!((0.0..1.0).contains(&v));
            let v = r.next_range(5.0, 9.0);
            assert!((5.0..9.0).contains(&v));
            assert!(r.next_index(4) < 4);
        }
    }

    #[test]
    fn cell_hash_is_stable_and_position_sensitive() {
        assert_eq!(cell_hash(9, 3, 4), cell_hash(9, 3, 4));
        assert_ne!(cell_hash(9, 3, 4), cell_hash(9, 4, 3));
        assert_ne!(cell_hash(9, 3, 4), cell_hash(10, 3, 4));
    }
}
