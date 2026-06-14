//! `.RAW`: uncompressed indexed bitmap, 1 byte per pixel, no header.
//!
//! Dimensions are not stored and must be supplied (e.g. BACK is 320 wide).
//! Palette comes from the active level's `.WAD` for in-game graphics.

use crate::{Dimensions, IndexedImage, Result};

/// Decodes raw indexed pixels into an image of the given dimensions.
pub fn decode(bytes: &[u8], size: Dimensions) -> Result<IndexedImage> {
    IndexedImage::new(size, bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DecodeError;

    #[test]
    fn decodes_when_byte_count_matches() {
        let image = decode(&[1, 2, 3, 4], Dimensions::new(2, 2)).unwrap();
        assert_eq!(image.pixels, vec![1, 2, 3, 4]);
    }

    #[test]
    fn rejects_byte_count_that_does_not_fill_dimensions() {
        let error = decode(&[1, 2, 3], Dimensions::new(2, 2)).unwrap_err();
        assert_eq!(
            error,
            DecodeError::SizeMismatch {
                expected: 4,
                actual: 3
            }
        );
    }
}
