//! Just enough ISO9660 to list the data track's files.
//!
//! The volume is ISO9660 Level 1: uppercase 8.3 names with a `;1` version
//! suffix, a root directory, and one `FLI/` subdirectory. We read the Primary
//! Volume Descriptor at logical sector 16, walk the root directory extent, and
//! recurse one level into `FLI/`. No Joliet/Rock Ridge, no deep nesting.

use crate::error::{DiscError, Result};
use crate::sector::{SectorReader, USER_DATA};

/// Logical sector holding the Primary Volume Descriptor.
const PVD_LBA: u32 = 16;
/// Byte offset of the root directory record within the PVD.
const ROOT_RECORD_OFFSET: usize = 156;

// Field offsets within a directory record.
const EXTENT_LBA: usize = 2; // 4-byte LE (followed by the BE copy)
const DATA_LEN: usize = 10; // 4-byte LE (followed by the BE copy)
const FILE_FLAGS: usize = 25;
const NAME_LEN: usize = 32;
const NAME: usize = 33;
const DIRECTORY_FLAG: u8 = 0x02;

/// One entry parsed out of a directory extent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirRecord {
    /// Canonical name: uppercase, `;1` stripped (and `FLI/`-prefixed for
    /// entries inside the subdirectory).
    pub name: String,
    pub lba: u32,
    pub size: u32,
    pub is_dir: bool,
}

/// Read and verify the PVD, then return every file on the volume (recursing one
/// level into `FLI/`, whose children are prefixed `FLI/`). Directories
/// themselves are not included.
pub fn list_files(reader: &SectorReader) -> Result<Vec<DirRecord>> {
    let pvd = reader.read_file(PVD_LBA, USER_DATA as u32)?;
    if pvd.get(1..6) != Some(b"CD001") {
        return Err(DiscError::NotIso9660);
    }

    // The root directory record's own identifier is a single 0x00 byte, which
    // `parse_record` skips as ".", so read its extent and length directly.
    let root_record = pvd
        .get(ROOT_RECORD_OFFSET..)
        .ok_or(DiscError::Malformed("missing root directory record"))?;
    let root_lba =
        le_u32(root_record, EXTENT_LBA).ok_or(DiscError::Malformed("bad root extent LBA"))?;
    let root_size =
        le_u32(root_record, DATA_LEN).ok_or(DiscError::Malformed("bad root data length"))?;

    let root_extent = reader.read_file(root_lba, root_size)?;
    let mut files = Vec::new();

    for entry in walk_dir(&root_extent) {
        if entry.is_dir {
            if entry.name == "FLI" {
                let sub = reader.read_file(entry.lba, entry.size)?;
                for child in walk_dir(&sub) {
                    if !child.is_dir {
                        files.push(DirRecord {
                            name: format!("FLI/{}", child.name),
                            ..child
                        });
                    }
                }
            }
        } else {
            files.push(entry);
        }
    }

    Ok(files)
}

/// Walk a directory extent's records, skipping `.`/`..`. Pure over the bytes so
/// it can be tested without an image.
pub fn walk_dir(bytes: &[u8]) -> Vec<DirRecord> {
    let mut entries = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        let len = bytes[pos] as usize;
        if len == 0 {
            // Records never straddle a sector; a zero length pads to the next.
            pos = (pos / USER_DATA + 1) * USER_DATA;
            continue;
        }

        let Some(record) = bytes.get(pos..pos + len) else {
            break;
        };
        if let Some(entry) = parse_record(record) {
            entries.push(entry);
        }
        pos += len;
    }

    entries
}

/// Parse a single directory record, returning `None` for `.`/`..` or malformed
/// records.
fn parse_record(record: &[u8]) -> Option<DirRecord> {
    let name_len = *record.get(NAME_LEN)? as usize;
    let raw_name = record.get(NAME..NAME + name_len)?;

    // '.' and '..' are encoded as a single 0x00 / 0x01 byte.
    if raw_name == [0x00] || raw_name == [0x01] {
        return None;
    }

    let lba = le_u32(record, EXTENT_LBA)?;
    let size = le_u32(record, DATA_LEN)?;
    let is_dir = record.get(FILE_FLAGS)? & DIRECTORY_FLAG != 0;

    let name = String::from_utf8_lossy(raw_name);
    // Strip the ";1" version suffix from file names.
    let name = name.split(';').next().unwrap_or(&name).to_string();

    Some(DirRecord {
        name,
        lba,
        size,
        is_dir,
    })
}

fn le_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes(slice.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal directory record (LE extent/size only; the BE copies and
    /// dates are zero-filled, which the parser ignores).
    fn record(name: &[u8], lba: u32, size: u32, is_dir: bool) -> Vec<u8> {
        let pad = name.len().is_multiple_of(2); // pad byte when name length is even
        let len = NAME + name.len() + usize::from(pad);
        let mut rec = vec![0u8; len];
        rec[0] = len as u8;
        rec[EXTENT_LBA..EXTENT_LBA + 4].copy_from_slice(&lba.to_le_bytes());
        rec[DATA_LEN..DATA_LEN + 4].copy_from_slice(&size.to_le_bytes());
        rec[FILE_FLAGS] = if is_dir { DIRECTORY_FLAG } else { 0 };
        rec[NAME_LEN] = name.len() as u8;
        rec[NAME..NAME + name.len()].copy_from_slice(name);
        rec
    }

    #[test]
    fn walks_records_stripping_version_and_skipping_dot_entries() {
        let mut blob = Vec::new();
        blob.extend(record(&[0x00], 16, 2048, true)); // "."
        blob.extend(record(&[0x01], 16, 2048, true)); // ".."
        blob.extend(record(b"COVER3.PAL;1", 100, 768, false));
        blob.extend(record(b"FLI", 200, 4096, true));

        let entries = walk_dir(&blob);
        assert_eq!(entries.len(), 2, "the two dot entries are skipped");

        assert_eq!(
            entries[0],
            DirRecord {
                name: "COVER3.PAL".to_string(),
                lba: 100,
                size: 768,
                is_dir: false,
            }
        );
        assert_eq!(entries[1].name, "FLI");
        assert!(entries[1].is_dir);
    }

    #[test]
    fn zero_length_record_pads_to_next_sector() {
        let mut blob = record(b"A.TXT;1", 50, 10, false);
        // A zero terminates the first sector; the next record sits at sector 1.
        blob.resize(USER_DATA, 0);
        blob.extend(record(b"B.TXT;1", 60, 20, false));

        let entries = walk_dir(&blob);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "A.TXT");
        assert_eq!(entries[1].name, "B.TXT");
        assert_eq!(entries[1].lba, 60);
    }
}
