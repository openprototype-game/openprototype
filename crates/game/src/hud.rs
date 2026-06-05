//! Drawing the in-game HUD.
//!
//! The HUD is a fixed panel along the bottom of the play screen, the same on
//! every level. [`draw_hud`] composites it: the panel background, then the
//! readouts driven by [`GameState`].
//!
//! Element positions are taken straight from the original's HUD draw routine
//! (vaddr `0xde35`), which writes each element to a Mode X destination offset
//! `di`. The screen is 320x200 logical (the level's VGA mode is double-scanned
//! 320x200, displayed square as 320x240). A `di` maps to a screen pixel via the
//! 80-byte Mode X plane stride: `x = (di % 80) * 4`, `y = di / 80`, offset down
//! by the panel's top row. The panel's exact screen row is still being pinned
//! (the WAD's split-screen line compare did not reconcile cleanly); only
//! [`PANEL_TOP`] is provisional, the relative offsets are exact.

use openprototype_core::GameState;
use openprototype_core::framebuffer::Framebuffer;
use prototype_formats::{Dimensions, IndexedImage};

use crate::assets::HudAssets;

/// Mode X plane-row stride: 320 px / 4 planes.
const HUD_STRIDE: i32 = 80;

/// Screen row of the panel's top edge. Provisional: bottom-aligned in the 320x200
/// frame (`200 - 32`); the original's split-screen row is not yet pinned.
const PANEL_TOP: i32 = 168;

/// Score readout: six digits, leading zeros. `di` `0x325`, `+4` (16 px) per digit.
const SCORE_DI: i32 = 0x325;
const SCORE_ADVANCE_DI: i32 = 4;
const SCORE_PLACES: u32 = 6;
const SCORE_DIGIT: Dimensions = Dimensions {
    width: 16,
    height: 13,
};

/// Lives count: a single digit at `di` `0x34c`.
const LIVES_DI: i32 = 0x34c;
const NUMBER_DIGIT: Dimensions = Dimensions {
    width: 12,
    height: 10,
};

/// Screen `(x, y)` of a HUD element from its Mode X destination offset `di`.
fn di_to_screen(di: i32) -> (i32, i32) {
    ((di % HUD_STRIDE) * 4, PANEL_TOP + di / HUD_STRIDE)
}

/// Composite the HUD for `state` onto `frame`.
pub fn draw_hud(state: &GameState, assets: &HudAssets, frame: &mut Framebuffer) {
    frame.blit(&assets.panel, 0, PANEL_TOP);
    draw_score(state.score, assets, frame);
    draw_lives(state.lives, assets, frame);
}

/// Draw the six-digit score, most significant digit first, with leading zeros.
fn draw_score(score: u32, assets: &HudAssets, frame: &mut Framebuffer) {
    for place in 0..SCORE_PLACES {
        let digit = (score / 10u32.pow(SCORE_PLACES - 1 - place) % 10) as usize;
        let glyph = glyph(&assets.score_digits, SCORE_DIGIT, digit);
        let (x, y) = di_to_screen(SCORE_DI + place as i32 * SCORE_ADVANCE_DI);

        frame.blit(&glyph, x, y);
    }
}

/// Draw the lives digit. The numeral sheet starts at 1, so a count of `n` draws
/// glyph `n - 1`; zero lives draws nothing.
fn draw_lives(lives: u8, assets: &HudAssets, frame: &mut Framebuffer) {
    if lives == 0 {
        return;
    }

    let glyph = glyph(&assets.number_digits, NUMBER_DIGIT, (lives - 1) as usize);
    let (x, y) = di_to_screen(LIVES_DI);

    frame.blit(&glyph, x, y);
}

/// Slice one glyph out of a stacked digit sheet (sheet width == glyph width, so
/// each glyph is a contiguous run of rows).
fn glyph(sheet: &IndexedImage, size: Dimensions, index: usize) -> IndexedImage {
    let len = size.pixel_count();
    let start = index * len;

    IndexedImage::new(size, sheet.pixels[start..start + len].to_vec())
        .expect("glyph slice matches its dimensions")
}
