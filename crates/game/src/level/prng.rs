//! The engine's pseudo-random number generator.
//!
//! Every generated level scatters its enemy/pickup spawns with one engine RNG:
//! an additive lagged-Fibonacci generator over a word table that is itself
//! seeded by a 16-bit linear congruential generator. The same routine drives all four
//! generated WADs (relinked per level). This mirrors the disassembly in
//! `reference/formats/level-layout.md` 1:1 — the lag pointers stay as byte
//! offsets into the word table, not tidied-up indices, so a transcription error
//! can't hide behind a cleaner form.

/// LCG multiplier (`0x7ab7` = 31415) and increment (`0x11` = 17), applied mod
/// 2^16 to each table word in turn.
const LCG_MUL: u16 = 0x7ab7;
const LCG_INC: u16 = 0x11;

/// The seed used when both lag pointers wrap to zero mid-stream.
///
/// This is *not* the initial seed: the original seeds from the wall clock at
/// level start, so the layout varies per play (the port passes that seed to
/// [`EngineRng::new`]).
const RESEED: u16 = 0x3039;

/// Both lag pointers wrap to byte offset `0x74`; `lag_a` also starts there.
const LAG_WRAP: u16 = 0x74;
/// `lag_b` starts `0x2e` bytes in.
const LAG_B_START: u16 = 0x2e;

/// Number of words the seeder fills.
///
/// Byte offset `0x74` (the lag wrap target) is word index 58, one past these,
/// so the table is 59 words. The seeder never touches the wrap slot; it starts
/// zero and is written by feedback once `si` reaches `0x74`. (Verified: a
/// non-zero init breaks the byte-exact match.)
const SEEDED_WORDS: usize = 58;

/// The engine's additive lagged-Fibonacci PRNG, seeded by a 16-bit LCG.
pub struct EngineRng {
    /// 59 words: the 58 the seeder fills plus the wrap slot at index 58.
    table: [u16; SEEDED_WORDS + 1],
    /// Lag pointers, kept as byte offsets into `table` (word index = offset / 2).
    lag_a: u16,
    lag_b: u16,
}

impl EngineRng {
    /// Builds and seeds the generator exactly as the level init does.
    ///
    /// The seeder fills words 0..=57 from the LCG; the wrap slot (index 58)
    /// stays zero.
    pub fn new(seed: u16) -> Self {
        let mut rng = Self {
            table: [0; SEEDED_WORDS + 1],
            lag_a: LAG_WRAP,
            lag_b: LAG_B_START,
        };

        rng.seed(seed);
        rng
    }

    /// Refills the seeded words from the LCG and resets the lag pointers.
    ///
    /// The wrap slot is deliberately left untouched, matching the seeder.
    fn seed(&mut self, seed: u16) {
        self.lag_a = LAG_WRAP;
        self.lag_b = LAG_B_START;

        let mut x = seed;

        for slot in &mut self.table[..SEEDED_WORDS] {
            x = x.wrapping_mul(LCG_MUL).wrapping_add(LCG_INC);
            *slot = x;
        }
    }

    /// Draws the next value, reduced into `[0, modulus)`.
    ///
    /// Every layout draw passes a nonzero modulus; the original's raw
    /// (modulus 0) path is unused here.
    ///
    /// # Panics
    ///
    /// In debug builds, panics if `modulus` is 0 (a debug assertion).
    pub fn next(&mut self, modulus: u16) -> u16 {
        debug_assert!(modulus != 0, "layout draws always pass a nonzero modulus");

        let mut si = self.lag_a;
        let mut di = self.lag_b;

        if si == 0 {
            if di == 0 {
                // Both lags wrapped to zero. Reseed, but leave si/di at zero so
                // they are saved as (0, 0) again — the original keeps reseeding
                // every call from here, and a long draw stream goes constant.
                self.seed(RESEED);
            } else {
                si = LAG_WRAP;
                di -= 2;
            }
        } else {
            si -= 2;

            if di == 0 {
                di = LAG_WRAP;
            } else {
                di -= 2;
            }
        }

        let a = self.table[(si / 2) as usize];
        let b = self.table[(di / 2) as usize];
        let value = a.wrapping_add(b);
        self.table[(si / 2) as usize] = value;

        self.lag_a = si;
        self.lag_b = di;

        value % modulus
    }
}

/// The engine's wall-clock seed: `(sec << 8 | centi) + (hour << 8 | min)`, the
/// DOS get-time formula the original feeds [`EngineRng::new`] at level start.
///
/// The components come from the system clock in UTC (std has no local-time
/// access); a fixed hour/minute offset against DOS's local clock shifts the
/// seed but not its randomness. Centiseconds keep the modern clock's full
/// precision instead of re-quantizing to the 18.2 Hz timer.
pub fn clock_seed() -> u16 {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = since_epoch.as_secs();
    let hour = (secs / 3600) % 24;
    let minute = (secs / 60) % 60;
    let sec = secs % 60;
    let centi = u64::from(since_epoch.subsec_millis() / 10);

    (((sec << 8) | centi) as u16).wrapping_add(((hour << 8) | minute) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeder_matches_the_lcg() {
        // seed 0: table[0] = 0*0x7ab7 + 0x11 = 0x11;
        //         table[1] = 0x11*0x7ab7 + 0x11 = 0x2638 (mod 2^16).
        let rng = EngineRng::new(0);
        assert_eq!(rng.table[0], 0x0011);
        assert_eq!(rng.table[1], 0x2638);
    }

    #[test]
    fn wrap_slot_starts_zero() {
        let rng = EngineRng::new(0);
        assert_eq!(rng.table[SEEDED_WORDS], 0);
    }

    #[test]
    fn bounded_draws_stay_in_range() {
        let mut rng = EngineRng::new(12345);

        for modulus in 1..=256u16 {
            assert!(rng.next(modulus) < modulus);
        }
    }

    #[test]
    fn same_seed_is_deterministic() {
        let mut a = EngineRng::new(0x8ed2);
        let mut b = EngineRng::new(0x8ed2);

        for _ in 0..2000 {
            assert_eq!(a.next(100), b.next(100));
        }
    }
}
