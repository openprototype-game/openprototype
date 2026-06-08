//! `.FLI`: Autodesk Animator animation (intro, credits, cutscenes).
//!
//! These files are the FLI variant of the FLIC format:
//! magic `0xAF11`, 320x200, 8 bits per pixel. Frame 0 is a full keyframe; every
//! later frame is a delta against the one before it, so the decoder keeps a
//! **persistent canvas** and applies each frame's chunks on top. Clearing the
//! canvas between frames corrupts every delta frame.
//!
//! Only three chunk types appear across every file:
//!
//! - `COLOR64` (11): incremental 6-bit palette update.
//! - `BRUN` (15): full-frame byte-run RLE (the keyframe).
//! - `LC` (12): line-compressed delta.
//!
//! `BRUN` and `LC` use **opposite** sign conventions for their packet count
//! byte (run vs literal); see the two `apply_*` functions.

use crate::color::{Palette, Rgb, expand_6bit};
use crate::error::DecodeError;
use crate::{Dimensions, IndexedImage, Result};

const FILE_HEADER_LEN: usize = 128;
const FRAME_HEADER_LEN: usize = 16;
const CHUNK_HEADER_LEN: usize = 6;

const FLI_MAGIC: u16 = 0xAF11;
const FRAME_MAGIC: u16 = 0xF1FA;

const CHUNK_COLOR64: u16 = 11;
const CHUNK_LC: u16 = 12;
const CHUNK_BRUN: u16 = 15;

/// The fixed facts from a FLI file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlicHeader {
    pub width: u32,
    pub height: u32,
    pub frame_count: u16,
    /// Frame delay in 1/70-second jiffies.
    pub speed: u32,
}

/// One decoded frame: the current canvas and palette, plus its delay. Both
/// references borrow the player, so only one frame is live at a time.
pub struct Frame<'a> {
    pub image: &'a IndexedImage,
    pub palette: &'a Palette,
    pub delay_jiffies: u32,
}

/// A streaming FLI player. Holds the persistent canvas and palette and advances
/// one frame per [`Flic::next_frame`] call.
pub struct Flic<'a> {
    data: &'a [u8],
    header: FlicHeader,
    cursor: usize,
    frames_left: u16,
    canvas: IndexedImage,
    palette: Palette,
}

impl<'a> Flic<'a> {
    /// Parse and validate the file header; the canvas starts blank.
    pub fn new(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() < FILE_HEADER_LEN {
            return Err(DecodeError::UnexpectedLength {
                expected: FILE_HEADER_LEN,
                actual: bytes.len(),
            });
        }

        if read_u16(bytes, 4)? != FLI_MAGIC {
            return Err(DecodeError::Unrecognized {
                reason: "not an FLI animation (bad magic)",
            });
        }

        let frame_count = read_u16(bytes, 6)?;
        let width = u32::from(read_u16(bytes, 8)?);
        let height = u32::from(read_u16(bytes, 10)?);
        let speed = u32::from(read_u16(bytes, 16)?);

        let size = Dimensions::new(width, height);
        let canvas = IndexedImage::new(size, vec![0u8; size.pixel_count()])?;

        Ok(Self {
            data: bytes,
            header: FlicHeader {
                width,
                height,
                frame_count,
                speed,
            },
            cursor: FILE_HEADER_LEN,
            frames_left: frame_count,
            canvas,
            palette: Palette {
                colors: [Rgb::default(); 256],
            },
        })
    }

    pub fn header(&self) -> &FlicHeader {
        &self.header
    }

    /// Apply the next frame to the persistent canvas and lend it back. Returns
    /// `None` once every header-declared frame has been produced.
    pub fn next_frame(&mut self) -> Option<Result<Frame<'_>>> {
        if self.frames_left == 0 {
            return None;
        }

