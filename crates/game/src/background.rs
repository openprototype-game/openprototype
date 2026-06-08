//! The level's parallax background.
//!
//! The playfield shows horizontal strips of the level's 640-wide SP image, each
//! scrolling at its own rate for depth. L1's canyon slices into seven strips (a
//! tall far horizon that crawls, thin near edges that rush); the race background
//! (L2/4/6) is a single full-height strip. On top of the horizontal scroll the
//! whole image pans vertically (the camera) as the ship moves up and down.
//!
//! Scroll positions are 1/16-pixel fixed point so the slow far strips can move
//! at fractional rates while keeping exact ratios; the displayed column is the
//! position divided by 16. Positions accumulate one tick per VGA vertical
//! refresh (~60Hz), wrapping at the image width so the two-screen-wide image
//! loops seamlessly.
//!
//! The strip layout is a property of the SP image, not the level (see
//! [`Sp::strips`]), so the race levels share one definition. Canyon heights are
//! from `LEVEL_1.WAD` `cs:0x31ae`, rates `cs:0x261a`; RACEB2 is one strip
//! `{160, rate 32}`.

use openprototype_core::framebuffer::Framebuffer;
use prototype_formats::IndexedImage;

/// SP backgrounds are two screens wide; the scroll wraps at this width.
const BACKGROUND_WIDTH: u32 = 640;

/// Scroll positions are 1/16-pixel fixed point.
const SUBPIXEL: u32 = 16;

/// Positions wrap at the image width in sub-pixel units (the WAD's `0x2800`).
const WRAP: u32 = BACKGROUND_WIDTH * SUBPIXEL;

/// One horizontal strip: its height in rows and scroll rate (1/16-pixel per tick).
struct Strip {
    height: i32,
    rate: u32,
}

/// The five SP background image sets. The four shooter levels each have their
/// own; the three race levels (2/4/6) share Raceb2.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sp {
    Canyon,
    Wald,
    Alienbg,
    Lavah,
    Raceb2,
}

impl Sp {
    /// The `.SPn` filename stem on the disc; the four planes are `<stem>.SP1..4`.
    pub fn stem(self) -> &'static str {
        match self {
            Sp::Canyon => "CANYON",
            Sp::Wald => "WALD",
            Sp::Alienbg => "ALIENBG",
            Sp::Lavah => "LAVAH",
            Sp::Raceb2 => "RACEB2",
        }
    }

    /// The background's parallax strips, top to bottom.
    fn strips(self) -> &'static [Strip] {
        match self {
            Sp::Canyon => &CANYON_STRIPS,
            Sp::Raceb2 => &RACEB2_STRIPS,
            Sp::Wald | Sp::Alienbg | Sp::Lavah => {
                todo!("strip layout for {self:?} is not yet reverse-engineered")
            }
        }
    }
}

/// L1's canyon: seven strips, the symmetric depth gradient.
const CANYON_STRIPS: [Strip; 7] = [
    Strip {
        height: 14,
        rate: 16,
    },
    Strip {
        height: 13,
        rate: 10,
    },
    Strip { height: 9, rate: 6 },
    Strip {
        height: 89,
        rate: 3,
    },
    Strip { height: 8, rate: 6 },
    Strip {
        height: 12,
        rate: 10,
    },
    Strip {
        height: 15,
        rate: 16,
    },
];

/// L2/4/6's race background: one full-height strip (rate 32, confirmed for L2).
const RACEB2_STRIPS: [Strip; 1] = [Strip {
    height: 160,
    rate: 32,
}];

/// A level's parallax background: the decoded SP image and its strip layout.
/// Immutable; the scroll state lives in [`BackgroundScroll`].
pub struct Background {
    image: IndexedImage,
    strips: &'static [Strip],
}

impl Background {
    pub fn new(image: IndexedImage, sp: Sp) -> Self {
        Self {
            image,
            strips: sp.strips(),
        }
    }

    /// A fresh scroll state for this background, every strip at zero.
    pub fn scroll(&self) -> BackgroundScroll {
        BackgroundScroll {
            offsets: vec![0; self.strips.len()],
        }
    }

    /// Advance every strip by `ticks` of its own rate, wrapping at the image
    /// width. One tick is one ~60Hz vertical refresh.
    pub fn advance(&self, scroll: &mut BackgroundScroll, ticks: u32) {
        for (strip, offset) in self.strips.iter().zip(&mut scroll.offsets) {
            let wrap = u64::from(WRAP);
            let advanced = u64::from(*offset) + u64::from(strip.rate) * u64::from(ticks);
            *offset = (advanced % wrap) as u32;
        }
    }

    /// Draw the background into the top `height` rows of `frame` (the playfield
    /// window, above the panel).
    ///
    /// `camera_y` is the uniform vertical scroll: the image row shown at the top
    /// of the playfield, the 160-tall image panning over the window. Each
    /// playfield row reads image row `row + camera_y`, scrolled horizontally by
    /// the offset of the strip that row belongs to, wrapping at the image width.
    pub fn render(
        &self,
        scroll: &BackgroundScroll,
        frame: &mut Framebuffer,
        camera_y: i32,
        height: i32,
    ) {
        let image_width = self.image.size.width as i32;
        let image_height = self.image.size.height as i32;
        let frame_width = frame.image.size.width as i32;

        for dest_y in 0..height {
            let image_y = dest_y + camera_y;

            if image_y < 0 || image_y >= image_height {
                continue;
            }

            let column = (scroll.offsets[strip_at(self.strips, image_y)] / SUBPIXEL) as i32;
            let image_row = (image_y * image_width) as usize;
            let dest_row = (dest_y * frame_width) as usize;

            for dest_x in 0..frame_width {
                let src_x = (column + dest_x).rem_euclid(image_width) as usize;
                frame.image.pixels[dest_row + dest_x as usize] =
                    self.image.pixels[image_row + src_x];
            }
        }
    }
}

