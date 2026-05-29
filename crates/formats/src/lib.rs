//! Decoders for Prototype's on-disk file formats.
//!
//! Pure decoding only: no rendering, no audio device, no SDL. Everything here
//! is testable against the original 1995 game files.
//!
//! The format details come from the game's original developer; see the
//! per-format notes under `reference/formats/`.

pub mod bdy;
pub mod fli;
pub mod pal;
pub mod raw;
pub mod smp;
pub mod sprite;
pub mod wad;
