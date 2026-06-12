//! The between-levels FLI.
//!
//! `START.EXE`'s chain loop plays an interstitial movie before launching
//! every level past the first (file `0x3d0a`): mode 13h, the file from the
//! drive-patched table at vaddr `0x36f4` indexed by the incoming level, paced
//! per movie by `cs:[0x3022]` timer ticks per frame. There is no key skip:
//! the player's abort flag (`cs:[0x34f2]`) is cleared at the menu loop and
//! never armed. Nothing touches the CD either, so the tail of the finished
//! level's track keeps playing underneath; the next level starts its own
//! music at its GET READY dismissal.
//!
//! The movie past the last level (`LAVA.FLI`) leads into the ending
//! sequence.

use std::time::Duration;

use prototype_formats::{Dimensions, Palette, Rgb};

use crate::flic_player::FlicPlayer;
use crate::levels::Level;
use crate::scene::{Scene, SceneId, SceneOutput, Transition};
use crate::screen::{SCREEN_HEIGHT, SCREEN_WIDTH};
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
    fn update(&mut self, dt: Duration, _input: &[KeyEvent]) -> SceneOutput {
        let mut output = SceneOutput::default();

        let finished = match &mut self.player {
            Some(player) => {
                player.advance(dt);
                player.finished()
            }
            None => true,
        };

        self.render_fli_frame();

        if finished {
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
