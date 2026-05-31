//! `.BIN` / `.BN1`: compiled sprites.
//!
//! The per-level `.BIN` files and the shared `PTURN1.BN1` hold short x86
//! subroutines that write palette indices into VGA Mode X memory. Grouping the
//! four plane-subroutines into a sprite lives in a catalog inside the level
//! `.WAD`, not the BIN. See `reference/formats/bin.md` for the full layout.
//!
//! Two catalog shapes exist, with one decoder each:
//! - [`decode_banked`] for the level scenery BINs (e.g. `OUT.BIN`): 10-byte
//!   records `[field0, p0..p3]`, where `field0` is an EMS logical page and each
//!   plane pointer is either a clip-header or a direct subroutine offset.
//! - [`decode_ship`] for `PTURN1.BN1`: 8-byte cell records `[p0..p3]` grouped
//!   into frames by a trailing cell count, all direct, in a single segment.
//!
//! Decoding is palette-independent: a [`Sprite`] holds palette indices, with
//! transparent pixels left unwritten.

use std::collections::BTreeSet;

use crate::error::{DecodeError, Result};
use crate::image::Dimensions;

/// File offset of `OUT.BIN`'s sprite catalog in `LEVEL_1.WAD`.
pub const OUT_BIN_CATALOG: usize = 0xf9f0;
/// File offset of the player-ship frame catalog in `LEVEL_1.WAD` (segment
/// `0x0F7F` is the r2 vaddr `0x6bdc`; the on-disk offset is `+0x200`).
pub const PTURN1_CATALOG: usize = 0x6ddc;

/// Mode X plane-buffer row stride: 320 columns / 4 planes.
const PLANE_STRIDE: i64 = 80;
/// EMS logical page size; `field0 * EMS_PAGE` is a record's base in the BIN.
const EMS_PAGE: usize = 0x4000;
/// Each draw cell is 32 screen columns wide.
const CELL_WIDTH: i64 = 32;
/// A clip-header's first word is a small forward offset; larger values mean the
/// plane pointer is a direct subroutine instead.
const MAX_HEADER_OFFSET: usize = 0x800;

/// One decoded sprite as a paletted bitmap with transparency.
///
/// `pixels` has `size.pixel_count()` entries in row-major order; `None` is a
/// transparent pixel (the format only emits the opaque ones).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    pub size: Dimensions,
    pub pixels: Vec<Option<u8>>,
}

/// All sprites of one BIN, in catalog order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteSheet {
    pub sprites: Vec<Sprite>,
}

/// One opaque pixel written by a plane subroutine, in plane-buffer coordinates.
struct PlaneWrite {
    column: i64,
    row: i64,
    value: u8,
}

/// One absolute pixel after interleaving planes (and cells) onto the screen.
struct Pixel {
    x: i64,
    y: i64,
    value: u8,
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let pair = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([pair[0], pair[1]]))
}

/// Outcome of stepping over one compiled-sprite instruction.
enum Step {
    /// Advanced to the next instruction at this index.
    Next(usize),
    /// Hit `RETF`: the subroutine ends here.
    Done,
}

/// Step over the instruction at `index`, emitting any pixel writes into `out`.
///
/// `si`/`ax` are the running destination pointer and row stride. Returns `None`
/// on an unknown opcode or a read past the end (i.e. not a valid subroutine).
fn step_instruction(
    bin: &[u8],
    index: usize,
    si: &mut i64,
    ax: &mut i64,
    out: &mut Vec<PlaneWrite>,
) -> Option<Step> {
    let opcode = *bin.get(index)?;

    match opcode {
        0xCB => Some(Step::Done),
        0x03 if bin.get(index + 1) == Some(&0xF0) => {
            *si += *ax;
            Some(Step::Next(index + 2))
        }
        0x81 if bin.get(index + 1) == Some(&0xC6) => {
            *si += i64::from(read_u16(bin, index + 2)? as i16);
            Some(Step::Next(index + 4))
        }
        0xB8 => {
            *ax = i64::from(read_u16(bin, index + 1)?);
            Some(Step::Next(index + 3))
        }
        0xC6 | 0xC7 | 0x66 => {
            let modrm_index = index + if opcode == 0x66 { 2 } else { 1 };
            let (displacement, displacement_len) = decode_displacement(bin, modrm_index)?;
            let immediate_len = match opcode {
                0xC7 => 2,
                0x66 => 4,
                _ => 1,
            };
            let immediate_index = modrm_index + 1 + displacement_len;

            for byte in 0..immediate_len {
                let value = *bin.get(immediate_index + byte)?;
                write_pixel(*si + displacement + byte as i64, value, out);
            }

            Some(Step::Next(immediate_index + immediate_len))
        }
        _ => None,
    }
}

