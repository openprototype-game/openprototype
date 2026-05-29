//! Decode an asset and write it to a PNG for visual inspection.
//!
//! Run with `--help` for the commands. Image commands take an optional
//! `--palette`; without one they fall back to a grayscale ramp that shows
//! geometry when the real palette is not yet known.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use image::{ImageBuffer, Rgb as ImageRgb, RgbImage};
use prototype_formats::font::Font;
use prototype_formats::{Dimensions, IndexedImage, Palette, StartExe, background, bdy, pal, raw};

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
        Command::Background {
            input,
            output,
            palette,
        } => {
            let planes = [
                read(&input.with_extension("SP1"))?,
                read(&input.with_extension("SP2"))?,
                read(&input.with_extension("SP3"))?,
                read(&input.with_extension("SP4"))?,
            ];
            let image = background::decode([&planes[0], &planes[1], &planes[2], &planes[3]])
                .context("combining background planes")?;
            render_indexed(&image, &output, palette.as_deref())
        }
        Command::Menu {
            start_exe,
            background,
            font,
            output,
        } => {
            let exe = read(&start_exe)?;
            let palette = StartExe::new(&exe)
                .context("reading START.EXE")?
                .menu_palette()
                .context("decoding menu palette")?;

            let mut canvas = raw::decode(&read(&background)?, Dimensions::new(320, 200))
                .context("decoding background")?;
            let font = Font::decode(&read(&font)?).context("decoding font")?;

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
