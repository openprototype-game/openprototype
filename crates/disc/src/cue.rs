//! Minimal `.cue` sheet parser for a single-`FILE` bin/cue image.
//!
//! Only the directives this project's image uses are recognised: `FILE`,
//! `TRACK n MODE1/2352|AUDIO`, and `INDEX nn mm:ss:ff`. `PREGAP`/`POSTGAP` and
//! other metadata lines are accepted and ignored. Index times are
//! frame-accurate offsets from the start of the bin file, so the in-file LBA is
//! `(m*60+s)*75+f` with no 150-frame lead-in adjustment.

use crate::error::{DiscError, Result};

/// The data layout of a track, as declared by its `TRACK` directive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackMode {
    /// `MODE1/2352`: 2352-byte sectors with a 2048-byte user-data payload.
    Mode1_2352,
    /// `AUDIO`: raw 2352-byte red-book CD-DA sectors.
    Audio,
}

/// One `TRACK` and its parsed `INDEX` points.
#[derive(Debug, Clone)]
pub struct CueTrack {
    pub number: u8,
    pub mode: TrackMode,
    /// `(index number, in-file LBA)` in file order; e.g. an `INDEX 00` pregap
    /// precedes the `INDEX 01` audio start.
    pub indices: Vec<(u8, u32)>,
}

impl CueTrack {
    /// The LBA of the first listed index (the pregap `INDEX 00` if present,
    /// else `INDEX 01`).
    pub fn first_index_lba(&self) -> Option<u32> {
        self.indices.first().map(|&(_, lba)| lba)
    }

    /// The LBA of `INDEX 01` (where the track's content actually starts).
    pub fn index01_lba(&self) -> Option<u32> {
        self.indices
            .iter()
            .find(|&&(number, _)| number == 1)
            .map(|&(_, lba)| lba)
    }
}

/// A parsed cue sheet: the referenced bin file plus its tracks.
#[derive(Debug, Clone)]
pub struct Cue {
    pub bin_filename: String,
    pub tracks: Vec<CueTrack>,
}

impl Cue {
    pub fn parse(text: &str) -> Result<Self> {
        let mut bin_filename = None;
        let mut tracks: Vec<CueTrack> = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            let mut tokens = line.split_whitespace();
            let Some(keyword) = tokens.next() else {
                continue;
            };

            match keyword.to_ascii_uppercase().as_str() {
                "FILE" => bin_filename = Some(parse_file_name(line)?),
                "TRACK" => {
                    let number = tokens
                        .next()
                        .and_then(|token| token.parse::<u8>().ok())
                        .ok_or_else(|| DiscError::Cue(format!("bad TRACK number: {line}")))?;
                    let mode = match tokens.next() {
                        Some("MODE1/2352") => TrackMode::Mode1_2352,
                        Some("AUDIO") => TrackMode::Audio,
                        other => {
                            return Err(DiscError::Cue(format!(
                                "unsupported track mode: {other:?}"
                            )));
                        }
                    };
                    tracks.push(CueTrack {
                        number,
                        mode,
                        indices: Vec::new(),
                    });
                }
                "INDEX" => {
                    let track = tracks
                        .last_mut()
                        .ok_or_else(|| DiscError::Cue("INDEX before any TRACK".to_string()))?;
                    let number = tokens
                        .next()
                        .and_then(|token| token.parse::<u8>().ok())
                        .ok_or_else(|| DiscError::Cue(format!("bad INDEX number: {line}")))?;
                    let lba = tokens
                        .next()
                        .ok_or_else(|| DiscError::Cue(format!("INDEX missing time: {line}")))
                        .and_then(msf_to_lba)?;
                    track.indices.push((number, lba));
                }
                // PREGAP/POSTGAP/FLAGS/REM/CATALOG/PERFORMER/TITLE: not needed.
                _ => {}
            }
        }

        let bin_filename =
            bin_filename.ok_or_else(|| DiscError::Cue("no FILE directive".to_string()))?;
        if tracks.is_empty() {
            return Err(DiscError::Cue("no TRACK directives".to_string()));
        }

