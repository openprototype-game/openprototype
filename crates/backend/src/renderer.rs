//! The window-facing renderer: a wgpu surface wrapped around a [`Compositor`].
//!
//! This owns the swapchain and translates a window into something the
//! [`Compositor`] can draw into. Acquiring the surface texture is where the old
//! `pixels` backend wedged on X11: its internal retry loop spun forever when the
//! drawable changed under it. Here a surface-state error reconfigures once and
//! skips the frame, re-arming a redraw so a static scene recovers.

use std::sync::Arc;

use anyhow::{Context, Result};
use openprototype_core::framebuffer::Framebuffer;
use prototype_formats::Dimensions;
use tracing::debug;
use winit::window::Window;

use crate::compositor::Compositor;

pub struct Renderer {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    compositor: Compositor,
}

impl Renderer {
    /// Build the renderer for `window`, sized for an initial `source_size` frame.
    pub fn new(window: Arc<Window>, source_size: Dimensions) -> Result<Self> {
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .context("creating the render surface")?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }))
        .context("no GPU adapter could drive the window")?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("openprototype device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .context("requesting the GPU device")?;

        let caps = surface.get_capabilities(&adapter);
        // A non-sRGB format so the 8-bit palette colors reach the screen
        // unchanged, the way the VGA DAC drove them (no gamma re-encode).
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .unwrap_or(caps.formats[0]);

        // Mailbox never blocks presentation, so the window loop's frame timer
        // can run the scene's true rate (70Hz front-end, 60Hz level) on any
        // panel and the compositor picks the freshest frame each vblank. Where
        // it is unavailable (some Wayland compositors, macOS) vsync-capped
        // Fifo is fine too: the loop's catch-up keeps logic on the wall clock,
        // it just coalesces steps when the panel is slower than the scene.
        let present_mode = if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else {
            wgpu::PresentMode::AutoVsync
        };
        debug!("presenting with {present_mode:?}");

        let physical = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: physical.width.max(1),
            height: physical.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let compositor = Compositor::new(device, queue, source_size, format);

        Ok(Self {
            window,
            surface,
            config,
            compositor,
        })
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    /// Reconfigure the surface to a new window size. A zero dimension (minimised)
    /// is ignored; the next non-zero resize restores it.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.config.width = width;
        self.config.height = height;
        self.surface
            .configure(self.compositor.device(), &self.config);
    }

    /// Toggle borderless fullscreen on the current monitor.
    pub fn toggle_fullscreen(&self) {
        let fullscreen = match self.window.fullscreen() {
            Some(_) => None,
            None => Some(winit::window::Fullscreen::Borderless(None)),
        };

        self.window.set_fullscreen(fullscreen);
    }

    /// Present `frame`. Surface-state errors (a resize or fullscreen toggle in
    /// flight) reconfigure and skip the frame rather than retry-looping, which is
    /// what wedged the old `pixels` backend on X11.
    pub fn render(&mut self, frame: &Framebuffer) -> Result<()> {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                // The drawable changed under us (a resize or fullscreen toggle
                // mid-flight). Reconfigure to the window's real size and ask for
                // another frame: a static scene (a menu) has no animation timer
                // to retry on its own, so without the redraw it would sit on a
                // stale frame until the next input.
                let size = self.window.inner_size();

                if size.width > 0 && size.height > 0 {
                    self.resize(size.width, size.height);
                    self.window.request_redraw();
                }

                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return Ok(()),
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.compositor
            .render_to(frame, &view, self.config.width, self.config.height);

        surface_texture.present();
        Ok(())
    }
}
