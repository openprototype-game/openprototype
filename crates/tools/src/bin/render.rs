//! Decode an asset and write it to a PNG for visual inspection.
//!
//! Commands: `palette` (a .PAL as a swatch grid), `raw`, and `bdy`. For `raw`
//! and `bdy`, pass `--palette` to colour the image, or omit it to fall back to
//! a grayscale ramp that shows geometry without a known palette.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use image::{ImageBuffer, Rgb as ImageRgb, RgbImage};
use prototype_formats::{Dimensions, IndexedImage, Palette, bdy, pal, raw};

#[derive(Parser)]
#[command(about = "Render Prototype assets to PNG for inspection")]
struct Cli {
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
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Palette {
            input,
            output,
            cell,
        } => render_palette(&input, &output, cell),
        Command::Raw {
            input,
            output,
            width,
            height,
            palette,
        } => {
            let pixels = read(&input)?;
            let image =
                raw::decode(&pixels, Dimensions::new(width, height)).context("decoding raw")?;
            render_indexed(&image, &output, palette.as_deref())
        }
        Command::Bdy {
            input,
            output,
            width,
            height,
            palette,
        } => {
            let pixels = read(&input)?;
            let image =
                bdy::decode(&pixels, Dimensions::new(width, height)).context("decoding bdy")?;
            render_indexed(&image, &output, palette.as_deref())
        }
    }
}

fn render_palette(input: &std::path::Path, output: &std::path::Path, cell: u32) -> Result<()> {
    let palette = pal::decode(&read(input)?).context("decoding palette")?;
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
    image: &IndexedImage,
    output: &std::path::Path,
    palette: Option<&std::path::Path>,
) -> Result<()> {
    let palette = match palette {
        Some(path) => pal::decode(&read(path)?).context("decoding palette")?,
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

fn read(path: &std::path::Path) -> Result<Vec<u8>> {
    fs::read(path).with_context(|| format!("reading {}", path.display()))
}

fn save(canvas: &RgbImage, output: &std::path::Path) -> Result<()> {
    canvas
        .save(output)
        .with_context(|| format!("writing {}", output.display()))
}
