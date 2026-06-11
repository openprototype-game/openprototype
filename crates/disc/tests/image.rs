//! Integration tests against the real CD image.
//!
//! These need the original CD image, so they are gated behind the `disc-tests`
//! feature and `#[ignore]`d without it (a plain `cargo test` lists them as
//! ignored, never as passed). Run them with `--features disc-tests` after
//! setting `$PROTOTYPE_DISC` to the cue path or dropping `PROTOTYPE.cue`/`.bin`
//! at the repo root.

use std::path::{Path, PathBuf};

use prototype_disc::{AssetSource, DiscImage, manifest};

/// Locate a cue: `$PROTOTYPE_DISC` first, then `PROTOTYPE.cue` at the repo root
/// (two levels up from this crate).
fn locate_cue() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PROTOTYPE_DISC") {
        let path = PathBuf::from(path);
        return path.exists().then_some(path);
    }

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../PROTOTYPE.cue");
    repo_root.exists().then_some(repo_root)
}

/// Open the image. These tests only run with `disc-tests` enabled, which is the
/// caller asserting the image is present, so a missing image is a hard error.
fn open() -> DiscImage {
    let cue = locate_cue()
        .expect("no disc image (set PROTOTYPE_DISC or place PROTOTYPE.cue at the repo root)");
    DiscImage::open(&cue).expect("opening the disc image")
}

#[test]
#[cfg_attr(not(feature = "disc-tests"), ignore = "requires the disc image")]
fn lists_root_files_and_the_fli_subdir() {
    let image = open();

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
#[cfg_attr(not(feature = "disc-tests"), ignore = "requires the disc image")]
fn reads_files_to_their_declared_length() {
    let image = open();

    // Decoding the bytes (the cross-crate path) lives in `openprototype-integration-tests`;
    // here we only prove `read` returns each file's declared byte count.
    assert_eq!(image.read("COVER3.PAL").unwrap().len(), 768);
    assert_eq!(image.read("INSTALL.EXE").unwrap().len(), 47752);
    assert_eq!(image.read("FLI/INTRO.FLI").unwrap().len(), 386376);
}

#[test]
#[cfg_attr(not(feature = "disc-tests"), ignore = "requires the disc image")]
fn audio_tracks_match_the_verified_layout() {
    let image = open();

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

#[test]
#[cfg_attr(not(feature = "disc-tests"), ignore = "requires the disc image")]
fn manifest_matches_the_image() {
    let image = open();

    let mismatches = manifest::verify(&image).expect("reading the data track");
    assert!(
        mismatches.is_empty(),
        "the image deviates from the manifest:\n{}",
        mismatches
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );

    // The manifest covers the whole data track; if it ever lags behind the
    // file list, the startup check silently loses coverage.
    assert_eq!(
        manifest::MANIFEST.len(),
        image.names().len(),
        "one manifest entry per data-track file"
    );
}
