//! Local, offline install integration for OpenPrototype.
//!
//! The download lives in the per-OS install scripts, which use the system's own
//! HTTP tools, so the shipped binary never needs network access. This crate
//! does only filesystem work: [`install`] copies the binary into its per-user
//! location, places the disc image in the data directory, writes the platform
//! launcher entry with an icon decoded from the disc, and records the disc path
//! so the bare binary finds it afterwards.
//!
//! The icon is generated from the user's disc rather than bundled, the same
//! policy the rest of the project follows. [`decode_ship_icon`] is shared with
//! the window icon the running game sets.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use prototype_disc::{AssetSource, DiscImage};
use prototype_formats::bin::decode_ship;
use prototype_formats::{Palette, Sprite, wad};

#[cfg(target_os = "linux")]
mod linux;

/// The application identity, shared with the runtime's `directories` lookups
/// and the window's WM_CLASS.
const QUALIFIER: &str = "de";
const ORGANIZATION: &str = "dasprids";
const APPLICATION: &str = "OpenPrototype";

/// The installed executable's filename.
const BINARY_NAME: &str = "openprototype";
/// The launcher's display name and the WM_CLASS the window sets.
const DISPLAY_NAME: &str = "OpenPrototype";
const APP_ID: &str = "OpenPrototype";

/// Where to find the icon source in the disc (LEVEL_1's ship).
///
/// Supplied by the game binary from its level table so this crate stays
/// independent of the game crate.
pub struct IconSource {
    /// The level WAD holding the ship catalog and palette (e.g. `LEVEL_1.WAD`).
    pub wad_name: &'static str,
    /// The ship frame catalog's file offset (`ShipData::catalog`).
    pub ship_catalog: usize,
    /// The level palette's file offset (`LevelData::palette_offset`).
    pub palette_offset: usize,
}

/// A square straight-RGBA icon, `side * side * 4` bytes, row-major.
pub struct IconImage {
    pub rgba: Vec<u8>,
    pub side: u32,
}

/// What an [`install`] run created, for the caller to report.
pub struct Report {
    pub binary: PathBuf,
    pub disc: PathBuf,
    pub launcher: PathBuf,
    pub icon: PathBuf,
}

/// The inputs an install needs.
pub struct InstallSpec {
    /// The disc cue sheet to decode the icon from and place in the data dir.
    pub cue: PathBuf,
    pub icon: IconSource,
}

/// Nearest-upscale factors for the icon: 2:3 is the level's 1.5 pixel aspect.
const SCALE_X: u32 = 8;
const SCALE_Y: u32 = 12;
/// Transparent margin around the ship, in upscaled pixels.
const ICON_MARGIN: u32 = 18;

/// Decodes the player ship into a square transparent icon.
///
/// Frame 0 of `PTURN1.BN1` over the level palette, nearest-upscaled to the
/// level's 1.5 pixel aspect and centered, so a downscaler has a crisp source.
pub fn decode_ship_icon(disc: &DiscImage, source: &IconSource) -> Result<IconImage> {
    let wad = disc
        .read(source.wad_name)
        .with_context(|| format!("reading {}", source.wad_name))?;
    let pturn1 = disc.read("PTURN1.BN1").context("reading PTURN1.BN1")?;
    let frames =
        decode_ship(&pturn1, &wad, source.ship_catalog).context("decoding PTURN1.BN1")?;
    let palette =
        wad::palette_at(&wad, source.palette_offset).context("reading the level palette")?;
    let sprite = frames
        .sprites
        .first()
        .context("PTURN1.BN1 has no ship frames")?;

    Ok(composite_icon(sprite, &palette))
}

fn composite_icon(sprite: &Sprite, palette: &Palette) -> IconImage {
    let (sprite_w, sprite_h) = (sprite.size.width, sprite.size.height);
    let (scaled_w, scaled_h) = (sprite_w * SCALE_X, sprite_h * SCALE_Y);
    let side = scaled_w.max(scaled_h) + ICON_MARGIN * 2;
    let (origin_x, origin_y) = ((side - scaled_w) / 2, (side - scaled_h) / 2);

    let mut rgba = vec![0u8; (side * side * 4) as usize];

    for y in 0..sprite_h {
        for x in 0..sprite_w {
            let Some(index) = sprite.pixels[(y * sprite_w + x) as usize] else {
                continue;
            };

            let color = palette.colors[usize::from(index)];

            for block_y in 0..SCALE_Y {
                for block_x in 0..SCALE_X {
                    let px = origin_x + x * SCALE_X + block_x;
                    let py = origin_y + y * SCALE_Y + block_y;
                    let at = ((py * side + px) * 4) as usize;
                    rgba[at] = color.r;
                    rgba[at + 1] = color.g;
                    rgba[at + 2] = color.b;
                    rgba[at + 3] = 0xff;
                }
            }
        }
    }

    IconImage { rgba, side }
}

