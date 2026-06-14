//! The ending sequence.
//!
//! `START.EXE`'s chain loop branches here past the last level (file
//! `0x4e06`), after `LAVA.FLI`: the CD stops, the screen fades the
//! `PVESSEL.RAW` backdrop in from black over 90 steps of one tick each
//! (the palette interpolation at file `0x30c4`, target palette at image
//! offset `0x5130`, the menu palette), then twelve 20-character lines
//! (the string table at vaddr `0x5dd`, rows 16 pixels apart from y 4)
//! land one by one. A key then starts the menu theme and runs the same
//! high-score check as the game-over flow. During the fade and the text
//! the int9 press flag (`[0x2dec]`) is written on both edges, so a key
//! still held when the last line lands exits instantly through the
//! blocking read at `0x4f0f`, while a tapped-and-released key clears the
//! flag and waits for a fresh press.
//!
//! Each line lands with a 25-step zoom (the scaler at file `0x2870`, shared
//! with the high-score zoom; see [`crate::zoom`]): per line the screen is
//! snapshotted, the line is drawn at its final position on a transparent
//! page, and the page zooms out from 25x over the snapshot. Blank records
//! run the full zoom with no visual change: deliberate pauses. The original
//! leaves the steps unpaced (no tick wait, CPU-bound); one tick per step
//! approximates real hardware here.

use std::time::Duration;

use prototype_formats::{IndexedImage, Palette, Rgb};

use crate::assets::EndingAssets;
use crate::scene::{Scene, SceneId, SceneOutput, Transition};
use crate::zoom;
use openprototype_core::audio::AudioCommand;
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::input::KeyEvent;

/// One front-end timer tick (the mode 13h ~70Hz frame).
const TICK: Duration = Duration::from_micros(14_286);

/// Palette fade-in steps (`dl = 0x5a` at the call site, one tick each).
const FADE_STEPS: u32 = 90;

/// The twelve ending lines, pre-padded to center in the 20-column screen.
///
/// From vaddr `0x5dd` (22-byte records, skipping the leading marker byte). The
/// font maps `=` to the exclamation mark, as everywhere else.
const LINES: [&str; 12] = [
    "                    ",
    " WITH BLAZING GUNS  ",
    " YOU BLEW EVEN THE  ",
    "  LAST ALIEN AWAY   ",
    " BEFORE THEY COULD  ",
    " ENSLAVE HUMANITY=  ",
    "                    ",
    "       BUT          ",
    "   BE AWARE OF      ",
    "    THE EVIL=       ",
    "                    ",
    "                    ",
];

/// First text row and the per-line stride (`di = 0x500`, `+0x1400`).
const FIRST_ROW_Y: i32 = 4;
const ROW_STRIDE: i32 = 16;

/// The menu music, started by the key that leaves the ending.
const MENU_TRACK: u8 = 2;

/// Where the sequence is, advanced one tick at a time.
enum Phase {
    /// Fading the backdrop in from black; the step scales the palette.
    FadeIn { step: u32 },
    /// Zooming line `line` in; `step` zoom steps have landed. `src` holds
    /// the line at its final position (index 0 transparent), `bg` the screen
    /// snapshot the zoom composites over.
    Lines {
        line: usize,
        step: u32,
        src: IndexedImage,
        bg: IndexedImage,
    },
    /// Everything shown; the next key leaves.
    AwaitKey,
}

/// The ending scene: backdrop fade-in, then the zoomed text lines.
pub struct EndingScene {
    assets: std::rc::Rc<EndingAssets>,
    phase: Phase,
    /// The int9 press flag's port view (`[0x2dec]`, written on both edges):
    /// true while a key is down. A key still held when the last line lands
    /// exits the moment AwaitKey begins.
    key_down: bool,
    /// Where the key takes the player: the name entry on a qualifying
    /// score, the menu otherwise (decided by the app at build time).
    next: SceneId,
    framebuffer: Framebuffer,
    tick_elapsed: Duration,
    music_stopped: bool,
}

impl EndingScene {
    /// Builds the ending scene with the backdrop loaded, fade not started.
    pub fn new(assets: std::rc::Rc<EndingAssets>, next: SceneId) -> Self {
        let mut framebuffer = Framebuffer::new(
            assets.backdrop.size,
            Palette {
                colors: [Rgb::default(); 256],
            },
        );
        framebuffer.blit_screen(&assets.backdrop);

        Self {
            assets,
            phase: Phase::FadeIn { step: 0 },
            key_down: false,
            next,
            framebuffer,
            tick_elapsed: Duration::ZERO,
            music_stopped: false,
        }
    }

    /// The fade's palette: the target scaled by `step / FADE_STEPS`.
    fn fade_palette(&self, step: u32) -> Palette {
        let scale = |channel: u8| (u32::from(channel) * step / FADE_STEPS) as u8;
        let mut palette = self.assets.palette.clone();

        for color in &mut palette.colors {
            *color = Rgb {
                r: scale(color.r),
                g: scale(color.g),
                b: scale(color.b),
            };
        }

        palette
    }

