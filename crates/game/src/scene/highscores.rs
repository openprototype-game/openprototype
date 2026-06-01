//! The high-score screen.
//!
//! Mirrors `START.EXE`'s display path (`0x4068`): play `HIGHSCOR.FLI`, then fly
//! the eight entries in one at a time and wait for a key before returning to the
//! menu. From the menu there is never a qualifying score, so this is the
//! read-only path; name entry (a won game) needs the level engine.
//!
//! Each entry flies in with a zoom (`0x2670`): it starts large near the bottom
//! and shrinks to its row. The size follows the original's ramp exactly,
//! `size = 25 / (step + 1)` over 25 steps (`K = 1000`, the counter `cs:[0x2643]`
//! 960→0 by 40, so `K - counter = 40·(step + 1)`). The fast-then-slow look comes
//! entirely from that size ramp, not the pacing: the scaler's inner loop runs a
//! fixed 195 rows per step (`0x26ed`) regardless of zoom, so every step is the
//! same constant work and the timing is uniform. There is no retrace or timer
//! wait. The absolute per-step pace is set from a recording (see `STEP_DURATION`).

use std::rc::Rc;
use std::time::Duration;

use prototype_formats::{Dimensions, Highscores, IndexedImage, Palette, Rgb};

use crate::assets::HighscoreAssets;
use crate::flic_player::FlicPlayer;
use crate::scene::{Scene, SceneId, SceneOutput, Transition};
use crate::screen::{SCREEN_HEIGHT, SCREEN_WIDTH};
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
const STRIP_HEIGHT: u32 = 15;

/// The fly-in ramp: 25 steps, size `25 / (step + 1)` (`0x2670`).
const FLY_STEPS: usize = 25;
/// Per-step duration. Steps advance at a uniform rate; a recording of the
/// original runs the eight-entry fly-in over ~25s, so `8 * 25` steps ≈ 125ms each.
const STEP_DURATION: Duration = Duration::from_millis(125);
/// Where the entry's centre starts before it shrinks up to its row.
const START_CENTER_Y: f32 = 188.0;

enum Phase {
    /// `HIGHSCOR.FLI` is playing.
    Playing(FlicPlayer),
    /// Entries are flying in one at a time.
    FlyingIn(Box<FlyIn>),
    /// All entries shown; waiting for a key.
    Resting,
}

