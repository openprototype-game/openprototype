//! Decode an asset and write it to a PNG for visual inspection.
//!
//! Run with `--help` for the commands. Image commands take an optional
//! `--palette`; without one they fall back to a grayscale ramp that shows
//! geometry when the real palette is not yet known.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use image::{ImageBuffer, Rgb as ImageRgb, RgbImage, Rgba as ImageRgba, RgbaImage};
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::{GameState, Secondary, Weapon, WeaponLevel};
use openprototype_tools::read_asset;
use prototype_disc::DiscImage;
use prototype_formats::font::Font;
use prototype_formats::{
    Dimensions, Flic, IndexedImage, Palette, Sprite, StartExe, background, bdy, bin, pal, raw, smp,
};

#[derive(Parser)]
#[command(about = "Render Prototype assets to PNG for inspection")]
struct Cli {
    /// Read inputs from a CD image (cue path) instead of the filesystem;
    /// positional inputs are then canonical asset names (e.g. FLI/INTRO.FLI).
    #[arg(long, global = true)]
    cue: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render a .PAL as a 16x16 swatch grid.
    Palette {
        input: PathBuf,
        output: PathBuf,
        /// Side length of each colour cell, in pixels.
        #[arg(long, default_value_t = 16)]
        cell: u32,
    },
    /// Render a .RAW. Without --palette, indices map to a grayscale ramp.
    Raw {
        input: PathBuf,
        output: PathBuf,
        #[arg(long)]
        width: u32,
        #[arg(long)]
        height: u32,
        #[arg(long)]
        palette: Option<PathBuf>,
    },
    /// Render a .BDY (ByteRun1-compressed). Without --palette, grayscale.
    Bdy {
        input: PathBuf,
        output: PathBuf,
        #[arg(long)]
        width: u32,
        #[arg(long)]
        height: u32,
        #[arg(long)]
        palette: Option<PathBuf>,
    },
    /// Combine a level background from its four planes. Pass any .SP1 path;
    /// the .SP2..SP4 siblings are read automatically.
    Background {
        input: PathBuf,
        output: PathBuf,
        #[arg(long)]
        palette: Option<PathBuf>,
    },
    /// Preview the menu: blit back3.raw and draw the menu items with font.raw,
    /// using the palette read from START.EXE.
    Menu {
        /// START.EXE (source of the menu palette).
        start_exe: PathBuf,
        /// 320x200 background (BACK3.RAW).
        background: PathBuf,
        /// Glyph sheet (FONT.RAW).
        font: PathBuf,
        output: PathBuf,
    },
    /// Decode a .FLI animation. Emit any combination of an animated GIF,
    /// per-frame PNGs, and a contact sheet tiling every frame.
    Fli {
        input: PathBuf,
        /// Animated GIF of all frames.
        #[arg(long)]
        gif: Option<PathBuf>,
        /// Directory to dump frame_0000.png .. one per frame.
        #[arg(long)]
        frames_dir: Option<PathBuf>,
        /// Single PNG tiling every frame (downscaled) into a grid.
        #[arg(long)]
        contact_sheet: Option<PathBuf>,
    },
    /// Decode a .BIN / .BN1 compiled-sprite file against its level .WAD
    /// catalog. Emit any combination of a contact sheet, per-sprite PNGs, and
    /// an animation GIF. Without --palette, indices map to a grayscale ramp.
    Bin {
        /// The sprite BIN (e.g. OUT.BIN) or BN1 (PTURN1.BN1).
        input: PathBuf,
        /// The level .WAD holding the sprite catalog (e.g. LEVEL_1.WAD).
        wad: PathBuf,
        /// Decode as the PTURN1 ship frame catalog instead of a scenery BIN.
        #[arg(long)]
        ship: bool,
        #[arg(long)]
        palette: Option<PathBuf>,
        /// Single PNG laying out every sprite over a checkerboard grid.
        #[arg(long)]
        sheet: Option<PathBuf>,
        /// Directory to dump sprite_0000.png .. one per non-empty sprite.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Animated GIF of all sprites in catalog order (best for the ship).
        #[arg(long)]
        gif: Option<PathBuf>,
    },
    /// Decode a .SMP sound sample to a WAV for listening.
    Smp {
        input: PathBuf,
        output: PathBuf,
        /// Playback sample rate in Hz. Default 22222 = the engine's DSP time
        /// constant 0xD3 (`256 - 1000000/rate`).
        #[arg(long, default_value_t = 22222)]
        rate: u32,
        /// Treat the source as unsigned 8-bit (default: signed, the on-disk format).
        #[arg(long)]
        unsigned: bool,
    },
    /// Render the in-game HUD (panel plus a sample game state) to a PNG.
    /// Requires `--cue`; the HUD assets and palette come from the disc.
    Hud { output: PathBuf },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let source = openprototype_tools::open_source(cli.cue.as_deref())?;
    let source = source.as_ref();

