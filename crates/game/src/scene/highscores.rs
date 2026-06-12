//! The high-score screen.
//!
//! Mirrors `START.EXE`'s display path (`0x4068`): play `HIGHSCOR.FLI`, then fly
//! the eight entries in one at a time and wait for a key before returning to the
//! menu. From the menu there is never a qualifying score, so this is the
//! read-only path; name entry (a won game) needs the level engine.
//!
//! Each entry flies in through the 25-step zoom reveal (the scaler at file
//! `0x2870`, shared with the ending; see [`crate::zoom`]): the row, drawn at
//! its final position on a transparent page, zooms out from 25x over the
//! settled screen (the FLI's last frame plus the rows already landed). Rows
//! above mid-screen fly down from above, rows below fly up, and the middle
//! rows shrink in place; the fast-then-slow look comes entirely from the
//! hyperbolic scale ramp, not the pacing — every step is the same constant
//! work, with no retrace or timer wait. The absolute per-step pace is set
//! from a recording (see `STEP_DURATION`).
//!
//! Neither the FLI nor the fly-in can be interrupted. The routine restores
//! the DOS int9 before playing the FLI and reinstalls its own ISR after
//! (file `0x412a`/`0x427a`), so keys during the movie do nothing at all.
//! During the fly-in the ISR writes the press flag on both edges (press 1,
//! release 0), so only a key still held when the last entry lands exits
//! instantly; a tapped-and-released key waits for a fresh press.

use std::rc::Rc;
use std::time::Duration;

use prototype_formats::{Dimensions, Highscores, IndexedImage, Palette, Rgb};

use crate::assets::HighscoreAssets;
use crate::flic_player::FlicPlayer;
use crate::scene::{Scene, SceneId, SceneOutput, Transition};
use crate::screen::{SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::zoom;
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::input::KeyEvent;

/// `HIGHSCOR.FLI` plays at 4 ticks per frame (`cs:[0x3022]=4` before `0x4077`).
const FLI_FRAME_DELAY: Duration = Duration::from_micros(4 * 1_000_000 / 70);

/// The full record fills the width at 16 px per glyph; rows step 16 scanlines
/// (`di += 0x1400`) from a first row at y=65 (`di = 0x5140` before the entry loop
/// at `0x408f`, and `0x5140 / 320 = 65`). Both proven from the disassembly.
const ENTRY_X: i32 = 0;
const FIRST_ROW_Y: i32 = 65;
const ROW_HEIGHT: i32 = 16;

/// Per-step duration. Steps advance at a uniform rate; a recording of the
/// original runs the eight-entry fly-in over ~25s, so `8 * 25` steps ≈ 125ms each.
const STEP_DURATION: Duration = Duration::from_millis(125);

enum Phase {
    /// `HIGHSCOR.FLI` is playing.
    Playing(FlicPlayer),
    /// Entries are flying in one at a time.
    FlyingIn(Box<FlyIn>),
    /// All entries shown; waiting for a key.
    Resting,
}

/// State of the entry fly-in: the backdrop (FLI frame plus already-landed
/// entries), the in-flight entry's source page (the row at its final
/// position, index 0 transparent), and which entry/step is in flight.
struct FlyIn {
    backdrop: Framebuffer,
    src: IndexedImage,
    entry: usize,
    step: u32,
    elapsed: Duration,
}

pub struct HighscoreScreen {
    assets: Rc<HighscoreAssets>,
    scores: Highscores,
    framebuffer: Framebuffer,
    phase: Phase,
    /// The original's press flag: the ISR writes it on both edges, so it
    /// mirrors "a key is currently down". Set during the fly-in only (the
    /// DOS int9 owns the keyboard while the FLI plays); a key still down
    /// when the last entry lands exits the screen instantly.
    key_down: bool,
}

impl HighscoreScreen {
    pub fn new(assets: Rc<HighscoreAssets>, scores: Highscores) -> Self {
        let mut screen = Self {
            assets,
            scores,
            framebuffer: Framebuffer::new(Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT), black()),
            phase: Phase::Resting,
            key_down: false,
        };

        match FlicPlayer::decode_at(&screen.assets.fli, FLI_FRAME_DELAY) {
            Ok(player) => {
                screen.phase = Phase::Playing(player);
                screen.render_fli_frame();
            }
            Err(_) => screen.start_fly_in(),
        }

        screen
    }

    /// Begin the fly-in: the current framebuffer (the FLI's last frame, or
    /// black) becomes the backdrop the entries fly onto.
    fn start_fly_in(&mut self) {
        let backdrop = Framebuffer {
            image: self.framebuffer.image.clone(),
            palette: self.framebuffer.palette.clone(),
        };

        self.phase = Phase::FlyingIn(Box::new(FlyIn {
            backdrop,
            src: entry_page(&self.assets, &self.scores, 0),
            entry: 0,
            step: 0,
            elapsed: Duration::ZERO,
        }));
        self.render_fly_in();
    }

    fn render_fli_frame(&mut self) {
        if let Phase::Playing(player) = &self.phase {
            let frame = player.current();
            self.framebuffer.blit_screen(&frame.image);
            self.framebuffer.palette = frame.palette.clone();
        }
    }

    /// Composite the in-flight entry over the backdrop into the framebuffer.
    fn render_fly_in(&mut self) {
        let Phase::FlyingIn(fly) = &self.phase else {
            return;
        };

        self.framebuffer
            .image
            .pixels
            .copy_from_slice(&fly.backdrop.image.pixels);
        self.framebuffer.palette = fly.backdrop.palette.clone();

        if fly.entry < self.scores.entries().len() {
            zoom::composite_step(
                &fly.src,
                &fly.backdrop.image,
                fly.step + 1,
                &mut self.framebuffer.image,
            );
        }
    }
}

