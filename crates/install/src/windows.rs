//! Windows launcher integration.
//!
//! The binary goes to `%LOCALAPPDATA%\Programs\OpenPrototype` with an `.ico`
//! beside it, and a Start Menu `.lnk` is written through PowerShell's
//! `WScript.Shell` COM object, so no shortcut crate is needed. The taskbar and
//! Alt-Tab icon already come from the window's runtime icon; the shortcut is for
//! launching.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use directories::BaseDirs;
use image::imageops::FilterType;

use crate::{BINARY_NAME, DISPLAY_NAME, IconImage, Report, powershell_shortcut, remove_path};

/// The icon size written into the `.ico` (256 is the format's maximum).
const ICON_SIZE: u32 = 256;

fn program_dir(dirs: &BaseDirs) -> PathBuf {
    dirs.data_local_dir().join("Programs").join(DISPLAY_NAME)
}

fn start_menu_lnk(dirs: &BaseDirs) -> PathBuf {
    dirs.data_dir()
        .join(r"Microsoft\Windows\Start Menu\Programs")
        .join(format!("{DISPLAY_NAME}.lnk"))
}

pub fn integrate(exe: &Path, icon: &IconImage, disc: &Path) -> Result<Report> {
    let dirs = BaseDirs::new().context("no home directory")?;

    let program = program_dir(&dirs);
    std::fs::create_dir_all(&program).with_context(|| format!("creating {}", program.display()))?;

    let binary = program.join(format!("{BINARY_NAME}.exe"));
    // Reading the running exe is allowed; this writes a new file at `binary`.
    std::fs::copy(exe, &binary)
        .with_context(|| format!("copying the binary to {}", binary.display()))?;

    let icon_path = program.join(format!("{BINARY_NAME}.ico"));
    write_icon_ico(icon, &icon_path)?;

    let launcher = start_menu_lnk(&dirs);

    if let Some(parent) = launcher.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    create_shortcut(&launcher, &binary, &icon_path, &program)?;

    Ok(Report {
        binary,
        disc: disc.to_path_buf(),
        launcher,
        icon: icon_path,
    })
}

/// Removes the shortcut and the program directory.
///
/// A running exe cannot be deleted on Windows, so when `uninstall` runs from the
/// installed binary its own file stays until the uninstall script removes it
/// after this process exits.
pub fn remove(removed: &mut Vec<PathBuf>) -> Result<()> {
    let dirs = BaseDirs::new().context("no home directory")?;

    remove_path(&start_menu_lnk(&dirs), removed);
    remove_path(&program_dir(&dirs), removed);

    Ok(())
}

fn write_icon_ico(icon: &IconImage, dest: &Path) -> Result<()> {
    let image = image::RgbaImage::from_raw(icon.side, icon.side, icon.rgba.clone())
        .context("the icon's pixel buffer does not match its size")?;
    let resized = image::imageops::resize(&image, ICON_SIZE, ICON_SIZE, FilterType::Triangle);
    resized
        .save(dest)
        .with_context(|| format!("writing {}", dest.display()))?;

    Ok(())
}

fn create_shortcut(lnk: &Path, target: &Path, icon: &Path, workdir: &Path) -> Result<()> {
    let command = powershell_shortcut(
        &lnk.to_string_lossy(),
        &target.to_string_lossy(),
        &icon.to_string_lossy(),
        &workdir.to_string_lossy(),
    );

    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &command])
        .status()
        .context("running PowerShell to create the Start Menu shortcut")?;

    if !status.success() {
        bail!("PowerShell could not create the Start Menu shortcut");
    }

    Ok(())
}
