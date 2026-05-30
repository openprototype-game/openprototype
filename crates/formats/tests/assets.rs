//! Smoke tests against the original game files under `assets/game/`.
//!
//! These assert structural facts (sizes, that decoding succeeds), not pixel
//! values: the colour-correctness assumptions stay unpinned until verified
//! visually.

use std::path::{Path, PathBuf};

use prototype_formats::color::Rgb;
use prototype_formats::{Dimensions, Encoding, Flic, StartExe, background, bdy, pal, raw, smp};

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

#[test]
fn surplogo_bdy_unpacks_to_320x200() {
    let bytes = std::fs::read(asset("SURPLOGO.BDY")).unwrap();

    let image =
        bdy::decode(&bytes, Dimensions::new(320, 200)).expect("SURPLOGO.BDY should be 320x200");
    assert_eq!(image.pixels.len(), 64_000);
}

#[test]
fn start_exe_menu_palette_decodes() {
    let bytes = std::fs::read(asset("START.EXE")).unwrap();

    let palette = StartExe::new(&bytes)
        .expect("START.EXE should be the recognized build")
        .menu_palette()
        .expect("menu palette should decode");

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
fn every_fli_decodes_all_frames() {
    let files = [
        "INTRO.FLI",
        "CANYON.FLI",
        "CREDZ.FLI",
        "FLY.FLI",
        "GO2.FLI",
        "HIGHSCOR.FLI",
        "LAVA.FLI",
    ];

    for name in files {
        let bytes = std::fs::read(asset(&format!("FLI/{name}"))).unwrap();
        let mut flic = Flic::new(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));

        let expected = flic.header().frame_count;
        let size = Dimensions::new(flic.header().width, flic.header().height);
        assert_eq!(size, Dimensions::new(320, 200), "{name} dimensions");

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
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/game");
    let mut count = 0;

    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let is_smp = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("smp"));

        if !is_smp {
            continue;
        }

        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.is_empty(), "{} is empty", path.display());

        let samples = smp::decode(&bytes, Encoding::Signed);
        assert_eq!(
            samples.len(),
            bytes.len(),
            "{} sample count",
            path.display()
        );
        count += 1;
    }

    assert_eq!(count, 20, "expected 20 .SMP files");
}

#[test]
fn canyon_background_combines_to_640x160() {
    let planes: Vec<Vec<u8>> = (1..=4)
        .map(|n| std::fs::read(asset(&format!("CANYON.SP{n}"))).unwrap())
        .collect();

    let image = background::decode([&planes[0], &planes[1], &planes[2], &planes[3]])
        .expect("CANYON.SP1-4 should combine");
    assert_eq!(image.size, Dimensions::new(640, 160));
}
