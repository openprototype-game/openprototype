//! The core contract every scene implements.
//!
//! A scene is a step function: it consumes the key events that arrived since
//! the last frame and produces the next framebuffer plus any side effects
//! (audio, quit). The core never owns the loop. The platform calls [`step`],
//! presents [`framebuffer`], drains the audio, and stops on `quit`.
//!
//! [`step`]: Game::step
//! [`framebuffer`]: Game::framebuffer

use crate::core::audio::AudioCommand;
use crate::core::framebuffer::Framebuffer;
use crate::core::input::KeyEvent;

/// The side effects a single [`Game::step`] produced. The framebuffer is read
/// separately via [`Game::framebuffer`] so a step never has to clone 64 KB.
#[derive(Debug, Default)]
pub struct StepOutput {
    /// Music changes to apply this frame, in order.
    pub audio: Vec<AudioCommand>,
    /// The platform should tear down and exit after presenting this frame.
    pub quit: bool,
}

/// A driveable scene: advance one frame, then expose the result.
pub trait Game {
    /// Advance one frame given the key events since the last call.
    fn step(&mut self, input: &[KeyEvent]) -> StepOutput;

    /// The frame produced by the most recent [`step`](Game::step).
    fn framebuffer(&self) -> &Framebuffer;
}
