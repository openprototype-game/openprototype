//! `START.EXE`: typed access to data baked into the menu front-end.
//!
//! `START.EXE` is a DOS MZ executable. Its loaded image begins at file offset
//! [`HEADER_SIZE`] (the MZ header), and the disassembly's data offsets are
//! image-relative, so `file = HEADER_SIZE + image`. This module strips the
//! header once and works in image-offset space, so its constants match the r2
//! disassembly directly.
//!
//! Only binary blobs that are painful to transcribe by hand are read here. The
//! menu palette is 768 numeric DAC values; hand-typing it as a Rust constant
//! would be an error magnet. Short textual/structural data (menu labels, level
//! filenames) is owned by the port in Rust instead, because it needs
//! case/locale adaptation the DOS data does not carry. For that data,
//! `START.EXE` stays the reverse-engineering reference, not the runtime source.

use crate::Result;
use crate::color::Palette;
use crate::error::DecodeError;

/// Size of the MZ header; the loaded image starts here.
const HEADER_SIZE: usize = 0x200;

/// Image offset of the 256-color menu palette (768 bytes, 6-bit VGA).
const MENU_PALETTE_OFFSET: usize = 0x5130;
const PALETTE_LEN: usize = 768;

/// Image offset of the menu string table.
///
/// Its first entry is `NEW GAME`, used here only as a version guard.
const MENU_ANCHOR_OFFSET: usize = 0x30f;
const MENU_ANCHOR: &[u8] = b"NEW GAME";

/// A validated view over `START.EXE`'s loaded image.
pub struct StartExe<'a> {
    image: &'a [u8],
}

impl<'a> StartExe<'a> {
    /// Validates `bytes` (the raw `START.EXE` file) and borrows its image.
    ///
    /// Checks the `MZ` magic and that the menu string table sits at its known
    /// offset, so a wrong file or a different build is rejected rather than
    /// read as garbage.
    pub fn new(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() < HEADER_SIZE {
            return Err(DecodeError::UnexpectedLength {
                expected: HEADER_SIZE,
                actual: bytes.len(),
            });
        }

        if &bytes[0..2] != b"MZ" {
            return Err(DecodeError::Unrecognized {
                reason: "not an MZ executable",
            });
        }

        let image = &bytes[HEADER_SIZE..];

        let anchor = image.get(MENU_ANCHOR_OFFSET..MENU_ANCHOR_OFFSET + MENU_ANCHOR.len());

        if anchor != Some(MENU_ANCHOR) {
            return Err(DecodeError::Unrecognized {
                reason: "menu string table not at the expected offset",
            });
        }

        Ok(Self { image })
    }

    /// Decodes the menu palette.
    ///
    /// The 256 colors are uploaded to the DAC before the menu loop: index 0 is
    /// black, index 1 white, then a gray-to-rust ramp.
    pub fn menu_palette(&self) -> Result<Palette> {
        let bytes = self
            .image
            .get(MENU_PALETTE_OFFSET..MENU_PALETTE_OFFSET + PALETTE_LEN)
            .ok_or(DecodeError::Unrecognized {
                reason: "menu palette past end of image",
            })?;

        Palette::from_vga_6bit(bytes)
    }
}
