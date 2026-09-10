//! Getting a Wine onto this Mac, so a track can be compiled with nothing installed first.
//!
//! The compilers are PiBoSo's Windows binaries and the app already fetches those rather than
//! making anyone find them ([`crate::download_track_tools`]). What it did not do was find
//! anything to *run* them with: on a Mac with no CrossOver, no Whisky and no Wine, the build
//! stopped and told the person to go and set one up. This closes that — the last step of
//! building a track that needed anything outside the app.
//!
//! Gcenx's build is the one used because it is what the track pipeline was proven on: a
//! tarball, no installer, no sudo, no Homebrew. It is x86-64 and runs under Rosetta on Apple
//! silicon, which for a console compiler costs nothing worth measuring.

#[cfg(target_os = "macos")]
use std::path::Path;
use std::path::PathBuf;

use tauri::Manager;

/// The build that gets fetched. Pinned rather than asked of the GitHub API: a build is only
/// interesting here if the compilers run under it, and this one is measured. Moving it up is
/// this line and the folder name below. Unused on Windows, which runs the compilers itself.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const URL: &str =
    "https://github.com/Gcenx/macOS_Wine_builds/releases/download/11.16/wine-devel-11.16-osx64.tar.xz";

/// Where the binary sits inside the tarball once it is unpacked.
const INSIDE: &str = "Wine Devel.app/Contents/Resources/wine/bin/wine";

/// Our copy of Wine, whether or not it has been fetched yet.
pub fn ours(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(dir(app)?.join(INSIDE))
}

/// Where a fetched Wine is kept: beside the app's own data, not in Applications. Nothing is
/// installed on this machine — deleting the folder undoes all of it.
fn dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no data directory: {e}"))?
        .join("wine"))
}

/// A Wine runner for the compilers: one that is installed, else ours, else fetched.
///
/// Returns what to hand [`crate::trackbuild::host`] as its runner: empty when the machine
/// has its own and the resolver should pick, a path when the one to use is ours.
///
/// `configured` is the Settings box, and wins outright — someone who named a runner is not
/// to be second-guessed, or quietly given a second Wine to go with the one they chose.
pub async fn ensure_runner(app: &tauri::AppHandle, configured: String) -> Result<String, String> {
    // Windows runs the compilers itself, and a Wine there would be nonsense.
    if cfg!(target_os = "windows") || !configured.trim().is_empty() {
        return Ok(configured);
    }
    // Anything already on the machine — CrossOver, Whisky, Homebrew, the game-porting
    // toolkit. Nothing to fetch, and a wrapper's own Wine is better than ours for its bottles.
    if mxb_core::winehost::resolve("", None).is_some() {
        return Ok(String::new());
    }
    let ours = ours(app)?;
    if ours.is_file() {
        return Ok(ours.to_string_lossy().into_owned());
    }
    fetch(app).await
}

/// Download and unpack the Wine build. macOS only — see [`fetch`] for other systems.
#[cfg(target_os = "macos")]
async fn fetch(app: &tauri::AppHandle) -> Result<String, String> {
    let into = dir(app)?;
    let bytes = reqwest::Client::new()
        .get(URL)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("couldn't reach {URL}: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("the Wine download stopped early: {e}"))?;

    let ours = tauri::async_runtime::spawn_blocking(move || unpack(&bytes, &into))
        .await
        .map_err(|e| format!("unpacking Wine failed: {e}"))??;
    Ok(ours.to_string_lossy().into_owned())
}

/// Nothing to fetch anywhere else. Linux has Wine in every package manager and installing
/// system software behind someone's back is not this app's business; Windows never asks.
#[cfg(not(target_os = "macos"))]
async fn fetch(_app: &tauri::AppHandle) -> Result<String, String> {
    Err("no Wine here to run the compilers with — install it with your package manager \
         (`wine` in every distribution) and this works with nothing else to set up."
        .into())
}

/// Unpack the tarball into `into` and hand back the binary in it.
///
/// Through `tar` rather than a crate: the archive is `.tar.xz`, macOS's `tar` reads that
/// natively, and it is one process against an xz decoder and a tar reader linked into the
/// app for a file most people will never download.
#[cfg(target_os = "macos")]
fn unpack(bytes: &[u8], into: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(into).map_err(|e| format!("{}: {e}", into.display()))?;
    // Beside the destination rather than in the system temp folder, so an unpack that has to
    // move files never crosses a device, and a half-finished download is cleaned up with the
    // rest of the app's data.
    let tarball = into.join("wine.tar.xz.part");
    std::fs::write(&tarball, bytes).map_err(|e| format!("{}: {e}", tarball.display()))?;

    let out = std::process::Command::new("/usr/bin/tar")
        .arg("-xJf")
        .arg(&tarball)
        .arg("-C")
        .arg(into)
        .output()
        .map_err(|e| format!("couldn't run tar: {e}"))?;
    let _ = std::fs::remove_file(&tarball);
    if !out.status.success() {
        return Err(format!(
            "that Wine download didn't unpack: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    let wine = into.join(INSIDE);
    if !wine.is_file() {
        return Err(format!(
            "Wine unpacked but there's no binary at {}",
            wine.display()
        ));
    }
    Ok(wine)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pinned URL and the path inside the tarball have to agree about the build, or the
    /// unpack succeeds and the binary is looked for in a folder that isn't there.
    #[test]
    fn the_url_and_the_path_inside_it_name_the_same_build() {
        assert!(URL.ends_with(".tar.xz"), "{URL}");
        assert!(URL.contains("wine-devel"), "{URL}");
        assert!(INSIDE.starts_with("Wine Devel.app/"), "{INSIDE}");
    }
}
