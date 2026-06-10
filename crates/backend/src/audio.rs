//! Music and sound-effect playback behind traits.
//!
//! The core emits [`AudioCommand`](openprototype_core::audio::AudioCommand)s; the
//! platform turns them into device calls through these traits.
//! [`make_music_player`] and [`make_sfx_player`] pick the real rodio-backed
//! players (built with the `audio` feature) or, failing that, the silent null
//! players. The core never sees any of this.

use std::sync::Arc;

use prototype_disc::DiscImage;
use tracing::debug;
#[cfg(feature = "audio")]
use tracing::warn;

/// Plays the game's CD-DA music.
pub trait MusicPlayer {
    /// Start (or restart) the given track, playing it once.
    fn play_track(&mut self, track: u8);

    /// Stop whatever is playing.
    fn stop(&mut self);
}

/// A silent player used when the `audio` feature is off or no audio device
/// could be opened. It announces once that music is disabled, then ignores
/// every command, so the app runs without sound and without log noise.
pub struct NullMusicPlayer;

// `new` logs a one-time notice, so a derived `Default` would hide that side
// effect behind an implicit call.
#[allow(clippy::new_without_default)]
impl NullMusicPlayer {
    pub fn new() -> Self {
        debug!("music disabled");
        Self
    }
}

impl MusicPlayer for NullMusicPlayer {
    fn play_track(&mut self, _track: u8) {}

    fn stop(&mut self) {}
}

/// Build the best available music player for `disc`. With the `audio` feature
/// this is a [`RodioMusicPlayer`]; if the audio device cannot be opened (or the
/// feature is off) it falls back to [`NullMusicPlayer`] so the app still runs.
#[cfg(feature = "audio")]
pub fn make_music_player(disc: Arc<DiscImage>) -> Box<dyn MusicPlayer> {
    match RodioMusicPlayer::new(disc) {
        Ok(player) => Box::new(player),
        Err(error) => {
            warn!("no audio output ({error:#}); music disabled");
            Box::new(NullMusicPlayer::new())
        }
    }
}

#[cfg(not(feature = "audio"))]
pub fn make_music_player(_disc: Arc<DiscImage>) -> Box<dyn MusicPlayer> {
    Box::new(NullMusicPlayer::new())
}

/// Plays the game's sound effects: a three-channel sample mixer (see
/// [`SFX_CHANNELS`](openprototype_core::audio::SFX_CHANNELS)), matching the
/// original's DMA feed. A play replaces whatever its channel holds.
pub trait SfxPlayer {
    /// Play a sample (signed 8-bit mono, 22222 Hz) on `channel`, replacing
    /// the channel's current sound. A looped sample restarts at its end until
    /// [`end_loop`](SfxPlayer::end_loop) or a replacing play.
    fn play(&mut self, channel: usize, sample: Arc<[i8]>, looped: bool);

    /// End a channel's loop: the current pass plays to its end and the
    /// channel then frees.
    fn end_loop(&mut self, channel: usize);
}

/// A silent sound-effect player used when the `audio` feature is off or no
/// audio device could be opened.
pub struct NullSfxPlayer;

// `new` logs a one-time notice, so a derived `Default` would hide that side
// effect behind an implicit call.
#[allow(clippy::new_without_default)]
impl NullSfxPlayer {
    pub fn new() -> Self {
        debug!("sound effects disabled");
        Self
    }
}

impl SfxPlayer for NullSfxPlayer {
    fn play(&mut self, _channel: usize, _sample: Arc<[i8]>, _looped: bool) {}

    fn end_loop(&mut self, _channel: usize) {}
}

/// Build the best available sound-effect player. With the `audio` feature this
/// is a [`RodioSfxPlayer`]; if the audio device cannot be opened (or the
/// feature is off) it falls back to [`NullSfxPlayer`] so the app still runs.
#[cfg(feature = "audio")]
pub fn make_sfx_player() -> Box<dyn SfxPlayer> {
    match RodioSfxPlayer::new() {
        Ok(player) => Box::new(player),
        Err(error) => {
            warn!("no audio output ({error:#}); sound effects disabled");
            Box::new(NullSfxPlayer::new())
        }
    }
}

#[cfg(not(feature = "audio"))]
pub fn make_sfx_player() -> Box<dyn SfxPlayer> {
    Box::new(NullSfxPlayer::new())
}

#[cfg(feature = "audio")]
mod rodio_backend {
    use std::num::{NonZeroU16, NonZeroU32};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use anyhow::{Context, Result};
    use prototype_disc::{AudioTrack, DiscImage};
    use rodio::source::Source;
    use rodio::{ChannelCount, DeviceSinkBuilder, MixerDeviceSink, Player, Sample, SampleRate};
    use tracing::{error, warn};

