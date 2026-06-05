//! Backend-agnostic game core: the contract scenes implement.
//!
//! Nothing in here knows about winit, pixels, rodio, or a disc image. A scene
//! draws into a [`Framebuffer`] and reacts to [`KeyEvent`]s, emitting
//! [`AudioCommand`]s for the backend to execute. That keeps the whole core
//! headless-testable and lets the backend be swapped without touching it.
//!
//! [`Framebuffer`]: framebuffer::Framebuffer
//! [`KeyEvent`]: input::KeyEvent
//! [`AudioCommand`]: audio::AudioCommand

pub mod audio;
pub mod framebuffer;
pub mod game;
pub mod game_state;
pub mod input;

pub use audio::AudioCommand;
pub use framebuffer::Framebuffer;
pub use game::{Game, StepOutput};
pub use game_state::{GameState, Secondary, Weapon, WeaponLevel};
pub use input::KeyEvent;
