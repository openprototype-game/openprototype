//! Drawing the in-game HUD.
//!
//! The HUD is a fixed panel along the bottom of the play screen, the same on
//! every level. [`draw_hud`] composites it: the panel background, then the
//! readouts driven by [`GameState`].
//!
//! Element positions are taken straight from the original's HUD draw routine
//! (vaddr `0xde35`), which writes each element to a Mode X destination offset
//! `di`. The level runs in Mode X 320x160 (480 scanlines, each row tripled,
//! shown on a 4:3 CRT so pixels are 1.5x taller than wide). A `di` maps to a
//! screen pixel via the 80-byte Mode X plane stride: `x = (di % 80) * 4`,
//! `y = di / 80`, offset down by the panel's top row. The relative offsets are
//! exact; the panel's top row is passed in by the caller (defaulting to
//! [`crate::playfield::PANEL_TOP`]).

use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::{ActiveWeapon, GameState, Weapon};
use prototype_formats::{Dimensions, IndexedImage};

use crate::assets::HudAssets;

/// Mode X plane-row stride: 320 px / 4 planes.
const HUD_STRIDE: i32 = 80;

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

/// Weapon charge bars: four stacked 32x4 gauges. `di` `0x172`, then `+0x230` per
/// weapon. Each shows a 32-px window into its 64-px gradient row, slid right by
/// the level (stored as eighths: `0..=4` -> `0,8,16,24,31` source columns).
const BAR_BASE_DI: i32 = 0x172;
const BAR_PITCH_DI: i32 = 0x230;
const BAR_SIZE: Dimensions = Dimensions {
    width: 32,
    height: 4,
};
const BAR_LEVEL_STEP: usize = 8;
const BAR_MAX_OFFSET: usize = 31;

/// Smart-bomb indicator: one of four frames (counts 0..=3) at `di` `0x744`.
const SMART_DI: i32 = 0x744;
const SMART_FRAME: Dimensions = Dimensions {
    width: 40,
    height: 9,
};
const SMART_MAX: u8 = 3;

/// Weapon-selector lights: four stacked slots at `di` `0x12b`, `+0x230` each. The
/// LIGHTS sheet is eight 12x7 glyphs: 0..=3 the unselected slots, 4..=7 the
/// highlights, so a slot draws glyph `n`, or `4 + n` when it is the active one.
const MARKER_BASE_DI: i32 = 0x12b;
const MARKER_PITCH_DI: i32 = 0x230;
const MARKER_COUNT: usize = 4;
const MARKER_SIZE: Dimensions = Dimensions {
    width: 12,
    height: 7,
};

/// Weapon pod in the panel's right recess. The original copies one 56x32 cell
/// out of the EXTRAS sheet (5 weapons across, 6 animation frames down) to panel
/// `di` `0x3f` (screen x 252). The firing weapon picks the column; the bottom
/// row (`5`) is the settled frame. The minigun's pod is the leftmost column.
const POD_DI: i32 = 0x3f;
const POD_SIZE: Dimensions = Dimensions {
    width: 56,
    height: 32,
};
pub const POD_SETTLED_FRAME: usize = 5;

/// Screen `(x, y)` of a HUD element from its Mode X destination offset `di`,
/// with the panel's top edge at `panel_top`.
fn di_to_screen(di: i32, panel_top: i32) -> (i32, i32) {
    ((di % HUD_STRIDE) * 4, panel_top + di / HUD_STRIDE)
}

/// Composite the HUD for `state` onto `frame`, with the panel's top edge at
/// `panel_top`. The weapon pod is drawn in its settled state; the level scene
/// drives the open/settle animation with [`draw_weapon_pod`].
pub fn draw_hud(state: &GameState, assets: &HudAssets, panel_top: i32, frame: &mut Framebuffer) {
    frame.blit(&assets.panel, 0, panel_top);
    draw_score(state.score, assets, panel_top, frame);
    draw_lives(state.lives.get(), assets, panel_top, frame);
    draw_weapon_bars(state, assets, panel_top, frame);
    draw_smart_bombs(state.smart_bombs.get(), assets, panel_top, frame);
    draw_selector(state.selected, assets, panel_top, frame);
}

/// Draw the `active` weapon's pod at animation frame `pod_frame` into the
/// panel's right recess. Frame `0` draws NOTHING -- the original's rise and
/// lower phases blit sheet rows 1..5 only, so the panel background's empty
/// recess shows through (sheet row 0 is the first animation frame, not an
/// empty cell). [`POD_SETTLED_FRAME`] is the settled state. The scene owns
/// this call, keyed on its pod latch, not the live resolve.
pub fn draw_weapon_pod(
    active: ActiveWeapon,
    pod_frame: usize,
    assets: &HudAssets,
    panel_top: i32,
    frame: &mut Framebuffer,
) {
    if pod_frame == 0 {
        return;
    }

    let pod = pod_cell(&assets.weapon_pods, pod_column(active), pod_frame);
    let (x, y) = di_to_screen(POD_DI, panel_top);

    frame.blit(&pod, x, y);
}

