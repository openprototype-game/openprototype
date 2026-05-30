//! Decode every real game asset, sourced from the CD image.
//!
//! Self-describing formats (FLI, SMP, PAL, the four-plane SP backgrounds) are
//! decoded in full over the whole corpus — the loops double as totality checks
//! (no decoder errors on any real file). Formats whose geometry is external
//! (RAW, BDY) or build-specific (EXE) keep a representative case, since you
//! cannot decode them without a per-file width/height the disc does not carry.

use prototype_disc::AssetSource;
use prototype_formats::color::Rgb;
use prototype_formats::{Dimensions, Encoding, Flic, StartExe, background, bdy, pal, raw, smp};
use prototype_integration_tests::{names_with_ext, open_test_image};

#[test]
fn every_fli_decodes_all_frames() {
    let Some(image) = open_test_image() else {
        return;
    };

    let names = names_with_ext(&image, ".FLI");
    assert_eq!(names.len(), 12, "FLI files on the disc");

    for name in &names {
        let bytes = image.read(name).unwrap();
        let mut flic = Flic::new(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));

        let expected = flic.header().frame_count;
        let size = Dimensions::new(flic.header().width, flic.header().height);
        let mut decoded = 0u16;

        while let Some(frame) = flic.next_frame() {
            let frame = frame.unwrap_or_else(|error| panic!("{name} frame {decoded}: {error}"));
            assert_eq!(frame.image.size, size, "{name} frame {decoded} size");
            decoded += 1;
        }

        assert_eq!(decoded, expected, "{name} frame count");
    }
}

#[test]
fn every_smp_decodes_to_full_length() {
    let Some(image) = open_test_image() else {
        return;
    };

    let names = names_with_ext(&image, ".SMP");
    assert_eq!(names.len(), 20, "SMP files on the disc");

    for name in &names {
        let bytes = image.read(name).unwrap();
        assert!(!bytes.is_empty(), "{name} is empty");

        let samples = smp::decode(&bytes, Encoding::Signed);
        assert_eq!(samples.len(), bytes.len(), "{name} sample count");
    }
}

#[test]
fn every_palette_decodes_to_real_colours() {
    let Some(image) = open_test_image() else {
        return;
    };

    let names = names_with_ext(&image, ".PAL");
    assert_eq!(names.len(), 3, "PAL files on the disc");

    for name in &names {
        let bytes = image.read(name).unwrap();
        assert_eq!(bytes.len(), 768, "{name} is 256 RGB triples");

        let palette = pal::decode(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(
            palette.colors.iter().any(|color| *color != Rgb::default()),
            "{name} is not all-black"
        );
    }
}

#[test]
fn every_background_combines_to_640x160() {
    let Some(image) = open_test_image() else {
        return;
    };

    let sp1_names = names_with_ext(&image, ".SP1");
    assert_eq!(sp1_names.len(), 5, "SP background sets on the disc");

    for sp1 in &sp1_names {
        let stem = &sp1[..sp1.len() - 1]; // drop the "1" of ".SP1"
        let planes: Vec<Vec<u8>> = (1..=4)
            .map(|plane| image.read(&format!("{stem}{plane}")).unwrap())
            .collect();

        let combined = background::decode([&planes[0], &planes[1], &planes[2], &planes[3]])
            .unwrap_or_else(|error| panic!("{sp1}: {error}"));
        assert_eq!(combined.size, Dimensions::new(640, 160), "{sp1}");
    }
}

#[test]
fn back3_raw_decodes_at_320x200() {
    let Some(image) = open_test_image() else {
        return;
    };

    let bytes = image.read("BACK3.RAW").unwrap();
    let decoded = raw::decode(&bytes, Dimensions::new(320, 200)).expect("BACK3.RAW is 320x200");
    assert_eq!(decoded.pixels.len(), 64_000);
}

#[test]
fn surplogo_bdy_unpacks_to_320x200() {
    let Some(image) = open_test_image() else {
        return;
    };

    let bytes = image.read("SURPLOGO.BDY").unwrap();
    let decoded = bdy::decode(&bytes, Dimensions::new(320, 200)).expect("SURPLOGO.BDY is 320x200");
    assert_eq!(decoded.pixels.len(), 64_000);
}

#[test]
fn start_exe_menu_palette_decodes() {
    let Some(image) = open_test_image() else {
        return;
    };

    let bytes = image.read("START.EXE").unwrap();
    let palette = StartExe::new(&bytes)
        .expect("START.EXE is the recognized build")
        .menu_palette()
        .expect("menu palette decodes");

    // The menu palette's DAC order is anchored at index 1 = white.
    assert_eq!(palette.colors.len(), 256);
    assert_eq!(
        palette.colors[1],
        Rgb {
            r: 255,
            g: 255,
            b: 255,
        }
    );
}

#[test]
fn fli_decoding_is_deterministic() {
    let Some(image) = open_test_image() else {
        return;
    };

    // A stateful decoder (FLI applies per-frame deltas) is the meaningful place
    // to check determinism: same bytes in, identical frames out.
    let bytes = image.read("FLI/INTRO.FLI").unwrap();
    assert_eq!(
        all_frame_pixels(&bytes),
        all_frame_pixels(&bytes),
        "FLI decode must be deterministic"
    );
}

/// Decode every frame and concatenate the indexed pixels, for equality checks.
fn all_frame_pixels(bytes: &[u8]) -> Vec<u8> {
    let mut flic = Flic::new(bytes).expect("FLI header");
    let mut pixels = Vec::new();

    while let Some(frame) = flic.next_frame() {
        pixels.extend_from_slice(&frame.expect("frame decodes").image.pixels);
    }

    pixels
}
