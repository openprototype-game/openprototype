//! Loading game assets from the original disc.
//!
//! The front-end reads its graphics from the CD: `BACK3.RAW` (the menu
//! background), `FONT.RAW` (the glyph sheet) and the menu palette baked into
//! `START.EXE`. This module turns those raw bytes into decoded values the
//! scenes and the audio backend consume. It depends on the disc reader but on
//! nothing graphical and nothing audio-device specific.

use anyhow::{Context, Result};
use prototype_disc::{AssetSource, DiscImage};
use prototype_formats::bin::{OUT_BIN_CATALOG, SpriteSheet, decode_banked};
use prototype_formats::font::Font;
use prototype_formats::{Dimensions, Flic, IndexedImage, Palette, StartExe, bdy, pal, raw, wad};

use crate::screen::{SCREEN_HEIGHT, SCREEN_WIDTH};

/// Everything the main menu needs to render.
pub struct MenuAssets {
    pub background: IndexedImage,
    pub font: Font,
    pub palette: Palette,
}

/// A full-screen still with its own palette (a `.BDY` image plus its `.PAL`).
pub struct StillImage {
    pub image: IndexedImage,
    pub palette: Palette,
}

/// Everything the intro sequence needs. The stills are decoded up front; the
/// FLIs are kept as raw bytes and decoded when their beat starts (each is large,
/// and the intro plays once). Their headers are validated here so gross
/// corruption surfaces at load, not mid-intro.
pub struct IntroAssets {
    pub neo: StillImage,
    pub surplogo: StillImage,
    pub cover: StillImage,
    pub intro_fli: Vec<u8>,
    pub fly_fli: Vec<u8>,
    pub credz_fli: Vec<u8>,
    pub font: Font,
}

/// What the high-score screen needs: the `HIGHSCOR.FLI` backdrop (kept as bytes,
/// decoded when the scene starts) and the second font the original draws the
/// entries with. The table itself comes from the [`HighscoreStore`], loaded when
/// the scene is built.
///
/// [`HighscoreStore`]: crate::highscores::HighscoreStore
pub struct HighscoreAssets {
    pub fli: Vec<u8>,
    pub font: Font,
}

/// `COVER3.BDY` decodes to a 320x478 image (taller than the screen); the intro
/// shows the top 320x200, where the PROTOTYPE title and ship sit.
const COVER_HEIGHT: u32 = 478;

/// Load and decode the menu assets from the disc image.
pub fn load_menu_assets(disc: &DiscImage) -> Result<MenuAssets> {
    let background_bytes = disc.read("BACK3.RAW").context("reading BACK3.RAW")?;
    let background = raw::decode(
        &background_bytes,
        Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT),
    )
    .context("decoding BACK3.RAW")?;

    let font_bytes = disc.read("FONT.RAW").context("reading FONT.RAW")?;
    let font = Font::decode(&font_bytes).context("decoding FONT.RAW")?;

    let start_exe_bytes = disc.read("START.EXE").context("reading START.EXE")?;
    let palette = StartExe::new(&start_exe_bytes)
        .context("parsing START.EXE")?
        .menu_palette()
        .context("decoding menu palette")?;

    Ok(MenuAssets {
        background,
        font,
        palette,
    })
}

/// Everything the in-game HUD needs to render.
///
/// The HUD shares the level's embedded palette (read from the `.WAD`); the
/// digit sheets are columns of stacked glyphs, sliced per digit at draw time.
pub struct HudAssets {
    pub palette: Palette,
    /// The panel background, 320x32.
    pub panel: IndexedImage,
    /// The score readout's ten LCD numerals 0..=9, 16x13 each, stacked.
    pub score_digits: IndexedImage,
    /// The count numerals 1..=9 (lives and similar), 12x10 each, stacked.
    pub number_digits: IndexedImage,
    /// The four weapon charge bars as 64-wide gradient rows (4 rows per weapon);
    /// a 32-px window into each slides right with the weapon's level.
    pub weapon_bars: IndexedImage,
    /// The smart-bomb indicator's four frames (counts 0..=3), 40x9 each, stacked.
    pub smart_frames: IndexedImage,
    /// The weapon-selector lights, 12 wide: a 28-row base of four unselected
    /// slots, then four 7-row highlights, one per selected slot.
    pub selector_lights: IndexedImage,
    /// The weapon pods drawn in the panel's right recess: 5 weapons (columns) by
    /// 6 animation frames (rows), 56x32 each; the bottom row is the settled
    /// state. EXTRAS.RAW, 280x192.
    pub weapon_pods: IndexedImage,
}

