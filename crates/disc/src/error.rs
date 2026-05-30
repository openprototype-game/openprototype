//! Error type shared by the disc reader and the [`AssetSource`] trait.
//!
//! [`AssetSource`]: crate::AssetSource

use thiserror::Error;

/// Convenience alias: every fallible operation in this crate returns this.
pub type Result<T> = std::result::Result<T, DiscError>;

#[derive(Debug, Error)]
pub enum DiscError {
    /// Wraps an underlying filesystem error (opening or reading the image).
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The `.cue` sheet could not be parsed.
    #[error("cue parse error: {0}")]
    Cue(String),
    /// The data track is not an ISO9660 volume (no `CD001` magic).
    #[error("not an ISO9660 volume")]
    NotIso9660,
    /// A file with the given canonical name is not present in the image.
    #[error("file not found in image: {0}")]
    FileNotFound(String),
    /// The image is shorter than its structures claim, or otherwise malformed.
    #[error("disc image is truncated or malformed: {0}")]
    Malformed(&'static str),
}
