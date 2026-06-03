//! The engine's pseudo-random number generator.
//!
//! Every generated level scatters its scenery with one engine RNG: an additive
//! lagged-Fibonacci generator over a word table that is itself seeded by a
//! 16-bit linear congruential generator. The same routine drives all four
//! generated WADs (relinked per level). This mirrors the disassembly in
//! `reference/formats/level-layout.md` 1:1 — the lag pointers stay as byte
//! offsets into the word table, not tidied-up indices, so a transcription error
//! can't hide behind a cleaner form.

/// LCG multiplier (`0x7ab7` = 31415) and increment (`0x11` = 17), applied mod
/// 2^16 to each table word in turn.
const LCG_MUL: u16 = 0x7ab7;
const LCG_INC: u16 = 0x11;

/// The seed used when both lag pointers wrap to zero mid-stream. This is *not*
/// the initial seed: the original seeds from the wall clock at level start, so
/// the layout varies per play (the port passes that seed to [`EngineRng::new`]).
const RESEED: u16 = 0x3039;

/// Both lag pointers wrap to byte offset `0x74`; `lag_a` also starts there.
const LAG_WRAP: u16 = 0x74;
/// `lag_b` starts `0x2e` bytes in.
const LAG_B_START: u16 = 0x2e;

/// Byte offset `0x74` is word index 58, one past the 58 words the seeder fills,
/// so the generator can read a 59th "wrap" slot the seeder never initialises. It
/// keeps its loaded-image value; for LEVEL_1 that is `0x800`. This is per-WAD and
/// must be re-read when porting levels 3/5/7.
const WRAP_SLOT_INIT: u16 = 0x800;

/// Number of words the seeder fills (the wrap slot at index 58 is excluded).
const SEEDED_WORDS: usize = 58;

pub struct EngineRng {
    /// 59 words: the 58 the seeder fills plus the wrap slot at index 58.
    table: [u16; SEEDED_WORDS + 1],
    /// Lag pointers, kept as byte offsets into `table` (word index = offset / 2).
    lag_a: u16,
    lag_b: u16,
}

impl EngineRng {
    /// Build and seed the generator exactly as the level init does: the seeder
    /// fills words 0..=57 from the LCG; the wrap slot keeps its image value.
    pub fn new(seed: u16) -> Self {
        let mut rng = Self {
            table: [0; SEEDED_WORDS + 1],
            lag_a: LAG_WRAP,
            lag_b: LAG_B_START,
        };

        rng.table[SEEDED_WORDS] = WRAP_SLOT_INIT;
        rng.seed(seed);
        rng
    }

    /// Refill the seeded words from the 16-bit LCG and reset the lag pointers.
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

    /// Draw the next value, reduced into `[0, modulus)`. Every layout draw passes
    /// a nonzero modulus; the original's raw (modulus 0) path is unused here.
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
    fn seeder_leaves_the_wrap_slot() {
        let rng = EngineRng::new(0);
        assert_eq!(rng.table[SEEDED_WORDS], WRAP_SLOT_INIT);
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