        match self.advance() {
            Ok(()) => {
                self.frames_left -= 1;
                Some(Ok(Frame {
                    image: &self.canvas,
                    palette: &self.palette,
                    delay_jiffies: self.header.speed,
                }))
            }
            Err(error) => {
                self.frames_left = 0;
                Some(Err(error))
            }
        }
    }

    /// Apply one frame's chunks, then move the cursor to the next frame.
    fn advance(&mut self) -> Result<()> {
        let data = self.data;
        let frame_start = self.cursor;

        let frame_size = read_u32(data, frame_start)? as usize;

        if read_u16(data, frame_start + 4)? != FRAME_MAGIC {
            return Err(DecodeError::Unrecognized {
                reason: "bad frame magic",
            });
        }

        let chunk_count = read_u16(data, frame_start + 6)?;
        let frame_end = frame_start
            .checked_add(frame_size)
            .filter(|end| *end <= data.len())
            .ok_or(DecodeError::TruncatedRun)?;

        let mut chunk_at = frame_start + FRAME_HEADER_LEN;

        for _ in 0..chunk_count {
            let chunk_size = read_u32(data, chunk_at)? as usize;
            let chunk_type = read_u16(data, chunk_at + 4)?;

            let body_start = chunk_at + CHUNK_HEADER_LEN;
            let body_end = chunk_at
                .checked_add(chunk_size.max(CHUNK_HEADER_LEN))
                .filter(|end| *end <= frame_end)
                .ok_or(DecodeError::TruncatedRun)?;
            let body = &data[body_start..body_end];

            match chunk_type {
                CHUNK_COLOR64 => apply_color64(&mut self.palette, body)?,
                CHUNK_BRUN => apply_brun(&mut self.canvas, body)?,
                CHUNK_LC => apply_lc(&mut self.canvas, body)?,
                _ => {} // no other types appear in these files
            }

            chunk_at = body_end;
        }

        self.cursor = frame_end;

        Ok(())
    }
}

/// `COLOR64`: `u16` packet count, then `(skip, count)` packets of 6-bit RGB.
/// `count == 0` means 256. Indices advance by `skip` then by each color written.
fn apply_color64(palette: &mut Palette, data: &[u8]) -> Result<()> {
    let packet_count = read_u16(data, 0)?;
    let mut at = 2;
    let mut index = 0usize;

    for _ in 0..packet_count {
        index += usize::from(read_u8(data, at)?);
        let count = match read_u8(data, at + 1)? {
            0 => 256,
            n => usize::from(n),
        };
        at += 2;

        for _ in 0..count {
            let entry = palette
                .colors
                .get_mut(index)
                .ok_or(DecodeError::TruncatedRun)?;
            *entry = Rgb {
                r: expand_6bit(read_u8(data, at)?),
                g: expand_6bit(read_u8(data, at + 1)?),
                b: expand_6bit(read_u8(data, at + 2)?),
            };
            at += 3;
            index += 1;
        }
    }

    Ok(())
}

/// `BRUN`: a full frame. Per line, a leading packet count (ignored; we fill to
/// width) then packets whose signed count is **positive → run** of the next
/// byte, **negative → literal** copy.
fn apply_brun(canvas: &mut IndexedImage, data: &[u8]) -> Result<()> {
    let width = canvas.size.width as usize;
    let height = canvas.size.height as usize;
    let mut at = 0;

    for y in 0..height {
        let row = y * width;
        let mut x = 0usize;
        at += 1; // per-line packet count, ignored

        while x < width {
            let count = read_u8(data, at)? as i8;
            at += 1;

            if count >= 0 {
                let value = read_u8(data, at)?;
                at += 1;
                fill(canvas, row, &mut x, width, count as usize, value)?;
            } else {
                let literal = count.unsigned_abs() as usize;

                for _ in 0..literal {
                    let value = read_u8(data, at)?;
                    at += 1;
                    put(canvas, row, &mut x, width, value)?;
                }
            }
        }
    }

    Ok(())
}

