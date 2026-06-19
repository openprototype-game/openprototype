//! The wgpu two-pass compositor: a [`Framebuffer`] to a scaled RGBA image.
//!
//! Two render passes turn a frame into the final window image:
//!
//! 1. **Expand** the palette indices into an offscreen RGBA texture the size of
//!    the source frame. The index frame uploads as an `R8Uint` texture and the
//!    256-color palette as a `256x1` RGBA texture; the shader does
//!    `rgba = palette[index]`. This is the GPU equivalent of the VGA DAC, and it
//!    means a palette-only change (a fade, FLIC cycling) is a 1 KB re-upload with
//!    the index texture untouched.
//! 2. **Scale** the expanded frame into a centered 4:3 rectangle with
//!    sharp-bilinear filtering, clearing the rest of the target black (the
//!    letterbox bars). Keeping expansion and scaling in separate passes lets a
//!    future scaler menu swap only the second pass.
//!
//! The source texture is sized from the frame itself and rebuilt when a scene
//! changes resolution (the front-end is 320x200; in-game modes differ), so the
//! pipeline never assumes a fixed size. The compositor knows nothing about a
//! window or surface: it renders into any [`TextureView`](wgpu::TextureView), so
//! it can target the swapchain or an offscreen texture in a test.

use openprototype_core::framebuffer::Framebuffer;
use prototype_formats::Dimensions;

/// The display aspect every frame is fitted into: a 4:3 CRT.
///
/// Applied regardless of the source's own pixel aspect (320x200 is stretched,
/// 320x240 is already 4:3). A future square-pixel toggle would derive this from
/// the source size.
const TARGET_ASPECT: f32 = 4.0 / 3.0;

/// The offscreen (expand-pass output) format.
///
/// Linear unorm so the 8-bit palette colors blend and present unchanged, the
/// way the VGA DAC drove them.
const OFFSCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// How the centered content rect is sized inside the window.
///
/// Both modes hold the 4:3 display aspect; they differ in whether the vertical
/// scale is free or snapped to the pixel grid.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ScaleMode {
    /// Largest 4:3 rect that fits the window. Both axes scale fractionally, so
    /// the sharp shader keeps a sub-pixel interpolation band on each.
    #[default]
    PerfectFit,

    /// Vertical scale snapped down to a whole multiple of the source height, so
    /// every scanline is the same thickness; the width is re-derived to keep
    /// exact 4:3, leaving only the horizontal axis fractional. Costs a little
    /// extra letterbox in return for a uniform pixel grid vertically.
    PerfectY,
}

/// The `Fit` uniform, matching the `blit.wgsl` layout.
///
/// 32 bytes so the `vec2` members land at the offsets the WGSL uniform layout
/// expects.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FitUniform {
    ndc_scale: [f32; 2],
    source_size: [f32; 2],
    output_size: [f32; 2],
    /// Clip-space translation of the content quad. Used by `PerfectY` to nudge
    /// the rect onto a whole-pixel vertical position; zero otherwise.
    ndc_offset: [f32; 2],
}

/// The source-size-dependent resources.
///
/// The uploaded index frame and the offscreen RGBA target the expand pass
/// writes and the scale pass samples.
struct Source {
    size: Dimensions,
    index_texture: wgpu::Texture,
    offscreen_view: wgpu::TextureView,
}

/// The two-pass palette-expand and scale compositor.
pub struct Compositor {
    device: wgpu::Device,
    queue: wgpu::Queue,

    expand_pipeline: wgpu::RenderPipeline,
    scale_pipeline: wgpu::RenderPipeline,
    expand_layout: wgpu::BindGroupLayout,
    scale_layout: wgpu::BindGroupLayout,

    palette_texture: wgpu::Texture,
    sampler: wgpu::Sampler,
    fit_buffer: wgpu::Buffer,

    source: Source,
    expand_bind_group: wgpu::BindGroup,
    scale_bind_group: wgpu::BindGroup,

    scale_mode: ScaleMode,
}

