//! The winit + pixels backend.
//!
//! This is the only module that knows a windowing toolkit or a GPU surface
//! exists. It owns the event loop, translates physical keys into [`KeyEvent`]s,
//! drives the core's [`step`](Game::step), presents the 320x200 framebuffer
//! scaled (pixels letterboxes to keep the 4:2.5 aspect), and routes audio
//! commands to a [`MusicPlayer`]. Swapping backends means rewriting this file
//! and nothing in `core`.

use std::sync::Arc;

use anyhow::{Context, Result};
use pixels::{Pixels, SurfaceTexture};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::core::framebuffer::{SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::core::game::Game;
use crate::core::input::KeyEvent;
use crate::platform::audio::{LoggingMusicPlayer, MusicPlayer};

const INITIAL_SCALE: u32 = 4;

/// Run the given scene until it quits or the window closes.
pub fn run(game: Box<dyn Game>) -> Result<()> {
    let event_loop = EventLoop::new().context("creating the event loop")?;
    let mut app = App {
        game,
        music: Box::new(LoggingMusicPlayer),
        render: None,
        pending_error: None,
    };

    event_loop
        .run_app(&mut app)
        .context("running the event loop")?;

    match app.pending_error.take() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// The window and its GPU surface, created once the event loop is active.
struct Render {
    window: Arc<Window>,
    pixels: Pixels<'static>,
}

struct App {
    game: Box<dyn Game>,
    music: Box<dyn MusicPlayer>,
    render: Option<Render>,
    pending_error: Option<anyhow::Error>,
}

impl App {
    /// Advance the core by `input`, execute its audio, present, and exit on quit.
    fn pump(&mut self, event_loop: &ActiveEventLoop, input: &[KeyEvent]) {
        let output = self.game.step(input);

        for command in &output.audio {
            match command {
                crate::core::audio::AudioCommand::PlayTrack(track) => self.music.play_track(*track),
                crate::core::audio::AudioCommand::StopMusic => self.music.stop(),
            }
        }

        if let Err(error) = self.present() {
            self.fail(event_loop, error);
            return;
        }

        if output.quit {
            event_loop.exit();
        }
    }

    /// Copy the current framebuffer into the surface and draw it.
    fn present(&mut self) -> Result<()> {
        let Some(render) = self.render.as_mut() else {
            return Ok(());
        };

        let rgba = self.game.framebuffer().to_rgba8();
        render.pixels.frame_mut().copy_from_slice(&rgba);
        render.pixels.render().context("presenting the frame")?;
        Ok(())
    }

    /// Record an error to return from [`run`] and stop the loop.
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.pending_error = Some(error);
        event_loop.exit();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.render.is_some() {
            return;
        }

        match create_render(event_loop) {
            Ok(render) => {
                self.render = Some(render);
                self.pump(event_loop, &[]);
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                if let Some(render) = self.render.as_mut() {
                    let _ = render.pixels.resize_surface(size.width, size.height);
                    render.window.request_redraw();
                }
            }

            WindowEvent::RedrawRequested => {
                if let Err(error) = self.present() {
                    self.fail(event_loop, error);
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed || event.repeat {
                    return;
                }

                if let Some(key) = translate_key(&event.logical_key) {
                    self.pump(event_loop, &[key]);
                }
            }

            _ => {}
        }
    }
}

fn create_render(event_loop: &ActiveEventLoop) -> Result<Render> {
    let size = LogicalSize::new(SCREEN_WIDTH * INITIAL_SCALE, SCREEN_HEIGHT * INITIAL_SCALE);
    let attributes = Window::default_attributes()
        .with_title("Prototype")
        .with_inner_size(size)
        .with_min_inner_size(LogicalSize::new(SCREEN_WIDTH, SCREEN_HEIGHT));

    let window = Arc::new(
        event_loop
            .create_window(attributes)
            .context("creating the window")?,
    );

    let physical = window.inner_size();
    let surface_texture = SurfaceTexture::new(physical.width, physical.height, window.clone());
    let pixels = Pixels::new(SCREEN_WIDTH, SCREEN_HEIGHT, surface_texture)
        .context("creating the pixel surface")?;

    Ok(Render { window, pixels })
}

fn translate_key(key: &Key) -> Option<KeyEvent> {
    match key {
        Key::Named(NamedKey::ArrowUp) => Some(KeyEvent::Up),
        Key::Named(NamedKey::ArrowDown) => Some(KeyEvent::Down),
        Key::Named(NamedKey::Enter) => Some(KeyEvent::Enter),
        Key::Named(NamedKey::Escape) => Some(KeyEvent::Esc),
        Key::Character(text) => text.chars().next().map(KeyEvent::Char),
        _ => None,
    }
}
