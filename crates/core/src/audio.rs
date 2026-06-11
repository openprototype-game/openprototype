//! Audio commands the core emits.
//!
//! The original game's only music is CD-DA: track 1 is data, tracks 2-8 are the
//! seven songs, driven through MSCDEX. Sound effects are the `.SMP` files
//! (signed 8-bit mono at 11111 Hz), mixed by the level engine into three
//! channels on the Sound Blaster. The core never opens an audio device. It
//! emits these commands from [`step`](crate::game::Game::step) and the
//! platform drains and executes them, keeping the core free of any audio
//! backend.

use std::sync::Arc;

/// The sound-effect mixer's channel count. The original's DMA feed adds
/// exactly three sample streams (explosions, player weapons, events) into
/// the playback buffer.
pub const SFX_CHANNELS: usize = 3;

/// A request to change what is playing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioCommand {
    /// Start (or restart) the given CD-DA track, playing it once (the original
    /// game does not loop its music).
    PlayTrack(u8),
    /// Stop whatever music is playing.
    StopMusic,
    /// Play a sound effect on a mixer channel, replacing whatever the channel
    /// holds (the original's triggers overwrite the channel registers with no
    /// fade or priority).
    PlaySfx(PlaySfx),
    /// Stop a channel's loop: the current pass plays to its end and the
    /// channel then frees, like the original clearing its loop flag.
    EndSfxLoop {
        /// The channel whose loop ends, `0..`[`SFX_CHANNELS`].
        channel: usize,
    },
}

/// One sound-effect trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaySfx {
    /// The mixer channel to play on, `0..`[`SFX_CHANNELS`].
    pub channel: usize,
    /// The sample: signed 8-bit mono at 11111 Hz, already cut to the
    /// trigger's authored length.
    pub sample: Arc<[i8]>,
    /// Restart the sample at its end instead of freeing the channel, until
    /// [`AudioCommand::EndSfxLoop`] or a replacing trigger.
    pub looped: bool,
}