    use openprototype_core::audio::SFX_CHANNELS;
    use prototype_formats::pcm::append_i16_le_to_f32;

    use super::{MusicPlayer, SfxPlayer};

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
            let mut sink =
                DeviceSinkBuilder::open_default_sink().context("opening audio output")?;
            // Dropping the sink on exit is how we stop the music, so rodio's
            // "audio will stop" drop notice is just noise.
            sink.log_on_drop(false);

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
                warn!(track, "disc has no audio track");
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
                    append_i16_le_to_f32(&bytes, &mut self.buffer);
                    self.position = 0;
                    self.next_lba = chunk_end;
                }
                Err(error) => {
                    error!(track = self.track_number, "track read failed: {error:#}");
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

    /// The original's sample rate: Sound Blaster time constant `0xD3`,
    /// `1000000 / (256 - 211)`.
    const SFX_SAMPLE_RATE: u32 = 22222;

    /// Samples mixed per buffer refill. 250 is the original's DMA chunk, so
    /// trigger latency and end-of-sample granularity (~11 ms) match it.
    const MIX_BLOCK: usize = 250;

    /// One mixer channel: the playing sample and the read position.
    #[derive(Default)]
    struct SfxChannel {
        sample: Option<Arc<[i8]>>,
        position: usize,
        looped: bool,
    }

    impl SfxChannel {
        /// The channel's next sample value, advancing it: wraps while looped,
        /// frees itself at the end otherwise.
        fn next(&mut self) -> i32 {
            let Some(sample) = &self.sample else {
                return 0;
            };

            let value = i32::from(sample[self.position]);
            self.position += 1;

            if self.position >= sample.len() {
                self.position = 0;

                if !self.looped {
                    self.sample = None;
                }
            }

            value
        }
    }

    type SharedChannels = Arc<Mutex<[SfxChannel; SFX_CHANNELS]>>;

    /// Plays sound effects through the default audio device: a three-channel
    /// additive mixer, like the original's DMA feed (which adds the channels'
    /// signed bytes; this saturates instead of wrapping on overflow). The
    /// [`MixerDeviceSink`] must stay alive for playback.
    pub struct RodioSfxPlayer {
        channels: SharedChannels,
        _sink: MixerDeviceSink,
        _player: Player,
    }

    impl RodioSfxPlayer {
        pub fn new() -> Result<Self> {
            let mut sink =
                DeviceSinkBuilder::open_default_sink().context("opening audio output")?;
            sink.log_on_drop(false);

            let channels: SharedChannels = Arc::default();
            let player = Player::connect_new(sink.mixer());
            player.append(SfxSource {
                channels: channels.clone(),
                buffer: Vec::new(),
                position: 0,
            });

            Ok(Self {
                channels,
                _sink: sink,
                _player: player,
            })
        }
    }

    impl SfxPlayer for RodioSfxPlayer {
        fn play(&mut self, channel: usize, sample: Arc<[i8]>, looped: bool) {
            let mut channels = self.channels.lock().expect("sfx mixer lock");

            let Some(slot) = channels.get_mut(channel) else {
                return;
            };

            // An empty sample would underflow the mixer's position math;
            // treat it as silence.
            *slot = SfxChannel {
                sample: (!sample.is_empty()).then_some(sample),
                position: 0,
                looped,
            };
        }

        fn end_loop(&mut self, channel: usize) {
            let mut channels = self.channels.lock().expect("sfx mixer lock");

            if let Some(slot) = channels.get_mut(channel) {
                slot.looped = false;
            }
        }
    }

    /// An endless mono [`Source`] mixing the three channels a block at a
    /// time, so the lock is taken once per ~11 ms rather than per sample.
    /// Idle channels contribute silence; the source never ends.
    struct SfxSource {
        channels: SharedChannels,
        buffer: Vec<f32>,
        position: usize,
    }

    impl Iterator for SfxSource {
        type Item = Sample;

        fn next(&mut self) -> Option<Sample> {
            if self.position >= self.buffer.len() {
                let mut channels = self.channels.lock().expect("sfx mixer lock");
                self.buffer.clear();

                for _ in 0..MIX_BLOCK {
                    let sum: i32 = channels.iter_mut().map(SfxChannel::next).sum();
                    self.buffer.push(sum.clamp(-128, 127) as f32 / 128.0);
                }

                self.position = 0;
            }

            let sample = self.buffer[self.position];
            self.position += 1;
            Some(sample)
        }
    }

    impl Source for SfxSource {
        fn current_span_len(&self) -> Option<usize> {
            None
        }

        fn channels(&self) -> ChannelCount {
            NonZeroU16::new(1).expect("mono is non-zero")
        }

        fn sample_rate(&self) -> SampleRate {
            NonZeroU32::new(SFX_SAMPLE_RATE).expect("SFX_SAMPLE_RATE is non-zero")
        }

        fn total_duration(&self) -> Option<Duration> {
            None
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Frames per CD sector (2352 bytes / 4 bytes per stereo frame) times the
        /// two interleaved channels.
        const SAMPLES_PER_SECTOR: usize = 588 * CHANNELS as usize;

        /// A mixer source over fresh channels, with the channel handle to
        /// drive it (the device-facing [`RodioSfxPlayer`] is not needed to
        /// exercise the mixing).
        fn sfx_source() -> (SfxSource, SharedChannels) {
            let channels: SharedChannels = Arc::default();
            let source = SfxSource {
                channels: channels.clone(),
                buffer: Vec::new(),
                position: 0,
            };

            (source, channels)
        }

        fn set(channels: &SharedChannels, channel: usize, bytes: &[i8], looped: bool) {
            channels.lock().expect("sfx mixer lock")[channel] = SfxChannel {
                sample: Some(Arc::from(bytes.to_vec())),
                position: 0,
                looped,
            };
        }

        #[test]
        fn mixes_the_channels_additively_and_saturates() {
            let (mut source, channels) = sfx_source();
            set(&channels, 0, &[10, 100, -100], false);
            set(&channels, 1, &[20, 100, -100], false);
            set(&channels, 2, &[30, 0, 0], false);

            let mixed: Vec<f32> = source.by_ref().take(4).collect();

            assert_eq!(mixed[0], 60.0 / 128.0);
            // 200 and -200 overflow the byte range: saturated, not wrapped.
            assert_eq!(mixed[1], 127.0 / 128.0);
            assert_eq!(mixed[2], -1.0);
            assert_eq!(mixed[3], 0.0, "ended channels mix as silence");
        }

        #[test]
        fn a_sample_plays_once_and_frees_its_channel() {
            let (mut source, channels) = sfx_source();
            set(&channels, 1, &[1, 2, 3], false);

            let mixed: Vec<f32> = source.by_ref().take(5).collect();

            assert_eq!(mixed[..3], [1.0 / 128.0, 2.0 / 128.0, 3.0 / 128.0]);
            assert_eq!(mixed[3..], [0.0, 0.0]);
            assert!(
                channels.lock().expect("sfx mixer lock")[1].sample.is_none(),
                "the channel frees itself at the sample's end"
            );
        }

        #[test]
        fn a_looped_sample_wraps_until_its_loop_ends_then_plays_out() {
            let (mut source, channels) = sfx_source();
            set(&channels, 1, &[1, 2], true);

            let mixed: Vec<f32> = source.by_ref().take(4).collect();
            assert_eq!(
                mixed,
                [1.0 / 128.0, 2.0 / 128.0, 1.0 / 128.0, 2.0 / 128.0],
                "the loop wraps at the sample's end"
            );

            // End the loop. The already-mixed remainder of the first block
            // keeps sounding; the next block plays the sample's final pass
            // (it had wrapped to its start) and then frees the channel.
            channels.lock().expect("sfx mixer lock")[1].looped = false;
            let rest: Vec<f32> = source.by_ref().take(MIX_BLOCK * 2).collect();
            let next_block = MIX_BLOCK - 4;
            assert_eq!(rest[next_block..next_block + 2], [1.0 / 128.0, 2.0 / 128.0]);
            assert!(rest[next_block + 2..].iter().all(|&sample| sample == 0.0));
        }

        #[test]
        #[cfg_attr(not(feature = "disc-tests"), ignore = "requires the disc image")]
        fn streams_and_refills_across_chunk_boundaries() {
            let disc = DiscImage::open_default().expect(
                "no disc image (set PROTOTYPE_DISC or place PROTOTYPE.cue at the repo root)",
            );

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
        #[cfg_attr(not(feature = "disc-tests"), ignore = "requires the disc image")]
        fn plays_once_then_ends() {
            let disc = DiscImage::open_default().expect(
                "no disc image (set PROTOTYPE_DISC or place PROTOTYPE.cue at the repo root)",
            );

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
pub use rodio_backend::{RodioMusicPlayer, RodioSfxPlayer};
