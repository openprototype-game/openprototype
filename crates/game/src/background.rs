//! The level's parallax background.
//!
//! The playfield shows horizontal strips of the level's 640-wide SP image, each
//! scrolling at its own rate for depth. L1's canyon slices into seven strips (a
//! tall far horizon that crawls, thin near edges that rush); the race background
//! (L2/4/6) is a single full-height strip. On top of the horizontal scroll the
//! whole image pans vertically (the camera) as the ship moves up and down.
//!
//! Scroll positions are sub-pixel fixed point so the slow far strips can move
//! at fractional rates while keeping exact ratios: 1/16 pixel for most levels,
//! 1/256 for the lava background (see [`Sp::subpixel_shift`]). Positions
//! accumulate one tick per VGA vertical refresh (~60Hz), wrapping at the image
//! width so the two-screen-wide image loops seamlessly.
//!
//! The strip layout is a property of the SP image, not the level (see
//! [`Sp::strips`]), so the race levels share one definition. Each WAD holds a
//! strip table ({count, heights, first accumulator pointer}; accumulators are
//! consecutive, their rates in the vsync ISR's rate table). Strips need not
//! cover the playfield: the forest (120 rows) and alien (135 rows) backgrounds
//! leave black rows above the panel.

use crate::playfield;
use openprototype_core::framebuffer::Framebuffer;
use prototype_formats::IndexedImage;

/// SP backgrounds are two screens wide; the scroll wraps at this width.
const BACKGROUND_WIDTH: u32 = 640;