    match cli.command {
        Command::Palette {
            input,
            output,
            cell,
        } => render_palette(source, &input, &output, cell),
        Command::Raw {
            input,
            output,
            width,
            height,
            palette,
        } => {
            let pixels = read(source, &input)?;
            let image =
                raw::decode(&pixels, Dimensions::new(width, height)).context("decoding raw")?;
            render_indexed(source, &image, &output, palette.as_deref())
        }
        Command::Bdy {
            input,
            output,
            width,
            height,
            palette,
        } => {
            let pixels = read(source, &input)?;
            let image =
                bdy::decode(&pixels, Dimensions::new(width, height)).context("decoding bdy")?;
            render_indexed(source, &image, &output, palette.as_deref())
        }
        Command::Background {
            input,
            output,
            palette,
        } => {
            let planes = [
                read(source, &input.with_extension("SP1"))?,
                read(source, &input.with_extension("SP2"))?,
                read(source, &input.with_extension("SP3"))?,
                read(source, &input.with_extension("SP4"))?,
            ];
            let image = background::decode([&planes[0], &planes[1], &planes[2], &planes[3]])
                .context("combining background planes")?;
            render_indexed(source, &image, &output, palette.as_deref())
        }
        Command::Menu {
            start_exe,
            background,
            font,
            output,
        } => {
            let exe = read(source, &start_exe)?;
            let palette = StartExe::new(&exe)
                .context("reading START.EXE")?
                .menu_palette()
                .context("decoding menu palette")?;

            let mut canvas = raw::decode(&read(source, &background)?, Dimensions::new(320, 200))
                .context("decoding background")?;
            let font = Font::decode(&read(source, &font)?).context("decoding font")?;

            // From entry0's menu setup (0x4abb..0x4ae5): labels at x=90, cursor
            // at x=70, rows at y=60..124 step 16. '>' (glyph 0x3e) is the
            // filled triangle cursor. Labels are port-owned, not read from the EXE.
            let items = ["NEW GAME", "LOAD GAME", "HIGHSCORES", "MUSIC MENU", "QUIT"];
            for (row, label) in items.iter().enumerate() {
                let y = 60 + row as i32 * 16;
                font.draw_into(&mut canvas, 90, y, label);
            }
            font.draw_into(&mut canvas, 70, 60, ">");

            save(&to_png(&canvas, &palette), &output)
        }
        Command::Fli {
            input,
            gif,
            frames_dir,
            contact_sheet,
        } => {
            if gif.is_none() && frames_dir.is_none() && contact_sheet.is_none() {
                anyhow::bail!("pass at least one of --gif, --frames-dir, --contact-sheet");
            }

            let bytes = read(source, &input)?;
            let frames = decode_fli_frames(&bytes)?;

            if let Some(dir) = frames_dir.as_deref() {
                dump_frames(&frames, dir)?;
            }

            if let Some(path) = gif.as_deref() {
                write_gif(&frames, path)?;
            }

            if let Some(path) = contact_sheet.as_deref() {
                save(&contact_sheet_of(&frames), path)?;
            }

            Ok(())
        }
        Command::Bin {
            input,
            wad,
            ship,
            palette,
            sheet,
            out_dir,
            gif,
        } => {
            if sheet.is_none() && out_dir.is_none() && gif.is_none() {
                anyhow::bail!("pass at least one of --sheet, --out-dir, --gif");
            }

            let bin_bytes = read(source, &input)?;
            let wad_bytes = read(source, &wad)?;
            let sprites = if ship {
                bin::decode_ship(&bin_bytes, &wad_bytes, bin::PTURN1_CATALOG)
            } else {
                bin::decode_banked(&bin_bytes, &wad_bytes, bin::OUT_BIN_CATALOG)
            }
            .context("decoding sprites")?
            .sprites;

            let palette = match palette.as_deref() {
                Some(path) => pal::decode(&read(source, path)?).context("decoding palette")?,
                None => grayscale_ramp(),
            };

            if let Some(path) = sheet.as_deref() {
                save(&sprites_contact_sheet(&sprites, &palette), path)?;
            }

            if let Some(dir) = out_dir.as_deref() {
                dump_sprites(&sprites, &palette, dir)?;
            }

            if let Some(path) = gif.as_deref() {
                write_sprite_gif(&sprites, &palette, path)?;
            }

            Ok(())
        }
        Command::Smp {
            input,
            output,
            rate,
            unsigned,
        } => {
            let encoding = if unsigned {
                smp::Encoding::Unsigned
            } else {
                smp::Encoding::Signed
            };
            let samples = smp::decode(&read(source, &input)?, encoding);

            write_wav(&samples, rate, &output)
        }
        Command::Hud { output } => render_hud(source, &output),
    }
}

