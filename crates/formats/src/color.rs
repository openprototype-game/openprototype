//! Colours and palettes.

use crate::error::{DecodeError, Result};

/// An 8-bit-per-channel RGB colour, ready for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// A 256-entry VGA palette with display-ready 8-bit channels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub colors: [Rgb; 256],
}

impl Palette {
    /// Build from a raw `.PAL` dump: 768 bytes of 6-bit VGA DAC values
    /// (0..=63), expanded to 8 bits by bit replication.
    pub fn from_vga_6bit(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 768 {
            return Err(DecodeError::UnexpectedLength {
                expected: 768,
                actual: bytes.len(),
            });
        }

        let mut colors = [Rgb::default(); 256];

        for (index, channels) in bytes.chunks_exact(3).enumerate() {
            colors[index] = Rgb {
                r: expand_6bit(channels[0]),
                g: expand_6bit(channels[1]),
                b: expand_6bit(channels[2]),
            };
        }

        Ok(Self { colors })
    }
}

/// Map a 6-bit DAC value (0..=63) to the full 8-bit range.
fn expand_6bit(value: u8) -> u8 {
    let clamped = value & 0x3f;
    (clamped << 2) | (clamped >> 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_6bit_endpoints_to_full_range() {
        assert_eq!(expand_6bit(0), 0);
        assert_eq!(expand_6bit(63), 255);
    }

    #[test]
    fn expands_6bit_midpoint_by_replication() {
        // (32 << 2) | (32 >> 4) == 128 | 2
        assert_eq!(expand_6bit(32), 130);
    }

    #[test]
    fn rejects_palette_of_wrong_length() {
        let error = Palette::from_vga_6bit(&[0; 767]).unwrap_err();
        assert_eq!(
            error,
            DecodeError::UnexpectedLength {
                expected: 768,
                actual: 767,
            }
        );
    }

    #[test]
    fn maps_entries_in_file_order() {
        let mut bytes = vec![0u8; 768];
        bytes[3] = 63; // colour 1, red
        bytes[5] = 32; // colour 1, blue
        let palette = Palette::from_vga_6bit(&bytes).unwrap();

        assert_eq!(palette.colors[0], Rgb { r: 0, g: 0, b: 0 });
        assert_eq!(
            palette.colors[1],
            Rgb {
                r: 255,
                g: 0,
                b: 130
            }
        );
    }
}