impl Compositor {
    /// Builds the pipelines for an initial `source_size` frame.
    ///
    /// `target_format` is the format of the views [`render_to`](Self::render_to)
    /// will draw into (the swapchain format, or an offscreen format in a test).
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        source_size: Dimensions,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()),
        });

        let expand_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("expand"),
            entries: &[
                texture_entry(0, wgpu::TextureSampleType::Uint),
                texture_entry(1, wgpu::TextureSampleType::Float { filterable: false }),
            ],
        });

        let scale_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scale"),
            entries: &[
                texture_entry(0, wgpu::TextureSampleType::Float { filterable: true }),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let expand_pipeline = build_pipeline(
            &device,
            &shader,
            &expand_layout,
            "vs_fullscreen",
            "fs_expand",
            OFFSCREEN_FORMAT,
            wgpu::PrimitiveTopology::TriangleList,
        );
        let scale_pipeline = build_pipeline(
            &device,
            &shader,
            &scale_layout,
            "vs_fit",
            "fs_scale",
            target_format,
            wgpu::PrimitiveTopology::TriangleStrip,
        );

        let palette_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("palette"),
            size: wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("source"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let fit_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fit"),
            size: std::mem::size_of::<FitUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let source = build_source(&device, source_size);
        let palette_view = palette_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let expand_bind_group =
            build_expand_bind_group(&device, &expand_layout, &source, &palette_view);
        let scale_bind_group =
            build_scale_bind_group(&device, &scale_layout, &source, &sampler, &fit_buffer);

        Self {
            device,
            queue,
            expand_pipeline,
            scale_pipeline,
            expand_layout,
            scale_layout,
            palette_texture,
            sampler,
            fit_buffer,
            source,
            expand_bind_group,
            scale_bind_group,
            scale_mode: ScaleMode::default(),
        }
    }

    /// The wgpu device the pipelines were built on.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Switches to the next scaling mode, returning the one now active.
    ///
    /// Takes effect on the next [`render_to`](Self::render_to); the caller
    /// requests a redraw to apply it immediately.
    pub fn cycle_scale_mode(&mut self) -> ScaleMode {
        self.scale_mode = match self.scale_mode {
            ScaleMode::PerfectFit => ScaleMode::PerfectY,
            ScaleMode::PerfectY => ScaleMode::PerfectFit,
        };

        self.scale_mode
    }

    /// Uploads `frame` and renders it into `target`.
    ///
    /// `target` is a view `target_width` by `target_height` pixels. Expands
    /// indices to the offscreen texture, then scales into a centered 4:3 rect
    /// with the rest cleared black.
    pub fn render_to(
        &mut self,
        frame: &Framebuffer,
        target: &wgpu::TextureView,
        target_width: u32,
        target_height: u32,
    ) {
        if frame.image.size != self.source.size {
            self.rebuild_source(frame.image.size);
        }

        self.upload(frame, target_width, target_height);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("present"),
            });

        self.expand_pass(&mut encoder);
        self.scale_pass(&mut encoder, target);

        self.queue.submit([encoder.finish()]);
    }

    /// Pushes the frame's indices, palette and fit transform to the GPU.
    fn upload(&self, frame: &Framebuffer, target_width: u32, target_height: u32) {
        let size = self.source.size;
        self.queue.write_texture(
            self.source.index_texture.as_image_copy(),
            &frame.image.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size.width),
                rows_per_image: Some(size.height),
            },
            wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth_or_array_layers: 1,
            },
        );

        let mut palette = [0u8; 256 * 4];
        for (index, color) in frame.palette.colors.iter().enumerate() {
            palette[index * 4..index * 4 + 4].copy_from_slice(&[color.r, color.g, color.b, 0xff]);
        }

        self.queue.write_texture(
            self.palette_texture.as_image_copy(),
            &palette,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256 * 4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
        );

        let fit = compute_fit(target_width, target_height, size, self.scale_mode);
        self.queue
            .write_buffer(&self.fit_buffer, 0, bytemuck::bytes_of(&fit));
    }

    fn expand_pass(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("expand"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.source.offscreen_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.expand_pipeline);
        pass.set_bind_group(0, &self.expand_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn scale_pass(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scale"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.scale_pipeline);
        pass.set_bind_group(0, &self.scale_bind_group, &[]);
        pass.draw(0..4, 0..1);
    }

    fn rebuild_source(&mut self, size: Dimensions) {
        self.source = build_source(&self.device, size);
        let palette_view = self
            .palette_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.expand_bind_group = build_expand_bind_group(
            &self.device,
            &self.expand_layout,
            &self.source,
            &palette_view,
        );
        self.scale_bind_group = build_scale_bind_group(
            &self.device,
            &self.scale_layout,
            &self.source,
            &self.sampler,
            &self.fit_buffer,
        );
    }
}

/// The largest centered 4:3 rectangle that fits in the target.
///
/// Returned as the quad's clip-space half-extent plus the sizes the
/// sharp-bilinear shader needs.
fn compute_fit(
    target_width: u32,
    target_height: u32,
    source: Dimensions,
    mode: ScaleMode,
) -> FitUniform {
    let target_width = target_width as f32;
    let target_height = target_height as f32;

    // Largest 4:3 rect that fits the window.
    let (mut content_width, mut content_height) = if target_width / target_height > TARGET_ASPECT {
        (target_height * TARGET_ASPECT, target_height)
    } else {
        (target_width, target_width / TARGET_ASPECT)
    };

    let mut ndc_offset = [0.0, 0.0];

    if mode == ScaleMode::PerfectY {
        // Snap the height down to a whole multiple of the source height (at
        // least 1x), then re-derive the width to hold exact 4:3. The snapped
        // rect is never taller than the fitted one, so it still fits the
        // window; only the horizontal scale stays fractional.
        let scale_y = (content_height / source.height as f32).floor().max(1.0);
        content_height = scale_y * source.height as f32;
        content_width = content_height * TARGET_ASPECT;

        // Pixel-align the vertical position. Centering on an odd letterbox would
        // start the rect on a half-pixel, pushing the sample points off the grid
        // and re-softening the band the integer scale was meant to keep crisp.
        // Snap the top to a whole pixel; one physical pixel is 2 / target_height
        // in clip space.
        let top = (target_height - content_height) / 2.0;
        ndc_offset[1] = (top.round() - top) * -2.0 / target_height;
    }

    FitUniform {
        ndc_scale: [content_width / target_width, content_height / target_height],
        source_size: [source.width as f32, source.height as f32],
        output_size: [content_width, content_height],
        ndc_offset,
    }
}

fn build_source(device: &wgpu::Device, size: Dimensions) -> Source {
    let index_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("index"),
        size: wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let offscreen = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen"),
        size: wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: OFFSCREEN_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    let offscreen_view = offscreen.create_view(&wgpu::TextureViewDescriptor::default());

    Source {
        size,
        index_texture,
        offscreen_view,
    }
}

fn build_expand_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    source: &Source,
    palette_view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    let index_view = source
        .index_texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("expand"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&index_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(palette_view),
            },
        ],
    })
}

