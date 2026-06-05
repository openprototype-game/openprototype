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

    /// Copy `source` onto the frame with its top-left at `(x, y)`, clipped to
    /// the frame edges.
    ///
    /// Every index is copied as-is (opaque); rows and columns that fall outside
    /// the frame are skipped, so partly- or fully-offscreen placements are
    /// safe. This is the compositing primitive for the HUD panel and the cells
    /// blitted onto it.
    pub fn blit(&mut self, source: &IndexedImage, x: i32, y: i32) {
        let frame_width = self.image.size.width as i32;
        let frame_height = self.image.size.height as i32;
        let source_width = source.size.width as i32;

        for row in 0..source.size.height as i32 {
            let dest_y = y + row;

            if dest_y < 0 || dest_y >= frame_height {
                continue;
            }

            let dest_x0 = x.max(0);
            let dest_x1 = (x + source_width).min(frame_width);

            if dest_x0 >= dest_x1 {
                continue;
            }

            let source_x0 = dest_x0 - x;
            let len = (dest_x1 - dest_x0) as usize;
            let dest = (dest_y * frame_width + dest_x0) as usize;
            let src = (row * source_width + source_x0) as usize;

            self.image.pixels[dest..dest + len].copy_from_slice(&source.pixels[src..src + len]);
        }
    }
}
