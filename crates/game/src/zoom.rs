//! The front-end's zoom reveal (`START.EXE` file `0x2870`).
//!
//! Animates one full-screen composite over 25 steps, zooming out from 25x
//! magnification to an exact 1:1: per step, the source page (new content at
//! its final position on an all-zero background, index 0 = transparent) is
//! sampled nearest-neighbor through the step's scale and laid over a
//! background snapshot read at the destination coordinate. New content rushes
//! in from the screen center as giant pixels and shrinks into place over the
//! untouched previous screen.
//!
//! The ending zooms its twelve text lines in this way; the high-score screen
//! its eight table rows. The original paces neither: the steps run as fast as
//! the VGA writes allow, with no tick wait and no key poll. Callers pick the
//! pace.
//!
//! The scale progression comes from a parameter `z` walking 960 down to 0 by
//! 40 (file `0x2846`): with `d = 1000 - z`, the source would cover
//! `w = 2*(160000/d)` by `h = 2*(100000/d)` pixels (8000x5000 down to
//! 320x200, integer division), of which only the centered 320x195 window is
//! composited; output rows 195..200 are never touched. The per-pixel source
//! coordinates step through 16.16 fixed-point accumulators whose carry out of
//! the fraction is applied one add LATE (the original's rotated-dword `adc`
//! trick at file `0x1565`/`0x28d9`); that off-by-one against straightforward
//! accumulation is reproduced bit-exactly here.

use prototype_formats::IndexedImage;

/// The zoom's step count (`cx = 0x19` at file `0x2888`).
pub const STEPS: u32 = 25;

/// The composite covers output rows `0..195` only (`cx = 0xc3` at file
/// `0x28ed`); the bottom five rows keep whatever they held.
const ROWS: usize = 195;

const WIDTH: usize = 320;

/// A 16.16 fixed-point accumulator with the original's delayed carry: the
/// fractional overflow of one add increments the integer part only at the
/// next add.
struct DelayedCarry {
    int: u16,
    frac: u16,
    carry: bool,
}

impl DelayedCarry {
    fn new(int: u16, frac: u16) -> Self {
        Self {
            int,
            frac,
            carry: false,
        }
    }

    /// The current integer part, then advance by `step` (16.16).
    fn take(&mut self, step: u32) -> u16 {
        let current = self.int;
        let (frac, carry) = self.frac.overflowing_add(step as u16);
        self.int = self
            .int
            .wrapping_add((step >> 16) as u16)
            .wrapping_add(u16::from(self.carry));
        self.frac = frac;
        self.carry = carry;
        current
    }
}

/// Composite one zoom step (1-based, `1..=STEPS`) of `src` over `bg` into
/// `target`. All three are 320x200 pages; `target` may alias neither input.
pub fn composite_step(src: &IndexedImage, bg: &IndexedImage, step: u32, target: &mut IndexedImage) {
    debug_assert!((1..=STEPS).contains(&step));

    let d = 40 * step;
    let w = (160_000 / d) * 2;
    let h = (100_000 / d) * 2;

    // Horizontal: a 320-entry source-column table (file 0x1565, patched into
    // the unrolled blitter in the original).
    let hstep = (320u32 << 16) / w;
    let hstart = hstep.wrapping_mul(w - 320) >> 1;
    let mut column = DelayedCarry::new((hstart >> 16) as u16, hstart as u16);
    let mut columns = [0u16; WIDTH];

    for entry in &mut columns {
        *entry = column.take(hstep);
    }

    // Vertical: the start row's fraction is discarded (file 0x28d5), so the
    // accumulator opens at frac 0.
    let vstep = (200u32 << 16) / h;
    let vstart = (((h - 200) / 2) * vstep + vstep / 2) >> 16;
    let mut row = DelayedCarry::new(vstart as u16, 0);

    for target_row in 0..ROWS {
        let base = usize::from(row.take(vstep)) * WIDTH;
        let line = target_row * WIDTH;

        for (x, &source_column) in columns.iter().enumerate() {
            let pixel = src.pixels[base + usize::from(source_column)];

            target.pixels[line + x] = if pixel != 0 {
                pixel
            } else {
                bg.pixels[line + x]
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prototype_formats::Dimensions;

    fn page(fill: u8) -> IndexedImage {
        IndexedImage::new(Dimensions::new(320, 200), vec![fill; 64_000])
            .expect("page matches its dimensions")
    }

    #[test]
    fn the_final_step_is_an_exact_composite() {
        let mut src = page(0);
        src.pixels[100 * 320 + 17] = 5;
        let bg = page(9);
        let mut target = page(0);

        composite_step(&src, &bg, STEPS, &mut target);

        assert_eq!(target.pixels[100 * 320 + 17], 5);
        assert_eq!(target.pixels[100 * 320 + 18], 9);
        assert_eq!(target.pixels[0], 9);
        assert_eq!(target.pixels[194 * 320 + 319], 9);
    }

    #[test]
    fn the_bottom_five_rows_are_never_touched() {
        let src = page(3);
        let bg = page(9);

        for step in 1..=STEPS {
            let mut target = page(0);
            composite_step(&src, &bg, step, &mut target);

            assert!(target.pixels[..195 * 320].iter().all(|&pixel| pixel == 3));
            assert!(target.pixels[195 * 320..].iter().all(|&pixel| pixel == 0));
        }
    }

    #[test]
    fn the_first_step_magnifies_the_source_center() {
        // At 25x the center of the screen samples the source's center pixel
        // as a giant block.
        let mut src = page(0);
        src.pixels[97 * 320 + 160] = 7;
        let bg = page(9);
        let mut target = page(0);

        composite_step(&src, &bg, 1, &mut target);

        let hits = target.pixels.iter().filter(|&&pixel| pixel == 7).count();
        assert!(hits > 300, "one source pixel covers a large block: {hits}");
    }

    #[test]
    fn every_step_stays_in_bounds() {
        // Indexing panics would fail this; the magnified sample window stays
        // inside the 320x200 source on every step.
        let src = page(1);
        let bg = page(2);
        let mut target = page(0);

        for step in 1..=STEPS {
            composite_step(&src, &bg, step, &mut target);
        }
    }
}
