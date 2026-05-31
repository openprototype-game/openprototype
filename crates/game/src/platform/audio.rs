//! Music playback behind a trait.
//!
//! The core emits [`AudioCommand`](crate::core::audio::AudioCommand)s; the
//! platform turns them into device calls through this trait. [`make_music_player`]
//! picks the real CD-DA player ([`RodioMusicPlayer`], built with the `audio`
//! feature) or, failing that, the silent [`LoggingMusicPlayer`]. The core never
//! sees any of this.

use std::sync::Arc;

use prototype_disc::DiscImage;

/// Plays the game's CD-DA music.
pub trait MusicPlayer {
    /// Start (or restart) the given track, playing it once.
    fn play_track(&mut self, track: u8);

    /// Stop whatever is playing.
    fn stop(&mut self);
}

/// A no-op player that logs the commands it receives. Used when the `audio`
/// feature is off or no audio device could be opened.
pub struct LoggingMusicPlayer;

impl MusicPlayer for LoggingMusicPlayer {
    fn play_track(&mut self, track: u8) {
        eprintln!("[audio] play track {track}");
    }

    fn stop(&mut self) {
        eprintln!("[audio] stop");
    }
}

/// Build the best available music player for `disc`. With the `audio` feature
/// this is a [`RodioMusicPlayer`]; if the audio device cannot be opened (or the
/// feature is off) it falls back to [`LoggingMusicPlayer`] so the app still runs.
#[cfg(feature = "audio")]
pub fn make_music_player(disc: Arc<DiscImage>) -> Box<dyn MusicPlayer> {
    match RodioMusicPlayer::new(disc) {
        Ok(player) => Box::new(player),
        Err(error) => {
            eprintln!("[audio] no audio output ({error:#}); music disabled");
            Box::new(LoggingMusicPlayer)
        }
    }
}

#[cfg(not(feature = "audio"))]
pub fn make_music_player(_disc: Arc<DiscImage>) -> Box<dyn MusicPlayer> {
    Box::new(LoggingMusicPlayer)
}

#[cfg(feature = "audio")]
mod rodio_backend {
    use std::num::{NonZeroU16, NonZeroU32};
    use std::sync::Arc;
    use std::time::Duration;

    use anyhow::{Context, Result};
    use prototype_disc::{AudioTrack, DiscImage};
    use rodio::source::Source;
    use rodio::{ChannelCount, DeviceSinkBuilder, MixerDeviceSink, Player, Sample, SampleRate};

    use super::MusicPlayer;
    use crate::assets::append_pcm_i16_le_as_f32;

    /// Red-book CD-DA: 44100 Hz, 2 channels.
    const SAMPLE_RATE: u32 = 44100;
    const CHANNELS: u16 = 2;

    /// Sectors decoded per refill (~150 KB, ~0.85 s of audio). Small enough that
    /// a refill is a couple of milliseconds even in a debug build, so the read
    /// never stalls the audio pull noticeably; large enough to keep syscalls rare.
    const CHUNK_SECTORS: u32 = 64;

    /// Plays the disc's CD-DA tracks through the default audio device, each
    /// once (the original game does not loop). The [`MixerDeviceSink`] must stay
    /// alive for playback; dropping it (or the current [`Player`]) stops the sound.
    pub struct RodioMusicPlayer {
        disc: Arc<DiscImage>,
        sink: MixerDeviceSink,
        current: Option<Player>,
    }

    impl RodioMusicPlayer {
        pub fn new(disc: Arc<DiscImage>) -> Result<Self> {
            let sink = DeviceSinkBuilder::open_default_sink().context("opening audio output")?;

            Ok(Self {
                disc,
                sink,
                current: None,
            })
        }
    }

    impl MusicPlayer for RodioMusicPlayer {
        fn play_track(&mut self, track: u8) {
            // Dropping the old Player stops it, so two tracks never overlap.
            self.current = None;

            let Some(source) = TrackSource::new(self.disc.clone(), track) else {
                eprintln!("[audio] disc has no audio track {track}");
                return;
            };

            let player = Player::connect_new(self.sink.mixer());
            player.append(source);
            self.current = Some(player);
        }

        fn stop(&mut self) {
            self.current = None;
        }
    }

    /// A rodio [`Source`] that streams one CD-DA track straight off the disc,
    /// decoding a chunk at a time as rodio pulls samples and ending at the track
    /// end. No upfront read or whole-track buffer: it holds ~one chunk.
    ///
    /// The track plays once and stops, matching the original (MSCDEX plays the
    /// track's sector range once; nothing in `START.EXE` re-triggers it).
    ///
    /// `next` runs on rodio's playback thread, so the per-chunk disc read happens
    /// there (the same pattern as rodio's own file decoder), never on the main
    /// loop.
    struct TrackSource {
        disc: Arc<DiscImage>,
        track_number: u8,
        end_lba: u32,
        next_lba: u32,
        buffer: Vec<f32>,
        position: usize,
        ended: bool,
    }