/// Decode the displacement of a `mov [si+disp], imm` from its ModR/M byte:
/// `0x44` = disp8, `0x84` = disp16, anything else (`0x04`) = `[si]`, no disp.
fn decode_displacement(bin: &[u8], modrm_index: usize) -> Option<(i64, usize)> {
    match bin.get(modrm_index)? {
        0x44 => Some((i64::from(*bin.get(modrm_index + 1)? as i8), 1)),
        0x84 => Some((i64::from(read_u16(bin, modrm_index + 1)? as i16), 2)),
        _ => Some((0, 0)),
    }
}

fn write_pixel(offset: i64, value: u8, out: &mut Vec<PlaneWrite>) {
    out.push(PlaneWrite {
        column: offset.rem_euclid(PLANE_STRIDE),
        row: offset.div_euclid(PLANE_STRIDE),
        value,
    });
}

/// Run the plane subroutine at `offset`, returning its pixel writes, or `None`
/// if it is not a valid `RETF`-terminated subroutine.
fn run_subroutine(bin: &[u8], offset: usize) -> Option<Vec<PlaneWrite>> {
    let mut writes = Vec::new();
    let mut index = offset;
    let mut si = 0i64;
    let mut ax = PLANE_STRIDE;

    loop {
        match step_instruction(bin, index, &mut si, &mut ax, &mut writes)? {
            Step::Next(next) => index = next,
            Step::Done => return Some(writes),
        }
    }
}

/// Resolve a banked plane pointer to its subroutine writes. The pointer is
/// either a clip-header (`hp + u16(bin[hp])`) for small clippable sprites, or a
/// direct subroutine (`hp`) for large scenery; the data is self-describing.
fn resolve_plane(bin: &[u8], base: usize, plane_pointer: usize) -> Option<Vec<PlaneWrite>> {
    let header_pointer = base.checked_add(plane_pointer)?;
    let word0 = read_u16(bin, header_pointer)? as usize;

    let header = (1..MAX_HEADER_OFFSET)
        .contains(&word0)
        .then(|| run_subroutine(bin, header_pointer + word0))
        .flatten();
    let direct = run_subroutine(bin, header_pointer);

    // Both interpretations can run to RETF; prefer the one that actually draws
    // pixels (a header pointing at an empty sub otherwise wins over a direct
    // sprite that has content). An all-empty result is a genuine blank entry.
    match (header, direct) {
        (Some(header), Some(direct)) => {
            if header.is_empty() && !direct.is_empty() {
                Some(direct)
            } else {
                Some(header)
            }
        }
        (Some(header), None) => Some(header),
        (None, direct) => direct,
    }
}

/// Interleave plane writes onto the screen and crop to the bounding box. Plane
/// `p` owns the columns where `x % 4 == p`; `cell` shifts a wide sprite right by
/// 32 columns per cell.
fn assemble(planes: &[(usize, Vec<PlaneWrite>)]) -> Sprite {
    let mut pixels = Vec::new();

    for &(plane_and_cell, ref writes) in planes {
        let cell = (plane_and_cell as i64) / 4;
        let plane = (plane_and_cell as i64) % 4;

        for write in writes {
            pixels.push(Pixel {
                x: cell * CELL_WIDTH + write.column * 4 + plane,
                y: write.row,
                value: write.value,
            });
        }
    }

    build_sprite(&pixels)
}

/// Crop the absolute pixels to their bounding box. A record whose four planes
/// draw nothing is a genuine blank catalog slot, returned as a `0x0` sprite.
fn build_sprite(pixels: &[Pixel]) -> Sprite {
    if pixels.is_empty() {
        return Sprite {
            size: Dimensions::new(0, 0),
            pixels: Vec::new(),
        };
    }

    let min_x = pixels.iter().map(|pixel| pixel.x).min().expect("non-empty");
    let min_y = pixels.iter().map(|pixel| pixel.y).min().expect("non-empty");
    let max_x = pixels.iter().map(|pixel| pixel.x).max().expect("non-empty");
    let max_y = pixels.iter().map(|pixel| pixel.y).max().expect("non-empty");

    let size = Dimensions::new((max_x - min_x + 1) as u32, (max_y - min_y + 1) as u32);
    let mut buffer = vec![None; size.pixel_count()];

    for pixel in pixels {
        let local_x = (pixel.x - min_x) as usize;
        let local_y = (pixel.y - min_y) as usize;
        buffer[local_y * size.width as usize + local_x] = Some(pixel.value);
    }

    Sprite {
        size,
        pixels: buffer,
    }
}

