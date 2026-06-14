//! `.BDY`: IFF ILBM BODY chunk, RLE-compressed (ByteRun1 / PackBits).
//!
//! Just the BODY payload, not a full IFF FORM, so there is no header giving
//! dimensions: the caller supplies them. The decompressed bytes are chunky 8bpp
//! (one palette index per pixel).
//! Source: developer mail (Erik Pojar).

use crate::error::DecodeError;
use crate::{Dimensions, IndexedImage, Result};

/// Decodes an RLE BODY chunk into an indexed image of the given dimensions.
pub fn decode(bytes: &[u8], size: Dimensions) -> Result<IndexedImage> {
    IndexedImage::new(size, unpack_byte_run1(bytes)?)
}

/// ByteRun1 decompression.
///
/// Each control byte is read as a signed value:
/// - `0..=127`: copy the next `n + 1` bytes literally.
/// - `-127..=-1`: repeat the next byte `1 - n` times.
/// - `-128`: no-op (the PackBits convention).
///
/// The original's unpacker (START.EXE file `0x3104`) instead computes
/// `0x101 - control` for every control `> 0x7f`, so it treats `0x80` as a
/// 129-byte run rather than a no-op. No shipped `.BDY` contains a `0x80`
/// control, so the decoded output is identical either way; the port keeps
/// the standard PackBits reading.
fn unpack_byte_run1(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut input = bytes.iter().copied();

    while let Some(control) = input.next() {
        match control as i8 {
            count @ 0..=127 => {
                for _ in 0..=count {
                    out.push(input.next().ok_or(DecodeError::TruncatedRun)?);
                }
            }
            count @ -127..=-1 => {
                let value = input.next().ok_or(DecodeError::TruncatedRun)?;
                out.extend(std::iter::repeat_n(value, (1 - count as i32) as usize));
            }
            -128 => {}
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_literal_runs() {
        assert_eq!(
            unpack_byte_run1(&[0x02, 0xAA, 0xBB, 0xCC]).unwrap(),
            vec![0xAA, 0xBB, 0xCC]
        );
    }

    #[test]
    fn expands_replicate_runs() {
        // -1 as control => repeat next byte 2 times.
        assert_eq!(unpack_byte_run1(&[0xFF, 0xAA]).unwrap(), vec![0xAA, 0xAA]);
    }

    #[test]
    fn skips_noop_control() {
        assert_eq!(unpack_byte_run1(&[0x80]).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn errors_on_run_past_end() {
        assert_eq!(
            unpack_byte_run1(&[0x02, 0xAA]).unwrap_err(),
            DecodeError::TruncatedRun
        );
    }
}
