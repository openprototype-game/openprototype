//! The application: a flat scene state machine.
//!
//! Owns the current scene and the shared menu assets, and applies the
//! transitions scenes request. Implements the platform-facing [`Game`] trait,
//! so the platform stays unaware of scenes. The title theme is started once
//! here at boot (the intro scene will own this later); the platform keeps it
//! playing across scene switches, matching the original.

use std::rc::Rc;
use std::time::Duration;

use crate::assets::MenuAssets;
use crate::core::audio::AudioCommand;
use crate::core::framebuffer::Framebuffer;
use crate::core::game::{Game, StepOutput};
use crate::core::input::KeyEvent;
use crate::scene::{Menu, MusicMenu, Scene, SceneId, Transition};

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
        SceneId::MusicMenu => Box::new(MusicMenu::new(assets.clone())),
    }
}

impl Game for App {
    fn step(&mut self, dt: Duration, input: &[KeyEvent]) -> StepOutput {
        let mut audio = Vec::new();

        if !self.booted {
            audio.push(AudioCommand::PlayTrack(BOOT_TRACK));
            self.booted = true;
        }

        let output = self.current.update(dt, input);
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

    fn is_animating(&self) -> bool {
        self.current.is_animating()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::test_menu_assets;

    const FRAME: Duration = Duration::ZERO;

    #[test]
    fn boot_starts_the_title_theme_once() {
        let mut app = App::new(test_menu_assets());

        assert_eq!(
            app.step(FRAME, &[]).audio,
            vec![AudioCommand::PlayTrack(BOOT_TRACK)]
        );
        assert!(app.step(FRAME, &[]).audio.is_empty());
    }

    #[test]
    fn esc_on_the_menu_quits() {
        let mut app = App::new(test_menu_assets());

        assert!(app.step(FRAME, &[KeyEvent::Esc]).quit);
    }

    #[test]
    fn enters_the_jukebox_plays_a_track_and_returns() {
        let mut app = App::new(test_menu_assets());
        app.step(FRAME, &[]); // boot: starts the title theme

        // MUSIC MENU is the fourth item; open it.
        let open = app.step(
            FRAME,
            &[
                KeyEvent::Down,
                KeyEvent::Down,
                KeyEvent::Down,
                KeyEvent::Enter,
            ],
        );
        assert!(!open.quit);

        // The jukebox starts on MUSIC 1, which is CD track 2.
        assert_eq!(
            app.step(FRAME, &[KeyEvent::Enter]).audio,
            vec![AudioCommand::PlayTrack(2)]
        );

        // Esc returns to the menu rather than quitting.
        assert!(!app.step(FRAME, &[KeyEvent::Esc]).quit);

        // The menu was rebuilt with the cursor on NEW GAME; Up wraps to QUIT.
        assert!(app.step(FRAME, &[KeyEvent::Up, KeyEvent::Enter]).quit);
    }
}
