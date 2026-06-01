//! The 320x200 indexed framebuffer the core draws into.
//!
//! The original game runs in VGA mode 13h: a 320x200 frame of palette indices
//! with a 256-colour DAC. The core mirrors that exactly. It never touches RGB
//! or a window; the platform layer resolves indices through the palette and
//! presents the result scaled.

use prototype_formats::{Dimensions, IndexedImage, Palette};

/// Mode 13h width.
pub const SCREEN_WIDTH: u32 = 320;
/// Mode 13h height.
pub const SCREEN_HEIGHT: u32 = 200;

/// A full screen of palette indices plus the active 256-colour palette.
pub struct Framebuffer {
    pub image: IndexedImage,
    pub palette: Palette,
}

impl Framebuffer {
    /// A black screen (all index 0) with the given palette.
    pub fn new(palette: Palette) -> Self {
        let image = IndexedImage::new(
            Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT),
            vec![0u8; (SCREEN_WIDTH * SCREEN_HEIGHT) as usize],
        )
        .expect("320x200 buffer matches its dimensions");

        Self { image, palette }
    }

    /// Replace the whole frame with a full-screen background. The source must
    /// be 320x200; a mismatch is a programming error in the asset pipeline.
    pub fn blit_screen(&mut self, background: &IndexedImage) {
        debug_assert_eq!(
            background.size, self.image.size,
            "background must be 320x200"
        );
        self.image.pixels.copy_from_slice(&background.pixels);
    }

    /// Resolve the frame to packed 8-bit RGB (`width * height * 3` bytes).
    pub fn to_rgb8(&self) -> Vec<u8> {
        self.image.to_rgb8(&self.palette)
    }

    /// Resolve the frame to packed RGBA (`width * height * 4` bytes), the
    /// layout `pixels` expects. Every pixel is fully opaque.
    pub fn to_rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.image.pixels.len() * 4);

        for &index in &self.image.pixels {
            let color = self.palette.colors[index as usize];
            out.extend_from_slice(&[color.r, color.g, color.b, 0xff]);
        }

        out
    }
}
