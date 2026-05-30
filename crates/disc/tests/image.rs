//! Integration tests against the real CD image.
//!
//! These are gated on the image being present: set `$PROTOTYPE_DISC` to the
//! cue path, or drop `PROTOTYPE.cue`/`.bin` at the repo root. When neither is
//! found the tests print a notice and pass, so CI needs no game files.

use std::path::{Path, PathBuf};

use prototype_disc::{AssetSource, DiscImage};

/// Locate a cue: `$PROTOTYPE_DISC` first, then `PROTOTYPE.cue` at the repo root
/// (two levels up from this crate). `None` means "skip".
fn locate_cue() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PROTOTYPE_DISC") {
        let path = PathBuf::from(path);
        return path.exists().then_some(path);
    }

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../PROTOTYPE.cue");
    repo_root.exists().then_some(repo_root)
}

/// Open the image, or print why we are skipping and return `None`.
fn open() -> Option<DiscImage> {
    match locate_cue() {
        Some(cue) => Some(DiscImage::open(&cue).expect("opening the disc image")),
        None => {
            eprintln!(
                "skipping: no disc image (set PROTOTYPE_DISC or place PROTOTYPE.cue at repo root)"
            );
            None
        }
    }
}

#[test]
fn lists_root_files_and_the_fli_subdir() {
    let Some(image) = open() else { return };

    let root_files = image
        .files()
        .iter()
        .filter(|entry| !entry.name.contains('/'))
        .count();
    let fli_files = image
        .files()
        .iter()
        .filter(|entry| entry.name.starts_with("FLI/"))
        .count();
    assert_eq!(root_files, 75, "files in the root directory");
    assert_eq!(fli_files, 12, "files in the FLI/ subdirectory");

    assert!(image.contains("COVER3.PAL"));
    assert!(image.contains("FLI/INTRO.FLI"), "recursed into FLI/");
    assert!(
        image.contains("fli/intro.fli"),
        "lookups are case-insensitive"
    );

    let cover = image
        .files()
        .iter()
        .find(|e| e.name == "COVER3.PAL")
        .unwrap();
    assert_eq!(cover.size, 768);
    let install = image
        .files()
        .iter()
        .find(|e| e.name == "INSTALL.EXE")
        .unwrap();
    assert_eq!(install.size, 47752);
}

#[test]
fn reads_files_to_their_declared_length() {
    let Some(image) = open() else { return };

    // Decoding the bytes (the cross-crate path) lives in `prototype-integration-tests`;
    // here we only prove `read` returns each file's declared byte count.
    assert_eq!(image.read("COVER3.PAL").unwrap().len(), 768);
    assert_eq!(image.read("INSTALL.EXE").unwrap().len(), 47752);
    assert_eq!(image.read("FLI/INTRO.FLI").unwrap().len(), 386376);
}

#[test]
fn audio_tracks_match_the_verified_layout() {
    let Some(image) = open() else { return };

    let tracks = image.audio_tracks();
    let starts: Vec<u32> = tracks.iter().map(|t| t.start_lba).collect();
    assert_eq!(
        starts,
        vec![17709, 33796, 49164, 66466, 84189, 101019, 106804],
        "seven CD-DA tracks at the verified INDEX 01 LBAs"
    );

    for track in tracks {
        assert!(
            track.end_lba > track.start_lba,
            "track {} non-empty",
            track.number
        );
    }

    let track2 = &tracks[0];
    let pcm = image.read_track_pcm(track2).unwrap();
    assert_eq!(pcm.len() % 2352, 0, "PCM is a whole number of raw sectors");
    assert_eq!(
        pcm.len(),
        (track2.end_lba - track2.start_lba) as usize * 2352
    );
}
