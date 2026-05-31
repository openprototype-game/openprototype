//! Platform layer: the event loop, the GPU surface, and the audio device.
//!
//! Everything backend-specific lives here. The core stays unaware of winit,
//! pixels and the audio device, so a different backend means rewriting this
//! module alone.

pub mod audio;
pub mod window;

pub use window::run;
