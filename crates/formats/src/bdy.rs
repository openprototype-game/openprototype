//! `.BDY`: IFF ILBM BODY chunk, RLE-compressed (ByteRun1).
//!
//! Decodes to an indexed bitmap; render with the paired `.PAL`. Reference:
//! the ILBM spec (PackBits-style runs). Source: developer mail (Erik Pojar).
