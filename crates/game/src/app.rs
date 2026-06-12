//! The application: a flat scene state machine.
//!
//! Owns the current scene and the shared assets, and applies the transitions
//! scenes request. Implements the platform-facing [`Game`] trait, so the
//! platform stays unaware of scenes. The app boots into the intro, which starts
//! the title theme; the platform keeps it playing across scene switches,
//! matching the original.

use std::rc::Rc;
use std::time::Duration;

use crate::assets::{
    EndingAssets, GameOverAssets, HighscoreAssets, IntroAssets, LevelAssets, MenuAssets,
};
use crate::highscores::HighscoreStore;
use crate::levels::Level;
use crate::scene::{
    EndingScene, GameOverScene, HighscoreEntry, HighscoreScreen, Intro, LevelScene,
    LevelTransition, Menu, MusicMenu, Scene, SceneId, Transition,
};
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::game::{Game, StepOutput};
use openprototype_core::input::KeyEvent;

/// Loads one level's assets on demand, when the chain reaches it (the
/// original loads each level as its own executable).
pub type LevelLoader = Box<dyn Fn(Level) -> anyhow::Result<LevelAssets>>;

/// Loads one FLI's bytes by disc path, for the between-levels movies.
pub type FliLoader = Box<dyn Fn(&str) -> anyhow::Result<Vec<u8>>>;

pub struct App {
    current: Box<dyn Scene>,
    menu_assets: Rc<MenuAssets>,
    intro_assets: Rc<IntroAssets>,
    highscore_assets: Rc<HighscoreAssets>,
    gameover_assets: Rc<GameOverAssets>,
    ending_assets: Rc<EndingAssets>,
    level_loader: LevelLoader,
    fli_loader: FliLoader,
    /// Dev fast-forward (`--skip`), in logic ticks; consumed by the first
    /// level scene built, so a chained next level starts normally.
    level_skip_ticks: u32,
    highscore_store: Rc<HighscoreStore>,
}

/// The front-end's eagerly loaded assets, bundled for [`App::new`].
pub struct FrontEndAssets {
    pub menu: MenuAssets,
    pub intro: IntroAssets,
    pub highscore: HighscoreAssets,
    pub gameover: GameOverAssets,
    pub ending: EndingAssets,
}

impl App {
    /// Build the app on the intro.
    pub fn new(
        assets: FrontEndAssets,
        level_loader: LevelLoader,
        fli_loader: FliLoader,
        highscore_store: HighscoreStore,
    ) -> Self {
        let menu_assets = Rc::new(assets.menu);
        let intro_assets = Rc::new(assets.intro);

        Self {
            current: Box::new(Intro::new(intro_assets.clone(), menu_assets.clone())),
            menu_assets,
            intro_assets,
            highscore_assets: Rc::new(assets.highscore),
            gameover_assets: Rc::new(assets.gameover),
            ending_assets: Rc::new(assets.ending),
            level_loader,
            fli_loader,
            level_skip_ticks: 0,
            highscore_store: Rc::new(highscore_store),
        }
    }

    /// Set the level scene's dev fast-forward (`--skip`), in logic ticks.
    pub fn set_level_skip(&mut self, ticks: u32) {
        self.level_skip_ticks = ticks;
    }

    /// Replace the current scene, to boot straight into one (the `--scene` flag).
    pub fn start_on(&mut self, id: SceneId) {
        self.current = self.build(id);
    }

    /// The end-of-run high-score routing, shared by the game-over and ending
    /// flows: the original's qualify test is strict (`0x4bde`), so the score
    /// must beat the table's lowest entry to reach the name entry; anything
    /// else returns to the menu.
    fn after_run(&self, score: u32) -> SceneId {
        let scores = self.highscore_store.load();
        let lowest = scores.entries().last().map_or(0, |entry| entry.score);

        if score > lowest {
            SceneId::HighscoreEntry { score }
        } else {
            SceneId::MainMenu
        }
    }