/// Compose the HUD over a sample game state and write it to a PNG.
fn render_hud(source: Option<&DiscImage>, output: &std::path::Path) -> Result<()> {
    let disc = source.context("the hud command needs --cue to read its assets")?;
    let assets = openprototype::assets::load_hud_assets(disc).context("loading HUD assets")?;

    let state = GameState {
        score: 13477,
        lives: 3,
        smart_bombs: 2,
        weapons: [
            WeaponLevel::new(3),
            WeaponLevel::new(1),
            WeaponLevel::new(0),
            WeaponLevel::new(2),
        ],
        current: Weapon::Minigun,
        selected: Secondary::One,
    };

    let mut frame = Framebuffer::new(Dimensions::new(320, 200), assets.palette.clone());
    openprototype::hud::draw_hud(&state, &assets, &mut frame);

    // The level runs a double-scanned 320x200 mode shown with square pixels, so
    // present it stretched to 4:3 (here 640x480) rather than the squished native.
    let native = to_png(&frame.image, &frame.palette);
    let square = image::imageops::resize(&native, 640, 480, image::imageops::FilterType::Nearest);
    save(&square, output)
}

/// Write mono 8-bit unsigned PCM as a WAV file (44-byte header + samples).
fn write_wav(samples: &[u8], rate: u32, output: &std::path::Path) -> Result<()> {
    let data_len = samples.len() as u32;
    let byte_rate = rate; // channels(1) * bits/8(1) * rate

    let mut wav = Vec::with_capacity(44 + samples.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // block align
    wav.extend_from_slice(&8u16.to_le_bytes()); // bits per sample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(samples);

    fs::write(output, &wav).with_context(|| format!("writing {}", output.display()))
}

/// One decoded FLI frame: a ready-to-write RGB image and its delay in jiffies.
struct FliFrame {
    image: RgbImage,
    delay_jiffies: u32,
}

fn decode_fli_frames(bytes: &[u8]) -> Result<Vec<FliFrame>> {
    let mut flic = Flic::new(bytes).context("reading FLI header")?;
    let mut frames = Vec::with_capacity(usize::from(flic.header().frame_count));

    while let Some(frame) = flic.next_frame() {
        let frame = frame.context("decoding FLI frame")?;
        frames.push(FliFrame {
            image: to_png(frame.image, frame.palette),
            delay_jiffies: frame.delay_jiffies,
        });
    }

    Ok(frames)
}

fn dump_frames(frames: &[FliFrame], dir: &std::path::Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    for (index, frame) in frames.iter().enumerate() {
        let path = dir.join(format!("frame_{index:04}.png"));
        save(&frame.image, &path)?;
    }

    Ok(())
}

fn write_gif(frames: &[FliFrame], output: &std::path::Path) -> Result<()> {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame as GifFrame};

    let file = fs::File::create(output).with_context(|| format!("writing {}", output.display()))?;
    let mut encoder = GifEncoder::new(std::io::BufWriter::new(file));
    encoder
        .set_repeat(Repeat::Infinite)
        .context("setting GIF loop")?;

    for frame in frames {
        let millis = frame.delay_jiffies.max(1) * 1000 / 70;
        let delay = Delay::from_numer_denom_ms(millis, 1);
        let rgba = image::DynamicImage::ImageRgb8(frame.image.clone()).into_rgba8();
        encoder
            .encode_frame(GifFrame::from_parts(rgba, 0, 0, delay))
            .context("encoding GIF frame")?;
    }

    Ok(())
}

