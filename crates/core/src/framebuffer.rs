//! The indexed framebuffer the core draws into.
//!
//! The original game runs in VGA modes whose frames are palette indices with a
//! 256-color DAC (the front-end is mode 13h, 320x200). The core mirrors that:
//! a frame is indices plus the active palette, at whatever size the scene works
//! in. It never touches RGB or a window; the backend resolves indices through
//! the palette and presents the result scaled.

use prototype_formats::{Dimensions, IndexedImage, Palette};

/// A frame of palette indices plus the active 256-color palette.
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

    /// Copy a masked sprite onto the frame with its top-left at `(x, y)`, clipped
    /// to the frame edges.
    ///
    /// `pixels` is `size.width * size.height` entries in row-major order; `None`
    /// is transparent and leaves the frame untouched, `Some(index)` overwrites.
    /// This is how the level overlay (a `bin` sprite with trimmed margins) draws
    /// over the playfield and HUD without a color key.
    pub fn blit_transparent(&mut self, pixels: &[Option<u8>], size: Dimensions, x: i32, y: i32) {
        let frame_width = self.image.size.width as i32;
        let frame_height = self.image.size.height as i32;
        let source_width = size.width as i32;

        for row in 0..size.height as i32 {
            let dest_y = y + row;

            if dest_y < 0 || dest_y >= frame_height {
                continue;
            }

            for column in 0..source_width {
                let dest_x = x + column;

                if dest_x < 0 || dest_x >= frame_width {
                    continue;
                }

                if let Some(index) = pixels[(row * source_width + column) as usize] {
                    self.image.pixels[(dest_y * frame_width + dest_x) as usize] = index;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn black_palette() -> Palette {
        Palette::from_vga_6bit(&[0u8; 768]).expect("palette decodes")
    }

    #[test]
    fn blit_transparent_writes_opaque_pixels_and_skips_none() {
        let mut frame = Framebuffer::new(Dimensions::new(4, 2), black_palette());
        // A 2x2 sprite: top row opaque, bottom row transparent.
        let pixels = [Some(7), Some(8), None, None];

        frame.blit_transparent(&pixels, Dimensions::new(2, 2), 1, 0);

        assert_eq!(frame.image.pixels, vec![0, 7, 8, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn blit_transparent_clips_at_the_edges() {
        let mut frame = Framebuffer::new(Dimensions::new(2, 2), black_palette());
        let pixels = [Some(1), Some(2), Some(3), Some(4)];

        // Placed one pixel up and left: only the bottom-right cell lands at (0, 0).
        frame.blit_transparent(&pixels, Dimensions::new(2, 2), -1, -1);

        assert_eq!(frame.image.pixels, vec![4, 0, 0, 0]);
    }
}
