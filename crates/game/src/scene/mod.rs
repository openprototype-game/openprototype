//! Scenes and the transitions between them.
//!
//! A scene is one screen of the front-end (the menu, the jukebox, ...). It
//! advances one frame via [`Scene::update`] and may ask the [`App`] to switch
//! scenes with a [`Transition`]. The front-end is a flat state machine:
//! switching builds the target scene fresh, mirroring `START.EXE` (every menu
//! entry redraws and resets the cursor). Music lives outside scene state: it is
//! started once at boot, and the platform keeps it playing across switches.
//!
//! [`App`]: crate::app::App

pub mod ending;
pub mod gameover;
pub mod highscore_entry;
pub mod highscores;
pub mod intro;
pub mod level;
pub mod list_menu;
pub mod menu;
pub mod music;
pub mod transition;

pub use ending::EndingScene;
pub use gameover::GameOverScene;
pub use highscore_entry::HighscoreEntry;
pub use highscores::HighscoreScreen;
pub use intro::Intro;
pub use level::LevelScene;
pub use list_menu::ListMenu;
pub use menu::Menu;
pub use music::MusicMenu;
pub use transition::LevelTransition;

use std::time::Duration;

use openprototype_core::audio::AudioCommand;
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::game_state::Handoff;
use openprototype_core::input::KeyEvent;

/// The scenes the [`App`](crate::app::App) can switch to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneId {
    Intro,
    MainMenu,
    MusicMenu,
    Highscores,
    /// The game-over sequence (`GO2.FLI` under CD track 8), carrying the
    /// final score toward the high-score check.
    GameOver {
        score: u32,
    },
    /// The high-score name entry, for a score that made the table.
    HighscoreEntry {
        score: u32,
    },
    /// One level of the game chain, carrying the between-levels payload
    /// (the original's `f:message`). NEW GAME enters at L1 with the
    /// new-game carry; a completed level chains to the next with its
    /// writeback.
    Level {
        level: crate::levels::Level,
        handoff: Handoff,
    },
    /// The interstitial FLI after a finished level, leading to its
    /// successor (or into the ending past the last level).
    LevelTransition {
        after: crate::levels::Level,
        handoff: Handoff,
    },
    /// The ending sequence past the last level, carrying the final score
    /// toward the high-score check.
    Ending {
        score: u32,
    },
    /// Boot the savegame in this slot: the in-game LOAD GAME for a
    /// different level relaunches through here (the original bounces back
    /// to START.EXE with the corrected level byte), and the main menu's
    /// LOAD GAME will route here too.
    LoadGame {
        slot: usize,
    },
}

/// A scene's request to change the app state, returned from [`Scene::update`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// Replace the current scene with a freshly built one.
    To(SceneId),
    /// Tear down and exit the app.
    Quit,
}

/// The side effects of a single [`Scene::update`].
///
/// Music to play and an optional transition. The framebuffer is read
/// separately via [`Scene::framebuffer`] so a frame never has to clone it.
#[derive(Debug, Default)]
pub struct SceneOutput {
    pub audio: Vec<AudioCommand>,
    pub transition: Option<Transition>,
}

/// One screen of the front-end.
pub trait Scene {
    /// Advances one frame, given the elapsed time and key events since the last call.
    ///
    /// Static scenes (menu, jukebox) ignore `dt`.
    fn update(&mut self, dt: Duration, input: &[KeyEvent]) -> SceneOutput;

    /// The frame produced by the most recent [`update`](Scene::update).
    fn framebuffer(&self) -> &Framebuffer;

    /// Whether the scene is animating and needs the platform to keep ticking.
    ///
    /// Defaults to `false`: a static scene only redraws on input.
    fn is_animating(&self) -> bool {
        false
    }

    /// The frame period this scene runs at, its VGA mode's refresh.
    ///
    /// Defaults to the front-end's mode 13h (~70Hz); the level overrides to the
    /// 480-line Mode X (~60Hz). The platform ticks one frame per period.
    fn frame_interval(&self) -> Duration {
        Duration::from_micros(14_286)
    }
}
