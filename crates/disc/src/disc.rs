//! The concrete CD-image backend: cue + sectors + ISO9660, plus CD-DA ripping.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::AssetSource;
use crate::cue::{Cue, TrackMode};
use crate::error::{DiscError, Result};
use crate::iso9660;
use crate::sector::SectorReader;

/// Default cue filename looked up in the working directory.
const DEFAULT_CUE: &str = "PROTOTYPE.cue";
/// Environment variable overriding the cue path for [`DiscImage::open_default`].
const ENV_OVERRIDE: &str = "PROTOTYPE_DISC";

/// A file in the ISO9660 data track.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Canonical name, e.g. `"COVER3.PAL"` or `"FLI/INTRO.FLI"`.
    pub name: String,
    pub lba: u32,
    pub size: u32,
}

/// A CD-DA audio track, as an LBA half-open range `[start_lba, end_lba)`.
#[derive(Debug, Clone, Copy)]
pub struct AudioTrack {
    pub number: u8,
    pub start_lba: u32,
    pub end_lba: u32,
}

/// An opened bin/cue image: the data-track file map and the audio-track table.
pub struct DiscImage {
    reader: SectorReader,
    files: Vec<FileEntry>,
    /// Uppercased canonical name -> index into `files`.
    index: BTreeMap<String, usize>,
    audio: Vec<AudioTrack>,
}

impl DiscImage {
    /// Opens the image at `cue_path`.
    ///
    /// Parses the cue, opens the bin it references (resolved relative to the
    /// cue), reads the ISO directory, and computes the audio-track ranges.
    pub fn open(cue_path: impl AsRef<Path>) -> Result<Self> {
        let cue_path = cue_path.as_ref();
        let text = std::fs::read_to_string(cue_path)?;
        let cue = Cue::parse(&text)?;

        let bin_path = resolve_bin(cue_dir(cue_path), &cue.bin_filename)?;
        let reader = SectorReader::open(&bin_path)?;

        let files: Vec<FileEntry> = iso9660::list_files(&reader)?
            .into_iter()
            .map(|record| FileEntry {
                name: record.name,
                lba: record.lba,
                size: record.size,
            })
            .collect();
        let index = files
            .iter()
            .enumerate()
            .map(|(idx, entry)| (entry.name.to_ascii_uppercase(), idx))
            .collect();

        let audio = audio_tracks(&cue, reader.sector_count())?;

        Ok(Self {
            reader,
            files,
            index,
            audio,
        })
    }

    /// Opens the image at `$PROTOTYPE_DISC`, or `./PROTOTYPE.cue` if unset.
    pub fn open_default() -> Result<Self> {
        let path = std::env::var_os(ENV_OVERRIDE)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CUE));
        Self::open(path)
    }

    /// The data-track files, in directory order.
    pub fn files(&self) -> &[FileEntry] {
        &self.files
    }

    /// The CD-DA audio tracks, in track-number order.
    pub fn audio_tracks(&self) -> &[AudioTrack] {
        &self.audio
    }

    /// Reads a track's raw red-book PCM.
    ///
    /// 44100 Hz, 16-bit stereo, little-endian interleaved (each 2352-byte sector
    /// is 588 frames).
    pub fn read_track_pcm(&self, track: &AudioTrack) -> Result<Vec<u8>> {
        self.reader.read_raw_range(track.start_lba, track.end_lba)
    }
}

