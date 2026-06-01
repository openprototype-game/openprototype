// Two-pass present: expand palette indices to RGBA, then scale into a 4:3
// letterbox. Splitting expansion from scaling keeps the scaler swappable (a
// future scaler menu replaces only the second pass).

struct Stage {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// ---- Pass 1: expand indices through the palette ----
//
// Renders a fullscreen triangle over an offscreen target the size of the
// source frame, so each output pixel maps one-to-one to a source index.

@group(0) @binding(0) var index_tex: texture_2d<u32>;
@group(0) @binding(1) var palette_tex: texture_2d<f32>;

@vertex
fn vs_fullscreen(@builtin(vertex_index) vertex_index: u32) -> Stage {
    let uv = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    var out: Stage;
    out.position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_expand(in: Stage) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(floor(in.position.xy));
    let index = textureLoad(index_tex, coord, 0).r;
    return textureLoad(palette_tex, vec2<i32>(i32(index), 0), 0);
}

// ---- Pass 2: sharp-bilinear scale into the 4:3 content rect ----
//
// The quad covers only the centered 4:3 area (the surface is cleared black
// first, giving the letterbox bars). Sharp-bilinear keeps pixels crisp at the
// non-integer 4:3 scale: nearest within a texel, a one-output-pixel bilinear
// band at the edges. It needs a linear sampler on the expanded RGBA.

struct Fit {
    // Half-extent of the content quad in clip space (content / surface).
    ndc_scale: vec2<f32>,
    // Source frame size in texels.
    source_size: vec2<f32>,
    // Content rect size in physical pixels (drives the prescale factor).
    output_size: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0) var source_tex: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> fit: Fit;

@vertex
fn vs_fit(@builtin(vertex_index) vertex_index: u32) -> Stage {
    // Four-vertex triangle strip: (0,0), (1,0), (0,1), (1,1).
    let uv = vec2<f32>(f32(vertex_index & 1u), f32((vertex_index >> 1u) & 1u));
    let ndc = (uv * 2.0 - 1.0) * fit.ndc_scale;
    var out: Stage;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_scale(in: Stage) -> @location(0) vec4<f32> {
    let texel = in.uv * fit.source_size;
    let scale = max(floor(fit.output_size / fit.source_size), vec2<f32>(1.0, 1.0));
    let texel_floor = floor(texel);
    let center = (texel - texel_floor) - 0.5;
    let region = 0.5 - 0.5 / scale;
    let offset = (center - clamp(center, -region, region)) * scale + 0.5;
    let uv = (texel_floor + offset) / fit.source_size;
    return textureSampleLevel(source_tex, source_sampler, uv, 0.0);
}