/// Decode a banked scenery BIN (e.g. `OUT.BIN`) against its `.WAD` catalog.
///
/// `catalog_offset` is the table position in `wad` (see [`OUT_BIN_CATALOG`]).
/// Records are read until one fails to resolve, which is the table's natural
/// end; the boundary terminator record after the last sprite stops the walk.
pub fn decode_banked(bin: &[u8], wad: &[u8], catalog_offset: usize) -> Result<SpriteSheet> {
    let mut sprites = Vec::new();
    let mut record_offset = catalog_offset;

    while record_offset + 10 <= wad.len() {
        let field0 = read_u16(wad, record_offset).expect("bounds checked") as usize;
        let base = field0 * EMS_PAGE;

        if base >= bin.len() {
            break;
        }

        let mut planes = Vec::with_capacity(4);

        for plane in 0..4 {
            let plane_pointer =
                read_u16(wad, record_offset + 2 + plane * 2).expect("bounds checked") as usize;

            match resolve_plane(bin, base, plane_pointer) {
                Some(writes) => planes.push((plane, writes)),
                None => return finish_banked(sprites),
            }
        }

        sprites.push(assemble(&planes));
        record_offset += 10;
    }

    finish_banked(sprites)
}

fn finish_banked(sprites: Vec<Sprite>) -> Result<SpriteSheet> {
    if sprites.is_empty() {
        return Err(DecodeError::MalformedSprite {
            reason: "no sprites at catalog offset",
        });
    }

    Ok(SpriteSheet { sprites })
}

/// Walk `bin` as a stream of compiled subroutines, returning every subroutine
/// start offset. Stops at the first byte that is not valid subroutine code.
fn subroutine_starts(bin: &[u8]) -> BTreeSet<usize> {
    let mut starts = BTreeSet::new();
    let mut index = 0;
    let mut start = 0;
    let mut si = 0i64;
    let mut ax = PLANE_STRIDE;
    let mut scratch = Vec::new();

    while index < bin.len() {
        match step_instruction(bin, index, &mut si, &mut ax, &mut scratch) {
            Some(Step::Next(next)) => index = next,
            Some(Step::Done) => {
                starts.insert(start);
                index += 1;
                start = index;
            }
            None => break,
        }

        scratch.clear();
    }

    starts
}