fn build_scale_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    source: &Source,
    sampler: &wgpu::Sampler,
    fit_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scale"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&source.offscreen_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: fit_buffer.as_entire_binding(),
            },
        ],
    })
}

fn texture_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn build_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    bind_group_layout: &wgpu::BindGroupLayout,
    vertex_entry: &str,
    fragment_entry: &str,
    format: wgpu::TextureFormat,
    topology: wgpu::PrimitiveTopology,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use prototype_formats::{IndexedImage, Palette, Rgb};

    /// A headless device, or `None` when the host has no GPU adapter (so the
    /// test soft-skips rather than failing on a machine without one).
    fn headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .ok()?;

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("compositor test"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            ..Default::default()
        }))
        .ok()
    }

    /// An 8x6 frame split into four solid quadrants (TL=1, TR=2, BL=3, BR=4),
    /// each a distinct palette color. The quadrant layout pins both axes'
    /// orientation; the solid blocks make interior pixels survive filtering
    /// byte-exact (every bilinear tap is the same color).
    fn quadrant_frame() -> (Framebuffer, [[u8; 3]; 4]) {
        let colors = [[200, 0, 0], [0, 200, 0], [0, 0, 200], [200, 200, 0]];

        let mut palette = Palette {
            colors: [Rgb { r: 0, g: 0, b: 0 }; 256],
        };
        for (slot, rgb) in colors.iter().enumerate() {
            palette.colors[slot + 1] = Rgb {
                r: rgb[0],
                g: rgb[1],
                b: rgb[2],
            };
        }

        let (width, height) = (8u32, 6u32);
        let mut pixels = vec![0u8; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                let quadrant = (x >= width / 2) as u8 + 2 * (y >= height / 2) as u8;
                pixels[(y * width + x) as usize] = quadrant + 1;
            }
        }

        let image = IndexedImage::new(Dimensions::new(width, height), pixels).unwrap();
        (Framebuffer { image, palette }, colors)
    }

    /// Render `frame` into a `width` x `height` RGBA target and read it back as
    /// tightly-packed `width * height * 4` bytes.
    fn render_to_bytes(
        device: wgpu::Device,
        queue: wgpu::Queue,
        frame: &Framebuffer,
        width: u32,
        height: u32,
        mode: ScaleMode,
    ) -> Vec<u8> {
        let mut compositor = Compositor::new(
            device,
            queue,
            frame.image.size,
            wgpu::TextureFormat::Rgba8Unorm,
        );

        if mode == ScaleMode::PerfectY {
            compositor.cycle_scale_mode();
        }

        let target = compositor.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());

        compositor.render_to(frame, &view, width, height);

        // copy_texture_to_buffer needs rows padded to 256 bytes.
        let unpadded = width * 4;
        let padded = unpadded.div_ceil(256) * 256;
        let buffer = compositor.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = compositor
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        compositor.queue.submit([encoder.finish()]);

        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        compositor
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let mapped = buffer.slice(..).get_mapped_range();

        let mut out = vec![0u8; (unpadded * height) as usize];
        for row in 0..height {
            let src = (row * padded) as usize;
            let dst = (row * unpadded) as usize;
            out[dst..dst + unpadded as usize]
                .copy_from_slice(&mapped[src..src + unpadded as usize]);
        }
        out
    }

    fn pixel(bytes: &[u8], width: u32, x: u32, y: u32) -> [u8; 3] {
        let offset = ((y * width + x) * 4) as usize;
        [bytes[offset], bytes[offset + 1], bytes[offset + 2]]
    }

    #[test]
    fn expands_and_scales_each_quadrant_to_its_colour() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("no GPU adapter; skipping compositor render test");
            return;
        };

        let (frame, colors) = quadrant_frame();
        // 16x12 is exactly 4:3, so the content fills the target with no bars.
        let (width, height) = (16u32, 12u32);
        let bytes = render_to_bytes(device, queue, &frame, width, height, ScaleMode::PerfectFit);

        // Deep inside each output quadrant, where filtering taps stay within one
        // solid block, the color is byte-exact. The quadrant positions also
        // prove both axes are oriented right (TL is red, not flipped).
        let samples = [
            ((4, 3), colors[0]),  // top-left
            ((12, 3), colors[1]), // top-right
            ((4, 9), colors[2]),  // bottom-left
            ((12, 9), colors[3]), // bottom-right
        ];
        for ((x, y), want) in samples {
            assert_eq!(pixel(&bytes, width, x, y), want, "quadrant at ({x},{y})");
        }
    }

    #[test]
    fn letterboxes_a_wider_target_with_black_bars() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("no GPU adapter; skipping compositor letterbox test");
            return;
        };

        let (frame, _) = quadrant_frame();
        // 24x12 is wider than 4:3, so the 16-wide content centers with 4px bars.
        let (width, height) = (24u32, 12u32);
        let bytes = render_to_bytes(device, queue, &frame, width, height, ScaleMode::PerfectFit);

        assert_eq!(pixel(&bytes, width, 0, 6), [0, 0, 0], "left bar is black");
        assert_eq!(pixel(&bytes, width, 23, 6), [0, 0, 0], "right bar is black");
        assert_ne!(
            pixel(&bytes, width, 12, 6),
            [0, 0, 0],
            "center content is not black"
        );
    }

    #[test]
    fn perfect_y_renders_crisp_pixel_aligned_rows() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("no GPU adapter; skipping compositor perfect-y test");
            return;
        };

        let (frame, colors) = quadrant_frame();
        // 40x21 is wider than 4:3, so it is height-limited. PerfectFit would fill
        // all 21 rows; PerfectY floors the height to 3x the source (18) and, with
        // a centered top of 1.5 (odd letterbox), snaps it to a whole pixel: 2 bar
        // rows on top, content in rows 2..19, 1 bar row at the bottom. Both axes
        // land on an exact 3x grid, so the boundary stays crisp.
        let (width, height) = (40u32, 21u32);
        let bytes = render_to_bytes(device, queue, &frame, width, height, ScaleMode::PerfectY);

        // x=12 sits inside the left column of quadrants (content spans x 8..32).
        assert_eq!(
            pixel(&bytes, width, 12, 0),
            [0, 0, 0],
            "top bar row 0 is black"
        );
        assert_eq!(
            pixel(&bytes, width, 12, 1),
            [0, 0, 0],
            "top bar row 1 is black"
        );
        // The first content row is the exact top-left color, not a blend: this is
        // what the vertical pixel-alignment buys. A half-pixel slip would soften
        // it to something between black and the quadrant color.
        assert_eq!(
            pixel(&bytes, width, 12, 2),
            colors[0],
            "first content row is crisp top-left"
        );
        assert_eq!(
            pixel(&bytes, width, 12, 20),
            [0, 0, 0],
            "bottom bar row 20 is black"
        );
    }

    #[test]
    fn perfect_fit_keeps_the_full_4_3_rect() {
        let source = Dimensions::new(320, 160);
        let fit = compute_fit(1000, 1000, source, ScaleMode::PerfectFit);

        // A square window is width-limited: the content spans the full width
        // and is exactly 4:3 tall, with no integer snapping.
        assert_eq!(fit.output_size, [1000.0, 750.0]);
        // PerfectFit never offsets the quad.
        assert_eq!(fit.ndc_offset, [0.0, 0.0]);
    }

    #[test]
    fn perfect_y_snaps_height_to_a_whole_source_multiple() {
        let source = Dimensions::new(320, 160);
        let fit = compute_fit(1000, 1000, source, ScaleMode::PerfectY);

        // Fitted height is 750; floored to a multiple of 160 that is 4x (640).
        let [width, height] = fit.output_size;
        assert_eq!(height, 640.0, "height snaps down to 4x the source");
        assert_eq!(
            height % source.height as f32,
            0.0,
            "height is a whole multiple"
        );
        assert!(
            (width / height - TARGET_ASPECT).abs() < 1e-4,
            "rect stays 4:3"
        );
        assert!(width <= 1000.0 && height <= 1000.0, "rect fits the window");
    }

    #[test]
    fn perfect_y_matches_perfect_fit_when_the_scale_is_already_integer() {
        let source = Dimensions::new(320, 160);
        // 1280x960 is exactly 4:3 and 960 is 6x the source height, so the two
        // modes agree: nothing to snap.
        let fitted = compute_fit(1280, 960, source, ScaleMode::PerfectFit);
        let snapped = compute_fit(1280, 960, source, ScaleMode::PerfectY);

        assert_eq!(fitted.output_size, snapped.output_size);
        assert_eq!(snapped.output_size, [1280.0, 960.0]);
    }

    #[test]
    fn perfect_y_pixel_aligns_the_vertical_position() {
        let source = Dimensions::new(320, 160);
        // Height-limited (wider than 4:3) with an odd letterbox: 933 floors to
        // a 5x = 800-tall rect, which centered starts at y = 66.5, off the grid.
        let fit = compute_fit(1300, 933, source, ScaleMode::PerfectY);

        let content_height = fit.output_size[1];
        let centered_top = (933.0 - content_height) / 2.0;
        assert_ne!(centered_top.fract(), 0.0, "test needs an odd letterbox");

        // Recover the top the offset actually places the rect at; it must be a
        // whole pixel. One physical pixel is 2 / target_height in clip space.
        let aligned_top = centered_top - fit.ndc_offset[1] * 933.0 / 2.0;
        assert!(
            (aligned_top - aligned_top.round()).abs() < 1e-3,
            "vertical top not pixel-aligned: {aligned_top}"
        );
    }
}
