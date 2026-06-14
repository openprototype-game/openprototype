//! macOS app-bundle integration.
//!
//! Builds `~/Applications/OpenPrototype.app` around a copy of the binary, writes
//! `Info.plist` and an `AppIcon.icns` decoded from the disc, and registers the
//! bundle with LaunchServices. winit's window icon is ignored on macOS, so the
//! bundle's `.icns` is the only icon the dock and Cmd-Tab see.

use std::io::BufWriter;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::BaseDirs;
use image::imageops::FilterType;

use crate::{BINARY_NAME, DISPLAY_NAME, IconImage, Report, macos_info_plist, remove_path};

/// The icns sizes written, all RGBA icon types (ic07/ic08/ic09). The OS
/// downscales these for the smaller slots.
const ICNS_SIZES: [u32; 3] = [128, 256, 512];

fn app_bundle(dirs: &BaseDirs) -> PathBuf {
    dirs.home_dir()
        .join("Applications")
        .join(format!("{DISPLAY_NAME}.app"))
}

pub fn integrate(exe: &Path, icon: &IconImage, disc: &Path) -> Result<Report> {
    let dirs = BaseDirs::new().context("no home directory")?;
    let app = app_bundle(&dirs);
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");

    std::fs::create_dir_all(&macos).with_context(|| format!("creating {}", macos.display()))?;
    std::fs::create_dir_all(&resources)
        .with_context(|| format!("creating {}", resources.display()))?;

    let binary = macos.join(BINARY_NAME);
    copy_binary(exe, &binary)?;

    std::fs::write(contents.join("Info.plist"), macos_info_plist())
        .context("writing Info.plist")?;

    let icon_path = resources.join("AppIcon.icns");
    write_icns(icon, &icon_path)?;

    register(&app);

    Ok(Report {
        binary,
        disc: disc.to_path_buf(),
        launcher: app,
        icon: icon_path,
    })
}

pub fn remove(removed: &mut Vec<PathBuf>) -> Result<()> {
    let dirs = BaseDirs::new().context("no home directory")?;
    remove_path(&app_bundle(&dirs), removed);

    Ok(())
}

fn copy_binary(exe: &Path, dest: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::copy(exe, dest)
        .with_context(|| format!("copying the binary to {}", dest.display()))?;
    std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))
        .with_context(|| format!("making {} executable", dest.display()))?;

    Ok(())
}

fn write_icns(icon: &IconImage, dest: &Path) -> Result<()> {
    let source = image::RgbaImage::from_raw(icon.side, icon.side, icon.rgba.clone())
        .context("the icon's pixel buffer does not match its size")?;
    let mut family = icns::IconFamily::new();

    for size in ICNS_SIZES {
        let resized = image::imageops::resize(&source, size, size, FilterType::Triangle);
        let image = icns::Image::from_data(icns::PixelFormat::RGBA, size, size, resized.into_raw())
            .context("building an icns image")?;
        family.add_icon(&image).context("adding an icns size")?;
    }

    let file =
        std::fs::File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
    family
        .write(BufWriter::new(file))
        .with_context(|| format!("writing {}", dest.display()))?;

    Ok(())
}

/// Registers the bundle with LaunchServices so its icon shows at once.
fn register(app: &Path) {
    const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Versions/A/\
        Frameworks/LaunchServices.framework/Versions/A/Support/lsregister";

    let _ = std::process::Command::new(LSREGISTER)
        .arg("-f")
        .arg(app)
        .status();
}
