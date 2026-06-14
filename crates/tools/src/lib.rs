//! Asset inspection and extraction tools.
//!
//! CLI binaries go under `src/bin/` (one file per tool) and share the
//! decoders from `prototype-formats`. Asset bytes are read through
//! [`read_asset`], which transparently sources them from a CD image (`--cue`)
//! or the filesystem.

use std::path::Path;

use anyhow::{Context, Result};
use prototype_disc::{AssetSource, DiscImage};

/// Opens the disc image at `cue`, or stays in filesystem mode if none is given.
///
/// Filesystem mode returns `None`. A missing `--cue` never implicitly opens
/// `PROTOTYPE.cue`.
pub fn open_source(cue: Option<&Path>) -> Result<Option<DiscImage>> {
    match cue {
        Some(path) => {
            let image = DiscImage::open(path)
                .with_context(|| format!("opening disc image {}", path.display()))?;
            Ok(Some(image))
        }
        None => Ok(None),
    }
}

/// Reads an asset by name.
///
/// With a disc `source`, `input` is a canonical asset name (e.g.
/// `FLI/INTRO.FLI`); otherwise it is a filesystem path.
pub fn read_asset(source: Option<&DiscImage>, input: &Path) -> Result<Vec<u8>> {
    match source {
        Some(image) => {
            let name = input.to_string_lossy();
            image
                .read(&name)
                .with_context(|| format!("reading {name} from disc image"))
        }
        None => std::fs::read(input).with_context(|| format!("reading {}", input.display())),
    }
}
