//! `.WAD`: level files.
//!
//! These are DOS EXE images (per the developer): the level code with assets
//! and an embedded palette compiled in or appended. `LEVEL_1.WAD`..`LEVEL_7.WAD`.

use crate::color::Palette;
use crate::error::{DecodeError, Result};

/// A VGA DAC channel is 6-bit, so every palette byte is at most `0x3f`.
const MAX_DAC: u8 = 0x3f;

/// Bytes in a 256-color VGA palette (`256 * 3`).
const PALETTE_LEN: usize = 768;

/// The bytes a palette block opens with: color 0 is black, color 1 is white.
///
/// A 30-step gray ramp follows, but black-then-white already pins the block
/// in exactly one place per WAD.
const SIGNATURE: [u8; 6] = [0x00, 0x00, 0x00, MAX_DAC, MAX_DAC, MAX_DAC];

/// Extracts the embedded 256-color palette at a known offset in a `.WAD`.
///
/// The palette is a raw 768-byte block of 6-bit VGA DAC values (see
/// [`Palette::from_vga_6bit`]). The per-WAD offsets are tabulated in
/// `reference/formats/wad.md`; the game keeps them in its level data. Use
/// [`level_palette`] only when the offset is unknown (a generic tool over an
/// arbitrary WAD).
///
/// # Errors
///
/// Returns [`DecodeError::Unrecognized`] when the block runs past the end of
/// the WAD or holds a non-DAC byte (`> 0x3f`).
pub fn palette_at(wad: &[u8], offset: usize) -> Result<Palette> {
    let block = wad
        .get(offset..offset + PALETTE_LEN)
        .filter(|window| window.iter().all(|&byte| byte <= MAX_DAC))
        .ok_or(DecodeError::Unrecognized {
            reason: "no valid palette block at the given WAD offset",
        })?;

    Palette::from_vga_6bit(block)
}

/// Locates the embedded palette by signature, when the offset is not known.
///
/// The palette's offset follows no rule across WADs, so a caller without a
/// known offset scans for the block: it opens black-then-white and every byte
/// is a valid DAC value (`<= 0x3f`), which matches in exactly one place per
/// WAD. The game passes the known per-level offset to [`palette_at`] instead;
/// this scan is for generic tools over an arbitrary WAD. See
/// `reference/formats/wad.md`.
///
/// # Errors
///
/// Returns [`DecodeError::Unrecognized`] when no palette block is found.
pub fn level_palette(wad: &[u8]) -> Result<Palette> {
    for start in 0..=wad.len().saturating_sub(PALETTE_LEN) {
        if wad[start..start + SIGNATURE.len()] != SIGNATURE {
            continue;
        }

        let window = &wad[start..start + PALETTE_LEN];

        if window.iter().all(|&byte| byte <= MAX_DAC) {
            return Palette::from_vga_6bit(window);
        }
    }

    Err(DecodeError::Unrecognized {
        reason: "no embedded palette found in WAD",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 768-byte palette block: the black-then-white signature, then a ramp.
    fn palette_bytes() -> Vec<u8> {
        let mut bytes = SIGNATURE.to_vec();
        bytes.extend((SIGNATURE.len()..PALETTE_LEN).map(|index| (index % 0x40) as u8));
        bytes
    }

    #[test]
    fn finds_the_palette_by_its_signature() {
        let mut wad = vec![0xff, 0x80]; // arbitrary bytes precede the block
        wad.extend(palette_bytes());
        wad.extend([0u8; 16]); // trailing padding

        let palette = level_palette(&wad).unwrap();
        assert_eq!(palette, Palette::from_vga_6bit(&palette_bytes()).unwrap());
    }

    #[test]
    fn reads_the_palette_at_a_known_offset() {
        let mut wad = vec![0xffu8; 5]; // arbitrary prefix
        wad.extend(palette_bytes());

        let palette = palette_at(&wad, 5).unwrap();
        assert_eq!(palette, Palette::from_vga_6bit(&palette_bytes()).unwrap());
    }

    #[test]
    fn rejects_a_known_offset_that_overruns_or_holds_a_non_dac_byte() {
        let wad = palette_bytes();
        assert!(palette_at(&wad, wad.len() - 10).is_err());

        let mut bad = palette_bytes();
        bad[400] = 0xff;
        assert!(palette_at(&bad, 0).is_err());
    }

    #[test]
    fn ignores_a_dac_run_without_the_signature() {
        let mut wad = vec![0xffu8];
        wad.extend((0..PALETTE_LEN).map(|i| (i % 0x40) as u8)); // all <= 0x3f, no signature
        wad.extend(palette_bytes()); // the real block follows

        let palette = level_palette(&wad).unwrap();
        assert_eq!(palette, Palette::from_vga_6bit(&palette_bytes()).unwrap());
    }

    #[test]
    fn rejects_a_signature_with_a_non_dac_byte_in_the_block() {
        let mut block = palette_bytes();
        block[400] = 0xff; // breaks the all-DAC requirement
        let mut wad = vec![0xffu8];
        wad.extend(block);

        assert_eq!(
            level_palette(&wad).unwrap_err(),
            DecodeError::Unrecognized {
                reason: "no embedded palette found in WAD",
            }
        );
    }
}