impl Scene for HighscoreScreen {
    fn update(&mut self, dt: Duration, input: &[KeyEvent]) -> SceneOutput {
        let mut output = SceneOutput::default();

        match self.phase {
            // The DOS int9 owns the keyboard during the movie: no abort, no
            // pending flag.
            Phase::Playing(_) => {}
            // The press flag tracks both edges, so a tapped key clears again.
            Phase::FlyingIn(_) => {
                for event in input {
                    self.key_down = event.pressed().is_some();
                }
            }
            Phase::Resting => {
                if input.iter().any(|event| event.pressed().is_some()) {
                    output.transition = Some(Transition::To(SceneId::MainMenu));
                    return output;
                }
            }
        }

        match &mut self.phase {
            Phase::Playing(player) => {
                let excess = player.advance(dt);

                if player.finished() {
                    self.start_fly_in();
                    self.advance_fly_in(excess);
                } else {
                    self.render_fli_frame();
                }
            }
            Phase::FlyingIn(_) => self.advance_fly_in(dt),
            Phase::Resting => {}
        }

        // A key still held when the table settles exits instantly.
        if self.key_down && matches!(self.phase, Phase::Resting) {
            output.transition = Some(Transition::To(SceneId::MainMenu));
        }

        output
    }

    fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    fn is_animating(&self) -> bool {
        matches!(self.phase, Phase::Playing(_) | Phase::FlyingIn(_))
    }
}

impl HighscoreScreen {
    fn advance_fly_in(&mut self, dt: Duration) {
        let entries = self.scores.entries().len();
        let Phase::FlyingIn(fly) = &mut self.phase else {
            return;
        };

        fly.elapsed += dt;

        let mut finished = false;

        while fly.elapsed >= STEP_DURATION {
            fly.elapsed -= STEP_DURATION;
            fly.step += 1;

            if fly.step >= zoom::STEPS {
                // The entry has reached full size: bake it into the backdrop and
                // move to the next, or finish.
                land(&mut fly.backdrop.image, &fly.src);
                fly.entry += 1;
                fly.step = 0;

                if fly.entry >= entries {
                    finished = true;
                    break;
                }

                fly.src = entry_page(&self.assets, &self.scores, fly.entry);
            }
        }

        if finished {
            // The last entry is baked into the backdrop; show the settled table.
            // Going straight to `Resting` here would freeze on the in-flight
            // render of a step the frame may have skipped past, leaving the last
            // entry a touch too big.
            self.land_all_entries();
        } else {
            self.render_fly_in();
        }
    }

    /// Settle the table: bake any entries still in flight into the backdrop
    /// at full size and rest.
    fn land_all_entries(&mut self) {
        let entries = self.scores.entries().len();

        if let Phase::FlyingIn(fly) = &mut self.phase {
            while fly.entry < entries {
                land(&mut fly.backdrop.image, &fly.src);
                fly.entry += 1;

                if fly.entry < entries {
                    fly.src = entry_page(&self.assets, &self.scores, fly.entry);
                }
            }

            self.framebuffer
                .image
                .pixels
                .copy_from_slice(&fly.backdrop.image.pixels);
            self.framebuffer.palette = fly.backdrop.palette.clone();
        }

        self.phase = Phase::Resting;
    }
}

