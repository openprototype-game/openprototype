//! Inspects and extracts from the original *Prototype* CD image.
//!
//! `ls` lists the data-track files and audio tracks; `cat` extracts a file's
//! raw bytes (handy for files the decoders don't cover, e.g. the EXEs); `rip`
//! exports a CD-DA track as a WAV. To decode an image file (PAL/RAW/FLI/...),
//! prefer `render --cue <image> <command> <NAME>`, which reads it directly.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use prototype_disc::{AssetSource, DiscImage};

#[derive(Parser)]
#[command(about = "Inspect and extract from the Prototype CD image")]
struct Cli {
    /// Cue path of the image. Defaults to $PROTOTYPE_DISC, else ./PROTOTYPE.cue.
    #[arg(long, global = true)]
    cue: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the data-track files and the audio tracks.
    Ls,
    /// Extract one file's raw bytes by canonical name (e.g. FLI/INTRO.FLI).
    Cat {
        name: String,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Export a CD-DA audio track as a 44100/16-bit/stereo WAV.
    Rip {
        /// Track number (audio tracks are 2..=8).
        track: u8,
        #[arg(short, long)]
        output: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let image = match cli.cue.as_deref() {
        Some(path) => {
            DiscImage::open(path).with_context(|| format!("opening {}", path.display()))?
        }
        None => DiscImage::open_default().context("opening default disc image")?,
    };

    match cli.command {
        Command::Ls => {
            println!("Files ({}):", image.files().len());
            for entry in image.files() {
                println!(
                    "  {:<14} {:>9} bytes  (lba {})",
                    entry.name, entry.size, entry.lba
                );
            }
            println!("\nAudio tracks ({}):", image.audio_tracks().len());
            for track in image.audio_tracks() {
                let sectors = track.end_lba - track.start_lba;
                println!(
                    "  track {:>2}  lba {:>6}..{:<6} ({} sectors)",
                    track.number, track.start_lba, track.end_lba, sectors
                );
            }
            Ok(())
        }
        Command::Cat { name, output } => {
            let bytes = image
                .read(&name)
                .with_context(|| format!("reading {name}"))?;
            std::fs::write(&output, &bytes)
                .with_context(|| format!("writing {}", output.display()))?;
            println!("wrote {} bytes to {}", bytes.len(), output.display());
            Ok(())
        }
        Command::Rip { track, output } => {
            let audio = image
                .audio_tracks()
                .iter()
                .find(|t| t.number == track)
                .with_context(|| format!("no audio track {track}"))?;
            let pcm = image.read_track_pcm(audio).context("reading track PCM")?;
            write_wav_pcm16_stereo(&pcm, &output)?;
            println!("wrote {} bytes of PCM to {}", pcm.len(), output.display());
            Ok(())
        }
    }
}

/// Writes raw red-book PCM (44100 Hz, 16-bit, stereo, LE-interleaved) as a WAV.
///
/// The SMP `write_wav` in `render` is mono/8-bit, so CD-DA needs its own
/// header.
fn write_wav_pcm16_stereo(pcm: &[u8], output: &Path) -> Result<()> {
    const RATE: u32 = 44_100;
    const CHANNELS: u16 = 2;
    const BITS: u16 = 16;
    const BLOCK_ALIGN: u16 = CHANNELS * BITS / 8; // 4
    const BYTE_RATE: u32 = RATE * BLOCK_ALIGN as u32; // 176_400

    let data_len = pcm.len() as u32;
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&CHANNELS.to_le_bytes());
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&BYTE_RATE.to_le_bytes());
    wav.extend_from_slice(&BLOCK_ALIGN.to_le_bytes());
    wav.extend_from_slice(&BITS.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(pcm);

    std::fs::write(output, &wav).with_context(|| format!("writing {}", output.display()))
}