        Ok(Cue {
            bin_filename,
            tracks,
        })
    }
}

/// Pull the quoted filename out of a `FILE "name" BINARY` line, falling back to
/// the second whitespace token if it is not quoted.
fn parse_file_name(line: &str) -> Result<String> {
    if let Some(open) = line.find('"') {
        if let Some(close) = line[open + 1..].find('"') {
            return Ok(line[open + 1..open + 1 + close].to_string());
        }
        return Err(DiscError::Cue(format!("unterminated FILE quote: {line}")));
    }

    line.split_whitespace()
        .nth(1)
        .map(str::to_string)
        .ok_or_else(|| DiscError::Cue(format!("FILE missing name: {line}")))
}

/// `mm:ss:ff` -> in-file LBA. Frames are 1/75 s; `(m*60+s)*75+f`.
fn msf_to_lba(msf: &str) -> Result<u32> {
    let mut parts = msf.split(':');
    let mut next = |what: &'static str| -> Result<u32> {
        parts
            .next()
            .and_then(|token| token.parse::<u32>().ok())
            .ok_or(DiscError::Malformed(what))
    };

    let minutes = next("cue MSF minutes")?;
    let seconds = next("cue MSF seconds")?;
    let frames = next("cue MSF frames")?;

    if seconds >= 60 || frames >= 75 {
        return Err(DiscError::Cue(format!("MSF out of range: {msf}")));
    }

    Ok((minutes * 60 + seconds) * 75 + frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped 8-track layout: a MODE1/2352 data track plus seven CD-DA
    /// tracks, with INDEX 00 pregaps on tracks 3-8.
    const SHEET: &str = r#"
FILE "PROTOTYPE.bin" BINARY
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 01 03:56:09
  TRACK 03 AUDIO
    INDEX 00 07:28:46
    INDEX 01 07:30:46
  TRACK 04 AUDIO
    INDEX 00 10:53:39
    INDEX 01 10:55:39
  TRACK 05 AUDIO
    INDEX 00 14:44:16
    INDEX 01 14:46:16
  TRACK 06 AUDIO
    INDEX 00 18:40:39
    INDEX 01 18:42:39
  TRACK 07 AUDIO
    INDEX 00 22:24:69
    INDEX 01 22:26:69
  TRACK 08 AUDIO
    INDEX 00 23:42:04
    INDEX 01 23:44:04
"#;

    #[test]
    fn parses_filename_and_track_modes() {
        let cue = Cue::parse(SHEET).unwrap();
        assert_eq!(cue.bin_filename, "PROTOTYPE.bin");
        assert_eq!(cue.tracks.len(), 8);
        assert_eq!(cue.tracks[0].mode, TrackMode::Mode1_2352);
        assert!(cue.tracks[1..].iter().all(|t| t.mode == TrackMode::Audio));
    }

    #[test]
    fn msf_converts_to_in_file_lba() {
        let cue = Cue::parse(SHEET).unwrap();
        assert_eq!(cue.tracks[0].index01_lba(), Some(0));
        assert_eq!(cue.tracks[1].index01_lba(), Some(17709));
        assert_eq!(cue.tracks[7].index01_lba(), Some(106804));
    }

    #[test]
    fn pregap_index00_precedes_index01() {
        let cue = Cue::parse(SHEET).unwrap();
        // Track 2 has no pregap; track 3 does.
        assert_eq!(cue.tracks[1].indices.len(), 1);
        assert_eq!(cue.tracks[2].indices.len(), 2);
        assert_eq!(cue.tracks[2].indices[0].0, 0);
        assert_eq!(cue.tracks[2].first_index_lba(), Some(33646));
        assert_eq!(cue.tracks[2].index01_lba(), Some(33796));
    }

    #[test]
    fn rejects_unknown_track_mode() {
        let bad = "FILE \"x.bin\" BINARY\n TRACK 01 MODE2/2352\n";
        assert!(matches!(Cue::parse(bad), Err(DiscError::Cue(_))));
    }
}
