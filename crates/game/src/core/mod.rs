//! Backend-agnostic game core.
//!
//! Nothing in here knows about winit, pixels, rodio, or a disc image. It draws
//! into a [`Framebuffer`] and reacts to [`KeyEvent`]s, emitting [`AudioCommand`]s
//! for the platform to execute. That keeps the whole core headless-testable and
//! lets the backend be swapped by rewriting only the platform layer.
//!
//! [`Framebuffer`]: framebuffer::Framebuffer
//! [`KeyEvent`]: input::KeyEvent
//! [`AudioCommand`]: audio::AudioCommand

pub mod audio;
pub mod framebuffer;
pub mod game;
pub mod input;

pub use audio::AudioCommand;
pub use framebuffer::{Framebuffer, SCREEN_HEIGHT, SCREEN_WIDTH};
pub use game::{Game, StepOutput};
pub use input::KeyEvent;