    /// Opens a line's zoom.
    ///
    /// Snapshots the screen and draws the line at its final position on a
    /// transparent page (the original copies VGA into the bg buffer and
    /// zero-fills the src buffer before each line).
    fn start_line(&self, line: usize) -> Phase {
        let bg = self.framebuffer.image.clone();
        let mut src = IndexedImage::new(
            bg.size,
            vec![0u8; (bg.size.width * bg.size.height) as usize],
        )
        .expect("source page matches its dimensions");

        self.assets.font.draw_into(
            &mut src,
            0,
            FIRST_ROW_Y + line as i32 * ROW_STRIDE,
            LINES[line],
        );

        Phase::Lines {
            line,
            step: 0,
            src,
            bg,
        }
    }

    fn advance_tick(&mut self) {
        match std::mem::replace(&mut self.phase, Phase::AwaitKey) {
            Phase::FadeIn { step } => {
                let step = step + 1;
                self.framebuffer.palette = self.fade_palette(step);

                self.phase = if step >= FADE_STEPS {
                    self.start_line(0)
                } else {
                    Phase::FadeIn { step }
                };
            }
            Phase::Lines {
                line,
                step,
                src,
                bg,
            } => {
                let step = step + 1;
                zoom::composite_step(&src, &bg, step, &mut self.framebuffer.image);

                self.phase = if step < zoom::STEPS {
                    Phase::Lines {
                        line,
                        step,
                        src,
                        bg,
                    }
                } else if line + 1 < LINES.len() {
                    self.start_line(line + 1)
                } else {
                    Phase::AwaitKey
                };
            }
            Phase::AwaitKey => {}
        }
    }
}

impl Scene for EndingScene {
    fn update(&mut self, dt: Duration, input: &[KeyEvent]) -> SceneOutput {
        let mut output = SceneOutput::default();

        // The entry stops the CD (the resident-driver call at 0x4e06), so
        // LAVA.FLI's music tail dies here.
        if !self.music_stopped {
            self.music_stopped = true;
            output.audio.push(AudioCommand::StopMusic);
        }

        let pressed = input.iter().any(|event| event.pressed().is_some());

        for event in input {
            self.key_down = event.pressed().is_some();
        }

        if matches!(self.phase, Phase::AwaitKey) && pressed {
            output.audio.push(AudioCommand::PlayTrack(MENU_TRACK));
            output.transition = Some(Transition::To(self.next));

            return output;
        }

        self.tick_elapsed += dt;

        while self.tick_elapsed >= TICK {
            self.tick_elapsed -= TICK;
            self.advance_tick();
        }

        // A key still down when the last line lands exits instantly (the
        // blocking read finds the pending press).
        if matches!(self.phase, Phase::AwaitKey) && self.key_down {
            output.audio.push(AudioCommand::PlayTrack(MENU_TRACK));
            output.transition = Some(Transition::To(self.next));
        }

        output
    }

    fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    fn is_animating(&self) -> bool {
        !matches!(self.phase, Phase::AwaitKey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::test_ending_assets;
    use openprototype_core::input::Key;
    use std::rc::Rc;

    fn test_scene() -> EndingScene {
        EndingScene::new(Rc::new(test_ending_assets()), SceneId::MainMenu)
    }

    /// The whole timed run: the fade plus all twelve lines' zooms.
    const FULL_RUN: u32 = FADE_STEPS + LINES.len() as u32 * zoom::STEPS;

    #[test]
    fn the_first_update_stops_the_leftover_music() {
        let mut scene = test_scene();

        assert_eq!(
            scene.update(Duration::ZERO, &[]).audio,
            vec![AudioCommand::StopMusic]
        );
    }

    #[test]
    fn keys_are_ignored_until_everything_has_shown() {
        let mut scene = test_scene();

        let output = scene.update(TICK, &[KeyEvent::Pressed(Key::Enter)]);
        assert_eq!(output.transition, None);
    }

    #[test]
    fn a_key_held_through_the_run_exits_the_moment_it_ends() {
        let mut scene = test_scene();
        scene.update(TICK, &[KeyEvent::Pressed(Key::Enter)]);

        let output = scene.update(TICK * FULL_RUN, &[]);
        assert_eq!(output.transition, Some(Transition::To(SceneId::MainMenu)));
    }

    #[test]
    fn a_tapped_key_during_the_run_waits_for_a_fresh_press() {
        let mut scene = test_scene();
        scene.update(TICK, &[KeyEvent::Pressed(Key::Enter)]);
        scene.update(TICK, &[KeyEvent::Released(Key::Enter)]);

        let output = scene.update(TICK * FULL_RUN, &[]);
        assert_eq!(output.transition, None);
    }

    #[test]
    fn a_key_after_the_run_starts_the_menu_theme_and_leaves() {
        let mut scene = test_scene();
        scene.update(TICK * FULL_RUN, &[]);
        assert!(!scene.is_animating(), "the run has finished");

        let output = scene.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Enter)]);
        assert_eq!(output.audio, vec![AudioCommand::PlayTrack(MENU_TRACK)]);
        assert_eq!(output.transition, Some(Transition::To(SceneId::MainMenu)));
    }
}