/// One horizontal strip: its height in rows and scroll rate, in the
/// background's sub-pixel units per tick (see [`Sp::subpixel_shift`]).
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
            Sp::Wald => &WALD_STRIPS,
            Sp::Alienbg => &ALIENBG_STRIPS,
            Sp::Lavah => &LAVAH_STRIPS,
        }
    }

    /// The fixed-point shift of this background's scroll positions. Most
    /// compositors keep 1/16-pixel positions (`shr eax, 4`, wrap `0x2800`);
    /// L7's lava keeps 1/256 (`shr eax, 8`, wrap `0x28000`) so its 65 wave
    /// strips can creep at fractions the coarser scale can't hold.
    fn subpixel_shift(self) -> u32 {
        match self {
            Sp::Lavah => 8,
            _ => 4,
        }
    }

    /// Every strip accumulator's initial value, straight from the WAD's data
    /// image (no code ever writes them before the ISR starts adding). This is
    /// load-bearing for the alien background: its bottom strip never moves, so
    /// the initial 164 px is what centers the sun over the playfield.
    fn initial_offset(self) -> u32 {
        match self {
            Sp::Canyon => 0x2770,
            Sp::Alienbg => 0xa40,
            Sp::Wald | Sp::Lavah | Sp::Raceb2 => 0,
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

/// L3's forest: one 120-row strip; the rows below it stay undrawn (black) above
/// the panel. From `LEVEL_3.WAD` heights `cs:0x4e3c`, accum `cs:0x38d8`.
const WALD_STRIPS: [Strip; 1] = [Strip {
    height: 120,
    rate: 4,
}];

/// L5's alien background: four strips covering 135 rows, fastest at the top and
/// a static floor at the bottom; the rest stays undrawn. From `LEVEL_5.WAD`
/// heights `cs:0x33ac`, accums `cs:0x2603`.
const ALIENBG_STRIPS: [Strip; 4] = [
    Strip {
        height: 20,
        rate: 8,
    },
    Strip {
        height: 23,
        rate: 4,
    },
    Strip {
        height: 22,
        rate: 1,
    },
    Strip {
        height: 70,
        rate: 0,
    },
];

/// L7's lava background: a 65-strip perspective gradient. 32 two-row lines
/// rush at the top (rates 256 down to 32), a 32-row horizon crawls in the
/// middle, and the mirror rushes back out at the bottom. From `LEVEL_7.WAD`
/// heights `cs:0x3e67`, accums `cs:0x2c64`, rates `cs:0x2d68`.
const LAVAH_STRIPS: [Strip; 65] = [
    Strip {
        height: 2,
        rate: 256,
    },
    Strip {
        height: 2,
        rate: 248,
    },
    Strip {
        height: 2,
        rate: 241,
    },
    Strip {
        height: 2,
        rate: 234,
    },
    Strip {
        height: 2,
        rate: 227,
    },
    Strip {
        height: 2,
        rate: 219,
    },
    Strip {
        height: 2,
        rate: 212,
    },
    Strip {
        height: 2,
        rate: 205,
    },
    Strip {
        height: 2,
        rate: 198,
    },
    Strip {
        height: 2,
        rate: 190,
    },
    Strip {
        height: 2,
        rate: 183,
    },
    Strip {
        height: 2,
        rate: 176,
    },
    Strip {
        height: 2,
        rate: 169,
    },
    Strip {
        height: 2,
        rate: 162,
    },
    Strip {
        height: 2,
        rate: 154,
    },
    Strip {
        height: 2,
        rate: 147,
    },
    Strip {
        height: 2,
        rate: 140,
    },
    Strip {
        height: 2,
        rate: 133,
    },
    Strip {
        height: 2,
        rate: 125,
    },
    Strip {
        height: 2,
        rate: 118,
    },
    Strip {
        height: 2,
        rate: 111,
    },
    Strip {
        height: 2,
        rate: 104,
    },
    Strip {
        height: 2,
        rate: 97,
    },
    Strip {
        height: 2,
        rate: 89,
    },
    Strip {
        height: 2,
        rate: 82,
    },
    Strip {
        height: 2,
        rate: 75,
    },
    Strip {
        height: 2,
        rate: 68,
    },
    Strip {
        height: 2,
        rate: 60,
    },
    Strip {
        height: 2,
        rate: 53,
    },
    Strip {
        height: 2,
        rate: 46,
    },
    Strip {
        height: 2,
        rate: 39,
    },
    Strip {
        height: 2,
        rate: 32,
    },
    Strip {
        height: 32,
        rate: 32,
    },
    Strip {
        height: 2,
        rate: 32,
    },
    Strip {
        height: 2,
        rate: 39,
    },
    Strip {
        height: 2,
        rate: 46,
    },
    Strip {
        height: 2,
        rate: 53,
    },
    Strip {
        height: 2,
        rate: 60,
    },
    Strip {
        height: 2,
        rate: 68,
    },
    Strip {
        height: 2,
        rate: 75,
    },
    Strip {
        height: 2,
        rate: 82,
    },
    Strip {
        height: 2,
        rate: 89,
    },
    Strip {
        height: 2,
        rate: 97,
    },
    Strip {
        height: 2,
        rate: 104,
    },
    Strip {
        height: 2,
        rate: 111,
    },
    Strip {
        height: 2,
        rate: 118,
    },
    Strip {
        height: 2,
        rate: 125,
    },
    Strip {
        height: 2,
        rate: 133,
    },
    Strip {
        height: 2,
        rate: 140,
    },
    Strip {
        height: 2,
        rate: 147,
    },
    Strip {
        height: 2,
        rate: 154,
    },
    Strip {
        height: 2,
        rate: 162,
    },
    Strip {
        height: 2,
        rate: 169,
    },
    Strip {
        height: 2,
        rate: 176,
    },
    Strip {
        height: 2,
        rate: 183,
    },
    Strip {
        height: 2,
        rate: 190,
    },
    Strip {
        height: 2,
        rate: 198,
    },
    Strip {
        height: 2,
        rate: 205,
    },
    Strip {
        height: 2,
        rate: 212,
    },
    Strip {
        height: 2,
        rate: 219,
    },
    Strip {
        height: 2,
        rate: 227,
    },
    Strip {
        height: 2,
        rate: 234,
    },
    Strip {
        height: 2,
        rate: 241,
    },
    Strip {
        height: 2,
        rate: 248,
    },
    Strip {
        height: 2,
        rate: 256,
    },
];

/// A level's parallax background: the decoded SP image and its strip layout.
/// Immutable; the scroll state lives in [`BackgroundScroll`].
pub struct Background {
    image: IndexedImage,
    strips: &'static [Strip],
    /// The scroll positions' fixed-point shift (see [`Sp::subpixel_shift`]).
    subpixel_shift: u32,
    /// Every strip's starting position (see [`Sp::initial_offset`]).
    initial_offset: u32,
}

impl Background {
    pub fn new(image: IndexedImage, sp: Sp) -> Self {
        Self {
            image,
            strips: sp.strips(),
            subpixel_shift: sp.subpixel_shift(),
            initial_offset: sp.initial_offset(),
        }
    }

    /// A fresh scroll state for this background, every strip at its WAD-baked
    /// starting position.
    pub fn scroll(&self) -> BackgroundScroll {
        BackgroundScroll {
            offsets: vec![self.initial_offset; self.strips.len()],
            subpixel_shift: self.subpixel_shift,
        }
    }

    /// Advance every strip by `ticks` of its own rate, wrapping at the image
    /// width. One tick is one ~60Hz vertical refresh.
    pub fn advance(&self, scroll: &mut BackgroundScroll, ticks: u32) {
        let wrap = u64::from(BACKGROUND_WIDTH << self.subpixel_shift);

        for (strip, offset) in self.strips.iter().zip(&mut scroll.offsets) {
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
            let dest_row = (dest_y * frame_width) as usize;

            // Rows past the strips are not composited by the original (the
            // forest and alien strips cover less than the playfield); they
            // show the cleared buffer.
            let strip = (image_y >= 0 && image_y < image_height)
                .then(|| strip_at(self.strips, image_y))
                .flatten();

            let Some(strip) = strip else {
                frame.image.pixels[dest_row..dest_row + frame_width as usize].fill(0);
                continue;
            };

            let column = (scroll.offsets[strip] >> self.subpixel_shift) as i32;
            let image_row = (image_y * image_width) as usize;

            // The strip's scroll position appears at the playfield window's
            // left edge (the original composes from buffer byte 4 = x 16); the
            // scene masks the side bars after all playfield layers.
            for dest_x in 0..frame_width {
                let src_x = (column + dest_x - playfield::LEFT).rem_euclid(image_width) as usize;
                frame.image.pixels[dest_row + dest_x as usize] =
                    self.image.pixels[image_row + src_x];
            }
        }
    }
}

/// The per-strip scroll positions for a [`Background`], advanced each tick.
pub struct BackgroundScroll {
    offsets: Vec<u32>,
    /// Copied from the background so position reads agree on the fixed point.
    subpixel_shift: u32,
}

impl BackgroundScroll {
    /// Place one strip's raw scroll position (a savegame's accumulator,
    /// already in this background's fixed point), wrapped to the image.
    pub fn restore_offset(&mut self, strip: usize, offset: u32) {
        if let Some(slot) = self.offsets.get_mut(strip) {
            *slot = offset % (BACKGROUND_WIDTH << self.subpixel_shift);
        }
    }

    /// One strip's raw scroll position (the savegame's accumulator view).
    pub fn offset(&self, strip: usize) -> u32 {
        self.offsets[strip]
    }

    /// The whole-pixel scroll column of strip `strip` (the sub-pixel dropped).
    pub fn pixel_column(&self, strip: usize) -> i32 {
        (self.offsets[strip] >> self.subpixel_shift) as i32
    }
}

/// Which strip the image row `y` belongs to, by cumulative strip heights, or
/// `None` past the last strip (the original draws nothing there).
fn strip_at(strips: &[Strip], y: i32) -> Option<usize> {
    let mut bottom = 0;

    for (index, strip) in strips.iter().enumerate() {
        bottom += strip.height;

        if y < bottom {
            return Some(index);
        }
    }

    None
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
        // The canyon's accumulators all start at the WAD-baked 0x2770.
        let background = canyon(blank(640, 160));
        let mut scroll = background.scroll();
        assert_eq!(scroll.offsets, [0x2770; 7]);

        background.advance(&mut scroll, 1);
        let moved: Vec<u32> = scroll.offsets.iter().map(|o| o - 0x2770).collect();
        assert_eq!(moved, [16, 10, 6, 3, 6, 10, 16]);

        background.advance(&mut scroll, 2);
        let moved: Vec<u32> = scroll.offsets.iter().map(|o| o - 0x2770).collect();
        assert_eq!(moved, [48, 30, 18, 9, 18, 30, 48]);
    }

    #[test]
    fn offsets_wrap_at_the_image_width() {
        let background = canyon(blank(640, 160));
        let mut scroll = background.scroll();

        // Strip 0 moves 16 sub-pixels (1 px) per tick; the wrap is 640*16, so
        // 640 ticks lands exactly back on the starting offset.
        background.advance(&mut scroll, 640);
        assert_eq!(scroll.offsets[0], 0x2770);

        background.advance(&mut scroll, 1);
        assert_eq!(scroll.offsets[0], 0x2770 + 16);
    }

    #[test]
    fn alien_background_starts_sun_centered() {
        // The static bottom strip never moves; its initial 164 px offset is
        // what the playfield shows forever.
        let background = Background::new(blank(640, 160), Sp::Alienbg);
        let scroll = background.scroll();
        assert_eq!(scroll.pixel_column(3), 164);
    }

    #[test]
    fn strip_at_maps_image_rows_to_strips() {
        let strips = Sp::Canyon.strips();
        assert_eq!(strip_at(strips, 0), Some(0));
        assert_eq!(strip_at(strips, 13), Some(0));
        assert_eq!(strip_at(strips, 14), Some(1));
        assert_eq!(strip_at(strips, 36), Some(3)); // main band starts at 14+13+9
        assert_eq!(strip_at(strips, 124), Some(3));
        assert_eq!(strip_at(strips, 125), Some(4));
        assert_eq!(strip_at(strips, 159), Some(6));
        assert_eq!(strip_at(strips, 160), None);
    }

    #[test]
    fn single_strip_scrolls_uniformly() {
        // The race background is one full-height strip: every row uses offset 0.
        let background = Background::new(blank(640, 160), Sp::Raceb2);
        let scroll = background.scroll();
        assert_eq!(scroll.offsets.len(), 1);
        assert_eq!(strip_at(Sp::Raceb2.strips(), 0), Some(0));
        assert_eq!(strip_at(Sp::Raceb2.strips(), 159), Some(0));
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
