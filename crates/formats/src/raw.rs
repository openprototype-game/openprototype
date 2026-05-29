//! `.RAW`: uncompressed indexed bitmap, 1 byte per pixel, no header.
//!
//! Dimensions are not stored and must be supplied (e.g. BACK is 320 wide).
//! Palette comes from the active level's `.WAD` for in-game graphics.

use crate::{Dimensions, IndexedImage, Result};

/// Decode raw indexed pixels into an image of the given dimensions.
pub fn decode(bytes: &[u8], size: Dimensions) -> Result<IndexedImage> {
    IndexedImage::new(size, bytes.to_vec())
}
