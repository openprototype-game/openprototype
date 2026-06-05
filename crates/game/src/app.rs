//! The application: a flat scene state machine.
//!
//! Owns the current scene and the shared assets, and applies the transitions
//! scenes request. Implements the platform-facing [`Game`] trait, so the
//! platform stays unaware of scenes. The app boots into the intro, which starts
//! the title theme; the platform keeps it playing across scene switches,
//! matching the original.

use std::rc::Rc;
use std::time::Duration;

use crate::assets::{HighscoreAssets, IntroAssets, LevelAssets, MenuAssets};
use crate::highscores::HighscoreStore;
use crate::scene::{
    HighscoreScreen, Intro, LevelScene, Menu, MusicMenu, Scene, SceneId, Transition,
};
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::game::{Game, StepOutput};
use openprototype_core::input::KeyEvent;

pub struct App {
    current: Box<dyn Scene>,
    menu_assets: Rc<MenuAssets>,
    intro_assets: Rc<IntroAssets>,
    highscore_assets: Rc<HighscoreAssets>,
    level_assets: Rc<LevelAssets>,
    highscore_store: HighscoreStore,
}

impl App {
    /// Build the app on the intro.
    pub fn new(
        menu_assets: MenuAssets,
        intro_assets: IntroAssets,
        highscore_assets: HighscoreAssets,
        level_assets: LevelAssets,
        highscore_store: HighscoreStore,
    ) -> Self {
        let menu_assets = Rc::new(menu_assets);
        let intro_assets = Rc::new(intro_assets);

        Self {
            current: Box::new(Intro::new(intro_assets.clone(), menu_assets.clone())),
            menu_assets,
            intro_assets,
            highscore_assets: Rc::new(highscore_assets),
            level_assets: Rc::new(level_assets),
            highscore_store,
        }
    }

    /// Replace the current scene, to boot straight into one (the `--scene` flag).
    pub fn start_on(&mut self, id: SceneId) {
        self.current = self.build(id);
    }

    fn build(&self, id: SceneId) -> Box<dyn Scene> {
        match id {
            SceneId::Intro => Box::new(Intro::new(
                self.intro_assets.clone(),
                self.menu_assets.clone(),
            )),
            SceneId::MainMenu => Box::new(Menu::new(self.menu_assets.clone())),
            SceneId::MusicMenu => Box::new(MusicMenu::new(self.menu_assets.clone())),
            SceneId::Highscores => Box::new(HighscoreScreen::new(
                self.highscore_assets.clone(),
                self.highscore_store.load(),
            )),
            SceneId::Level => Box::new(LevelScene::new(self.level_assets.clone())),
        }
    }
}

impl Game for App {
    fn step(&mut self, dt: Duration, input: &[KeyEvent]) -> StepOutput {
        let output = self.current.update(dt, input);

        let mut quit = false;

        if let Some(transition) = output.transition {
            match transition {
                Transition::To(id) => self.current = self.build(id),
                Transition::Quit => quit = true,
            }
        }

        StepOutput {
            audio: output.audio,
            quit,
        }
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
    use crate::assets::{
        test_highscore_assets, test_intro_assets, test_level_assets, test_menu_assets,
    };
    use crate::highscores::test_store;
    use openprototype_core::audio::AudioCommand;

    const FRAME: Duration = Duration::ZERO;

    fn test_app() -> App {
        App::new(
            test_menu_assets(),
            test_intro_assets(),
            test_highscore_assets(),
            test_level_assets(),
            test_store(),
        )
    }

    /// Skip the intro to land on the main menu. The intro emits the title theme
    /// on its first update, then the key transitions to the menu.
    fn skip_intro(app: &mut App) {
        app.step(FRAME, &[KeyEvent::Enter]);
    }

    #[test]
    fn boot_runs_the_intro_and_starts_the_title_theme() {
        let mut app = test_app();

        assert!(app.is_animating(), "the intro animates");
        assert_eq!(app.step(FRAME, &[]).audio, vec![AudioCommand::PlayTrack(2)]);
    }

    #[test]
    fn esc_on_the_menu_quits() {
        let mut app = test_app();
        skip_intro(&mut app);

        assert!(!app.is_animating(), "the menu is static");
        assert!(app.step(FRAME, &[KeyEvent::Esc]).quit);
    }

    #[test]
    fn enters_the_jukebox_plays_a_track_and_returns() {
        let mut app = test_app();
        skip_intro(&mut app);

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