/// State of the entry fly-in: the backdrop (FLI frame plus already-landed
/// entries), the per-entry text strips, and which entry/step is in flight.
struct FlyIn {
    backdrop: Framebuffer,
    strips: Vec<IndexedImage>,
    entry: usize,
    step: usize,
    elapsed: Duration,
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
            framebuffer: Framebuffer::new(Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT), black()),
            phase: Phase::Resting,
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

    /// Render an entry's text to a transparent strip (index 0 = transparent).
    fn strip(&self, entry: usize) -> IndexedImage {
        let highscore = &self.scores.entries()[entry];
        let text = format!("{:.<13} {:06}", highscore.name, highscore.score);

        let mut strip = IndexedImage::new(
            Dimensions::new(SCREEN_WIDTH, STRIP_HEIGHT),
            vec![0u8; (SCREEN_WIDTH * STRIP_HEIGHT) as usize],
        )
        .expect("strip matches its dimensions");

        self.assets.font.draw_into(&mut strip, ENTRY_X, 0, &text);
        strip
    }

    /// Begin the fly-in: the current framebuffer (the FLI's last frame, or
    /// black) becomes the backdrop the entries fly onto.
    fn start_fly_in(&mut self) {
        let backdrop = Framebuffer {
            image: self.framebuffer.image.clone(),
            palette: self.framebuffer.palette.clone(),
        };
        let strips = (0..self.scores.entries().len())
            .map(|entry| self.strip(entry))
            .collect();

        self.phase = Phase::FlyingIn(Box::new(FlyIn {
            backdrop,
            strips,
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

        if fly.entry < fly.strips.len() {
            let scale = step_scale(fly.step);
            let progress = fly.step as f32 / (FLY_STEPS - 1) as f32;
            let resting_center = resting_center_y(fly.entry);
            let center_y = START_CENTER_Y + (resting_center - START_CENTER_Y) * progress;

            blit_scaled(
                &mut self.framebuffer.image,
                &fly.strips[fly.entry],
                scale,
                SCREEN_WIDTH as f32 / 2.0,
                center_y,
            );
        }
    }
}

impl Scene for HighscoreScreen {
    fn update(&mut self, dt: Duration, input: &[KeyEvent]) -> SceneOutput {
        let mut output = SceneOutput::default();

        if !input.is_empty() {
            match self.phase {
                // A key skips the FLI, then the fly-in, then exits.
                Phase::Playing(_) => self.start_fly_in(),
                Phase::FlyingIn(_) => self.land_all_entries(),
                Phase::Resting => output.transition = Some(Transition::To(SceneId::MainMenu)),
            }

            return output;
        }

        match &mut self.phase {
            Phase::Playing(player) => {
                player.advance(dt);

                if player.finished() {
                    self.start_fly_in();
                } else {
                    self.render_fli_frame();
                }
            }
            Phase::FlyingIn(_) => self.advance_fly_in(dt),
            Phase::Resting => {}
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
        let Phase::FlyingIn(fly) = &mut self.phase else {
            return;
        };

        fly.elapsed += dt;

        let mut finished = false;

        while fly.elapsed >= STEP_DURATION {
            fly.elapsed -= STEP_DURATION;
            fly.step += 1;

            if fly.step >= FLY_STEPS {
                // The entry has reached full size: bake it into the backdrop and
                // move to the next, or finish.
                land_entry(fly);
                fly.entry += 1;
                fly.step = 0;

                if fly.entry >= fly.strips.len() {
                    finished = true;
                    break;
                }
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

    /// Skip the fly-in: bake every remaining entry into the backdrop and rest.
    fn land_all_entries(&mut self) {
        if let Phase::FlyingIn(fly) = &mut self.phase {
            while fly.entry < fly.strips.len() {
                land_entry(fly);
                fly.entry += 1;
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

/// Draw the current entry into the backdrop at full size and final position.
fn land_entry(fly: &mut FlyIn) {
    blit_scaled(
        &mut fly.backdrop.image,
        &fly.strips[fly.entry],
        1.0,
        SCREEN_WIDTH as f32 / 2.0,
        resting_center_y(fly.entry),
    );
}

fn resting_center_y(entry: usize) -> f32 {
    (FIRST_ROW_Y + entry as i32 * ROW_HEIGHT) as f32 + STRIP_HEIGHT as f32 / 2.0
}

/// The entry's scale at `step`: `25 / (step + 1)` (`0x2670`).
fn step_scale(step: usize) -> f32 {
    FLY_STEPS as f32 / (step + 1) as f32
}

/// Composite `strip` onto `target`, scaled by `scale` and centred at
/// (`center_x`, `center_y`). Nearest-neighbour, inverse-mapped so large scales
/// clip to the screen. Source index 0 is transparent.
fn blit_scaled(
    target: &mut IndexedImage,
    strip: &IndexedImage,
    scale: f32,
    center_x: f32,
    center_y: f32,
) {
    let dest_w = strip.size.width as f32 * scale;
    let dest_h = strip.size.height as f32 * scale;
    let left = center_x - dest_w / 2.0;
    let top = center_y - dest_h / 2.0;

    let x0 = left.floor().max(0.0) as i32;
    let x1 = (left + dest_w).ceil().min(SCREEN_WIDTH as f32) as i32;
    let y0 = top.floor().max(0.0) as i32;
    let y1 = (top + dest_h).ceil().min(SCREEN_HEIGHT as f32) as i32;

    for dy in y0..y1 {
        let source_y = ((dy as f32 + 0.5 - top) / scale) as i32;

        if source_y < 0 || source_y >= strip.size.height as i32 {
            continue;
        }

        for dx in x0..x1 {
            let source_x = ((dx as f32 + 0.5 - left) / scale) as i32;

            if source_x < 0 || source_x >= strip.size.width as i32 {
                continue;
            }

            let pixel =
                strip.pixels[(source_y as u32 * strip.size.width + source_x as u32) as usize];

            if pixel != 0 {
                target.pixels[(dy as u32 * SCREEN_WIDTH + dx as u32) as usize] = pixel;
            }
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
    fn a_key_during_the_fly_in_lands_everything() {
        let mut screen = test_screen();

        screen.update(Duration::ZERO, &[KeyEvent::Enter]);
        assert!(matches!(screen.phase, Phase::Resting));

        // A further key then returns to the menu.
        assert_eq!(
            screen.update(Duration::ZERO, &[KeyEvent::Enter]).transition,
            Some(Transition::To(SceneId::MainMenu))
        );
    }
}
