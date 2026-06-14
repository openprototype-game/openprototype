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
//! (~70 Hz front-end, ~60 Hz level). Logic advances in whole periods with that
//! exact `dt`, like the vsync-locked original. Deadlines advance from the
//! previous deadline, not from when the timer happened to fire, and a late
//! redraw runs every period it covers before presenting: logic time tracks wall
//! clock, so the visuals stay aligned with the real-time CD audio even when
//! vsync caps the redraw rate below the scene's frame rate. Input/resize frames
//! re-render with `dt = 0` and do not advance.
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

/// The most logic time one redraw may catch up.
///
/// A redraw that arrives late (vsync capping the rate, a busy compositor) runs
/// every period it covers, but in bursts of at most this much; the rest stays
/// in the backlog for the next redraws, so a stall amortizes instead of freezing
/// the app.
const MAX_CATCHUP_PER_FRAME: Duration = Duration::from_millis(250);

/// A backlog longer than this is abandoned rather than fast-forwarded.
///
/// A suspend or a debugger pause causes one; the scene resumes at its normal
/// rate.
const BACKLOG_RESET: Duration = Duration::from_secs(1);

/// How many whole `interval` periods a late redraw must run.
///
/// A redraw arriving `behind` its deadline runs enough periods to keep logic
/// time on wall clock, capped at [`MAX_CATCHUP_PER_FRAME`].
fn backlog_steps(behind: Duration, interval: Duration) -> u32 {
    let limit = (MAX_CATCHUP_PER_FRAME.as_nanos() / interval.as_nanos()).max(1) as u32;
    let due = (behind.as_nanos() / interval.as_nanos() + 1) as u32;
    due.min(limit)
}

/// Runs the given scene until it quits or the window closes.
///
/// `disc` is handed to the audio backend so it can stream the CD-DA tracks on
/// demand.
pub fn run(game: Box<dyn Game>, disc: Arc<DiscImage>) -> Result<()> {
    let event_loop = EventLoop::new().context("creating the event loop")?;
    let mut app = App {
        game,
        music: make_music_player(disc),
        sfx: make_sfx_player(),
        renderer: None,
        pending_input: Vec::new(),
        modifiers: ModifiersState::empty(),
        pending_steps: 0,
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
    /// Whole logic periods the next redraw must advance (usually one; more when
    /// the timer fired late). Input and resize frames leave it at zero and
    /// re-render without advancing.
    pending_steps: u32,
    /// When the next animating frame is due (only meaningful while animating).
    next_frame: Option<Instant>,
    pending_error: Option<anyhow::Error>,
}

impl App {
    /// Advances the core by the pending logic periods, then presents.
    ///
    /// With no periods due, re-renders as-is. Drains the queued input into the
    /// step, then presents and exits on quit.
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        // Fixed timestep: a timer frame advances every logic period it covers
        // (one, unless it arrived late); an input or resize frame just
        // re-renders the current state.
        let steps = mem::take(&mut self.pending_steps);
        let input = mem::take(&mut self.pending_input);

        let mut quit = false;

        if steps == 0 {
            quit = self.advance_game(Duration::ZERO, &input);
        } else {
            for step in 0..steps {
                // Re-read the interval per step: a step may switch scenes, and
                // the new scene's rate applies from that point on.
                let dt = self.game.frame_interval();
                let input = if step == 0 { input.as_slice() } else { &[] };

                if self.advance_game(dt, input) {
                    quit = true;
                    break;
                }
            }
        }

        if let Some(renderer) = self.renderer.as_mut()
            && let Err(error) = renderer.render(self.game.framebuffer())
        {
            self.fail(event_loop, error);
            return;
        }

        if quit {
            event_loop.exit();
        }
    }

    /// Steps the core once and executes its audio commands.
    ///
    /// Reports whether it asked to quit.
    fn advance_game(&mut self, dt: Duration, input: &[KeyEvent]) -> bool {
        let output = self.game.step(dt, input);

        for command in &output.audio {
            match command {
                AudioCommand::PlayTrack(track) => self.music.play_track(*track),
                AudioCommand::StopMusic => self.music.stop(),
                AudioCommand::PlaySfx(play) => {
                    self.sfx.play(
                        play.channel,
                        play.sample.clone(),
                        play.looped,
                        play.skip_if_busy,
                    );
                }
                AudioCommand::EndSfxLoop { channel } => self.sfx.end_loop(*channel),
                AudioCommand::SetEffectsVolume(volume) => self.sfx.set_volume(*volume),
                AudioCommand::SetMusicVolume(volume) => self.music.set_volume(*volume),
            }
        }

        output.quit
    }

    fn request_redraw(&self) {
        if let Some(renderer) = &self.renderer {
            renderer.window().request_redraw();
        }
    }

    /// Records an error to return from [`run`] and stops the loop.
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
        // Deadlines advance by whole intervals from the previous deadline, not
        // from `now`, so the timer's wakeup latency never accumulates into the
        // logic clock.
        let interval = self.game.frame_interval();
        let now = Instant::now();
        let due = self.next_frame.unwrap_or(now);

        if now >= due {
            let behind = now - due;

            if behind >= BACKLOG_RESET {
                self.pending_steps = self.pending_steps.saturating_add(1);
                self.next_frame = Some(now + interval);
            } else {
                let steps = backlog_steps(behind, interval);
                self.pending_steps = self.pending_steps.saturating_add(steps);
                // A capped catch-up leaves this in the past on purpose: the
                // next wakeup fires immediately and works off more backlog.
                self.next_frame = Some(due + interval * steps);
            }

            self.request_redraw();
        }

        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame.unwrap_or(now)));
    }
}

/// Creates the window at a 4:3 shape and builds its renderer.
///
/// The 4:3 shape means the content fills the window with no letterbox bars.
/// Sized for the initial `source` frame.
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
        Key::Named(NamedKey::Backspace) => Some(CoreKey::Backspace),
        Key::Character(text) => text.chars().next().map(CoreKey::Char),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL: Duration = Duration::from_micros(14_286);

    #[test]
    fn an_on_time_frame_runs_one_step() {
        assert_eq!(backlog_steps(Duration::ZERO, INTERVAL), 1);
        assert_eq!(backlog_steps(INTERVAL / 2, INTERVAL), 1);
    }

    #[test]
    fn a_late_frame_runs_every_period_it_covers() {
        // 1.5 periods late: the missed deadline plus the current one.
        assert_eq!(backlog_steps(INTERVAL + INTERVAL / 2, INTERVAL), 2);
        assert_eq!(backlog_steps(INTERVAL * 4, INTERVAL), 5);
    }

    #[test]
    fn catch_up_is_capped_per_frame() {
        let limit = (MAX_CATCHUP_PER_FRAME.as_nanos() / INTERVAL.as_nanos()) as u32;

        assert_eq!(backlog_steps(Duration::from_secs(1), INTERVAL), limit);
    }
}