/// Everything the in-game level scene needs to render: the level's scrolling
/// canyon background (decoded to one still here), the HUD, and the weapon-top
/// overlay sprites.
pub struct LevelAssets {
    /// The canyon background, de-interleaved from the four `.SPn` plane files to
    /// one 640x160 still. The canyon is wider than the screen; the level scrolls
    /// a 320-wide window across it. The test scene shows a fixed window.
    pub background: IndexedImage,
    pub hud: HudAssets,
    /// The weapon-top overlay that clips over the panel, indexed by `Weapon`
    /// (`0` minigun ..= `4` secondary 4). Each is the firing weapon's cut-off top.
    pub overlays: [OverlaySprite; WEAPON_COUNT],
    /// The overlay's per-frame slide as it settles, `(dx, dy)` relative to its
    /// settled position, indexed by `Weapon` then animation frame. Each weapon
    /// has its own profile (some snap, some kick sideways).
    pub overlay_slide: [[(i32, i32); OVERLAY_FRAMES]; WEAPON_COUNT],
}

/// A masked sprite: `None` is transparent. Used for the weapon overlay, which
/// the original draws over the playfield/panel without a colour key.
pub struct OverlaySprite {
    pub size: Dimensions,
    pub pixels: Vec<Option<u8>>,
}

/// The five firing weapons: the minigun and the four secondaries.
const WEAPON_COUNT: usize = 5;

/// Frames in a weapon's pod/overlay open-and-settle animation (`0` hidden ..=
/// `5` settled).
pub const OVERLAY_FRAMES: usize = 6;

/// The weapon-top overlay sprites as `(catalog index, cell count)`, indexed by
/// `Weapon`. Read from the canyon WAD's descriptor table at `cs:0x31d0`. The
/// cell counts explain the catalog gaps (`0xEE` spans two cells, so the next is
/// `0xF0`); the minigun's is a 4x4 stub, effectively blank.
const OVERLAY_CELLS: [(usize, usize); WEAPON_COUNT] = [
    (0xEA, 1), // minigun
    (0xEB, 1), // secondary 1
    (0xED, 1), // secondary 2
    (0xEE, 2), // secondary 3
    (0xF0, 2), // secondary 4
];

/// Width of one Mode X catalog cell, in pixels.
const CELL_WIDTH: usize = 32;

/// File offset of the overlay position table in `LEVEL_1.WAD` (`cs:0x9128`, with
/// `file = cs + 0x29F0`). Per weapon: a block of [`OVERLAY_BLOCK_FRAMES`] `(x, y)`
/// `u16` positions; the animation only uses the first [`OVERLAY_FRAMES`].
const OVERLAY_POSITION_TABLE: usize = 0x9128 + 0x29F0;
/// Positions stored per weapon block; frames 6 and 7 are unused padding.
const OVERLAY_BLOCK_FRAMES: usize = 8;
/// Frame index of the settled position, which the slide deltas are relative to.
const OVERLAY_SETTLED_FRAME: usize = 5;

/// One Mode X plane: 160 bytes per row (every fourth column of a 640-wide row),
/// 160 rows. The four `.SPn` files together make a 640x160 image: a canyon wider
/// than the screen, scrolled horizontally, the playfield's 160 rows tall.
const BACKGROUND_SIZE: Dimensions = Dimensions {
    width: 640,
    height: 160,
};
const SP_PLANE_STRIDE: usize = 160;
const SP_PLANE_LEN: usize = SP_PLANE_STRIDE * 160;

