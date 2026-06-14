//! Linux launcher integration.
//!
//! The binary goes to `~/.local/bin`, the icon into the hicolor theme, and a
//! `.desktop` entry whose `StartupWMClass` matches the window's WM_CLASS so
//! GNOME's app switcher uses the icon.

use std::path::Path;

use anyhow::{Context, Result};
use directories::BaseDirs;
use image::imageops::FilterType;

use crate::{APP_ID, BINARY_NAME, DISPLAY_NAME, IconImage, Report, remove_path};

/// The icon size written into the hicolor theme.
const ICON_SIZE: u32 = 256;

pub fn integrate(exe: &Path, icon: &IconImage, disc: &Path) -> Result<Report> {
    let dirs = BaseDirs::new().context("no home directory")?;
    let data = dirs.data_dir();

    let bin_dir = dirs
        .executable_dir()
        .context("no XDG executable directory")?
        .to_path_buf();
    std::fs::create_dir_all(&bin_dir).with_context(|| format!("creating {}", bin_dir.display()))?;
    let binary = bin_dir.join(BINARY_NAME);
    copy_binary(exe, &binary)?;

    let icon_dir = data.join(format!("icons/hicolor/{ICON_SIZE}x{ICON_SIZE}/apps"));
    std::fs::create_dir_all(&icon_dir)
        .with_context(|| format!("creating {}", icon_dir.display()))?;
    let icon_path = icon_dir.join(format!("{BINARY_NAME}.png"));
    write_icon_png(icon, &icon_path)?;

    let apps_dir = data.join("applications");
    std::fs::create_dir_all(&apps_dir)
        .with_context(|| format!("creating {}", apps_dir.display()))?;
    let launcher = apps_dir.join(format!("{BINARY_NAME}.desktop"));
    std::fs::write(&launcher, desktop_entry(&binary))
        .with_context(|| format!("writing {}", launcher.display()))?;

    // No icon-cache rebuild: GNOME finds a per-user icon by scanning the
    // hicolor dir, and `gtk-update-icon-cache` on a dir without an index.theme
    // just warns. GTK ignores a stale cache and scans anyway.

    Ok(Report {
        binary,
        disc: disc.to_path_buf(),
        launcher,
        icon: icon_path,
    })
}

/// Removes the binary, hicolor icon, and `.desktop` entry.
pub fn remove(removed: &mut Vec<std::path::PathBuf>) -> Result<()> {
    let dirs = BaseDirs::new().context("no home directory")?;
    let data = dirs.data_dir();

    if let Some(bin_dir) = dirs.executable_dir() {
        remove_path(&bin_dir.join(BINARY_NAME), removed);
    }

    remove_path(
        &data.join(format!(
            "icons/hicolor/{ICON_SIZE}x{ICON_SIZE}/apps/{BINARY_NAME}.png"
        )),
        removed,
    );
    remove_path(
        &data.join(format!("applications/{BINARY_NAME}.desktop")),
        removed,
    );

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

fn write_icon_png(icon: &IconImage, dest: &Path) -> Result<()> {
    let image = image::RgbaImage::from_raw(icon.side, icon.side, icon.rgba.clone())
        .context("the icon's pixel buffer does not match its size")?;
    let resized = image::imageops::resize(&image, ICON_SIZE, ICON_SIZE, FilterType::Triangle);
    resized
        .save(dest)
        .with_context(|| format!("writing {}", dest.display()))?;

    Ok(())
}

fn desktop_entry(binary: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={DISPLAY_NAME}\n\
         Comment=A faithful port of Prototype (1995)\n\
         Exec={exec}\n\
         Icon={BINARY_NAME}\n\
         Terminal=false\n\
         Categories=Game;\n\
         StartupWMClass={APP_ID}\n",
        exec = binary.display(),
    )
}
