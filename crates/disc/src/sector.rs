//! Sector-level reads over the raw `.bin` image.
//!
//! Every sector on the disc is 2352 bytes. For the `MODE1/2352` data track the
//! 2048-byte user payload sits 16 bytes in (12-byte sync + 4-byte header),
//! so logical block `lba` begins at file byte `lba * 2352 + 16`. CD-DA tracks
//! have no header: the whole 2352-byte sector is audio sample data.
//!
//! Reads go through a `Mutex<File>` so [`DiscImage`] can hand out asset bytes
//! behind `&self`.
//!
//! [`DiscImage`]: crate::DiscImage

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;

use crate::error::{DiscError, Result};

/// Bytes per raw CD sector.
pub const RAW_SECTOR: usize = 2352;
/// User-data bytes per `MODE1/2352` sector.
pub const USER_DATA: usize = 2048;
/// Offset of the user payload within a `MODE1/2352` sector (sync + header).
const MODE1_HEADER: u64 = 16;

/// A seekable handle on the `.bin` file, addressed by sector.
pub struct SectorReader {
    file: Mutex<File>,
    /// Total whole 2352-byte sectors in the file (the disc end LBA).
    sector_count: u32,
}

impl SectorReader {
    /// Opens the `.bin` image at `path`, recording its total sector count.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(Self {
            file: Mutex::new(file),
            sector_count: (len / RAW_SECTOR as u64) as u32,
        })
    }

    /// Total sectors in the image; also the LBA one past the last sector.
    pub fn sector_count(&self) -> u32 {
        self.sector_count
    }

    /// Reads `size` bytes of `MODE1/2352` user data at logical block `lba`.
    ///
    /// Assembles `ceil(size / 2048)` sectors and truncates to `size`.
    pub fn read_file(&self, lba: u32, size: u32) -> Result<Vec<u8>> {
        let size = size as usize;
        let sectors = size.div_ceil(USER_DATA);
        let mut out = Vec::with_capacity(sectors * USER_DATA);
        let mut sector = [0u8; USER_DATA];

        let mut file = self.file.lock().expect("sector reader mutex poisoned");
        for index in 0..sectors as u64 {
            let offset = (lba as u64 + index) * RAW_SECTOR as u64 + MODE1_HEADER;
            file.seek(SeekFrom::Start(offset))?;
            file.read_exact(&mut sector)
                .map_err(|_| DiscError::Malformed("data sector reads past end of image"))?;
            out.extend_from_slice(&sector);
        }

        out.truncate(size);
        Ok(out)
    }

    /// Reads raw 2352-byte sectors over `[start_lba, end_lba)` verbatim.
    ///
    /// For CD-DA tracks, the sector *is* the audio payload, with no header to
    /// skip.
    pub fn read_raw_range(&self, start_lba: u32, end_lba: u32) -> Result<Vec<u8>> {
        if end_lba < start_lba {
            return Err(DiscError::Malformed("audio track end precedes start"));
        }
        let count = (end_lba - start_lba) as usize;
        let mut out = vec![0u8; count * RAW_SECTOR];

        let mut file = self.file.lock().expect("sector reader mutex poisoned");
        file.seek(SeekFrom::Start(start_lba as u64 * RAW_SECTOR as u64))?;
        file.read_exact(&mut out)
            .map_err(|_| DiscError::Malformed("audio track reads past end of image"))?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A self-deleting temp file holding a crafted raw image.
    struct TempImage(std::path::PathBuf);

    impl TempImage {
        fn new(bytes: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!(
                "prototype-disc-sector-{}-{:p}.bin",
                std::process::id(),
                bytes
            ));
            File::create(&path).unwrap().write_all(bytes).unwrap();
            Self(path)
        }
    }

    impl Drop for TempImage {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /// Build `count` MODE1/2352 sectors whose user data byte equals the LBA.
    fn mode1_image(count: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; count as usize * RAW_SECTOR];
        for lba in 0..count {
            let base = lba as usize * RAW_SECTOR + MODE1_HEADER as usize;
            for byte in &mut bytes[base..base + USER_DATA] {
                *byte = lba as u8;
            }
        }
        bytes
    }

    #[test]
    fn user_data_starts_after_the_16_byte_header() {
        let image = TempImage::new(&mode1_image(3));
        let reader = SectorReader::open(&image.0).unwrap();

        let data = reader.read_file(1, USER_DATA as u32).unwrap();
        assert_eq!(data.len(), USER_DATA);
        assert!(data.iter().all(|&b| b == 1), "should read LBA 1's payload");
    }

    #[test]
    fn multi_sector_read_truncates_to_exact_size() {
        let image = TempImage::new(&mode1_image(3));
        let reader = SectorReader::open(&image.0).unwrap();

        // 2049 bytes spans two sectors but keeps only the first payload byte
        // of the second.
        let data = reader.read_file(0, USER_DATA as u32 + 1).unwrap();
        assert_eq!(data.len(), USER_DATA + 1);
        assert_eq!(data[USER_DATA - 1], 0);
        assert_eq!(data[USER_DATA], 1);
    }

    #[test]
    fn reading_past_the_end_is_malformed() {
        let image = TempImage::new(&mode1_image(1));
        let reader = SectorReader::open(&image.0).unwrap();
        assert_eq!(reader.sector_count(), 1);
        assert!(matches!(
            reader.read_file(5, 2048),
            Err(DiscError::Malformed(_))
        ));
    }
}