const PANEL_SIZE: Dimensions = Dimensions {
    width: 320,
    height: 32,
};
const SCORE_DIGITS_SIZE: Dimensions = Dimensions {
    width: 16,
    height: 130,
};
const NUMBER_DIGITS_SIZE: Dimensions = Dimensions {
    width: 12,
    height: 90,
};
const WEAPON_BARS_SIZE: Dimensions = Dimensions {
    width: 64,
    height: 16,
};
const SMART_FRAMES_SIZE: Dimensions = Dimensions {
    width: 40,
    height: 36,
};
const SELECTOR_LIGHTS_SIZE: Dimensions = Dimensions {
    width: 12,
    height: 56,
};
const WEAPON_PODS_SIZE: Dimensions = Dimensions {
    width: 280,
    height: 192,
};

/// Load and decode the in-game HUD assets from the disc image.
///
/// The palette is `LEVEL_1.WAD`'s embedded one for now; it stands in until the
/// level scene picks the palette for the level being played.
pub fn load_hud_assets(disc: &DiscImage) -> Result<HudAssets> {
    let wad_bytes = disc.read("LEVEL_1.WAD").context("reading LEVEL_1.WAD")?;
    let palette = wad::level_palette(&wad_bytes).context("extracting the level palette")?;

    let panel = decode_raw(disc, "PANEL.RAW", PANEL_SIZE)?;
    let score_digits = decode_raw(disc, "SCORE.RAW", SCORE_DIGITS_SIZE)?;
    let number_digits = decode_raw(disc, "NUMBERS.RAW", NUMBER_DIGITS_SIZE)?;
    let weapon_bars = decode_raw(disc, "BALKEN.RAW", WEAPON_BARS_SIZE)?;
    let smart_frames = decode_raw(disc, "SMART.RAW", SMART_FRAMES_SIZE)?;
    let selector_lights = decode_raw(disc, "LIGHTS.RAW", SELECTOR_LIGHTS_SIZE)?;
    let weapon_pods = decode_raw(disc, "EXTRAS.RAW", WEAPON_PODS_SIZE)?;

    Ok(HudAssets {
        palette,
        panel,
        score_digits,
        number_digits,
        weapon_bars,
        smart_frames,
        selector_lights,
        weapon_pods,
    })
}

/// Load and decode the level scene's assets: the canyon background, the HUD, and
/// the weapon overlays with their per-weapon slide tables.
pub fn load_level_assets(disc: &DiscImage) -> Result<LevelAssets> {
    let background = load_canyon_background(disc)?;
    let hud = load_hud_assets(disc)?;

    let out_bin = disc.read("OUT.BIN").context("reading OUT.BIN")?;
    let wad = disc.read("LEVEL_1.WAD").context("reading LEVEL_1.WAD")?;
    let overlays = load_overlays(&out_bin, &wad)?;
    let overlay_slide = read_overlay_slide(&wad)?;

    Ok(LevelAssets {
        background,
        hud,
        overlays,
        overlay_slide,
    })
}

/// Decode the canyon BIN catalog and assemble the five weapon overlays.
fn load_overlays(out_bin: &[u8], wad: &[u8]) -> Result<[OverlaySprite; WEAPON_COUNT]> {
    let sheet = decode_banked(out_bin, wad, OUT_BIN_CATALOG).context("decoding OUT.BIN")?;

    let needed = OVERLAY_CELLS
        .iter()
        .map(|(first, cells)| first + cells)
        .max()
        .expect("overlay table is not empty");

    if sheet.sprites.len() < needed {
        anyhow::bail!(
            "OUT.BIN has {} sprites, the overlays need {needed}",
            sheet.sprites.len()
        );
    }

    Ok(std::array::from_fn(|index| {
        let (first, cells) = OVERLAY_CELLS[index];
        assemble_overlay(&sheet, first, cells)
    }))
}

