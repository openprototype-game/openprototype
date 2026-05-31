//! The application: a flat scene state machine.
//!
//! Owns the current scene and the shared menu assets, and applies the
//! transitions scenes request. Implements the platform-facing [`Game`] trait,
//! so the platform stays unaware of scenes. The title theme is started once
//! here at boot (the intro scene will own this later); the platform keeps it
//! playing across scene switches, matching the original.

use std::rc::Rc;

use crate::assets::MenuAssets;
use crate::core::audio::AudioCommand;
use crate::core::framebuffer::Framebuffer;
use crate::core::game::{Game, StepOutput};
use crate::core::input::KeyEvent;
use crate::scene::{Menu, Scene, SceneId, Transition};

/// The CD-DA track the front-end starts with (the title theme).
const BOOT_TRACK: u8 = 2;

pub struct App {
    current: Box<dyn Scene>,
    assets: Rc<MenuAssets>,
    booted: bool,
}

impl App {
    /// Build the app on the main menu.
    pub fn new(assets: MenuAssets) -> Self {
        let assets = Rc::new(assets);
        let current = build(&assets, SceneId::MainMenu);

        Self {
            current,
            assets,
            booted: false,
        }
    }
}

fn build(assets: &Rc<MenuAssets>, id: SceneId) -> Box<dyn Scene> {
    match id {
        SceneId::MainMenu => Box::new(Menu::new(assets.clone())),
    }
}

impl Game for App {
    fn step(&mut self, input: &[KeyEvent]) -> StepOutput {
        let mut audio = Vec::new();

        if !self.booted {
            audio.push(AudioCommand::PlayTrack(BOOT_TRACK));
            self.booted = true;
        }

        let output = self.current.update(input);
        audio.extend(output.audio);

        let mut quit = false;

        if let Some(transition) = output.transition {
            match transition {
                Transition::To(id) => self.current = build(&self.assets, id),
                Transition::Quit => quit = true,
            }
        }

        StepOutput { audio, quit }
    }

    fn framebuffer(&self) -> &Framebuffer {
        self.current.framebuffer()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::test_menu_assets;

    #[test]
    fn boot_starts_the_title_theme_once() {
        let mut app = App::new(test_menu_assets());

        assert_eq!(
            app.step(&[]).audio,
            vec![AudioCommand::PlayTrack(BOOT_TRACK)]
        );
        assert!(app.step(&[]).audio.is_empty());
    }

    #[test]
    fn esc_on_the_menu_quits() {
        let mut app = App::new(test_menu_assets());

        assert!(app.step(&[KeyEvent::Esc]).quit);
    }
}
