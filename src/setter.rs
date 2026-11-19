//! Wallpaper setter — writes the processed image to a temp file and invokes
//! `termux-wallpaper` for both home and lock screen.

use image::RgbImage;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const TEMP_FILE: &str = "wall_tmp.png";

pub fn temp_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(TEMP_FILE)
}

fn run_termux_wallpaper(args: &[&OsStr]) -> Result<(), String> {
    // Capture the child's output: termux-wallpaper prints its own
    // "Wallpaper set successfully!" line, which would otherwise appear once per
    // call (home + lock screen). Our single `OK <name>` line is the confirmation.
    let output = Command::new("termux-wallpaper")
        .args(args)
        .output()
        .map_err(|e| format!("could not run termux-wallpaper: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err(format!("termux-wallpaper exited with {}", output.status));
        }
        return Err(format!(
            "termux-wallpaper exited with {}: {stderr}",
            output.status
        ));
    }
    Ok(())
}

pub fn set_wallpaper(image: &RgbImage, cache_dir: &Path) -> Result<(), String> {
    let temp = temp_path(cache_dir);
    if let Some(parent) = temp.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    image.save(&temp).map_err(|e| format!("save: {e}"))?;

    // Both use the same file so home and lock screen match.
    run_termux_wallpaper(&[OsStr::new("-f"), temp.as_os_str()])?;
    run_termux_wallpaper(&[OsStr::new("-f"), temp.as_os_str(), OsStr::new("-l")])?;

    Ok(())
}