/// Read each weapon's overlay slide from the WAD's position table, as `(dx, dy)`
/// per frame relative to the settled frame. Each weapon's profile differs, so
/// this is read rather than assumed.
fn read_overlay_slide(wad: &[u8]) -> Result<[[(i32, i32); OVERLAY_FRAMES]; WEAPON_COUNT]> {
    let table_end = OVERLAY_POSITION_TABLE + WEAPON_COUNT * OVERLAY_BLOCK_FRAMES * 4;

    if wad.len() < table_end {
        anyhow::bail!(
            "LEVEL_1.WAD is {} bytes, the overlay position table needs {table_end}",
            wad.len()
        );
    }

    let read_xy = |offset: usize| -> (i32, i32) {
        let x = u16::from_le_bytes([wad[offset], wad[offset + 1]]);
        let y = u16::from_le_bytes([wad[offset + 2], wad[offset + 3]]);
        (i32::from(x), i32::from(y))
    };

    Ok(std::array::from_fn(|weapon| {
        let block = OVERLAY_POSITION_TABLE + weapon * OVERLAY_BLOCK_FRAMES * 4;
        let (settled_x, settled_y) = read_xy(block + OVERLAY_SETTLED_FRAME * 4);

        std::array::from_fn(|frame| {
            let (x, y) = read_xy(block + frame * 4);
            (x - settled_x, y - settled_y)
        })
    }))
}

/// Stitch a multi-cell overlay into one masked sprite. Cell `k` occupies screen
/// columns `[k*32, k*32+32)`; within it, the decoded sprite sits at its trimmed
/// `origin`, so each cell's pixels land at `(k*32 + origin.x, origin.y)`.
fn assemble_overlay(sheet: &SpriteSheet, first: usize, cells: usize) -> OverlaySprite {
    let mut width = 0usize;
    let mut height = 0usize;

    for cell in 0..cells {
        let sprite = &sheet.sprites[first + cell];
        let origin_x = cell * CELL_WIDTH + sprite.origin.0.max(0) as usize;
        let origin_y = sprite.origin.1.max(0) as usize;
        width = width.max(origin_x + sprite.size.width as usize);
        height = height.max(origin_y + sprite.size.height as usize);
    }

    let mut pixels = vec![None; width * height];

    for cell in 0..cells {
        let sprite = &sheet.sprites[first + cell];
        let sprite_width = sprite.size.width as usize;
        let origin_x = cell * CELL_WIDTH + sprite.origin.0.max(0) as usize;
        let origin_y = sprite.origin.1.max(0) as usize;

        for sy in 0..sprite.size.height as usize {
            for sx in 0..sprite_width {
                if let Some(value) = sprite.pixels[sy * sprite_width + sx] {
                    pixels[(origin_y + sy) * width + origin_x + sx] = Some(value);
                }
            }
        }
    }

    OverlaySprite {
        size: Dimensions::new(width as u32, height as u32),
        pixels,
    }
}

/// De-interleave `CANYON.SP1..4` into one 640x160 still.
///
/// Each `.SPn` file is one Mode X plane holding every fourth column, so pixel
/// `(x, y)` lives in plane `x % 4` at byte `y * 160 + x / 4`.
fn load_canyon_background(disc: &DiscImage) -> Result<IndexedImage> {
    let mut planes = Vec::with_capacity(4);

    for index in 1..=4 {
        let name = format!("CANYON.SP{index}");
        let bytes = disc
            .read(&name)
            .with_context(|| format!("reading {name}"))?;

        if bytes.len() < SP_PLANE_LEN {
            anyhow::bail!("{name} is {} bytes, expected {SP_PLANE_LEN}", bytes.len());
        }

        planes.push(bytes);
    }

    let width = BACKGROUND_SIZE.width as usize;
    let height = BACKGROUND_SIZE.height as usize;
    let mut pixels = vec![0u8; width * height];

    for y in 0..height {
        for x in 0..width {
            pixels[y * width + x] = planes[x & 3][y * SP_PLANE_STRIDE + (x >> 2)];
        }
    }

    Ok(IndexedImage::new(BACKGROUND_SIZE, pixels).expect("canyon still matches its dimensions"))
}

