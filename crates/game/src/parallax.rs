//! The level's parallax canyon background.
//!
//! The playfield is seven horizontal strips of the 640-wide canyon, each
//! scrolling horizontally at its own speed for the depth illusion: the tall far
//! horizon crawls, the thin near edges rush. On top of that the whole canyon
//! pans vertically (the camera) as the ship moves up and down.
//!
//! Scroll positions are **1/16-pixel fixed point** so the slow far strips can
//! move at fractional speeds while keeping the exact speed ratios; the displayed
//! column is the position divided by 16. Positions accumulate one tick per VGA
//! vertical refresh (~60Hz, the rate the original syncs to), wrapping at the
//! canyon width so the two-screen-wide canyon loops seamlessly.
//!
//! Constants are from `LEVEL_1.WAD`: heights `cs:0x31ae` `[14,13,9,89,8,12,15]`,
//! speeds `cs:0x261a` entries 3-9 `[16,10,6,3,6,10,16]`, wrap `0x2800` = 640*16.

use openprototype_core::framebuffer::Framebuffer;
use prototype_formats::IndexedImage;

/// The canyon background is two screens wide; the scroll wraps at its width.
const CANYON_WIDTH: u32 = 640;

/// Scroll positions are 1/16-pixel fixed point.
const SUBPIXEL: u32 = 16;

/// Positions wrap at the canyon width in sub-pixel units (the WAD's `0x2800`).
const WRAP: u32 = CANYON_WIDTH * SUBPIXEL;

/// One parallax strip: its height in canyon rows and its scroll speed in
/// 1/16-pixel per tick.
struct Strip {
    height: i32,
    speed: u32,
}

/// The seven strips, top to bottom. Symmetric depth gradient: the tall middle
/// band (the far horizon) crawls at 3, the thin top/bottom edges rush at 16.
const STRIPS: [Strip; 7] = [
    Strip {
        height: 14,
        speed: 16,
    },
    Strip {
        height: 13,
        speed: 10,
    },
    Strip {
        height: 9,
        speed: 6,
    },
    Strip {
        height: 89,
        speed: 3,
    },
    Strip {
        height: 8,
        speed: 6,
    },
    Strip {
        height: 12,
        speed: 10,
    },
    Strip {
        height: 15,
        speed: 16,
    },
];

/// The seven strip scroll positions, each advancing at its own speed per tick.
#[derive(Default)]
pub struct Parallax {
    offsets: [u32; STRIPS.len()],
}

impl Parallax {
    /// Advance every strip by `ticks` of its own speed, wrapping at the canyon
    /// width. One tick is one ~60Hz vertical refresh.
    pub fn advance(&mut self, ticks: u32) {
        for (offset, strip) in self.offsets.iter_mut().zip(&STRIPS) {
            let wrap = u64::from(WRAP);
            let advanced = u64::from(*offset) + u64::from(strip.speed) * u64::from(ticks);
            *offset = (advanced % wrap) as u32;
        }
    }

    /// The whole-pixel scroll column of strip `strip` (the sub-pixel dropped).
    pub fn pixel_column(&self, strip: usize) -> i32 {
        (self.offsets[strip] / SUBPIXEL) as i32
    }

    /// Draw the parallax into the top `height` rows of `frame` (the playfield
    /// window, above the panel).
    ///
    /// `camera_y` is the uniform vertical scroll: the canyon row shown at the top
    /// of the playfield, the 160-tall canyon panning over the window. Each
    /// playfield row reads canyon row `row + camera_y`, scrolled horizontally by
    /// the offset of the strip that canyon row belongs to, wrapping at the canyon
    /// width.
    pub fn render(
        &self,
        canyon: &IndexedImage,
        frame: &mut Framebuffer,
        camera_y: i32,
        height: i32,
    ) {
        let canyon_width = canyon.size.width as i32;
        let canyon_height = canyon.size.height as i32;
        let frame_width = frame.image.size.width as i32;

        for dest_y in 0..height {
            let canyon_y = dest_y + camera_y;

            if canyon_y < 0 || canyon_y >= canyon_height {
                continue;
            }

            let column = (self.offsets[strip_at(canyon_y)] / SUBPIXEL) as i32;
            let canyon_row = (canyon_y * canyon_width) as usize;
            let dest_row = (dest_y * frame_width) as usize;

            for dest_x in 0..frame_width {
                let src_x = (column + dest_x).rem_euclid(canyon_width) as usize;
                frame.image.pixels[dest_row + dest_x as usize] = canyon.pixels[canyon_row + src_x];
            }
        }
    }
}

