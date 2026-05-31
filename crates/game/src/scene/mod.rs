//! Scenes: concrete [`Game`](crate::core::game::Game) implementations.
//!
//! Each screen of the front-end (menu, intro, jukebox, ...) is one scene. The
//! shell ships the menu; the rest arrive as they are ported.

pub mod menu;

pub use menu::Menu;
