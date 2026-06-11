//! The high-score screen.
//!
//! Mirrors `START.EXE`'s display path (`0x4068`): play `HIGHSCOR.FLI`, then fly
//! the eight entries in one at a time and wait for a key before returning to the
//! menu. From the menu there is never a qualifying score, so this is the
//! read-only path; name entry (a won game) needs the level engine.
//!
//! Each entry flies in with a zoom (`0x2670`): the line, at its final
//! position, is scaled about the screen center (160, 100) by
//! `25 / (step + 1)` over 25 steps (`K = 1000`, the counter `cs:[0x2643]`
//! 960→0 by 40, so `K - counter = 40·(step + 1)`). Rows above mid-screen fly
//! down from above, rows below fly up, and the middle rows mostly just
//! shrink in place; the composite touches rows 0..194 only. The fast-then-slow
//! look comes entirely from that hyperbolic ramp, not the pacing: the scaler's
//! inner loop runs a fixed 195 rows per step regardless of zoom, so every step
//! is the same constant work and the timing is uniform. There is no retrace or
//! timer wait. The absolute per-step pace is set from a recording (see
//! `STEP_DURATION`).
//!
//! The fly-in cannot be interrupted. A key aborts the FLI playback and stays
//! pending until the final blocking key read consumes it, so a key pressed
//! any time before the table settles exits the moment the last entry lands.

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
/// The zoom's fixed point: the in-flight line scales about the screen center,
/// so its apparent center is `100 + (final_y - 100) * scale`.
const SCREEN_CENTER_Y: f32 = 100.0;
/// The original's scaler composites output rows 0..194 only; the zoomed text
/// never touches the bottom five rows.
const COMPOSITE_ROWS: f32 = 195.0;

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
    /// A key arrived before the table settled. It stays pending, like the
    /// original's key flag, and exits the screen once the last entry lands.
    exit_when_landed: bool,
}

impl HighscoreScreen {
    pub fn new(assets: Rc<HighscoreAssets>, scores: Highscores) -> Self {
        let mut screen = Self {
            assets,
            scores,
            framebuffer: Framebuffer::new(Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT), black()),
            phase: Phase::Resting,
            exit_when_landed: false,
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
            // The strip spans the full width, so scaling it about the screen's
            // center column equals the original's zoom about (160, 100).
            blit_scaled(
                &mut self.framebuffer.image,
                &fly.strips[fly.entry],
                step_scale(fly.step),
                SCREEN_WIDTH as f32 / 2.0,
                fly_in_center_y(fly.entry, fly.step),
            );
        }
    }
}

impl Scene for HighscoreScreen {
    fn update(&mut self, dt: Duration, input: &[KeyEvent]) -> SceneOutput {
        let mut output = SceneOutput::default();

        if input.iter().any(|event| event.pressed().is_some()) {
            match self.phase {
                // A key aborts the FLI playback; the fly-in itself cannot be
                // interrupted. Either way the key stays pending and exits the
                // screen once the table settles.
                Phase::Playing(_) => {
                    self.exit_when_landed = true;
                    self.start_fly_in();
                }
                Phase::FlyingIn(_) => self.exit_when_landed = true,
                Phase::Resting => output.transition = Some(Transition::To(SceneId::MainMenu)),
            }

            return output;
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

        // The final blocking key read consumes a key pressed earlier the
        // moment the table settles.
        if self.exit_when_landed && matches!(self.phase, Phase::Resting) {
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

    /// Settle the table: bake any entries still in flight into the backdrop
    /// at full size and rest.
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

/// The in-flight entry's apparent center: its resting center pushed away from
/// the screen center by the zoom, collapsing onto its row as the scale reaches
/// one. Entries above mid-screen fly down from above, those below fly up.
fn fly_in_center_y(entry: usize, step: usize) -> f32 {
    SCREEN_CENTER_Y + (resting_center_y(entry) - SCREEN_CENTER_Y) * step_scale(step)
}

/// Composite `strip` onto `target`, scaled by `scale` and centered at
/// (`center_x`, `center_y`). Nearest-neighbor, inverse-mapped so large scales
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
    // The composite stops at row 194, like the original's fixed 195-row scaler
    // loop; the landed rows all sit above it, so only in-flight frames clip.
    let y1 = (top + dest_h).ceil().min(COMPOSITE_ROWS) as i32;

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
    fn a_key_during_the_fly_in_stays_pending_and_exits_once_landed() {
        let mut screen = test_screen();

        // The key does not interrupt the fly-in.
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
    fn the_fly_in_zooms_about_the_screen_center() {
        // The last step is scale 1: the entry sits exactly on its row.
        assert_eq!(fly_in_center_y(0, FLY_STEPS - 1), resting_center_y(0));
        assert_eq!(fly_in_center_y(7, FLY_STEPS - 1), resting_center_y(7));

        // At full zoom, entries above mid-screen start far off the top and
        // entries below start far off the bottom.
        assert!(fly_in_center_y(0, 0) < -500.0);
        assert!(fly_in_center_y(7, 0) > 2000.0);

        // An entry whose row straddles the center barely moves.
        let near_center = fly_in_center_y(2, 0);
        assert!((near_center - SCREEN_CENTER_Y).abs() < 200.0);
    }
}
