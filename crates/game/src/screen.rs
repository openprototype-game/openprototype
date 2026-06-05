//! The front-end's display size: VGA mode 13h, 320x200 indexed.
//!
//! These are the front-end's chosen dimensions, not a core invariant: the core
//! framebuffer is sized per scene, and the in-game modes (Mode X 320x160, the
//! 320x400 cover) use other sizes. They live here because the menu, intro and
//! high-score scenes all draw into a 320x200 frame.

/// Mode 13h width.
pub const SCREEN_WIDTH: u32 = 320;
/// Mode 13h height.
pub const SCREEN_HEIGHT: u32 = 200;