/// Read and decode a linear `.RAW` graphic of known dimensions from the disc.
fn decode_raw(disc: &DiscImage, name: &str, size: Dimensions) -> Result<IndexedImage> {
    let bytes = disc.read(name).with_context(|| format!("reading {name}"))?;
    raw::decode(&bytes, size).with_context(|| format!("decoding {name}"))
}

/// Load and decode the intro assets from the disc image.
pub fn load_intro_assets(disc: &DiscImage) -> Result<IntroAssets> {
    let neo = load_still(disc, "NEO.BDY", "NEO.PAL")?;
    let surplogo = load_still(disc, "SURPLOGO.BDY", "SURPLOGO.PAL")?;
    let cover = load_cover(disc)?;

    let intro_fli = load_fli_bytes(disc, "FLI/INTRO.FLI")?;
    let fly_fli = load_fli_bytes(disc, "FLI/FLY.FLI")?;
    let credz_fli = load_fli_bytes(disc, "FLI/CREDZ.FLI")?;

    let font_bytes = disc.read("FONT.RAW").context("reading FONT.RAW")?;
    let font = Font::decode(&font_bytes).context("decoding FONT.RAW")?;

    Ok(IntroAssets {
        neo,
        surplogo,
        cover,
        intro_fli,
        fly_fli,
        credz_fli,
        font,
    })
}

/// Decode a full-screen `.BDY` still and its `.PAL` palette.
fn load_still(disc: &DiscImage, body: &str, palette: &str) -> Result<StillImage> {
    let body_bytes = disc.read(body).with_context(|| format!("reading {body}"))?;
    let image = bdy::decode(&body_bytes, Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT))
        .with_context(|| format!("decoding {body}"))?;

    let palette_bytes = disc
        .read(palette)
        .with_context(|| format!("reading {palette}"))?;
    let palette = pal::decode(&palette_bytes).with_context(|| format!("decoding {palette}"))?;

    Ok(StillImage { image, palette })
}

/// Decode the PROTOTYPE cover and squeeze it to the screen. `COVER3.BDY` is a
/// 320x478 image; the original displays its top 320x400 in a Mode X 320x400
/// screen (the CRT squashes it to normal height). We reproduce that on the
/// 320x200 framebuffer by taking every other row of those top 400.
fn load_cover(disc: &DiscImage) -> Result<StillImage> {
    let body_bytes = disc.read("COVER3.BDY").context("reading COVER3.BDY")?;
    let full = bdy::decode(&body_bytes, Dimensions::new(SCREEN_WIDTH, COVER_HEIGHT))
        .context("decoding COVER3.BDY")?;

    let width = SCREEN_WIDTH as usize;
    let mut pixels = Vec::with_capacity((SCREEN_WIDTH * SCREEN_HEIGHT) as usize);

    for row in 0..SCREEN_HEIGHT as usize {
        let start = row * 2 * width;
        pixels.extend_from_slice(&full.pixels[start..start + width]);
    }

    let image = IndexedImage::new(Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT), pixels)
        .expect("squeezed cover matches its dimensions");

    let palette_bytes = disc.read("COVER3.PAL").context("reading COVER3.PAL")?;
    let palette = pal::decode(&palette_bytes).context("decoding COVER3.PAL")?;

    Ok(StillImage { image, palette })
}

/// Read a FLI's bytes and validate its header, so a corrupt file fails at load
/// rather than when its beat plays.
fn load_fli_bytes(disc: &DiscImage, name: &str) -> Result<Vec<u8>> {
    let bytes = disc.read(name).with_context(|| format!("reading {name}"))?;
    Flic::new(&bytes).with_context(|| format!("validating {name} header"))?;
    Ok(bytes)
}

/// Load and decode the high-score screen's assets from the disc image.
pub fn load_highscore_assets(disc: &DiscImage) -> Result<HighscoreAssets> {
    let fli = load_fli_bytes(disc, "FLI/HIGHSCOR.FLI")?;

    let font_bytes = disc.read("FONT2.RAW").context("reading FONT2.RAW")?;
    let font = Font::decode(&font_bytes).context("decoding FONT2.RAW")?;

    Ok(HighscoreAssets { fli, font })
}

