//! Reads the original *Prototype* CD image (bin/cue) directly.
//!
//! The game shipped as a mixed-mode CD: a `MODE1/2352` ISO9660 data track with
//! the game files, plus CD-DA tracks holding the soundtrack. Rather than bundle
//! the copyrighted files, the port reads them straight off the image the user
//! supplies (see the project README for where to obtain it).
//!
//! [`DiscImage`] is the concrete backend; consumers that only need game files
//! take a [`&dyn AssetSource`](AssetSource), so a future loose-files backend can
//! stand in without depending on the disc layout.

pub(crate) mod cue;
pub(crate) mod disc;
pub(crate) mod error;
pub(crate) mod iso9660;
pub mod manifest;
pub(crate) mod sector;

pub use disc::{AudioTrack, DiscImage, FileEntry};
pub use error::{DiscError, Result};

/// A source of named game files.
///
/// Names are canonical: uppercase, the `;1` version suffix stripped, and `/`
/// separating the `FLI` subdirectory, e.g. `"INTRO.FLI"` (root) or
/// `"FLI/INTRO.FLI"`. Lookups are case-insensitive and tolerate a trailing
/// `;1`. Reads return owned bytes (data-track sectors are assembled into a
/// contiguous buffer), which the `prototype-formats` decoders accept as
/// `&[u8]`.
pub trait AssetSource {
    /// Reads a file by canonical name.
    ///
    /// Errors with [`DiscError::FileNotFound`] if absent.
    fn read(&self, name: &str) -> Result<Vec<u8>>;

    /// Whether a file with the given name exists.
    fn contains(&self, name: &str) -> bool;

    /// The canonical names of every file, in directory order.
    fn names(&self) -> Vec<String>;
}
