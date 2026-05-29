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
}
