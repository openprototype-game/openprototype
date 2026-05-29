//! `.BDY`: IFF ILBM BODY chunk, RLE-compressed (ByteRun1 / PackBits).
//!
//! Just the BODY payload, not a full IFF FORM, so there is no header giving
//! dimensions: the caller supplies them. The decompressed bytes are chunky 8bpp
//! (one palette index per pixel).
//! Source: developer mail (Erik Pojar).

use crate::{Dimensions, IndexedImage, Result};

/// Decode an RLE BODY chunk into an indexed image of the given dimensions.
pub fn decode(bytes: &[u8], size: Dimensions) -> Result<IndexedImage> {
    let _ = (bytes, size);
    todo!("ByteRun1 unpack, then validate against size")
}
