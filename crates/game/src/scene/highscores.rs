//! The high-score screen.
//!
//! Mirrors `START.EXE`'s display path (`0x4068`): play `HIGHSCOR.FLI`, then show
//! the eight entries over its last frame and wait for a key before returning to
//! the menu. From the menu there is never a qualifying score, so this is the
//! read-only path; name entry (a won game) needs the level engine.
//!
//! The original flies each entry in with a zoom; that animation is added on top
//! of this scaffold. For now the entries are drawn at rest.

use std::rc::Rc;
use std::time::Duration;

use prototype_formats::{Highscores, Palette, Rgb};

use crate::assets::HighscoreAssets;
use crate::core::flic_player::FlicPlayer;
use crate::core::framebuffer::Framebuffer;
use crate::core::input::KeyEvent;
use crate::scene::{Scene, SceneId, SceneOutput, Transition};

/// `HIGHSCOR.FLI` plays at 4 ticks per frame (`cs:[0x3022]=4` before the play
/// at `0x4077`).
const FLI_FRAME_DELAY: Duration = Duration::from_micros(4 * 1_000_000 / 70);

/// Entry layout: the 20-character record fills the width at 16 px per glyph, and
/// rows step 16 scanlines (`di += 0x1400` in the original, proven).
const ENTRY_X: i32 = 0;
/// Provisional. The original never initialises the draw-loop `di`, so the
/// resting position comes out of the fly-in scaler's geometry; this gets pinned
/// to that when the fly-in lands.
const FIRST_ROW_Y: i32 = 65;
const ROW_HEIGHT: i32 = 16;

enum Phase {
    /// `HIGHSCOR.FLI` is playing.
    Playing(FlicPlayer),
    /// The entries are shown; waiting for a key.
    Resting,
}

pub struct HighscoreScreen {
    assets: Rc<HighscoreAssets>,
    scores: Highscores,
    framebuffer: Framebuffer,
    phase: Phase,
}

impl HighscoreScreen {
    pub fn new(assets: Rc<HighscoreAssets>, scores: Highscores) -> Self {
        let mut screen = Self {
            assets,
            scores,
            framebuffer: Framebuffer::new(black()),
            phase: Phase::Resting,
        };

        match FlicPlayer::decode_at(&screen.assets.fli, FLI_FRAME_DELAY) {
            Ok(player) => {
                screen.phase = Phase::Playing(player);
                screen.render_fli_frame();
            }
            Err(_) => screen.enter_resting(),
        }

        screen
    }

    /// Capture the FLI's last frame (if any) as the backdrop and draw the
    /// entries over it.
    fn enter_resting(&mut self) {
        if let Phase::Playing(player) = &self.phase {
            let frame = player.current();
            self.framebuffer.blit_screen(&frame.image);
            self.framebuffer.palette = frame.palette.clone();
        }

        self.draw_entries();
        self.phase = Phase::Resting;
    }

    fn draw_entries(&mut self) {
        for (index, entry) in self.scores.entries().iter().enumerate() {
            let text = format!("{:.<13} {:06}", entry.name, entry.score);
            let y = FIRST_ROW_Y + index as i32 * ROW_HEIGHT;
            self.assets
                .font
                .draw_into(&mut self.framebuffer.image, ENTRY_X, y, &text);
        }
    }

    fn render_fli_frame(&mut self) {
        if let Phase::Playing(player) = &self.phase {
            let frame = player.current();
            self.framebuffer.blit_screen(&frame.image);
            self.framebuffer.palette = frame.palette.clone();
        }
    }
}

impl Scene for HighscoreScreen {
    fn update(&mut self, dt: Duration, input: &[KeyEvent]) -> SceneOutput {
        let mut output = SceneOutput::default();

        if !input.is_empty() {
            match self.phase {
                Phase::Playing(_) => self.enter_resting(), // a key skips the FLI
                Phase::Resting => output.transition = Some(Transition::To(SceneId::MainMenu)),
            }

            return output;
        }

        if matches!(self.phase, Phase::Playing(_)) {
            let finished = match &mut self.phase {
                Phase::Playing(player) => {
                    player.advance(dt);
                    player.finished()
                }
                Phase::Resting => false,
            };

            if finished {
                self.enter_resting();
            } else {
                self.render_fli_frame();
            }
        }

        output
    }

    fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    fn is_animating(&self) -> bool {
        matches!(self.phase, Phase::Playing(_))
    }
}

fn black() -> Palette {
    Palette {
        colors: [Rgb::default(); 256],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::test_highscore_assets;

    fn test_screen() -> HighscoreScreen {
        // Synthetic assets have an empty FLI, so the scene starts at rest.
        let scores = (1..=8)
            .map(|rank| format!("{:.<13} {:06}$\n", "X", rank * 1000))
            .collect::<String>()
            .parse()
            .unwrap();

        HighscoreScreen::new(Rc::new(test_highscore_assets()), scores)
    }

    #[test]
    fn rests_without_a_fli_and_is_static() {
        let screen = test_screen();

        assert!(matches!(screen.phase, Phase::Resting));
        assert!(!screen.is_animating());
    }

    #[test]
    fn any_key_at_rest_returns_to_the_menu() {
        let mut screen = test_screen();

        assert_eq!(
            screen.update(Duration::ZERO, &[KeyEvent::Enter]).transition,
            Some(Transition::To(SceneId::MainMenu))
        );
    }
}