/// Which strip the canyon row `y` belongs to, by cumulative strip heights.
fn strip_at(y: i32) -> usize {
    let mut bottom = 0;

    for (index, strip) in STRIPS.iter().enumerate() {
        bottom += strip.height;

        if y < bottom {
            return index;
        }
    }

    STRIPS.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use prototype_formats::{Dimensions, Palette};

    #[test]
    fn advance_accumulates_each_strip_by_its_own_speed() {
        let mut parallax = Parallax::default();

        parallax.advance(1);
        assert_eq!(parallax.offsets, [16, 10, 6, 3, 6, 10, 16]);

        parallax.advance(2);
        assert_eq!(parallax.offsets, [48, 30, 18, 9, 18, 30, 48]);
    }

    #[test]
    fn offsets_wrap_at_the_canyon_width() {
        let mut parallax = Parallax::default();

        // Strip 0 moves 16 sub-pixels (1 px) per tick; WRAP = 640*16 = 10240, so
        // 640 ticks lands exactly on the wrap and resets to 0.
        parallax.advance(640);
        assert_eq!(parallax.offsets[0], 0);

        parallax.advance(1);
        assert_eq!(parallax.offsets[0], 16);
    }

    #[test]
    fn strip_at_maps_canyon_rows_to_strips() {
        assert_eq!(strip_at(0), 0);
        assert_eq!(strip_at(13), 0);
        assert_eq!(strip_at(14), 1);
        assert_eq!(strip_at(36), 3); // main band starts at 14+13+9
        assert_eq!(strip_at(124), 3);
        assert_eq!(strip_at(125), 4);
        assert_eq!(strip_at(159), 6);
    }

    #[test]
    fn render_fills_the_playfield_and_leaves_the_panel_band() {
        let canyon =
            IndexedImage::new(Dimensions::new(640, 160), vec![7u8; 640 * 160]).expect("canyon");
        let palette = Palette::from_vga_6bit(&[0u8; 768]).expect("palette");
        let mut frame = Framebuffer::new(Dimensions::new(320, 160), palette);

        Parallax::default().render(&canyon, &mut frame, 0, 128);

        assert_eq!(frame.image.pixels[0], 7); // top-left of the playfield
        assert_eq!(frame.image.pixels[127 * 320], 7); // last playfield row
        assert_eq!(frame.image.pixels[128 * 320], 0); // first panel row, untouched
    }

    #[test]
    fn camera_offsets_which_canyon_rows_show() {
        // Canyon where each row's pixels equal the row index, so we can read back
        // which canyon row landed on a given playfield row.
        let mut pixels = vec![0u8; 640 * 160];
        for y in 0..160 {
            for x in 0..640 {
                pixels[y * 640 + x] = y as u8;
            }
        }

        let canyon = IndexedImage::new(Dimensions::new(640, 160), pixels).expect("canyon");
        let palette = Palette::from_vga_6bit(&[0u8; 768]).expect("palette");
        let mut frame = Framebuffer::new(Dimensions::new(320, 160), palette);

        Parallax::default().render(&canyon, &mut frame, 10, 128);

        // Playfield row 0 shows canyon row 0 + camera 10 = 10.
        assert_eq!(frame.image.pixels[0], 10);
        assert_eq!(frame.image.pixels[5 * 320], 15);
    }
}
