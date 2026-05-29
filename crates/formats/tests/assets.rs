//! Smoke tests against the original game files under `assets/game/`.
//!
//! These assert structural facts (sizes, that decoding succeeds), not pixel
//! values: the colour-correctness assumptions stay unpinned until verified
//! visually.

use std::path::{Path, PathBuf};

use prototype_formats::color::Rgb;
use prototype_formats::{Dimensions, pal, raw};

fn asset(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/game")
        .join(name)
}

#[test]
fn cover3_palette_decodes_to_real_colours() {
    let bytes = std::fs::read(asset("COVER3.PAL")).unwrap();
    assert_eq!(bytes.len(), 768);

    let palette = pal::decode(&bytes).unwrap();
    assert!(palette.colors.iter().any(|color| *color != Rgb::default()));
}

#[test]
fn back3_raw_decodes_at_320x200() {
    let bytes = std::fs::read(asset("BACK3.RAW")).unwrap();

    let image =
        raw::decode(&bytes, Dimensions::new(320, 200)).expect("BACK3.RAW should be 320x200");
    assert_eq!(image.pixels.len(), 64_000);
}
