//! The indexed framebuffer the core draws into.
//!
//! The original game runs in VGA modes whose frames are palette indices with a
//! 256-colour DAC (the front-end is mode 13h, 320x200). The core mirrors that:
//! a frame is indices plus the active palette, at whatever size the scene works
//! in. It never touches RGB or a window; the backend resolves indices through
//! the palette and presents the result scaled.

use prototype_formats::{Dimensions, IndexedImage, Palette};

/// A frame of palette indices plus the active 256-colour palette.
pub struct Framebuffer {
    pub image: IndexedImage,
    pub palette: Palette,
}

impl Framebuffer {
    /// A black screen (all index 0) of the given size with the given palette.
    pub fn new(size: Dimensions, palette: Palette) -> Self {
        let pixels = vec![0u8; (size.width * size.height) as usize];
        let image = IndexedImage::new(size, pixels).expect("pixel buffer matches its dimensions");

        Self { image, palette }
    }

    /// Replace the whole frame with a full-screen background. The source must
    /// match the framebuffer's size; a mismatch is a programming error in the
    /// asset pipeline.
    pub fn blit_screen(&mut self, background: &IndexedImage) {
        debug_assert_eq!(
            background.size, self.image.size,
            "background must match the framebuffer size"
        );
        self.image.pixels.copy_from_slice(&background.pixels);
    }
}
