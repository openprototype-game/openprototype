//! Cross-crate tests that decode real game assets sourced from the CD image.
//!
//! These wire `prototype-disc` (the byte source) to `prototype-formats` (the
//! decoders), which is why they live here rather than inside either crate:
//! it keeps `formats` image-free and `disc` free of a `formats` dependency.
//!
//! These need the original CD image, so each is gated behind the `disc-tests`
//! feature and `#[ignore]`d without it. Run them with `--features disc-tests`
//! (see [`open_test_image`]).

use std::path::{Path, PathBuf};

use prototype_disc::{AssetSource, DiscImage};

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

/// Open the disc image. Callers are the `disc-tests`-gated suite, so reaching
/// this without an image is a hard error, not a silent skip.
pub fn open_test_image() -> DiscImage {
    let cue = locate_cue()
        .expect("no disc image (set PROTOTYPE_DISC or place PROTOTYPE.cue at the repo root)");
    DiscImage::open(&cue).expect("opening the disc image")
}

/// Canonical names of every file whose name ends with `ext` (case-insensitive,
/// e.g. `".FLI"`), sorted for deterministic iteration.
pub fn names_with_ext(image: &DiscImage, ext: &str) -> Vec<String> {
    let ext = ext.to_ascii_uppercase();
    let mut names: Vec<String> = image
        .names()
        .into_iter()
        .filter(|name| name.to_ascii_uppercase().ends_with(&ext))
        .collect();
    names.sort();
    names
}