/// The EXTRAS sheet column for a firing weapon: the chaingun is column `0`, then
/// the four real weapons in selector order.
fn pod_column(active: ActiveWeapon) -> usize {
    match active {
        ActiveWeapon::Chaingun => 0,
        ActiveWeapon::Selected(Weapon::Multishot) => 1,
        ActiveWeapon::Selected(Weapon::Burning) => 2,
        ActiveWeapon::Selected(Weapon::Plasma) => 3,
        ActiveWeapon::Selected(Weapon::Missile) => 4,
    }
}

/// Slice one 56x32 pod cell (weapon `column`, animation `row`) from the EXTRAS
/// sheet (which is `5 * 56` wide).
fn pod_cell(sheet: &IndexedImage, column: usize, row: usize) -> IndexedImage {
    let sheet_width = sheet.size.width as usize;
    let cell_width = POD_SIZE.width as usize;
    let x0 = column * cell_width;
    let y0 = row * POD_SIZE.height as usize;
    let mut pixels = Vec::with_capacity(POD_SIZE.pixel_count());

    for dy in 0..POD_SIZE.height as usize {
        let start = (y0 + dy) * sheet_width + x0;
        pixels.extend_from_slice(&sheet.pixels[start..start + cell_width]);
    }

    IndexedImage::new(POD_SIZE, pixels).expect("pod cell matches its dimensions")
}

/// Draw the four selector lights, highlighting the selected weapon's slot.
fn draw_selector(selected: Weapon, assets: &HudAssets, panel_top: i32, frame: &mut Framebuffer) {
    for (slot, &weapon) in Weapon::ALL.iter().enumerate() {
        let index = if weapon == selected {
            MARKER_COUNT + slot
        } else {
            slot
        };
        let light = glyph(&assets.selector_lights, MARKER_SIZE, index);
        let (x, y) = di_to_screen(MARKER_BASE_DI + slot as i32 * MARKER_PITCH_DI, panel_top);

        frame.blit(&light, x, y);
    }
}

/// Draw the smart-bomb indicator for `count`, clamped to the four frames.
fn draw_smart_bombs(count: u8, assets: &HudAssets, panel_top: i32, frame: &mut Framebuffer) {
    let glyph = glyph(
        &assets.smart_frames,
        SMART_FRAME,
        count.min(SMART_MAX) as usize,
    );
    let (x, y) = di_to_screen(SMART_DI, panel_top);

    frame.blit(&glyph, x, y);
}

/// Draw the four weapon charge bars, stacked, each filled to its level.
fn draw_weapon_bars(
    state: &GameState,
    assets: &HudAssets,
    panel_top: i32,
    frame: &mut Framebuffer,
) {
    for (index, &weapon) in Weapon::ALL.iter().enumerate() {
        let level = state.level(weapon).get() as usize;
        let offset = (level * BAR_LEVEL_STEP).min(BAR_MAX_OFFSET);
        let bar = bar_window(&assets.weapon_bars, index, offset);
        let (x, y) = di_to_screen(BAR_BASE_DI + index as i32 * BAR_PITCH_DI, panel_top);

        frame.blit(&bar, x, y);
    }
}

/// Slice the visible 32x4 window for `weapon`'s bar from the gradient sheet,
/// starting `offset` columns in. The sheet is 64 wide with four rows per weapon.
fn bar_window(sheet: &IndexedImage, weapon: usize, offset: usize) -> IndexedImage {
    let sheet_width = sheet.size.width as usize;
    let mut pixels = Vec::with_capacity(BAR_SIZE.pixel_count());

    for row in 0..BAR_SIZE.height as usize {
        let source_row = weapon * BAR_SIZE.height as usize + row;

        for column in 0..BAR_SIZE.width as usize {
            let source_column = (offset + column).min(sheet_width - 1);
            pixels.push(sheet.pixels[source_row * sheet_width + source_column]);
        }
    }

    IndexedImage::new(BAR_SIZE, pixels).expect("bar window matches its dimensions")
}

/// Draw the six-digit score, most significant digit first, with leading zeros.
fn draw_score(score: u32, assets: &HudAssets, panel_top: i32, frame: &mut Framebuffer) {
    for place in 0..SCORE_PLACES {
        let digit = (score / 10u32.pow(SCORE_PLACES - 1 - place) % 10) as usize;
        let glyph = glyph(&assets.score_digits, SCORE_DIGIT, digit);
        let (x, y) = di_to_screen(SCORE_DI + place as i32 * SCORE_ADVANCE_DI, panel_top);

        frame.blit(&glyph, x, y);
    }
}

/// Draw the lives digit. The numeral sheet starts at 1, so a count of `n` draws
/// glyph `n - 1`; zero lives draws nothing.
fn draw_lives(lives: u8, assets: &HudAssets, panel_top: i32, frame: &mut Framebuffer) {
    if lives == 0 {
        return;
    }

    let glyph = glyph(&assets.number_digits, NUMBER_DIGIT, (lives - 1) as usize);
    let (x, y) = di_to_screen(LIVES_DI, panel_top);

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
