//! `.PAL`: VGA color palette.
//!
//! 256 colors, 3 bytes each (R, G, B) = 768 bytes, no header. Components are
//! 6-bit VGA DAC values (0..=63). Paired with a `.BDY` image of the same name.

use crate::Result;
use crate::color::Palette;

/// Decode a `.PAL` file into a display-ready palette.
pub fn decode(bytes: &[u8]) -> Result<Palette> {
    Palette::from_vga_6bit(bytes)
}