    fn build(&mut self, id: SceneId) -> Box<dyn Scene> {
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
            SceneId::GameOver { score } => Box::new(GameOverScene::new(
                self.gameover_assets.clone(),
                self.after_run(score),
            )),
            SceneId::Ending { score } => Box::new(EndingScene::new(
                self.ending_assets.clone(),
                self.after_run(score),
            )),
            SceneId::HighscoreEntry { score } => Box::new(HighscoreEntry::new(
                self.menu_assets.clone(),
                self.highscore_store.clone(),
                score,
            )),
            SceneId::Level { level, handoff } => {
                // A load failure mid-chain means the disc went away under a
                // verified image; there is no graceful continuation.
                let assets = (self.level_loader)(level)
                    .unwrap_or_else(|error| panic!("loading {level:?} assets: {error:#}"));

                let scene = Box::new(LevelScene::new(
                    Rc::new(assets),
                    level,
                    handoff,
                    self.level_skip_ticks,
                ));
                self.level_skip_ticks = 0;

                scene
            }
            SceneId::LevelTransition { after, handoff } => {
                // A movie that fails to load plays as nothing: the scene
                // moves straight on to its destination.
                let (name, _) = crate::scene::transition::transition_fli(after);
                let fli = (self.fli_loader)(name).unwrap_or_else(|error| {
                    tracing::warn!("loading {name}: {error:#}");
                    Vec::new()
                });

                Box::new(LevelTransition::new(&fli, after, handoff))
            }
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

    fn frame_interval(&self) -> Duration {
        self.current.frame_interval()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{
        test_ending_assets, test_gameover_assets, test_highscore_assets, test_intro_assets,
        test_level_assets, test_menu_assets,
    };
    use crate::highscores::test_store;
    use openprototype_core::audio::AudioCommand;
    use openprototype_core::input::Key;

    const FRAME: Duration = Duration::ZERO;

    fn test_app() -> App {
        App::new(
            FrontEndAssets {
                menu: test_menu_assets(),
                intro: test_intro_assets(),
                highscore: test_highscore_assets(),
                gameover: test_gameover_assets(),
                ending: test_ending_assets(),
            },
            Box::new(|_| Ok(test_level_assets())),
            Box::new(|_| Ok(Vec::new())),
            test_store(),
        )
    }

    /// Skip the intro to land on the main menu. The intro emits the title theme
    /// on its first update, then the key transitions to the menu.
    fn skip_intro(app: &mut App) {
        // A key aborts the script onto the closing menu fade-in (40 ticks);
        // one generous step finishes the fade and hands over to the menu.
        app.step(FRAME, &[KeyEvent::Pressed(Key::Enter)]);
        app.step(Duration::from_secs(1), &[]);
    }

    #[test]
    fn boot_runs_the_intro_and_starts_the_title_theme() {
        let mut app = test_app();

        assert!(app.is_animating(), "the intro animates");
        assert_eq!(app.step(FRAME, &[]).audio, vec![AudioCommand::PlayTrack(2)]);
    }

    #[test]
    fn quitting_is_the_quit_item_not_esc() {
        let mut app = test_app();
        skip_intro(&mut app);

        assert!(!app.is_animating(), "the menu is static");
        // The original's menu loop ignores Esc; QUIT is the last item (Up
        // wraps to it).
        assert!(!app.step(FRAME, &[KeyEvent::Pressed(Key::Esc)]).quit);
        assert!(
            app.step(
                FRAME,
                &[KeyEvent::Pressed(Key::Up), KeyEvent::Pressed(Key::Enter)]
            )
            .quit
        );
    }

    #[test]
    fn enters_the_jukebox_plays_a_track_and_returns() {
        let mut app = test_app();
        skip_intro(&mut app);

        // MUSIC MENU is the fourth item; open it.
        let open = app.step(
            FRAME,
            &[
                KeyEvent::Pressed(Key::Down),
                KeyEvent::Pressed(Key::Down),
                KeyEvent::Pressed(Key::Down),
                KeyEvent::Pressed(Key::Enter),
            ],
        );
        assert!(!open.quit);

        // The jukebox starts on MUSIC 1, which is CD track 2.
        assert_eq!(
            app.step(FRAME, &[KeyEvent::Pressed(Key::Enter)]).audio,
            vec![AudioCommand::PlayTrack(2)]
        );

        // Esc returns to the menu rather than quitting.
        assert!(!app.step(FRAME, &[KeyEvent::Pressed(Key::Esc)]).quit);

        // The menu was rebuilt with the cursor on NEW GAME; Up wraps to QUIT.
        assert!(
            app.step(
                FRAME,
                &[KeyEvent::Pressed(Key::Up), KeyEvent::Pressed(Key::Enter)]
            )
            .quit
        );
    }
}
