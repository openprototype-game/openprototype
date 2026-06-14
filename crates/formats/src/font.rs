//! Bitmap fonts (`FONT.RAW`, `FONT2.RAW`).
//!
//! A font is a 320-pixel-wide indexed glyph sheet. Glyphs are 16x15, laid out
//! 20 per row, with the glyph area starting at y = 16. Character `c` maps to
//! glyph index `c - 0x20` (space first; the sheet is uppercase-only). Pixel
//! index 0 is transparent, so text composites over an existing background.
//!
//! Layout reverse-engineered from START.EXE's glyph blitter (`fcn.00003d03`);
//! see `reference/start-exe.md`.

use crate::error::DecodeError;
use crate::{Dimensions, IndexedImage, Result};

pub(crate) const GLYPH_WIDTH: u32 = 16;
pub(crate) const GLYPH_HEIGHT: u32 = 15;

const SHEET_WIDTH: u32 = 320;
const GLYPHS_PER_ROW: usize = 20;
const GLYPH_AREA_TOP: u32 = 16;
const FIRST_CHAR: u8 = 0x20;

/// A decoded glyph sheet.
pub struct Font {
    sheet: IndexedImage,
    glyph_count: usize,
}

impl Font {
    /// Decodes a `.RAW` glyph sheet.
    ///
    /// Width is fixed at 320; height comes from the byte count.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() || !bytes.len().is_multiple_of(SHEET_WIDTH as usize) {
            return Err(DecodeError::SizeMismatch {
                expected: SHEET_WIDTH as usize,
                actual: bytes.len(),
            });
        }

        let height = bytes.len() as u32 / SHEET_WIDTH;
        let rows = height.saturating_sub(GLYPH_AREA_TOP) / GLYPH_HEIGHT;
        let glyph_count = rows as usize * GLYPHS_PER_ROW;
        let sheet = IndexedImage::new(Dimensions::new(SHEET_WIDTH, height), bytes.to_vec())?;

        Ok(Self { sheet, glyph_count })
    }

    fn glyph_pixel(&self, glyph: usize, x: u32, y: u32) -> u8 {
        let band = glyph / GLYPHS_PER_ROW;
        let column = glyph % GLYPHS_PER_ROW;
        let source_x = column as u32 * GLYPH_WIDTH + x;
        let source_y = GLYPH_AREA_TOP + band as u32 * GLYPH_HEIGHT + y;

        self.sheet.pixels[(source_y * SHEET_WIDTH + source_x) as usize]
    }

    /// Composites `text` onto `target` at (`x`, `y`), 16 px per character.
    ///
    /// Glyph pixels of index 0 are transparent; out-of-range characters and
    /// out-of-bounds pixels are skipped.
    pub fn draw_into(&self, target: &mut IndexedImage, x: i32, y: i32, text: &str) {
        self.draw(target, x, y, text, None);
    }

    /// Composites like [`Font::draw_into`], but remaps each glyph pixel first.
    ///
    /// Each glyph pixel is routed through `map`. The level WADs draw dim text
    /// this way (their menu blitter at LEVEL_2 file `0xb4bb` routes the glyph
    /// bytes through the playfield's brightness table).
    pub fn draw_into_mapped(
        &self,
        target: &mut IndexedImage,
        x: i32,
        y: i32,
        text: &str,
        map: &[u8; 256],
    ) {
        self.draw(target, x, y, text, Some(map));
    }

    fn draw(&self, target: &mut IndexedImage, x: i32, y: i32, text: &str, map: Option<&[u8; 256]>) {
        for (cell, byte) in text.bytes().enumerate() {
            if byte < FIRST_CHAR {
                continue;
            }

            let glyph = (byte - FIRST_CHAR) as usize;

            if glyph >= self.glyph_count {
                continue;
            }

            let origin_x = x + cell as i32 * GLYPH_WIDTH as i32;

            for gy in 0..GLYPH_HEIGHT {
                for gx in 0..GLYPH_WIDTH {
                    let pixel = self.glyph_pixel(glyph, gx, gy);

                    if pixel == 0 {
                        continue;
                    }

                    let pixel = map.map_or(pixel, |map| map[usize::from(pixel)]);
                    let tx = origin_x + gx as i32;
                    let ty = y + gy as i32;

                    if tx < 0 || ty < 0 {
                        continue;
                    }

                    let (tx, ty) = (tx as u32, ty as u32);

                    if tx < target.size.width && ty < target.size.height {
                        target.pixels[(ty * target.size.width + tx) as usize] = pixel;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic sheet: 320 wide, glyph area (y >= 16) filled so glyph 0's
    /// top-left pixel is a known non-zero value.
    fn sheet() -> Vec<u8> {
        let height = GLYPH_AREA_TOP + GLYPH_HEIGHT; // one band of glyphs
        let mut bytes = vec![0u8; (SHEET_WIDTH * height) as usize];
        // glyph 0, pixel (0,0) is at (x=0, y=16)
        bytes[(GLYPH_AREA_TOP * SHEET_WIDTH) as usize] = 7;
        bytes
    }

    #[test]
    fn counts_glyphs_from_sheet_height() {
        let font = Font::decode(&sheet()).unwrap();
        assert_eq!(font.glyph_count, GLYPHS_PER_ROW); // exactly one band
    }

    #[test]
    fn rejects_non_320_multiple() {
        assert!(Font::decode(&[0u8; 321]).is_err());
    }

    #[test]
    fn draws_glyph_pixel_and_skips_transparent() {
        let font = Font::decode(&sheet()).unwrap();
        let mut target = IndexedImage::new(Dimensions::new(16, 15), vec![9u8; 16 * 15]).unwrap();
        // ' ' is glyph 0; its (0,0) pixel is value 7, the rest are 0 (transparent).
        font.draw_into(&mut target, 0, 0, " ");

        assert_eq!(target.pixels[0], 7); // drawn
        assert_eq!(target.pixels[1], 9); // transparent -> background kept
    }
}
