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
