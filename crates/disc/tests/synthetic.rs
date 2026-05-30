//! Full-pipeline test over a tiny synthetic image, generated at runtime.
//!
//! This exercises `DiscImage::open` end to end — cue parse, case-insensitive
//! bin resolution, the PVD, the root-directory walk, one level of `FLI/`
//! recursion, `AssetSource::read`, and the audio-track ranges — without the real
//! 282 MB CD. It also pins two regressions the real image first exposed: the
//! root directory record's own `0x00` identifier, and a cue that names the bin
//! in a different case than the file on disk.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use prototype_disc::{AssetSource, DiscImage};

const RAW_SECTOR: usize = 2352;
const MODE1_HEADER: usize = 16;
const USER_DATA: usize = 2048;

/// A raw MODE1/2352 image addressed by sector, with user data at `lba*2352+16`.
struct ImageBuilder {
    sectors: Vec<u8>,
}

impl ImageBuilder {
    fn new(sector_count: usize) -> Self {
        Self {
            sectors: vec![0u8; sector_count * RAW_SECTOR],
        }
    }

    fn user_mut(&mut self, lba: usize) -> &mut [u8] {
        let base = lba * RAW_SECTOR + MODE1_HEADER;
        &mut self.sectors[base..base + USER_DATA]
    }

    fn put(&mut self, lba: usize, bytes: &[u8]) {
        self.user_mut(lba)[..bytes.len()].copy_from_slice(bytes);
    }
}

/// Build one ISO9660 directory record (LE extent/size; BE copies left zero,
/// which the reader ignores).
fn dir_record(name: &[u8], lba: u32, size: u32, is_dir: bool) -> Vec<u8> {
    let pad = name.len().is_multiple_of(2);
    let len = 33 + name.len() + usize::from(pad);
    let mut record = vec![0u8; len];
    record[0] = len as u8;
    record[2..6].copy_from_slice(&lba.to_le_bytes());
    record[10..14].copy_from_slice(&size.to_le_bytes());
    record[25] = if is_dir { 0x02 } else { 0 };
    record[32] = name.len() as u8;
    record[33..33 + name.len()].copy_from_slice(name);
    record
}

fn concat(records: &[Vec<u8>]) -> Vec<u8> {
    records.iter().flatten().copied().collect()
}

/// A self-deleting temp directory holding the generated `tiny.bin`/`tiny.cue`.
struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    /// Lay out a 23-sector image:
    ///   16 PVD, 17 root dir, 18 FLI dir, 19 HELLO.TXT, 20 FLI/INTRO.FLI,
    ///   21-22 audio (track 2). Disc end = 23.
    fn build() -> Self {
        let mut image = ImageBuilder::new(23);

        // PVD: CD001 magic + a root directory record at offset 156.
        let pvd = image.user_mut(16);
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        let root = dir_record(&[0x00], 17, USER_DATA as u32, true);
        pvd[156..156 + root.len()].copy_from_slice(&root);

        // Root directory: ".", "..", a file, and the FLI subdirectory.
        image.put(
            17,
            &concat(&[
                dir_record(&[0x00], 17, USER_DATA as u32, true),
                dir_record(&[0x01], 17, USER_DATA as u32, true),
                dir_record(b"HELLO.TXT;1", 19, 12, false),
                dir_record(b"FLI", 18, USER_DATA as u32, true),
            ]),
        );

        // FLI subdirectory: ".", "..", and one file.
        image.put(
            18,
            &concat(&[
                dir_record(&[0x00], 18, USER_DATA as u32, true),
                dir_record(&[0x01], 17, USER_DATA as u32, true),
                dir_record(b"INTRO.FLI;1", 20, 8, false),
            ]),
        );

        image.put(19, b"Hello, disc!");
        image.put(20, b"FLICDATA");

        // Unique per fixture so parallel tests never share (and clean up) a dir.
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "prototype-disc-synthetic-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("tiny.bin"), &image.sectors).unwrap();
        // The cue names the bin in uppercase to exercise case-insensitive
        // resolution against the lowercase file. Track 2 starts at LBA 21
        // (MSF 00:00:21) and runs to the disc end.
        fs::write(
            dir.join("tiny.cue"),
            "FILE \"tiny.BIN\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 01 00:00:21\n",
        )
        .unwrap();

        Self { dir }
    }

    fn cue(&self) -> PathBuf {
        self.dir.join("tiny.cue")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn opens_and_reads_a_synthetic_image_end_to_end() {
    let fixture = Fixture::build();
    let image = DiscImage::open(fixture.cue()).expect("synthetic image opens");

    let root_files: Vec<&str> = image
        .files()
        .iter()
        .filter(|entry| !entry.name.contains('/'))
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(root_files, ["HELLO.TXT"], "one root file, ;1 stripped");
    assert!(image.contains("FLI/INTRO.FLI"), "recursed into FLI/");

    // Case-insensitive lookups, with and without the version suffix.
    assert_eq!(image.read("hello.txt").unwrap(), b"Hello, disc!");
    assert_eq!(image.read("HELLO.TXT;1").unwrap(), b"Hello, disc!");
    assert_eq!(image.read("fli/intro.fli").unwrap(), b"FLICDATA");

    assert!(matches!(
        image.read("MISSING.DAT"),
        Err(prototype_disc::DiscError::FileNotFound(_))
    ));
}

#[test]
fn synthetic_audio_track_spans_to_disc_end() {
    let fixture = Fixture::build();
    let image = DiscImage::open(fixture.cue()).expect("synthetic image opens");

    let tracks = image.audio_tracks();
    assert_eq!(tracks.len(), 1);
    assert_eq!(
        (tracks[0].number, tracks[0].start_lba, tracks[0].end_lba),
        (2, 21, 23),
        "track 2 runs from its INDEX 01 to the disc end"
    );

    let pcm = image.read_track_pcm(&tracks[0]).unwrap();
    assert_eq!(pcm.len(), 2 * RAW_SECTOR, "two raw audio sectors");
}