/// Tile every frame (downscaled) into one grid, so periodic corruption is
/// visible at a glance.
fn contact_sheet_of(frames: &[FliFrame]) -> RgbImage {
    use image::imageops::{FilterType, resize};

    const THUMB_WIDTH: u32 = 80;
    const THUMB_HEIGHT: u32 = 50;
    const PAD: u32 = 2;

    let columns = (frames.len() as f64).sqrt().ceil() as u32;
    let columns = columns.max(1);
    let rows = frames.len().div_ceil(columns as usize) as u32;

    let cell_w = THUMB_WIDTH + PAD;
    let cell_h = THUMB_HEIGHT + PAD;
    let mut sheet: RgbImage = ImageBuffer::new(columns * cell_w + PAD, rows * cell_h + PAD);

    for (index, frame) in frames.iter().enumerate() {
        let thumb = resize(&frame.image, THUMB_WIDTH, THUMB_HEIGHT, FilterType::Nearest);
        let column = index as u32 % columns;
        let row = index as u32 / columns;
        let origin_x = PAD + column * cell_w;
        let origin_y = PAD + row * cell_h;

        for (x, y, pixel) in thumb.enumerate_pixels() {
            sheet.put_pixel(origin_x + x, origin_y + y, *pixel);
        }
    }

    sheet
}

fn render_palette(
    source: Option<&DiscImage>,
    input: &std::path::Path,
    output: &std::path::Path,
    cell: u32,
) -> Result<()> {
    let palette = pal::decode(&read(source, input)?).context("decoding palette")?;
    let mut canvas: RgbImage = ImageBuffer::new(16 * cell, 16 * cell);

    for (index, color) in palette.colors.iter().enumerate() {
        let column = (index % 16) as u32;
        let row = (index / 16) as u32;
        let pixel = ImageRgb([color.r, color.g, color.b]);

        for y in 0..cell {
            for x in 0..cell {
                canvas.put_pixel(column * cell + x, row * cell + y, pixel);
            }
        }
    }

    save(&canvas, output)
}

fn render_indexed(
    source: Option<&DiscImage>,
    image: &IndexedImage,
    output: &std::path::Path,
    palette: Option<&std::path::Path>,
) -> Result<()> {
    let palette = match palette {
        Some(path) => pal::decode(&read(source, path)?).context("decoding palette")?,
        None => grayscale_ramp(),
    };

    save(&to_png(image, &palette), output)
}

/// A palette whose index maps straight to a shade of gray.
fn grayscale_ramp() -> Palette {
    let mut palette = Palette {
        colors: [prototype_formats::Rgb::default(); 256],
    };

    for (index, color) in palette.colors.iter_mut().enumerate() {
        let shade = index as u8;
        *color = prototype_formats::Rgb {
            r: shade,
            g: shade,
            b: shade,
        };
    }

    palette
}

fn to_png(image: &IndexedImage, palette: &Palette) -> RgbImage {
    let rgb = image.to_rgb8(palette);
    ImageBuffer::from_raw(image.size.width, image.size.height, rgb)
        .expect("to_rgb8 returns width * height * 3 bytes")
}

/// One sprite as an RGBA image, transparent where it has no pixel. Returns
/// `None` for a blank (`0x0`) catalog slot.
fn sprite_to_rgba(sprite: &Sprite, palette: &Palette) -> Option<RgbaImage> {
    if sprite.pixels.is_empty() {
        return None;
    }

    let mut image = RgbaImage::new(sprite.size.width, sprite.size.height);

    for (offset, pixel) in sprite.pixels.iter().enumerate() {
        if let Some(index) = pixel {
            let color = palette.colors[*index as usize];
            let x = offset as u32 % sprite.size.width;
            let y = offset as u32 / sprite.size.width;
            image.put_pixel(x, y, ImageRgba([color.r, color.g, color.b, 255]));
        }
    }

    Some(image)
}