/// Decode `PTURN1.BN1` against its `.WAD` frame catalog.
///
/// Frames are flat runs of 8-byte cell records `[p0..p3]` (direct subroutine
/// offsets, one per Mode X plane) terminated by a cell-count word. A frame is
/// `ncells` cells wide (32 columns each).
pub fn decode_ship(bin: &[u8], wad: &[u8], catalog_offset: usize) -> Result<SpriteSheet> {
    let starts = subroutine_starts(bin);
    let is_start = |word: u16| starts.contains(&(word as usize));

    let mut sprites = Vec::new();
    let mut offset = catalog_offset;

    loop {
        let mut planes = Vec::new();
        let mut cell = 0usize;

        while offset + 8 <= wad.len() {
            let cell_pointers = [
                read_u16(wad, offset).expect("bounds checked"),
                read_u16(wad, offset + 2).expect("bounds checked"),
                read_u16(wad, offset + 4).expect("bounds checked"),
                read_u16(wad, offset + 6).expect("bounds checked"),
            ];

            if !cell_pointers.iter().all(|&word| is_start(word)) {
                break;
            }

            for (plane, &pointer) in cell_pointers.iter().enumerate() {
                let writes =
                    run_subroutine(bin, pointer as usize).ok_or(DecodeError::MalformedSprite {
                        reason: "ship cell pointer is not a valid subroutine",
                    })?;
                planes.push((cell * 4 + plane, writes));
            }

            cell += 1;
            offset += 8;
        }

        if planes.is_empty() {
            break;
        }

        // Skip the trailing cell-count word that terminated the frame.
        if offset + 2 <= wad.len() {
            offset += 2;
        }

        sprites.push(assemble(&planes));
    }

    if sprites.is_empty() {
        return Err(DecodeError::MalformedSprite {
            reason: "no ship frames at catalog offset",
        });
    }

    Ok(SpriteSheet { sprites })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `mov word [si], 0xBBAA; retf` writes index 0xAA at (0,0) and 0xBB at (1,0).
    const WORD_SUB: &[u8] = &[0xC7, 0x04, 0xAA, 0xBB, 0xCB];

    #[test]
    fn run_subroutine_writes_two_pixels() {
        let writes = run_subroutine(WORD_SUB, 0).unwrap();
        assert_eq!(writes.len(), 2);
        assert_eq!(
            (writes[0].column, writes[0].row, writes[0].value),
            (0, 0, 0xAA)
        );
        assert_eq!(
            (writes[1].column, writes[1].row, writes[1].value),
            (1, 0, 0xBB)
        );
    }

    #[test]
    fn run_subroutine_advances_rows_with_add_si_ax() {
        // mov byte [si], 1 ; add si, ax ; mov byte [si], 2 ; retf
        let code = &[0xC6, 0x04, 0x01, 0x03, 0xF0, 0xC6, 0x04, 0x02, 0xCB];
        let writes = run_subroutine(code, 0).unwrap();
        assert_eq!((writes[0].row, writes[0].value), (0, 1));
        assert_eq!((writes[1].row, writes[1].value), (1, 2));
    }

    #[test]
    fn run_subroutine_rejects_unknown_opcode() {
        assert!(run_subroutine(&[0xF4], 0).is_none());
    }

    #[test]
    fn resolve_plane_follows_a_clip_header() {
        // header word0 = 2 -> subroutine sits 2 bytes after the header.
        let bin = &[0x02, 0x00, 0xC7, 0x04, 0xAA, 0xBB, 0xCB];
        let writes = resolve_plane(bin, 0, 0).unwrap();
        assert_eq!(writes.len(), 2);
    }

    #[test]
    fn resolve_plane_falls_back_to_direct() {
        // word0 = 0xBBAA is far too large to be a header offset, so pk is direct.
        let writes = resolve_plane(WORD_SUB, 0, 0).unwrap();
        assert_eq!(writes.len(), 2);
    }

    fn pixel_at(sprite: &Sprite, x: u32, y: u32) -> Option<u8> {
        sprite.pixels[(y * sprite.size.width + x) as usize]
    }

    #[test]
    fn decode_banked_reads_one_direct_record() {
        // bin: a one-pixel subroutine at offset 0.
        let bin = &[0xC6, 0x04, 0x42, 0xCB];
        // catalog: record [field0=0, p0=0, p1=0, p2=0, p3=0], then a terminator
        // whose field0 pushes the base past the bin (stops the walk).
        let mut wad = vec![0u8; 0xf9f0];
        wad.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        wad.extend_from_slice(&[0xFF, 0x7F, 0, 0, 0, 0, 0, 0, 0, 0]);

        let sheet = decode_banked(bin, &wad, 0xf9f0).unwrap();
        assert_eq!(sheet.sprites.len(), 1);
        // All four planes resolve to the same sub, so the pixel lands once per
        // plane at x = 0,1,2,3.
        assert_eq!(sheet.sprites[0].size, Dimensions::new(4, 1));
        assert_eq!(pixel_at(&sheet.sprites[0], 0, 0), Some(0x42));
        assert_eq!(pixel_at(&sheet.sprites[0], 3, 0), Some(0x42));
    }

    #[test]
    fn decode_banked_interleaves_planes() {
        // Each plane writes one pixel at (0,0); planes land at x = 0,1,2,3.
        let bin = &[0xC6, 0x04, 0x07, 0xCB];
        let mut wad = vec![0u8; 0xf9f0];
        wad.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        wad.extend_from_slice(&[0xFF, 0x7F, 0, 0, 0, 0, 0, 0, 0, 0]);

        let sheet = decode_banked(bin, &wad, 0xf9f0).unwrap();
        let sprite = &sheet.sprites[0];
        assert_eq!(sprite.size, Dimensions::new(4, 1));
        for x in 0..4 {
            assert_eq!(pixel_at(sprite, x, 0), Some(0x07));
        }
    }

    #[test]
    fn decode_ship_reads_one_two_cell_frame() {
        // Two identical one-pixel subroutines back to back in the bin.
        let bin = &[0xC6, 0x04, 0x09, 0xCB, 0xC6, 0x04, 0x09, 0xCB];
        // Frame: cell A at sub 0 (all planes), cell B at sub 4, count = 2.
        let mut wad = vec![0u8; 0x100];
        let catalog = 0x100;
        let mut record = Vec::new();
        record.extend_from_slice(&0u16.to_le_bytes()); // cell A planes -> sub 0
        record.extend_from_slice(&0u16.to_le_bytes());
        record.extend_from_slice(&0u16.to_le_bytes());
        record.extend_from_slice(&0u16.to_le_bytes());
        record.extend_from_slice(&4u16.to_le_bytes()); // cell B planes -> sub 4
        record.extend_from_slice(&4u16.to_le_bytes());
        record.extend_from_slice(&4u16.to_le_bytes());
        record.extend_from_slice(&4u16.to_le_bytes());
        record.extend_from_slice(&2u16.to_le_bytes()); // ncells terminator
        wad.extend_from_slice(&record);

        let sheet = decode_ship(bin, &wad, catalog).unwrap();
        assert_eq!(sheet.sprites.len(), 1);
        let sprite = &sheet.sprites[0];
        // Cell A occupies x = 0..3 (one column per plane), cell B x = 32..35.
        assert_eq!(pixel_at(sprite, 0, 0), Some(0x09));
        assert_eq!(pixel_at(sprite, 32, 0), Some(0x09));
        assert_eq!(sprite.size.width, 36);
    }
}
