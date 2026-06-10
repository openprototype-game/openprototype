//! The level screen's geometry: how the Mode X 320x160 frame splits into the
//! visible playfield window and the panel band below it.
//!
//! These are facts about the original's display pipeline, shared by every
//! renderer that draws into the level frame (the parallax background, the
//! scenery walker, the star field, the HUD), so they live here rather than
//! with any one consumer.

/// Screen row of the panel's top edge in the level's 320x160 frame.
///
/// The level runs a hand-programmed Mode X 320x160: 480 scanlines with each row
/// tripled (max-scan-line = 2), giving 160 logical rows. The CRTC line-compare
/// splits at scanline 383 (= row 128), freezing the bottom band for the HUD, and
/// `PANEL.RAW` (320x32) fills it at rows 128..160. Proven from the WAD (the
/// triple-scan write at file `0x2d85`, the line-compare 383 set via routine
/// `0xe285`) and confirmed against DOSBox-X's live mode readout `G320><160>480`.
pub const PANEL_TOP: i32 = 128;

/// Left edge of the visible playfield window, in pixels.
///
/// The original composes the playfield into a system buffer and blits only
/// bytes 4..76 of each plane row (72 bytes = 288 px) of the 128 playfield rows
/// to VGA, leaving 16-pixel black bars on both sides; the panel below is
/// full-width. Everything inside the window is placed in absolute screen
/// coordinates: the strip compositor and the tilemap walker both start at byte
/// 4 (`mov di, 4` / `si = row*80 + 4`), so a layer's scroll position appears at
/// x 16, and the star plotter writes `y*80 + x/4` unclipped with its respawn
/// window already spanning x 16..304. Whatever bleeds into the buffer's bar
/// bytes is dropped by the blit.
pub const LEFT: i32 = 16;

/// Width of the visible playfield window (see [`LEFT`]).
pub const WIDTH: i32 = 288;
