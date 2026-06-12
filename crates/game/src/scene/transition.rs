//! The between-levels FLI.
//!
//! `START.EXE`'s chain loop plays an interstitial movie before launching
//! every level past the first (file `0x3d0a`): mode 13h, the file from the
//! drive-patched table at vaddr `0x36f4` indexed by the incoming level, paced
//! per movie by `cs:[0x3022]` timer ticks per frame. The chain stops the CD
//! before playing the movie (file `0x4df8`, and again on the common launch
//! path `0x4798` -> `0x4781`); the next level starts its own music at its
//! GET READY dismissal.
//!
//! A key skips the movie: the shared player polls the int9 key-down counter
//! (`cs:[0x2dea]`) every frame and aborts when it reads exactly 1 (file
//! `0x3485`), so two keys held at once do NOT skip. (The often-cited
//! `cs:[0x34f2]` is a different gate, written 0 everywhere and never armed.)
//!
//! The movie past the last level (`LAVA.FLI`) leads into the ending
//! sequence.

use std::time::Duration;

use prototype_formats::{Dimensions, Palette, Rgb};

use crate::flic_player::FlicPlayer;
use crate::levels::Level;
use crate::scene::{Scene, SceneId, SceneOutput, Transition};
use crate::screen::{SCREEN_HEIGHT, SCREEN_WIDTH};
use openprototype_core::audio::AudioCommand;
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::game_state::Handoff;
use openprototype_core::input::KeyEvent;

/// The movie that plays after finishing a level, and its frame duration in
/// timer ticks (`START.EXE`'s table at vaddr `0x36f4` and the per-level
/// `cs:[0x3022]` writes at file `0x3d35..0x3da0`; the table indexes by the
/// level about to launch, which is the finished level plus one).
pub fn transition_fli(after: Level) -> (&'static str, u32) {
    match after {
        Level::L1 => ("FLI/CANYON.FLI", 1),
        Level::L2 => ("FLI/SPACE1.FLI", 4),
        Level::L3 => ("FLI/WALDENDE.FLI", 4),
        Level::L4 => ("FLI/SPACE2.FLI", 4),
        Level::L5 => ("FLI/TEND.FLI", 1),
        Level::L6 => ("FLI/SPACE3.FLI", 4),
        Level::L7 => ("FLI/LAVA.FLI", 1),
    }
}

pub struct LevelTransition {
    /// `None` when the bytes did not decode; the first update then moves on.
    player: Option<FlicPlayer>,
    /// The level just finished; its successor receives the handoff.
    after: Level,
    handoff: Handoff,
    framebuffer: Framebuffer,
    /// The port's view of the int9 key-down counter `cs:[0x2dea]`: presses
    /// increment, releases decrement. The skip poll wants exactly 1.
    keys_down: u32,
    /// The CD stop before the movie has been issued.
    music_stopped: bool,
}

impl LevelTransition {
    pub fn new(fli: &[u8], after: Level, handoff: Handoff) -> Self {
        let (_, ticks) = transition_fli(after);
        let frame_delay = Duration::from_micros(u64::from(ticks) * 1_000_000 / 70);
        let mut scene = Self {
            player: FlicPlayer::decode_at(fli, frame_delay).ok(),
            after,
            handoff,
            framebuffer: Framebuffer::new(
                Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT),
                Palette {
                    colors: [Rgb::default(); 256],
                },
            ),
            keys_down: 0,
            music_stopped: false,
        };
        scene.render_fli_frame();

        scene
    }

    fn render_fli_frame(&mut self) {
        if let Some(player) = &self.player {
            let frame = player.current();
            self.framebuffer.blit_screen(&frame.image);
            self.framebuffer.palette = frame.palette.clone();
        }
    }

    /// Where the chain goes once the movie ends.
    fn destination(&self) -> SceneId {
        match self.after.next() {
            Some(next) => SceneId::Level {
                level: next,
                handoff: self.handoff,
            },
            None => SceneId::Ending {
                score: self.handoff.score,
            },
        }
    }
}

impl Scene for LevelTransition {
    fn update(&mut self, dt: Duration, input: &[KeyEvent]) -> SceneOutput {
        let mut output = SceneOutput::default();

        if !self.music_stopped {
            self.music_stopped = true;
            output.audio.push(AudioCommand::StopMusic);
        }

        for event in input {
            match event {
                KeyEvent::Pressed(_) => self.keys_down += 1,
                KeyEvent::Released(_) => self.keys_down = self.keys_down.saturating_sub(1),
            }
        }

        let finished = match &mut self.player {
            Some(player) => {
                player.advance(dt);
                player.finished()
            }
            None => true,
        };

        self.render_fli_frame();

        // The per-frame skip poll: exactly one key down aborts the movie.
        if finished || self.keys_down == 1 {
            output.transition = Some(Transition::To(self.destination()));
        }

        output
    }

    fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    fn is_animating(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openprototype_core::input::Key;

    /// A minimal valid FLI: the 128-byte header plus `frames` empty frames.
    fn synthetic_fli(frames: u16) -> Vec<u8> {
        let mut bytes = vec![0u8; 128];
        bytes[4..6].copy_from_slice(&0xAF11u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&frames.to_le_bytes());
        bytes[8..10].copy_from_slice(&320u16.to_le_bytes());
        bytes[10..12].copy_from_slice(&200u16.to_le_bytes());

        for _ in 0..frames {
            let mut frame = [0u8; 16];
            frame[0..4].copy_from_slice(&16u32.to_le_bytes());
            frame[4..6].copy_from_slice(&0xF1FAu16.to_le_bytes());
            bytes.extend_from_slice(&frame);
        }

        bytes
    }

    #[test]
    fn the_movie_stops_the_music_on_its_first_frame() {
        let mut scene = LevelTransition::new(&synthetic_fli(10), Level::L1, Handoff::new_game());
        let output = scene.update(Duration::ZERO, &[]);

        assert!(output.audio.contains(&AudioCommand::StopMusic));
        assert_eq!(scene.update(Duration::ZERO, &[]).audio, vec![]);
    }

    #[test]
    fn exactly_one_held_key_skips_the_movie() {
        let mut scene = LevelTransition::new(&synthetic_fli(100), Level::L1, Handoff::new_game());
        assert_eq!(scene.update(Duration::ZERO, &[]).transition, None);

        // Two keys down: the counter reads 2, no skip.
        let two = [KeyEvent::Pressed(Key::Enter), KeyEvent::Pressed(Key::Esc)];
        assert_eq!(scene.update(Duration::ZERO, &two).transition, None);

        // One comes back up: the per-frame poll reads exactly 1 and aborts.
        let release = [KeyEvent::Released(Key::Esc)];
        assert!(scene.update(Duration::ZERO, &release).transition.is_some());
    }

    #[test]
    fn an_undecodable_fli_moves_straight_to_the_next_level() {
        let mut handoff = Handoff::new_game();
        handoff.score = 4_321;
        let mut scene = LevelTransition::new(&[], Level::L1, handoff);

        assert_eq!(
            scene.update(Duration::ZERO, &[]).transition,
            Some(Transition::To(SceneId::Level {
                level: Level::L2,
                handoff,
            }))
        );
    }

    #[test]
    fn the_movie_past_the_last_level_leads_into_the_ending() {
        let mut handoff = Handoff::new_game();
        handoff.score = 4_321;
        let mut scene = LevelTransition::new(&[], Level::L7, handoff);

        assert_eq!(
            scene.update(Duration::ZERO, &[]).transition,
            Some(Transition::To(SceneId::Ending { score: 4_321 }))
        );
    }
}