/// Render an entry's source page: the row at its final position on a
/// transparent (all-zero) page, like the original's zero-filled src buffer.
fn entry_page(assets: &HighscoreAssets, scores: &Highscores, entry: usize) -> IndexedImage {
    let highscore = &scores.entries()[entry];
    let text = format!("{:.<13} {:06}", highscore.name, highscore.score);

    let mut page = IndexedImage::new(
        Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT),
        vec![0u8; (SCREEN_WIDTH * SCREEN_HEIGHT) as usize],
    )
    .expect("page matches its dimensions");

    assets.font.draw_into(
        &mut page,
        ENTRY_X,
        FIRST_ROW_Y + entry as i32 * ROW_HEIGHT,
        &text,
    );

    page
}

/// Bake the landed page into the backdrop. The zoom's final step is an exact
/// 1:1 composite, so landing is a plain transparent overlay.
fn land(backdrop: &mut IndexedImage, src: &IndexedImage) {
    for (target, &pixel) in backdrop.pixels.iter_mut().zip(&src.pixels) {
        if pixel != 0 {
            *target = pixel;
        }
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
    use openprototype_core::input::Key;

    fn test_screen() -> HighscoreScreen {
        // Synthetic assets have an empty FLI, so the scene goes straight to the
        // fly-in.
        let scores = (1..=8)
            .map(|rank| format!("{:.<13} {:06}$\n", "X", rank * 1000))
            .collect::<String>()
            .parse()
            .unwrap();

        HighscoreScreen::new(Rc::new(test_highscore_assets()), scores)
    }

    #[test]
    fn flies_the_entries_in_then_rests() {
        let mut screen = test_screen();
        assert!(matches!(screen.phase, Phase::FlyingIn(_)));

        // Pump enough frames to land all eight entries.
        for _ in 0..4000 {
            screen.update(Duration::from_millis(16), &[]);

            if matches!(screen.phase, Phase::Resting) {
                break;
            }
        }

        assert!(matches!(screen.phase, Phase::Resting));
        assert!(!screen.is_animating());
    }

    #[test]
    fn a_key_held_through_the_fly_in_exits_once_landed() {
        let mut screen = test_screen();

        // The key does not interrupt the fly-in (and is never released).
        let output = screen.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Enter)]);
        assert!(matches!(screen.phase, Phase::FlyingIn(_)));
        assert_eq!(output.transition, None);

        // Once the last entry lands, the pending key exits to the menu on the
        // same update.
        let mut transition = None;

        for _ in 0..4000 {
            let output = screen.update(Duration::from_millis(16), &[]);

            if output.transition.is_some() {
                transition = output.transition;
                break;
            }

            assert!(
                matches!(screen.phase, Phase::FlyingIn(_)),
                "must not rest without transitioning"
            );
        }

        assert_eq!(transition, Some(Transition::To(SceneId::MainMenu)));
        assert!(matches!(screen.phase, Phase::Resting));
    }

    #[test]
    fn a_tapped_key_during_the_fly_in_does_not_exit_at_landing() {
        let mut screen = test_screen();

        // Press and release: the press flag clears on the release edge.
        screen.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Enter)]);
        screen.update(Duration::ZERO, &[KeyEvent::Released(Key::Enter)]);

        for _ in 0..4000 {
            let output = screen.update(Duration::from_millis(16), &[]);
            assert_eq!(output.transition, None, "must wait for a fresh press");

            if matches!(screen.phase, Phase::Resting) {
                break;
            }
        }

        assert!(matches!(screen.phase, Phase::Resting));

        let output = screen.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Enter)]);
        assert_eq!(output.transition, Some(Transition::To(SceneId::MainMenu)));
    }

    #[test]
    fn landing_bakes_the_page_into_the_backdrop() {
        let mut backdrop = IndexedImage::new(Dimensions::new(4, 1), vec![9, 9, 9, 9]).unwrap();
        let src = IndexedImage::new(Dimensions::new(4, 1), vec![0, 5, 0, 7]).unwrap();

        land(&mut backdrop, &src);

        assert_eq!(backdrop.pixels, vec![9, 5, 9, 7]);
    }
}
