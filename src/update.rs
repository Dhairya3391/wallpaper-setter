//! In-place self-updater via GitHub Releases.

use serde::Deserialize;
use std::fs;
use std::io::Read;
use std::time::Duration;

const REPO: &str = "Dhairya3391/wallpaper-setter";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

pub fn update() -> Result<(), String> {
    eprintln!("Checking for updates (current version: v{CURRENT_VERSION})...");

    let client = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .user_agent(&format!("wall/{CURRENT_VERSION}"))
        .build();

    let api_url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = match client.get(&api_url).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(404, _)) => {
            return Err("No public releases found yet on GitHub repository.".into());
        }
        Err(e) => return Err(format!("Could not reach GitHub Releases API: {e}")),
    };

    let release: GithubRelease = resp
        .into_json()
        .map_err(|e| format!("Failed to parse release information: {e}"))?;

    let latest_ver = release.tag_name.trim_start_matches('v');
    if !is_newer(latest_ver, CURRENT_VERSION) {
        println!("wall is already up to date (current: v{CURRENT_VERSION}, latest: v{latest_ver}).");
        return Ok(());
    }

    eprintln!("Found new release: v{latest_ver} (current: v{CURRENT_VERSION})");

    let download_url = release
        .assets
        .iter()
        .find(|a| a.name == "wall")
        .map(|a| a.browser_download_url.clone())
        .unwrap_or_else(|| {
            format!(
                "https://github.com/{REPO}/releases/download/{}/wall",
                release.tag_name
            )
        });

    eprintln!("Downloading update...");
    let resp = client
        .get(&download_url)
        .call()
        .map_err(|e| format!("Failed downloading binary: {e}"))?;

    let mut new_bytes = Vec::new();
    resp.into_reader()
        .read_to_end(&mut new_bytes)
        .map_err(|e| format!("Failed reading downloaded binary: {e}"))?;

    if new_bytes.is_empty() {
        return Err("Downloaded update was empty.".into());
    }

    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Could not determine location of running binary: {e}"))?;

    let temp_exe = current_exe.with_extension("update_tmp");

    fs::write(&temp_exe, &new_bytes)
        .map_err(|e| format!("Failed to write temporary binary: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&temp_exe)
            .map_err(|e| format!("Metadata error: {e}"))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&temp_exe, perms)
            .map_err(|e| format!("Failed to set executable permissions: {e}"))?;
    }

    fs::rename(&temp_exe, &current_exe).map_err(|e| {
        format!(
            "Failed to replace binary in-place at {}: {e}",
            current_exe.display()
        )
    })?;

    println!("✨ Successfully updated wall to v{latest_ver} in-place!");
    println!("Location: {}", current_exe.display());
    Ok(())
}

fn parse_version(v: &str) -> Vec<u32> {
    v.trim_start_matches('v')
        .split('.')
        .filter_map(|s| s.parse().ok())
        .collect()
}

fn is_newer(remote: &str, current: &str) -> bool {
    let r = parse_version(remote);
    let c = parse_version(current);
    r > c
}