/// The per-strip scroll positions for a [`Background`], advanced each tick.
pub struct BackgroundScroll {
    offsets: Vec<u32>,
}

impl BackgroundScroll {
    /// The whole-pixel scroll column of strip `strip` (the sub-pixel dropped).
    pub fn pixel_column(&self, strip: usize) -> i32 {
        (self.offsets[strip] / SUBPIXEL) as i32
    }
}

/// Which strip the image row `y` belongs to, by cumulative strip heights.
fn strip_at(strips: &[Strip], y: i32) -> usize {
    let mut bottom = 0;

    for (index, strip) in strips.iter().enumerate() {
        bottom += strip.height;

        if y < bottom {
            return index;
        }
    }

    strips.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use prototype_formats::{Dimensions, Palette};

    fn canyon(image: IndexedImage) -> Background {
        Background::new(image, Sp::Canyon)
    }

    fn blank(width: u32, height: u32) -> IndexedImage {
        IndexedImage::new(
            Dimensions::new(width, height),
            vec![0u8; (width * height) as usize],
        )
        .expect("image")
    }

    #[test]
    fn advance_accumulates_each_strip_by_its_own_rate() {
        let background = canyon(blank(640, 160));
        let mut scroll = background.scroll();

        background.advance(&mut scroll, 1);
        assert_eq!(scroll.offsets, [16, 10, 6, 3, 6, 10, 16]);

        background.advance(&mut scroll, 2);
        assert_eq!(scroll.offsets, [48, 30, 18, 9, 18, 30, 48]);
    }

    #[test]
    fn offsets_wrap_at_the_image_width() {
        let background = canyon(blank(640, 160));
        let mut scroll = background.scroll();

        // Strip 0 moves 16 sub-pixels (1 px) per tick; WRAP = 640*16 = 10240, so
        // 640 ticks lands exactly on the wrap and resets to 0.
        background.advance(&mut scroll, 640);
        assert_eq!(scroll.offsets[0], 0);

        background.advance(&mut scroll, 1);
        assert_eq!(scroll.offsets[0], 16);
    }

    #[test]
    fn strip_at_maps_image_rows_to_strips() {
        let strips = Sp::Canyon.strips();
        assert_eq!(strip_at(strips, 0), 0);
        assert_eq!(strip_at(strips, 13), 0);
        assert_eq!(strip_at(strips, 14), 1);
        assert_eq!(strip_at(strips, 36), 3); // main band starts at 14+13+9
        assert_eq!(strip_at(strips, 124), 3);
        assert_eq!(strip_at(strips, 125), 4);
        assert_eq!(strip_at(strips, 159), 6);
    }

    #[test]
    fn single_strip_scrolls_uniformly() {
        // The race background is one full-height strip: every row uses offset 0.
        let background = Background::new(blank(640, 160), Sp::Raceb2);
        let scroll = background.scroll();
        assert_eq!(scroll.offsets.len(), 1);
        assert_eq!(strip_at(Sp::Raceb2.strips(), 0), 0);
        assert_eq!(strip_at(Sp::Raceb2.strips(), 159), 0);
    }

    #[test]
    fn render_fills_the_playfield_and_leaves_the_panel_band() {
        let image =
            IndexedImage::new(Dimensions::new(640, 160), vec![7u8; 640 * 160]).expect("image");
        let palette = Palette::from_vga_6bit(&[0u8; 768]).expect("palette");
        let mut frame = Framebuffer::new(Dimensions::new(320, 160), palette);

        let background = canyon(image);
        background.render(&background.scroll(), &mut frame, 0, 128);

        assert_eq!(frame.image.pixels[0], 7); // top-left of the playfield
        assert_eq!(frame.image.pixels[127 * 320], 7); // last playfield row
        assert_eq!(frame.image.pixels[128 * 320], 0); // first panel row, untouched
    }

    #[test]
    fn camera_offsets_which_image_rows_show() {
        // Image where each row's pixels equal the row index, so we can read back
        // which image row landed on a given playfield row.
        let mut pixels = vec![0u8; 640 * 160];
        for y in 0..160 {
            for x in 0..640 {
                pixels[y * 640 + x] = y as u8;
            }
        }

        let image = IndexedImage::new(Dimensions::new(640, 160), pixels).expect("image");
        let palette = Palette::from_vga_6bit(&[0u8; 768]).expect("palette");
        let mut frame = Framebuffer::new(Dimensions::new(320, 160), palette);

        let background = canyon(image);
        background.render(&background.scroll(), &mut frame, 10, 128);

        // Playfield row 0 shows image row 0 + camera 10 = 10.
        assert_eq!(frame.image.pixels[0], 10);
        assert_eq!(frame.image.pixels[5 * 320], 15);
    }
}
