//! Desktop backend: the event loop, the GPU surface, and the audio device.
//!
//! Everything backend-specific lives in this crate. The core stays unaware of
//! winit, pixels and the audio device, so a different backend means rewriting
//! this crate alone. The game drives it through [`run`].

pub mod audio;
pub mod compositor;
pub mod renderer;
pub mod window;

pub use window::run;
