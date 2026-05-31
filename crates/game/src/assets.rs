//! Loading game assets from the original disc.
//!
//! The front-end reads its graphics from the CD: `BACK3.RAW` (the menu
//! background), `FONT.RAW` (the glyph sheet) and the menu palette baked into
//! `START.EXE`. This module turns those raw bytes into decoded values the core
//! scenes consume. It depends on the disc reader but on nothing graphical.

use anyhow::{Context, Result};
use prototype_disc::{AssetSource, DiscImage};
use prototype_formats::font::Font;
use prototype_formats::{Dimensions, IndexedImage, Palette, StartExe, raw};

use crate::core::framebuffer::{SCREEN_HEIGHT, SCREEN_WIDTH};

/// Everything the main menu needs to render.
pub struct MenuAssets {
    pub background: IndexedImage,
    pub font: Font,
    pub palette: Palette,
}

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
