//! Audio commands the core emits.
//!
//! The original game's only music is CD-DA: track 1 is data, tracks 2-8 are the
//! seven songs, driven through MSCDEX. The core never opens an audio device. It
//! emits these commands from [`step`](crate::game::Game::step) and the
//! platform drains and executes them, keeping the core free of any audio
//! backend.

/// A request to change what music is playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCommand {
    /// Start (or restart) the given CD-DA track, playing it once (the original
    /// game does not loop its music).
    PlayTrack(u8),
    /// Stop whatever is playing.
    StopMusic,
}
