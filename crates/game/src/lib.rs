//! Prototype (1995) game runtime.
//!
//! The runtime is the backend-agnostic half of the port. It implements the
//! [`Game`](openprototype_core::Game) contract from `openprototype-core` and
//! decodes its assets; the windowing, GPU and audio device live in
//! `openprototype-backend`, behind the `desktop` feature.
//!
//! - [`scene`] holds the scenes (the menu, for now) and the transitions between
//!   them.
//! - [`app`] is the scene state machine: it owns the current scene and applies
//!   transitions. It implements the backend-facing `Game` trait.
//! - [`assets`] decodes the disc's graphics into what scenes consume.
//! - [`fade`] and [`flic_player`] are the palette-fade and FLI-playback helpers
//!   scenes drive over time.
//! - [`highscores`] persists the high-score table in the OS data directory.

pub mod app;
pub mod assets;
pub mod background;
pub mod combat;
pub mod fade;
pub mod flic_player;
pub mod highscores;
pub mod hud;
pub mod ingame_menu;
pub mod level;
pub mod levels;
pub mod playfield;
pub mod savegame;
pub mod savestore;
pub mod scene;
pub mod scenery;
pub mod screen;
pub mod sfx;
pub mod ship;
pub mod shots;
pub mod spawns;
pub mod stars;
pub mod zoom;
