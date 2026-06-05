//! Loading game assets from the original disc.
//!
//! The front-end reads its graphics from the CD: `BACK3.RAW` (the menu
//! background), `FONT.RAW` (the glyph sheet) and the menu palette baked into
//! `START.EXE`. This module turns those raw bytes into decoded values the
//! scenes and the audio backend consume. It depends on the disc reader but on
//! nothing graphical and nothing audio-device specific.

use anyhow::{Context, Result};
use prototype_disc::{AssetSource, DiscImage};
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
}

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

    Ok(HudAssets {
        palette,
        panel,
        score_digits,
        number_digits,
        weapon_bars,
    })
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
