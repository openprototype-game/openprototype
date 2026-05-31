//! Prototype (1995) — game runtime.
//!
//! The library is split so a backend swap touches only the platform layer:
//!
//! - [`core`] is backend-agnostic. Scenes draw into a 320x200 indexed
//!   [`Framebuffer`](core::Framebuffer), react to [`KeyEvent`](core::KeyEvent)s,
//!   and emit [`AudioCommand`](core::AudioCommand)s. No windowing or audio
//!   library is in scope, so the whole core is headless-testable.
//! - [`scene`] holds the concrete scenes (the menu, for now).
//! - [`assets`] decodes the disc's graphics into what scenes consume.
//! - [`platform`] is the only place that knows winit, pixels and the audio
//!   device. It drives the loop: feed input, present the framebuffer scaled,
//!   execute audio commands.

pub mod assets;
pub mod core;
pub mod scene;

#[cfg(feature = "desktop")]
pub mod platform;
