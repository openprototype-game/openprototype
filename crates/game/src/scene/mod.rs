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

pub mod list_menu;
pub mod menu;
pub mod music;

pub use list_menu::ListMenu;
pub use menu::Menu;
pub use music::MusicMenu;

use std::time::Duration;

use crate::core::audio::AudioCommand;
use crate::core::framebuffer::Framebuffer;
use crate::core::input::KeyEvent;

/// The scenes the [`App`](crate::app::App) can switch to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneId {
    MainMenu,
    MusicMenu,
}

/// A scene's request to change the app state, returned from [`Scene::update`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// Replace the current scene with a freshly built one.
    To(SceneId),
    /// Tear down and exit the app.
    Quit,
}

/// The side effects of a single [`Scene::update`]: music to play and an optional
/// transition. The framebuffer is read separately via [`Scene::framebuffer`] so
/// a frame never has to clone it.
#[derive(Debug, Default)]
pub struct SceneOutput {
    pub audio: Vec<AudioCommand>,
    pub transition: Option<Transition>,
}

/// One screen of the front-end.
pub trait Scene {
    /// Advance one frame given the elapsed time and the key events since the
    /// last call. Static scenes (menu, jukebox) ignore `dt`.
    fn update(&mut self, dt: Duration, input: &[KeyEvent]) -> SceneOutput;

    /// The frame produced by the most recent [`update`](Scene::update).
    fn framebuffer(&self) -> &Framebuffer;

    /// Whether the scene is animating and needs the platform to keep ticking on
    /// a timer. Defaults to `false`: a static scene only redraws on input.
    fn is_animating(&self) -> bool {
        false
    }
}