/// Lay every sprite over a checkerboard grid, so extent and transparency show.
fn sprites_contact_sheet(sprites: &[Sprite], palette: &Palette) -> RgbImage {
    const PAD: u32 = 2;

    let cell_w = sprites.iter().map(|s| s.size.width).max().unwrap_or(1) + PAD;
    let cell_h = sprites.iter().map(|s| s.size.height).max().unwrap_or(1) + PAD;
    let columns = (sprites.len() as f64).sqrt().ceil() as u32;
    let columns = columns.max(1);
    let rows = sprites.len().div_ceil(columns as usize) as u32;

    let mut sheet =
        ImageBuffer::from_pixel(columns * cell_w, rows * cell_h, ImageRgb([24, 24, 32]));

    for (index, sprite) in sprites.iter().enumerate() {
        let origin_x = (index as u32 % columns) * cell_w + 1;
        let origin_y = (index as u32 / columns) * cell_h + 1;

        for y in 0..sprite.size.height {
            for x in 0..sprite.size.width {
                let shade = if (x + y) % 2 == 0 { 40 } else { 56 };
                sheet.put_pixel(origin_x + x, origin_y + y, ImageRgb([shade, shade, shade]));
            }
        }

        for (offset, pixel) in sprite.pixels.iter().enumerate() {
            if let Some(index) = pixel {
                let color = palette.colors[*index as usize];
                let x = offset as u32 % sprite.size.width;
                let y = offset as u32 / sprite.size.width;
                sheet.put_pixel(
                    origin_x + x,
                    origin_y + y,
                    ImageRgb([color.r, color.g, color.b]),
                );
            }
        }
    }

    sheet
}

fn dump_sprites(sprites: &[Sprite], palette: &Palette, dir: &std::path::Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    for (index, sprite) in sprites.iter().enumerate() {
        if let Some(image) = sprite_to_rgba(sprite, palette) {
            let path = dir.join(format!("sprite_{index:04}.png"));
            image
                .save(&path)
                .with_context(|| format!("writing {}", path.display()))?;
        }
    }

    Ok(())
}

/// Animate the sprites in catalog order, each centered on a transparent canvas
/// sized to the largest sprite so the animation does not jitter.
fn write_sprite_gif(sprites: &[Sprite], palette: &Palette, output: &std::path::Path) -> Result<()> {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame as GifFrame};

    let canvas_w = sprites.iter().map(|s| s.size.width).max().unwrap_or(1);
    let canvas_h = sprites.iter().map(|s| s.size.height).max().unwrap_or(1);

    let file = fs::File::create(output).with_context(|| format!("writing {}", output.display()))?;
    let mut encoder = GifEncoder::new(std::io::BufWriter::new(file));
    encoder
        .set_repeat(Repeat::Infinite)
        .context("setting GIF loop")?;

    for sprite in sprites {
        let Some(image) = sprite_to_rgba(sprite, palette) else {
            continue;
        };

        let mut canvas = RgbaImage::new(canvas_w, canvas_h);
        let offset_x = (canvas_w - sprite.size.width) / 2;
        let offset_y = (canvas_h - sprite.size.height) / 2;

        for (x, y, pixel) in image.enumerate_pixels() {
            canvas.put_pixel(offset_x + x, offset_y + y, *pixel);
        }

        let delay = Delay::from_numer_denom_ms(100, 1);
        encoder
            .encode_frame(GifFrame::from_parts(canvas, 0, 0, delay))
            .context("encoding GIF frame")?;
    }

    Ok(())
}

fn read(source: Option<&DiscImage>, path: &std::path::Path) -> Result<Vec<u8>> {
    read_asset(source, path)
}

fn save(canvas: &RgbImage, output: &std::path::Path) -> Result<()> {
    canvas
        .save(output)
        .with_context(|| format!("writing {}", output.display()))
}