/// `LC`: a delta. `u16 first_line`, `u16 num_lines`, then per line a packet
/// count and packets of `(skip, count)`. Sign is the **opposite** of `BRUN`:
/// **positive → literal** copy, **negative → run** of the next byte. Untouched
/// lines and skipped pixels keep their previous-frame values.
fn apply_lc(canvas: &mut IndexedImage, data: &[u8]) -> Result<()> {
    let width = canvas.size.width as usize;
    let first_line = usize::from(read_u16(data, 0)?);
    let num_lines = usize::from(read_u16(data, 2)?);
    let mut at = 4;

    for line in 0..num_lines {
        let row = (first_line + line) * width;
        let mut x = 0usize;
        let packets = read_u8(data, at)?;
        at += 1;

        for _ in 0..packets {
            x += usize::from(read_u8(data, at)?);
            let count = read_u8(data, at + 1)? as i8;
            at += 2;

            if count >= 0 {
                for _ in 0..count as usize {
                    let value = read_u8(data, at)?;
                    at += 1;
                    put(canvas, row, &mut x, width, value)?;
                }
            } else {
                let run = count.unsigned_abs() as usize;
                let value = read_u8(data, at)?;
                at += 1;
                fill(canvas, row, &mut x, width, run, value)?;
            }
        }
    }

    Ok(())
}

/// Write `value` at `row + x`, advancing `x`. Errors if the pixel is past width.
fn put(
    canvas: &mut IndexedImage,
    row: usize,
    x: &mut usize,
    width: usize,
    value: u8,
) -> Result<()> {
    if *x >= width {
        return Err(DecodeError::TruncatedRun);
    }

    canvas.pixels[row + *x] = value;
    *x += 1;
    Ok(())
}

/// Write `value` into the next `count` pixels of the row.
fn fill(
    canvas: &mut IndexedImage,
    row: usize,
    x: &mut usize,
    width: usize,
    count: usize,
    value: u8,
) -> Result<()> {
    for _ in 0..count {
        put(canvas, row, x, width, value)?;
    }

    Ok(())
}

fn read_u8(data: &[u8], pos: usize) -> Result<u8> {
    data.get(pos).copied().ok_or(DecodeError::TruncatedRun)
}

fn read_u16(data: &[u8], pos: usize) -> Result<u16> {
    let bytes = data.get(pos..pos + 2).ok_or(DecodeError::TruncatedRun)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], pos: usize) -> Result<u32> {
    let bytes = data.get(pos..pos + 4).ok_or(DecodeError::TruncatedRun)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(width: u32, height: u32, fill: u8) -> IndexedImage {
        let size = Dimensions::new(width, height);
        IndexedImage::new(size, vec![fill; size.pixel_count()]).unwrap()
    }

    #[test]
    fn color64_applies_skip_and_count() {
        let mut palette = Palette {
            colors: [Rgb::default(); 256],
        };
        // 1 packet: skip 2, change 1 color, 6-bit pure red.
        let data = [0x01, 0x00, 0x02, 0x01, 63, 0, 0];

        apply_color64(&mut palette, &data).unwrap();

        assert_eq!(palette.colors[1], Rgb::default());
        assert_eq!(palette.colors[2], Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(palette.colors[3], Rgb::default());
    }

    #[test]
    fn brun_fills_a_line_with_run_then_literal() {
        let mut image = canvas(4, 1, 0);
        // leading count (ignored), run of 3x9, then literal of one byte 5.
        let data = [0x02, 0x03, 0x09, 0xFF, 0x05];

        apply_brun(&mut image, &data).unwrap();

        assert_eq!(image.pixels, vec![9, 9, 9, 5]);
    }

    #[test]
    fn lc_updates_touched_lines_and_keeps_the_rest() {
        let mut image = canvas(4, 3, 7);
        // first_line 1, 2 lines.
        // row 1: skip 1, literal [3,4]  -> 7 3 4 7
        // row 2: skip 0, run of 2x8     -> 8 8 7 7
        let data = [
            0x01, 0x00, // first_line
            0x02, 0x00, // num_lines
            0x01, 0x01, 0x02, 0x03, 0x04, // row 1 packet
            0x01, 0x00, 0xFE, 0x08, // row 2 packet (0xFE = -2)
        ];

        apply_lc(&mut image, &data).unwrap();

        assert_eq!(&image.pixels[0..4], &[7, 7, 7, 7]); // untouched line persists
        assert_eq!(&image.pixels[4..8], &[7, 3, 4, 7]); // skip + literal
        assert_eq!(&image.pixels[8..12], &[8, 8, 7, 7]); // run + tail persists
    }
}
