//! Paletted bitmaps.

use crate::color::Palette;
use crate::error::{DecodeError, Result};

/// Pixel dimensions. RAW and BDY store no header, so the caller supplies these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

impl Dimensions {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Pixel count (`width * height`).
    pub fn pixel_count(self) -> usize {
        self.width as usize * self.height as usize
    }
}

/// A paletted bitmap: one byte per pixel, each an index into a [`Palette`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedImage {
    pub size: Dimensions,
    pub pixels: Vec<u8>,
}

impl IndexedImage {
    /// Wrap raw indices, checking the count matches `size`.
    pub fn new(size: Dimensions, pixels: Vec<u8>) -> Result<Self> {
        if pixels.len() != size.pixel_count() {
            return Err(DecodeError::SizeMismatch {
                expected: size.pixel_count(),
                actual: pixels.len(),
            });
        }

        Ok(Self { size, pixels })
    }

    /// Resolve through a palette to packed 8-bit RGB (`width * height * 3` bytes).
    pub fn to_rgb8(&self, palette: &Palette) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 3);

        for &index in &self.pixels {
            let color = palette.colors[index as usize];
            out.extend_from_slice(&[color.r, color.g, color.b]);
        }

        out
    }

    /// Resolve through a palette to packed RGBA (`width * height * 4` bytes).
    /// Every pixel is fully opaque: VGA has no alpha, so this is for backends
    /// that need a four-channel buffer (e.g. `pixels`).
    pub fn to_rgba8(&self, palette: &Palette) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 4);

        for &index in &self.pixels {
            let color = palette.colors[index as usize];
            out.extend_from_slice(&[color.r, color.g, color.b, 0xff]);
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgb;

    #[test]
    fn pixel_count_multiplies_dimensions() {
        assert_eq!(Dimensions::new(320, 200).pixel_count(), 64_000);
    }

    #[test]
    fn new_rejects_mismatched_pixel_count() {
        let error = IndexedImage::new(Dimensions::new(2, 2), vec![0; 3]).unwrap_err();
        assert_eq!(
            error,
            DecodeError::SizeMismatch {
                expected: 4,
                actual: 3
            }
        );
    }

    #[test]
    fn new_accepts_exact_pixel_count() {
        let image = IndexedImage::new(Dimensions::new(2, 1), vec![0, 1]).unwrap();
        assert_eq!(image.pixels, vec![0, 1]);
    }

    #[test]
    fn to_rgb8_packs_indices_through_palette() {
        let mut colors = [Rgb::default(); 256];
        colors[0] = Rgb {
            r: 10,
            g: 20,
            b: 30,
        };
        colors[1] = Rgb {
            r: 40,
            g: 50,
            b: 60,
        };
        let palette = Palette { colors };
        let image = IndexedImage::new(Dimensions::new(2, 1), vec![0, 1]).unwrap();

        assert_eq!(image.to_rgb8(&palette), vec![10, 20, 30, 40, 50, 60]);
    }
}