/// The directory the cue's bin path is resolved against.
///
/// The cue's parent, or the current directory when the cue is a bare filename.
/// A bare relative path like `PROTOTYPE.cue` has `parent()` `Some("")`, an empty
/// path that is not a usable directory (`read_dir` on it fails), so it falls
/// back to `.`.
fn cue_dir(cue_path: &Path) -> &Path {
    match cue_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

/// Resolves the bin file next to the cue.
///
/// Cue sheets often record the filename in a different case than the file on
/// disk (e.g. `PROTOTYPE.BIN` vs `PROTOTYPE.bin`), so it falls back to a
/// case-insensitive scan of the directory.
fn resolve_bin(dir: &Path, filename: &str) -> Result<PathBuf> {
    let direct = dir.join(filename);
    if direct.exists() {
        return Ok(direct);
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(filename)
        {
            return Ok(entry.path());
        }
    }

    Err(DiscError::Cue(format!(
        "bin file not found next to cue: {filename}"
    )))
}

/// Builds the audio-track table.
///
/// Each track runs from its `INDEX 01` to the next track's first index
/// (dropping the pregap gap); the last track runs to the end of the disc.
fn audio_tracks(cue: &Cue, disc_end: u32) -> Result<Vec<AudioTrack>> {
    let mut tracks = Vec::new();

    for (position, track) in cue.tracks.iter().enumerate() {
        if track.mode != TrackMode::Audio {
            continue;
        }

        let start_lba = track
            .index01_lba()
            .ok_or(DiscError::Malformed("audio track has no INDEX 01"))?;
        let end_lba = cue
            .tracks
            .get(position + 1)
            .and_then(|next| next.first_index_lba())
            .unwrap_or(disc_end);

        tracks.push(AudioTrack {
            number: track.number,
            start_lba,
            end_lba,
        });
    }

    Ok(tracks)
}

/// Normalises a lookup name to the index key: uppercase, `;1` suffix stripped.
fn normalize(name: &str) -> String {
    let name = name.split(';').next().unwrap_or(name);
    name.to_ascii_uppercase()
}

impl AssetSource for DiscImage {
    fn read(&self, name: &str) -> Result<Vec<u8>> {
        let &idx = self
            .index
            .get(&normalize(name))
            .ok_or_else(|| DiscError::FileNotFound(name.to_string()))?;
        let entry = &self.files[idx];
        self.reader.read_file(entry.lba, entry.size)
    }

    fn contains(&self, name: &str) -> bool {
        self.index.contains_key(&normalize(name))
    }

    fn names(&self) -> Vec<String> {
        self.files.iter().map(|entry| entry.name.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cue::{CueTrack, TrackMode};

    fn audio(number: u8, indices: Vec<(u8, u32)>) -> CueTrack {
        CueTrack {
            number,
            mode: TrackMode::Audio,
            indices,
        }
    }

    #[test]
    fn track_ends_at_next_first_index_dropping_pregap() {
        let cue = Cue {
            bin_filename: "x.bin".to_string(),
            tracks: vec![
                CueTrack {
                    number: 1,
                    mode: TrackMode::Mode1_2352,
                    indices: vec![(1, 0)],
                },
                audio(2, vec![(1, 17709)]),
                audio(3, vec![(0, 33646), (1, 33796)]),
            ],
        };

        let tracks = audio_tracks(&cue, 120104).unwrap();
        assert_eq!(tracks.len(), 2);
        // Track 2 ends where track 3's pregap (INDEX 00) begins.
        assert_eq!(
            (tracks[0].number, tracks[0].start_lba, tracks[0].end_lba),
            (2, 17709, 33646)
        );
        // The last track runs to the disc end.
        assert_eq!(
            (tracks[1].number, tracks[1].start_lba, tracks[1].end_lba),
            (3, 33796, 120104)
        );
    }

    #[test]
    fn normalize_strips_version_and_uppercases() {
        assert_eq!(normalize("fli/intro.fli"), "FLI/INTRO.FLI");
        assert_eq!(normalize("COVER3.PAL;1"), "COVER3.PAL");
    }

    #[test]
    fn cue_dir_falls_back_to_cwd_for_a_bare_filename() {
        // The case that broke `--cue PROTOTYPE.cue`: parent() is Some(""), which
        // read_dir rejects, so it must resolve to the current directory.
        assert_eq!(cue_dir(Path::new("PROTOTYPE.cue")), Path::new("."));
        assert_eq!(cue_dir(Path::new("sub/PROTOTYPE.cue")), Path::new("sub"));
        assert_eq!(cue_dir(Path::new("/abs/PROTOTYPE.cue")), Path::new("/abs"));
    }
}
