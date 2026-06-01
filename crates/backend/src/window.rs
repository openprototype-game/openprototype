//! The winit + pixels backend.
//!
//! This is the only module that knows a windowing toolkit or a GPU surface
//! exists. It owns the event loop, translates physical keys into [`KeyEvent`]s,
//! drives the core's [`step`](Game::step) with the elapsed time, presents the
//! 320x200 framebuffer scaled (pixels letterboxes to keep the 4:2.5 aspect),
//! and routes audio commands to a [`MusicPlayer`].
//!
//! When the game is static (a menu) the loop waits for input and only steps on
//! a key. When it reports [`is_animating`](Game::is_animating) (the intro) the
//! loop drives frames on a ~70 Hz timer, the original's VGA retrace rate.
//! Swapping backends means rewriting this file and nothing in `core`.

use std::mem;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use pixels::{Pixels, SurfaceTexture};
use prototype_disc::DiscImage;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::audio::{MusicPlayer, make_music_player};
use openprototype_core::audio::AudioCommand;
use openprototype_core::framebuffer::{SCREEN_HEIGHT, SCREEN_WIDTH};
use openprototype_core::game::Game;
use openprototype_core::input::KeyEvent;

const INITIAL_SCALE: u32 = 4;

/// Frame interval while animating: ~1/70 s, the original's VGA retrace rate.
const FRAME_INTERVAL: Duration = Duration::from_micros(14_286);

/// Largest `dt` handed to the core, so a stall (or a long idle before an
/// animation starts) can't make a scene jump far ahead in one frame.
const MAX_FRAME_DT: Duration = Duration::from_millis(100);

/// Run the given scene until it quits or the window closes. `disc` is handed to
/// the audio backend so it can stream the CD-DA tracks on demand.
pub fn run(game: Box<dyn Game>, disc: Arc<DiscImage>) -> Result<()> {
    let event_loop = EventLoop::new().context("creating the event loop")?;
    let mut app = App {
        game,
        music: make_music_player(disc),
        render: None,
        pending_input: Vec::new(),
        last_frame: None,
        next_frame: None,
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
    /// Keys that arrived since the last frame, drained into the next step.
    pending_input: Vec<KeyEvent>,
    /// When the previous frame ran, for measuring `dt`.
    last_frame: Option<Instant>,
    /// When the next animating frame is due (only meaningful while animating).
    next_frame: Option<Instant>,
    pending_error: Option<anyhow::Error>,
}

impl App {
    /// Advance the core by the elapsed time and the queued input, execute its
    /// audio, present, and exit on quit.
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .map(|prev| now.saturating_duration_since(prev).min(MAX_FRAME_DT))
            .unwrap_or_default();
        self.last_frame = Some(now);

        let input = mem::take(&mut self.pending_input);
        let output = self.game.step(dt, &input);

        for command in &output.audio {
            match command {
                AudioCommand::PlayTrack(track) => self.music.play_track(*track),
                AudioCommand::StopMusic => self.music.stop(),
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

    fn request_redraw(&self) {
        if let Some(render) = &self.render {
            render.window.request_redraw();
        }
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
                self.request_redraw(); // draw the first frame
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

            WindowEvent::RedrawRequested => self.frame(event_loop),

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed || event.repeat {
                    return;
                }

                if let Some(key) = translate_key(&event.logical_key) {
                    self.pending_input.push(key);
                    self.request_redraw();
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if !self.game.is_animating() {
            self.next_frame = None;
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }

        // Animating: drive frames on the retrace-rate timer.
        let now = Instant::now();
        let due = self.next_frame.unwrap_or(now);

        if now >= due {
            self.next_frame = Some(now + FRAME_INTERVAL);
            self.request_redraw();
        }

        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame.unwrap_or(now)));
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
