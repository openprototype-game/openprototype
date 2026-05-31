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

/// Read a CD-DA audio track as normalized interleaved stereo `f32` samples,
/// ready to hand to the audio backend. `cd_track` is the red-book track number
/// (track 1 is data; the music is tracks 2..=8). The on-disc PCM is 44100 Hz,
/// 16-bit stereo, little-endian.
pub fn load_track_pcm_f32(disc: &DiscImage, cd_track: u8) -> Result<Vec<f32>> {
    let track = disc
        .audio_tracks()
        .iter()
        .find(|track| track.number == cd_track)
        .with_context(|| format!("disc has no audio track {cd_track}"))?;

    let pcm = disc
        .read_track_pcm(track)
        .with_context(|| format!("reading audio track {cd_track}"))?;

    Ok(pcm_i16_le_to_f32(&pcm))
}

/// Convert raw little-endian 16-bit PCM into `f32` in `[-1.0, 1.0)`. A trailing
/// odd byte (never expected in red-book audio) is dropped.
fn pcm_i16_le_to_f32(bytes: &[u8]) -> Vec<f32> {
    let mut out = Vec::with_capacity(bytes.len() / 2);
    append_pcm_i16_le_as_f32(bytes, &mut out);
    out
}

/// Decode `bytes` (little-endian 16-bit PCM) into `out`, normalized to
/// `[-1.0, 1.0)`. Lets the streaming source refill its buffer without
/// reallocating. A trailing odd byte is dropped.
pub(crate) fn append_pcm_i16_le_as_f32(bytes: &[u8], out: &mut Vec<f32>) {
    out.extend(
        bytes
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32768.0),
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_i16_le_to_normalized_f32() {
        // 0, i16::MIN, i16::MAX, little-endian.
        let bytes = [0x00, 0x00, 0x00, 0x80, 0xff, 0x7f];
        let samples = pcm_i16_le_to_f32(&bytes);

        assert_eq!(samples[0], 0.0);
        assert_eq!(samples[1], -1.0);
        assert!((samples[2] - 0.999_969).abs() < 1e-4);
    }

    #[test]
    fn drops_trailing_odd_byte() {
        assert_eq!(pcm_i16_le_to_f32(&[0x00, 0x00, 0x7f]).len(), 1);
    }
}
