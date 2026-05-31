//! Music playback behind a trait.
//!
//! The core emits [`AudioCommand`](crate::core::audio::AudioCommand)s; the
//! platform turns them into device calls through this trait. The shell ships a
//! logging stub so the menu runs without an audio device. The real CD-DA player
//! (rodio over the disc's PCM tracks) replaces [`LoggingMusicPlayer`] when the
//! jukebox lands, without the core changing.

/// Plays the game's CD-DA music.
pub trait MusicPlayer {
    /// Start (or restart) the given track and loop it.
    fn play_track(&mut self, track: u8);

    /// Stop whatever is playing.
    fn stop(&mut self);
}

/// A no-op player that logs the commands it receives.
pub struct LoggingMusicPlayer;

impl MusicPlayer for LoggingMusicPlayer {
    fn play_track(&mut self, track: u8) {
        eprintln!("[audio] play track {track}");
    }

    fn stop(&mut self) {
        eprintln!("[audio] stop");
    }
}
