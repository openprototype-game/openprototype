//! Time-driven FLI playback.
//!
//! Pre-decodes a FLI into owned frames and advances by elapsed time, one frame
//! per its delay. The decoder ([`Flic`]) borrows its bytes and is stateful, so
//! a player cannot hold both the bytes and a live decoder; pre-decoding sidesteps
//! that and avoids decoding on the playback path. The intro plays `intro.fli`
//! and `fly.fli` this way. The frames stay in memory (an intro FLI is ~2.75 MB),
//! which is fine for a one-shot.

use std::time::Duration;

use prototype_formats::{DecodeError, Flic, IndexedImage, Palette, Result};

/// Jiffies are 1/70 second: the FLI delay unit and the VGA retrace rate.
fn jiffies_to_duration(jiffies: u32) -> Duration {
    Duration::from_micros(u64::from(jiffies) * 1_000_000 / 70)
}

/// One pre-decoded FLI frame: the full canvas, its palette, and its hold time.
pub struct FlicFrame {
    pub image: IndexedImage,
    pub palette: Palette,
    pub delay: Duration,
}

/// Plays a pre-decoded FLI once, advancing by elapsed time.
///
/// Stops on the last frame.
pub struct FlicPlayer {
    frames: Vec<FlicFrame>,
    index: usize,
    elapsed: Duration,
}

impl FlicPlayer {
    /// Decodes every frame of `bytes` up front.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut flic = Flic::new(bytes)?;
        let mut frames = Vec::with_capacity(flic.header().frame_count as usize);

        while let Some(frame) = flic.next_frame() {
            let frame = frame?;
            frames.push(FlicFrame {
                image: frame.image.clone(),
                palette: frame.palette.clone(),
                delay: jiffies_to_duration(frame.delay_jiffies),
            });
        }

        if frames.is_empty() {
            return Err(DecodeError::Unrecognized {
                reason: "FLI has no frames",
            });
        }

        Ok(Self {
            frames,
            index: 0,
            elapsed: Duration::ZERO,
        })
    }

    /// Advances by `dt`, moving to later frames as their delays elapse.
    ///
    /// Holds on the last frame. Returns the part of `dt` past the last frame's
    /// full delay, so the caller can roll it into whatever follows and the
    /// playback end loses no time.
    pub fn advance(&mut self, dt: Duration) -> Duration {
        self.elapsed += dt;

        while self.index + 1 < self.frames.len() && self.elapsed >= self.frames[self.index].delay {
            self.elapsed -= self.frames[self.index].delay;
            self.index += 1;
        }

        let last_delay = self.frames[self.index].delay;

        if self.index + 1 >= self.frames.len() && self.elapsed > last_delay {
            let excess = self.elapsed - last_delay;
            self.elapsed = last_delay;
            return excess;
        }

        Duration::ZERO
    }

    /// Decodes every frame, but plays at a fixed `frame_delay`.
    ///
    /// Ignores the FLI's own speed. The original's player (`START.EXE` `0x31fd`)
    /// ignores the header speed and delays a caller-set tick count after each
    /// frame, so the intro plays each FLI at its own rate; this mirrors that.
    pub fn decode_at(bytes: &[u8], frame_delay: Duration) -> Result<Self> {
        let mut player = Self::decode(bytes)?;

        for frame in &mut player.frames {
            frame.delay = frame_delay;
        }

        Ok(player)
    }

    /// Restarts from the first frame.
    ///
    /// Used to loop a short animation (the intro credits replay `credz.fli`
    /// under each text page).
    pub fn restart(&mut self) {
        self.index = 0;
        self.elapsed = Duration::ZERO;
    }

    /// Drops the final frame.
    ///
    /// The original's credits player (`START.EXE` `0x3293`) runs one frame short
    /// of the header count (`dec cx` before its frame loop), unlike the intro
    /// player (`0x31fd`) which plays them all.
    pub fn clip_last_frame(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    /// The frame to show now.
    pub fn current(&self) -> &FlicFrame {
        &self.frames[self.index]
    }

    /// Whether the last frame has been shown for its full delay.
    pub fn finished(&self) -> bool {
        self.index + 1 >= self.frames.len() && self.elapsed >= self.frames[self.index].delay
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prototype_formats::{Dimensions, Rgb};

    fn frame(delay_ms: u64) -> FlicFrame {
        FlicFrame {
            image: IndexedImage::new(Dimensions::new(1, 1), vec![0]).unwrap(),
            palette: Palette {
                colors: [Rgb::default(); 256],
            },
            delay: Duration::from_millis(delay_ms),
        }
    }

    fn player(delays: &[u64]) -> FlicPlayer {
        FlicPlayer {
            frames: delays.iter().map(|&delay| frame(delay)).collect(),
            index: 0,
            elapsed: Duration::ZERO,
        }
    }

    #[test]
    fn advances_frames_as_their_delays_elapse() {
        let mut player = player(&[100, 100, 100]);
        assert_eq!(player.index, 0);

        player.advance(Duration::from_millis(150)); // past frame 0, 50ms into frame 1
        assert_eq!(player.index, 1);

        player.advance(Duration::from_millis(60)); // 110ms on frame 1, past it
        assert_eq!(player.index, 2);
        assert!(!player.finished());

        player.advance(Duration::from_millis(100)); // last frame held its delay
        assert!(player.finished());
    }

    #[test]
    fn holds_on_the_last_frame() {
        let mut player = player(&[50]);
        player.advance(Duration::from_secs(10));

        assert_eq!(player.index, 0);
        assert!(player.finished());
    }

    #[test]
    fn clipping_drops_the_last_frame_but_never_the_only_one() {
        let mut clipped = player(&[100, 100, 100]);
        clipped.clip_last_frame();
        assert_eq!(clipped.frames.len(), 2);

        let mut single = player(&[100]);
        single.clip_last_frame();
        assert_eq!(single.frames.len(), 1);
    }

    #[test]
    fn returns_the_time_past_the_last_frames_delay() {
        let mut player = player(&[100, 100]);

        assert_eq!(player.advance(Duration::from_millis(150)), Duration::ZERO);
        assert_eq!(
            player.advance(Duration::from_millis(80)),
            Duration::from_millis(30)
        );
        assert!(player.finished());

        // Once finished, further time passes straight through.
        assert_eq!(
            player.advance(Duration::from_millis(40)),
            Duration::from_millis(40)
        );
    }

    #[test]
    #[cfg_attr(not(feature = "disc-tests"), ignore = "requires the disc image")]
    fn decodes_the_real_intro_fli() {
        use prototype_disc::{AssetSource, DiscImage};

        let disc = DiscImage::open_default().expect("disc image");
        let bytes = disc.read("FLI/INTRO.FLI").expect("reading INTRO.FLI");
        let player = FlicPlayer::decode(&bytes).expect("decoding INTRO.FLI");

        assert!(player.frames.len() > 1, "intro.fli should have many frames");
    }
}
