//! Level backgrounds (`.SP1`..`.SP4`).
//!
//! A level background is a 640x160 256-color image (two screens wide) stored
//! across four files as VGA "Mode X" byte planes: plane `p` holds the pixels
//! where `x % 4 == p`, so SP1..SP4 are planes 0..3. Each plane file is
//! `(640 / 4) * 160` = 25600 bytes.
//!
//! Verified by combining and rendering the CANYON, WALD, RACEB2 and ALIENBG
//! sets; the four files form one coherent image, not four parallax layers as
//! the developer mail guessed.

use crate::error::DecodeError;
use crate::{Dimensions, IndexedImage, Result};

/// Background dimensions: 640 wide, 160 tall.
pub(crate) const BACKGROUND_SIZE: Dimensions = Dimensions {
    width: 640,
    height: 160,
};

/// Bytes in a single plane file (`(640 / 4) * 160`).
const PLANE_LEN: usize = (640 / 4) * 160;

/// Combines the four Mode X plane files (SP1..SP4, in order) into one image.
pub fn decode(planes: [&[u8]; 4]) -> Result<IndexedImage> {
    for plane in planes {
        if plane.len() != PLANE_LEN {
            return Err(DecodeError::UnexpectedLength {
                expected: PLANE_LEN,
                actual: plane.len(),
            });
        }
    }

    let width = BACKGROUND_SIZE.width as usize;
    let height = BACKGROUND_SIZE.height as usize;
    let plane_width = width / 4;
    let mut pixels = vec![0u8; BACKGROUND_SIZE.pixel_count()];

    for y in 0..height {
        for x in 0..width {
            pixels[y * width + x] = planes[x % 4][y * plane_width + x / 4];
        }
    }

    IndexedImage::new(BACKGROUND_SIZE, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deinterleaves_planes_by_column() {
        let data: Vec<Vec<u8>> = (0..4).map(|p| vec![p as u8 + 1; PLANE_LEN]).collect();
        let image = decode([&data[0], &data[1], &data[2], &data[3]]).unwrap();

        assert_eq!(image.size, BACKGROUND_SIZE);
        // Column x reads from plane x % 4, which holds value (x % 4) + 1.
        assert_eq!(&image.pixels[0..5], &[1, 2, 3, 4, 1]);
    }

    #[test]
    fn rejects_plane_of_wrong_length() {
        let full = vec![0u8; PLANE_LEN];
        let short = vec![0u8; PLANE_LEN - 1];
        let error = decode([&full, &full, &full, &short]).unwrap_err();

        assert_eq!(
            error,
            DecodeError::UnexpectedLength {
                expected: PLANE_LEN,
                actual: PLANE_LEN - 1,
            }
        );
    }
}
