//! The core contract every scene implements.
//!
//! A scene is a step function: given the time since the last frame and the key
//! events that arrived in it, it produces the next framebuffer plus any side
//! effects (audio, quit). The core never owns the loop. The platform calls
//! [`step`], presents [`framebuffer`], drains the audio, and stops on `quit`.
//! [`is_animating`] tells the platform whether to keep ticking on a timer or
//! wait for the next key.
//!
//! [`step`]: Game::step
//! [`framebuffer`]: Game::framebuffer
//! [`is_animating`]: Game::is_animating

use std::time::Duration;

use crate::audio::AudioCommand;
use crate::framebuffer::Framebuffer;
use crate::input::KeyEvent;

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
    /// Advance one frame given the elapsed time and the key events since the
    /// last call.
    fn step(&mut self, dt: Duration, input: &[KeyEvent]) -> StepOutput;

    /// The frame produced by the most recent [`step`](Game::step).
    fn framebuffer(&self) -> &Framebuffer;

    /// Whether the game needs to keep advancing on a timer (animating) rather
    /// than waiting for input. The platform polls this after each step.
    fn is_animating(&self) -> bool {
        false
    }
}
