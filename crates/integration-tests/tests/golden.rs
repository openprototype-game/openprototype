//! Golden hashes of decoded output, as a regression tripwire.
//!
//! For every file we can decode, we hash the *decoded* bytes (not the source,
//! which would re-distribute the asset) and compare against a committed
//! manifest. A mismatch means a decoder's output changed — confirm the change
//! is intended, then regenerate with `UPDATE_GOLDEN=1`.
//!
//! These hashes are a snapshot of current behaviour, not proof of correctness:
//! pixel/colour fidelity is still unverified, so a hash only says "output is
//! unchanged", which is all a snapshot can honestly claim.

use std::collections::BTreeMap;
use std::path::Path;

use prototype_disc::{AssetSource, DiscImage};
use prototype_formats::color::Palette;
use prototype_formats::{
    Dimensions, Encoding, Flic, SpriteSheet, StartExe, background, bdy, bin, pal, raw, smp,
};
use prototype_integration_tests::{names_with_ext, open_test_image};
use sha2::{Digest, Sha256};

const MANIFEST: &str = "golden.sha256";

#[test]
fn decoded_output_matches_golden() {
    let Some(image) = open_test_image() else {
        return;
    };

    let computed = decoded_hashes(&image);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(MANIFEST);

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        write_manifest(&path, &computed);
        eprintln!(
            "UPDATE_GOLDEN: wrote {} entries to {}",
            computed.len(),
            path.display()
        );
        return;
    }

    let expected = read_manifest(&path);
    let mut failures = Vec::new();

    for (key, hash) in &computed {
        match expected.get(key) {
            Some(want) if want == hash => {}
            Some(want) => failures.push(format!("{key}: golden {want} != decoded {hash}")),
            None => failures.push(format!(
                "{key}: missing from manifest (run UPDATE_GOLDEN=1)"
            )),
        }
    }

    for key in expected.keys() {
        if !computed.iter().any(|(present, _)| present == key) {
            failures.push(format!("{key}: in manifest but no longer decoded"));
        }
    }

    assert!(
        failures.is_empty(),
        "golden mismatch ({} issue(s)):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Decode every file we have a decoder for and hash its output, dropping the
/// decoded bytes immediately so the big FLIs never pile up in memory. Keyed by
/// a stable label, sorted for a deterministic manifest.
fn decoded_hashes(image: &DiscImage) -> Vec<(String, String)> {
    let mut out = Vec::new();

    for name in names_with_ext(image, ".FLI") {
        let bytes = image.read(&name).unwrap();
        out.push((name.clone(), sha256_hex(&all_frame_pixels(&bytes))));
    }

    for name in names_with_ext(image, ".SMP") {
        let bytes = image.read(&name).unwrap();
        out.push((
            name.clone(),
            sha256_hex(&smp::decode(&bytes, Encoding::Signed)),
        ));
    }

    for name in names_with_ext(image, ".PAL") {
        let bytes = image.read(&name).unwrap();
        let palette = pal::decode(&bytes).unwrap();
        out.push((name.clone(), sha256_hex(&palette_bytes(&palette))));
    }

    for sp1 in names_with_ext(image, ".SP1") {
        let stem = &sp1[..sp1.len() - 1]; // drop the "1" of ".SP1"
        let planes: Vec<Vec<u8>> = (1..=4)
            .map(|plane| image.read(&format!("{stem}{plane}")).unwrap())
            .collect();
        let combined =
            background::decode([&planes[0], &planes[1], &planes[2], &planes[3]]).unwrap();
        out.push((format!("{stem}1-4"), sha256_hex(&combined.pixels)));
    }

    let back3 = raw::decode(&image.read("BACK3.RAW").unwrap(), Dimensions::new(320, 200)).unwrap();
    out.push(("BACK3.RAW".to_string(), sha256_hex(&back3.pixels)));

    let surplogo = bdy::decode(
        &image.read("SURPLOGO.BDY").unwrap(),
        Dimensions::new(320, 200),
    )
    .unwrap();
    out.push(("SURPLOGO.BDY".to_string(), sha256_hex(&surplogo.pixels)));

    let menu = StartExe::new(&image.read("START.EXE").unwrap())
        .unwrap()
        .menu_palette()
        .unwrap();
    out.push((
        "START.EXE#menu_palette".to_string(),
        sha256_hex(&palette_bytes(&menu)),
    ));

    // Compiled sprites: both catalogs live in LEVEL_1.WAD (the only level
    // reverse-engineered so far). Hash the whole decoded sheet per BIN.
    let wad = image.read("LEVEL_1.WAD").unwrap();
    let scenery =
        bin::decode_banked(&image.read("OUT.BIN").unwrap(), &wad, bin::OUT_BIN_CATALOG).unwrap();
    out.push(("OUT.BIN".to_string(), sha256_hex(&sheet_bytes(&scenery))));
    let ship = bin::decode_ship(
        &image.read("PTURN1.BN1").unwrap(),
        &wad,
        bin::PTURN1_CATALOG,
    )
    .unwrap();
    out.push(("PTURN1.BN1".to_string(), sha256_hex(&sheet_bytes(&ship))));

    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Serialize a decoded sheet deterministically: per sprite, its size then two
/// bytes per pixel (an opacity flag and the palette index, since transparent
/// and opaque-index-0 are different decodes).
fn sheet_bytes(sheet: &SpriteSheet) -> Vec<u8> {
    let mut bytes = Vec::new();

    for sprite in &sheet.sprites {
        bytes.extend_from_slice(&sprite.size.width.to_le_bytes());
        bytes.extend_from_slice(&sprite.size.height.to_le_bytes());

        for pixel in &sprite.pixels {
            match pixel {
                Some(index) => bytes.extend_from_slice(&[1, *index]),
                None => bytes.extend_from_slice(&[0, 0]),
            }
        }
    }

    bytes
}

/// Decode every frame and concatenate the indexed pixels.
fn all_frame_pixels(bytes: &[u8]) -> Vec<u8> {
    let mut flic = Flic::new(bytes).expect("FLI header");
    let mut pixels = Vec::new();

    while let Some(frame) = flic.next_frame() {
        pixels.extend_from_slice(&frame.expect("frame decodes").image.pixels);
    }

    pixels
}

fn palette_bytes(palette: &Palette) -> Vec<u8> {
    palette
        .colors
        .iter()
        .flat_map(|color| [color.r, color.g, color.b])
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_manifest(path: &Path) -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!(
            "reading {} ({error}); generate it with UPDATE_GOLDEN=1",
            path.display()
        )
    });

    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?.to_string(), fields.next()?.to_string()))
        })
        .collect()
}

fn write_manifest(path: &Path, entries: &[(String, String)]) {
    let mut text = String::new();
    text.push_str("# Golden SHA-256 of DECODED output (not the source files).\n");
    text.push_str("# A snapshot of current decoder behaviour, not proof of correctness.\n");
    text.push_str("# A mismatch means decoded output changed; regenerate with UPDATE_GOLDEN=1.\n");

    for (key, hash) in entries {
        text.push_str(&format!("{key} {hash}\n"));
    }

    std::fs::write(path, text).expect("writing the golden manifest");
}
