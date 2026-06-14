//! Prints the `MANIFEST` table in `prototype-disc`'s `src/manifest.rs`.
//!
//! Reads every file in the data track, hashes it, and emits the entries as
//! Rust source ready to paste into the manifest module. Run from the repo
//! root, or with `PROTOTYPE_DISC` set:
//!
//! ```text
//! cargo run -p openprototype-tools --bin generate_manifest
//! ```

use prototype_disc::{AssetSource, DiscImage, Result};

fn main() -> Result<()> {
    let disc = DiscImage::open_default()?;
    let mut total_bytes = 0usize;

    for name in disc.names() {
        let bytes = disc.read(&name)?;
        total_bytes += bytes.len();
        println!(
            "    ManifestEntry {{ name: {name:?}, size: {}, crc32: 0x{:08x} }},",
            bytes.len(),
            crc32fast::hash(&bytes),
        );
    }

    eprintln!("{} files, {} bytes total", disc.names().len(), total_bytes);

    Ok(())
}
