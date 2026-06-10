//! The winit event loop driving the wgpu [`Renderer`].
//!
//! This is the only module that knows a windowing toolkit exists. It owns the
//! event loop, translates physical keys into [`KeyEvent`]s, drives the core's
//! [`step`](Game::step) on a fixed timestep, presents the framebuffer through
//! the renderer, and routes audio commands to a [`MusicPlayer`].
//!
//! When the game is static (a menu) the loop waits for input and only steps on a
//! key (with `dt = 0`). When it reports [`is_animating`](Game::is_animating) the
//! loop drives frames on a timer at the scene's own
//! [`frame_interval`](Game::frame_interval), the VGA retrace rate of its mode
//! (~70 Hz front-end, ~60 Hz level). Each timer frame steps once with that exact
//! `dt`, so logic runs at the same rate on any host, like the vsync-locked
//! original. Input/resize frames re-render with `dt = 0` and do not advance.
//!
//! Alt+Enter toggles borderless fullscreen here (the OpenTyrian binding); it is
//! handled in this layer and never reaches the core as a key.

use std::mem;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use prototype_disc::DiscImage;
use prototype_formats::Dimensions;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use crate::audio::{MusicPlayer, SfxPlayer, make_music_player, make_sfx_player};
use crate::renderer::Renderer;
use openprototype_core::audio::AudioCommand;
use openprototype_core::game::Game;
use openprototype_core::input::{Key as CoreKey, KeyEvent};

const INITIAL_SCALE: u32 = 4;

/// Run the given scene until it quits or the window closes. `disc` is handed to
/// the audio backend so it can stream the CD-DA tracks on demand.
pub fn run(game: Box<dyn Game>, disc: Arc<DiscImage>) -> Result<()> {
    let event_loop = EventLoop::new().context("creating the event loop")?;
    let mut app = App {
        game,
        music: make_music_player(disc),
        sfx: make_sfx_player(),
        renderer: None,
        pending_input: Vec::new(),
        modifiers: ModifiersState::empty(),
        advance: false,
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

struct App {
    game: Box<dyn Game>,
    music: Box<dyn MusicPlayer>,
    sfx: Box<dyn SfxPlayer>,
    renderer: Option<Renderer>,
    /// Keys that arrived since the last frame, drained into the next step.
    pending_input: Vec<KeyEvent>,
    /// Held modifiers, tracked for the Alt+Enter fullscreen toggle.
    modifiers: ModifiersState,
    /// Set when the animation timer fires, so the next frame advances one logic
    /// tick. Input and resize frames leave it clear and re-render without ticking.
    advance: bool,
    /// When the next animating frame is due (only meaningful while animating).
    next_frame: Option<Instant>,
    pending_error: Option<anyhow::Error>,
}

impl App {
    /// Advance the core by the elapsed time and the queued input, execute its
    /// audio, present, and exit on quit.
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        // Fixed timestep: a timer frame advances one logic period; an input or
        // resize frame just re-renders the current state.
        let dt = if self.advance {
            self.game.frame_interval()
        } else {
            Duration::ZERO
        };
        self.advance = false;

        let input = mem::take(&mut self.pending_input);
        let output = self.game.step(dt, &input);

        for command in &output.audio {
            match command {
                AudioCommand::PlayTrack(track) => self.music.play_track(*track),
                AudioCommand::StopMusic => self.music.stop(),
                AudioCommand::PlaySfx(play) => {
                    self.sfx
                        .play(play.channel, play.sample.clone(), play.looped);
                }
                AudioCommand::EndSfxLoop { channel } => self.sfx.end_loop(*channel),
            }
        }

        if let Some(renderer) = self.renderer.as_mut()
            && let Err(error) = renderer.render(self.game.framebuffer())
        {
            self.fail(event_loop, error);
            return;
        }

        if output.quit {
            event_loop.exit();
        }
    }

    fn request_redraw(&self) {
        if let Some(renderer) = &self.renderer {
            renderer.window().request_redraw();
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
        if self.renderer.is_some() {
            return;
        }

        let source = self.game.framebuffer().image.size;

        match create_renderer(event_loop, source) {
            Ok(renderer) => {
                self.renderer = Some(renderer);
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
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }

                self.request_redraw();
            }

            WindowEvent::RedrawRequested => self.frame(event_loop),

            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),

            WindowEvent::KeyboardInput { event, .. } => {
                if event.repeat {
                    return;
                }

                let pressed = event.state == ElementState::Pressed;

                if pressed
                    && self.modifiers.alt_key()
                    && event.logical_key == Key::Named(NamedKey::Enter)
                {
                    if let Some(renderer) = &self.renderer {
                        renderer.toggle_fullscreen();
                    }

                    self.request_redraw();
                    return;
                }

                if let Some(key) = translate_key(&event.logical_key) {
                    self.pending_input.push(if pressed {
                        KeyEvent::Pressed(key)
                    } else {
                        KeyEvent::Released(key)
                    });
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

        // Animating: drive frames on the active scene's own retrace-rate timer.
        let interval = self.game.frame_interval();
        let now = Instant::now();
        let due = self.next_frame.unwrap_or(now);

        if now >= due {
            self.next_frame = Some(now + interval);
            self.advance = true;
            self.request_redraw();
        }

        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame.unwrap_or(now)));
    }
}

/// Create the window at a 4:3 shape (so it fills with no letterbox bars) and
/// build the renderer for the initial `source` frame size.
fn create_renderer(event_loop: &ActiveEventLoop, source: Dimensions) -> Result<Renderer> {
    let width = source.width * INITIAL_SCALE;
    let height = width * 3 / 4;
    let attributes = Window::default_attributes()
        .with_title("Prototype")
        .with_inner_size(LogicalSize::new(width, height))
        .with_min_inner_size(LogicalSize::new(source.width, source.width * 3 / 4));

    let window = Arc::new(
        event_loop
            .create_window(attributes)
            .context("creating the window")?,
    );

    Renderer::new(window, source)
}

fn translate_key(key: &Key) -> Option<CoreKey> {
    match key {
        Key::Named(NamedKey::ArrowUp) => Some(CoreKey::Up),
        Key::Named(NamedKey::ArrowDown) => Some(CoreKey::Down),
        Key::Named(NamedKey::ArrowLeft) => Some(CoreKey::Left),
        Key::Named(NamedKey::ArrowRight) => Some(CoreKey::Right),
        Key::Named(NamedKey::Enter) => Some(CoreKey::Enter),
        Key::Named(NamedKey::Escape) => Some(CoreKey::Esc),
        Key::Named(NamedKey::Control) => Some(CoreKey::Ctrl),
        Key::Named(NamedKey::Shift) => Some(CoreKey::Shift),
        Key::Character(text) => text.chars().next().map(CoreKey::Char),
        _ => None,
    }
}
