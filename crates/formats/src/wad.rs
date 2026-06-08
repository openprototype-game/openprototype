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
/// A 30-step gray ramp follows, but black-then-white already pins the block in
/// exactly one place per WAD.
const SIGNATURE: [u8; 6] = [0x00, 0x00, 0x00, MAX_DAC, MAX_DAC, MAX_DAC];

/// Extracts the level's embedded 256-color palette from a `.WAD`.
///
/// The palette is a raw 768-byte block of 6-bit VGA DAC values (see
/// [`Palette::from_vga_6bit`]) compiled in at a per-level offset that follows no
/// rule, so it is located by signature: the block opens black-then-white
/// ([`SIGNATURE`]) and every byte is a valid DAC value (`<= 0x3f`). That matches
/// in exactly one place per WAD. See `reference/formats/wad.md`.
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