/// Synthetic, all-zero menu assets for tests that exercise scene logic without
/// the disc. Visually blank, but the right shapes.
#[cfg(test)]
pub(crate) fn test_menu_assets() -> MenuAssets {
    let background = IndexedImage::new(
        Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT),
        vec![0u8; (SCREEN_WIDTH * SCREEN_HEIGHT) as usize],
    )
    .expect("synthetic background matches its dimensions");
    let font_sheet = vec![0u8; 320 * 62];
    let font = Font::decode(&font_sheet).expect("synthetic font sheet decodes");
    let palette = Palette::from_vga_6bit(&[0u8; 768]).expect("synthetic palette decodes");

    MenuAssets {
        background,
        font,
        palette,
    }
}

/// Synthetic intro assets for tests that exercise the intro's beat logic
/// without the disc. The stills are blank and the FLIs are empty: tests drive
/// the early stills/fades or skip the whole intro, and never reach a FLI beat.
#[cfg(test)]
pub(crate) fn test_intro_assets() -> IntroAssets {
    let still = || {
        let image = IndexedImage::new(
            Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT),
            vec![0u8; (SCREEN_WIDTH * SCREEN_HEIGHT) as usize],
        )
        .expect("synthetic still matches its dimensions");
        let palette = Palette::from_vga_6bit(&[0u8; 768]).expect("synthetic palette decodes");

        StillImage { image, palette }
    };

    let font_sheet = vec![0u8; 320 * 62];
    let font = Font::decode(&font_sheet).expect("synthetic font sheet decodes");

    IntroAssets {
        neo: still(),
        surplogo: still(),
        cover: still(),
        intro_fli: Vec::new(),
        fly_fli: Vec::new(),
        credz_fli: Vec::new(),
        font,
    }
}

/// A blank (all index 0) image of the given size, for synthetic test assets.
#[cfg(test)]
fn blank_image(size: Dimensions) -> IndexedImage {
    IndexedImage::new(size, vec![0u8; size.pixel_count()]).expect("blank image matches its size")
}

/// Synthetic, all-zero HUD assets for tests that exercise HUD/scene logic
/// without the disc. Blank, but the right shapes.
#[cfg(test)]
pub(crate) fn test_hud_assets() -> HudAssets {
    HudAssets {
        palette: Palette::from_vga_6bit(&[0u8; 768]).expect("synthetic palette decodes"),
        panel: blank_image(PANEL_SIZE),
        score_digits: blank_image(SCORE_DIGITS_SIZE),
        number_digits: blank_image(NUMBER_DIGITS_SIZE),
        weapon_bars: blank_image(WEAPON_BARS_SIZE),
        smart_frames: blank_image(SMART_FRAMES_SIZE),
        selector_lights: blank_image(SELECTOR_LIGHTS_SIZE),
        weapon_pods: blank_image(WEAPON_PODS_SIZE),
    }
}

/// Synthetic level assets (blank canyon, blank HUD) for headless scene tests.
#[cfg(test)]
pub(crate) fn test_level_assets() -> LevelAssets {
    LevelAssets {
        background: blank_image(BACKGROUND_SIZE),
        hud: test_hud_assets(),
        overlays: std::array::from_fn(|_| OverlaySprite {
            size: Dimensions::new(1, 1),
            pixels: vec![None],
        }),
        overlay_slide: [[(0, 0); OVERLAY_FRAMES]; WEAPON_COUNT],
    }
}

/// Synthetic high-score assets (empty FLI, blank font) for headless tests.
#[cfg(test)]
pub(crate) fn test_highscore_assets() -> HighscoreAssets {
    let font_sheet = vec![0u8; 320 * 62];
    let font = Font::decode(&font_sheet).expect("synthetic font sheet decodes");

    HighscoreAssets {
        fli: Vec::new(),
        font,
    }
}
