//! Decoders for Prototype's on-disk file formats.
//!
//! Pure decoding only: no rendering, no audio device, no SDL. Everything here
//! is testable against the original 1995 game files.
//!
//! The format details come from the game's original developer; see the
//! per-format notes under `reference/formats/`.

pub mod color;
pub mod error;
pub mod image;

pub mod background;
pub mod bdy;
pub mod bin;
pub mod fli;
pub mod font;
pub mod high;
pub mod pal;
pub mod raw;
pub mod smp;
pub mod start_exe;
pub mod wad;

pub use bin::{Sprite, SpriteSheet};
pub use color::{Palette, Rgb};
pub use error::{DecodeError, Result};
pub use fli::Flic;
pub use high::{Highscore, Highscores};
pub use image::{Dimensions, IndexedImage};
pub use smp::Encoding;
pub use start_exe::StartExe;
