//! The game-over sequence.
//!
//! Mirrors `START.EXE`'s state-5 branch (`0x4b9a`): CD track 8 starts,
//! `FLI/GO2.FLI` plays at 2 ticks per frame, the screen waits for a key, then
//! track 2 (the menu music) restarts and the flow branches on the high-score
//! check: a qualifying score goes to the name entry, anything else straight
//! back to the menu. A key pressed during the FLI aborts the playback, and the
//! original's key-available flag stays set into the wait, so one key anywhere
//! ends the whole sequence.

use std::rc::Rc;
use std::time::Duration;

use prototype_formats::{Dimensions, Palette, Rgb};

use crate::assets::GameOverAssets;
use crate::flic_player::FlicPlayer;
use crate::scene::{Scene, SceneId, SceneOutput, Transition};
use crate::screen::{SCREEN_HEIGHT, SCREEN_WIDTH};
use openprototype_core::audio::AudioCommand;
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::input::KeyEvent;

/// `GO2.FLI` plays at 2 ticks per frame (`cs:[0x3022]=2` before the player).
const FLI_FRAME_DELAY: Duration = Duration::from_micros(2 * 1_000_000 / 70);

/// The CD track under the game-over animation.
const GAME_OVER_TRACK: u8 = 8;

/// The menu music, restarted when the sequence ends.
const MENU_TRACK: u8 = 2;

/// The game-over scene: the GO2 animation, then a key to continue.
pub struct GameOverScene {
    /// `None` once the FLI has finished (or was skipped); the scene then
    /// waits for a key.
    player: Option<FlicPlayer>,
    /// Where a key takes the player: the name entry on a qualifying score,
    /// the menu otherwise (decided by the app at build time).
    next: SceneId,
    framebuffer: Framebuffer,
    music_started: bool,
}

impl GameOverScene {
    /// Builds the scene, decoding `GO2.FLI` and rendering its first frame.
    pub fn new(assets: Rc<GameOverAssets>, next: SceneId) -> Self {
        let mut scene = Self {
            player: FlicPlayer::decode_at(&assets.fli, FLI_FRAME_DELAY).ok(),
            next,
            framebuffer: Framebuffer::new(
                Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT),
                Palette {
                    colors: [Rgb::default(); 256],
                },
            ),
            music_started: false,
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
}

impl Scene for GameOverScene {
    fn update(&mut self, dt: Duration, input: &[KeyEvent]) -> SceneOutput {
        let mut output = SceneOutput::default();

        if !self.music_started {
            self.music_started = true;
            output.audio.push(AudioCommand::PlayTrack(GAME_OVER_TRACK));
        }

        if input.iter().any(|event| event.pressed().is_some()) {
            output.audio.push(AudioCommand::PlayTrack(MENU_TRACK));
            output.transition = Some(Transition::To(self.next));

            return output;
        }

        let mut finished = false;

        if let Some(player) = &mut self.player {
            player.advance(dt);
            finished = player.finished();
        }

        self.render_fli_frame();

        if finished {
            self.player = None;
        }

        output
    }

    fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    fn is_animating(&self) -> bool {
        self.player.is_some()
    }
}