    impl TrackSource {
        fn new(disc: Arc<DiscImage>, track_number: u8) -> Option<Self> {
            let track = disc
                .audio_tracks()
                .iter()
                .find(|track| track.number == track_number)?;
            let (start_lba, end_lba) = (track.start_lba, track.end_lba);

            Some(Self {
                disc,
                track_number,
                end_lba,
                next_lba: start_lba,
                buffer: Vec::new(),
                position: 0,
                ended: false,
            })
        }

        /// Read and decode the next chunk into `buffer`. Sets `ended` once the
        /// track's last sector has been read (the track plays once, no loop) or
        /// on a read error.
        fn refill(&mut self) {
            if self.next_lba >= self.end_lba {
                self.ended = true;
                return;
            }

            let chunk_end = self
                .next_lba
                .saturating_add(CHUNK_SECTORS)
                .min(self.end_lba);
            let chunk = AudioTrack {
                number: self.track_number,
                start_lba: self.next_lba,
                end_lba: chunk_end,
            };

            match self.disc.read_track_pcm(&chunk) {
                Ok(bytes) => {
                    self.buffer.clear();
                    append_pcm_i16_le_as_f32(&bytes, &mut self.buffer);
                    self.position = 0;
                    self.next_lba = chunk_end;
                }
                Err(error) => {
                    eprintln!("[audio] track {} read failed: {error:#}", self.track_number);
                    self.ended = true;
                }
            }
        }
    }

    impl Iterator for TrackSource {
        type Item = Sample;

        fn next(&mut self) -> Option<Sample> {
            if self.position >= self.buffer.len() {
                self.refill();

                if self.ended || self.buffer.is_empty() {
                    return None;
                }
            }

            let sample = self.buffer[self.position];
            self.position += 1;
            Some(sample)
        }
    }

    impl Source for TrackSource {
        fn current_span_len(&self) -> Option<usize> {
            // Channel count and sample rate never change, so there is no span
            // boundary to report.
            None
        }

        fn channels(&self) -> ChannelCount {
            NonZeroU16::new(CHANNELS).expect("CHANNELS is non-zero")
        }

        fn sample_rate(&self) -> SampleRate {
            NonZeroU32::new(SAMPLE_RATE).expect("SAMPLE_RATE is non-zero")
        }

        fn total_duration(&self) -> Option<Duration> {
            // The track loops, so it has no finite duration.
            None
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Frames per CD sector (2352 bytes / 4 bytes per stereo frame) times the
        /// two interleaved channels.
        const SAMPLES_PER_SECTOR: usize = 588 * CHANNELS as usize;

        #[test]
        fn streams_and_refills_across_chunk_boundaries() {
            let Ok(disc) = DiscImage::open_default() else {
                eprintln!("skipping: no disc image (set PROTOTYPE_DISC)");
                return;
            };

            let mut source = TrackSource::new(Arc::new(disc), 2).expect("track 2 exists");

            // Pull past one chunk so the source must refill at least once.
            let wanted = CHUNK_SECTORS as usize * SAMPLES_PER_SECTOR + 1000;
            let pulled: Vec<f32> = source.by_ref().take(wanted).collect();

            assert_eq!(pulled.len(), wanted, "source underran while refilling");
            assert!(
                pulled.iter().all(|sample| (-1.0..1.0).contains(sample)),
                "every streamed sample must be normalized into [-1.0, 1.0)"
            );
        }

        #[test]
        fn plays_once_then_ends() {
            let Ok(disc) = DiscImage::open_default() else {
                eprintln!("skipping: no disc image (set PROTOTYPE_DISC)");
                return;
            };

            let disc = Arc::new(disc);
            let start_lba = disc
                .audio_tracks()
                .iter()
                .find(|track| track.number == 2)
                .expect("track 2 exists")
                .start_lba;

            // A deliberately tiny window so the track end is reached after a
            // couple of sectors instead of the whole multi-minute track.
            const WINDOW_SECTORS: u32 = 2;
            let mut source = TrackSource {
                disc,
                track_number: 2,
                end_lba: start_lba + WINDOW_SECTORS,
                next_lba: start_lba,
                buffer: Vec::new(),
                position: 0,
                ended: false,
            };

            // Ask for far more than the window; the source must stop at the end
            // (the original plays each track once, with no loop).
            let window = WINDOW_SECTORS as usize * SAMPLES_PER_SECTOR;
            let pulled: Vec<f32> = source.by_ref().take(window * 4).collect();

            assert_eq!(
                pulled.len(),
                window,
                "track should play exactly once (its length) and then end"
            );
        }
    }
}

#[cfg(feature = "audio")]
pub use rodio_backend::RodioMusicPlayer;