/// Installs OpenPrototype: binary, disc, launcher entry, and icon.
///
/// Offline and per-user. The running binary (passed as the temp download by the
/// install script) copies itself to its final location, so nothing here needs
/// elevation or network.
pub fn install(spec: &InstallSpec) -> Result<Report> {
    let dirs = project_dirs()?;

    let disc = DiscImage::open(&spec.cue)
        .with_context(|| format!("opening the disc image {}", spec.cue.display()))?;
    let icon = decode_ship_icon(&disc, &spec.icon)?;
    drop(disc);

    let disc_dest = place_disc(&spec.cue, dirs.data_local_dir())?;
    record_disc_path(&disc_dest)?;

    let exe = std::env::current_exe().context("locating the running binary")?;

    integrate(&exe, &icon, &disc_dest)
}

/// Dispatches to the platform-specific launcher integration.
fn integrate(exe: &Path, icon: &IconImage, disc: &Path) -> Result<Report> {
    #[cfg(target_os = "linux")]
    {
        linux::integrate(exe, icon, disc)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (exe, icon, disc);
        bail!("`install` is not implemented on this platform yet");
    }
}

/// The recorded disc path the bare binary falls back to, if any.
pub fn configured_disc() -> Option<PathBuf> {
    let path = project_dirs().ok()?.config_dir().join("disc.path");
    let recorded = std::fs::read_to_string(path).ok()?;
    let trimmed = recorded.trim();

    if trimmed.is_empty() {
        return None;
    }

    Some(PathBuf::from(trimmed))
}

/// Records the disc cue path so the bare binary finds it after install.
fn record_disc_path(cue: &Path) -> Result<()> {
    let dir = project_dirs()?.config_dir().to_path_buf();
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    std::fs::write(dir.join("disc.path"), cue.to_string_lossy().as_bytes())
        .context("recording the disc path")?;

    Ok(())
}

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
        .context("no home directory for the per-user install")
}

/// Copies the disc (cue plus the BIN it references) into the data directory.
///
/// Copies rather than moves, so pointing `--cue` at a disc you keep elsewhere
/// leaves your original in place; the install script deletes its temp download.
fn place_disc(cue: &Path, dest_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dest_dir)
        .with_context(|| format!("creating {}", dest_dir.display()))?;

    let source_dir = cue
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let bin_source = resolve_referenced_bin(cue, source_dir)?;

    let bin_dest = dest_dir.join(file_name(&bin_source)?);
    std::fs::copy(&bin_source, &bin_dest)
        .with_context(|| format!("copying {}", bin_source.display()))?;

    let cue_dest = dest_dir.join(file_name(cue)?);
    std::fs::copy(cue, &cue_dest).with_context(|| format!("copying {}", cue.display()))?;

    Ok(cue_dest)
}

/// Finds the BIN file a cue references, resolving case-insensitively.
///
/// The cue names it uppercase (`PROTOTYPE.BIN`) while the file on disk is often
/// lowercase, so an exact match can miss.
fn resolve_referenced_bin(cue: &Path, dir: &Path) -> Result<PathBuf> {
    let text = std::fs::read_to_string(cue)
        .with_context(|| format!("reading the cue sheet {}", cue.display()))?;
    let named = text
        .lines()
        .find_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("FILE ")?;
            let start = rest.find('"')? + 1;
            let end = rest[start..].find('"')? + start;
            Some(rest[start..end].to_string())
        })
        .context("the cue sheet has no FILE entry")?;

    let exact = dir.join(&named);

    if exact.exists() {
        return Ok(exact);
    }

    let wanted = named.to_ascii_lowercase();

    for entry in std::fs::read_dir(dir).with_context(|| format!("listing {}", dir.display()))? {
        let path = entry?.path();

        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(&wanted))
        {
            return Ok(path);
        }
    }

    bail!("the cue references {named}, which is not next to it");
}

fn file_name(path: &Path) -> Result<&std::ffi::OsStr> {
    path.file_name()
        .with_context(|| format!("{} has no file name", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_is_a_square_with_the_par_aspect() {
        // A 4x3 sprite: every pixel opaque index 1.
        let sprite = Sprite {
            size: prototype_formats::Dimensions::new(4, 3),
            pixels: vec![Some(1u8); 12],
            origin: (0, 0),
        };
        let mut palette = Palette {
            colors: [prototype_formats::Rgb { r: 0, g: 0, b: 0 }; 256],
        };
        palette.colors[1] = prototype_formats::Rgb {
            r: 0x10,
            g: 0x20,
            b: 0x30,
        };

        let icon = composite_icon(&sprite, &palette);

        // 4*8=32 wide, 3*12=36 tall -> side = 36 + 2*18 = 72.
        assert_eq!(icon.side, 72);
        assert_eq!(icon.rgba.len(), (72 * 72 * 4) as usize);

        // The center pixel is opaque and carries the palette color.
        let center = ((36 * 72 + 36) * 4) as usize;
        assert_eq!(&icon.rgba[center..center + 4], &[0x10, 0x20, 0x30, 0xff]);

        // A corner is transparent (in the margin).
        assert_eq!(&icon.rgba[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn the_bin_resolves_case_insensitively() {
        let dir = std::env::temp_dir().join(format!("opi-cue-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cue = dir.join("PROTOTYPE.cue");
        std::fs::write(&cue, "FILE \"PROTOTYPE.BIN\" BINARY\n  TRACK 01 MODE1/2352\n").unwrap();
        std::fs::write(dir.join("prototype.bin"), b"x").unwrap();

        let resolved = resolve_referenced_bin(&cue, &dir).unwrap();
        assert_eq!(resolved, dir.join("prototype.bin"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
