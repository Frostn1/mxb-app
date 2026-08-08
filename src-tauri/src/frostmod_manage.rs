use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// FrostMod's GitHub repo — releases carry `frostmod.exe` + `frostmod.dll`.
const REPO: &str = "Frostn1/frostmod";
const UA: &str = "mxb-app";

/// The binaries a FrostMod release has to ship. Both land or neither does — a new
/// `frostmod.exe` beside an old `frostmod.dll` is a worse state than not updating.
const BINARIES: [&str; 2] = ["frostmod.exe", "frostmod.dll"];

/// Marks a binary moved aside because something still had it open. Swept on the
/// next install or start, by which point whatever held it has usually exited.
const RETIRED_MARK: &str = ".in-use-";

/// Managed FrostMod child process (only ever `Some` on Windows while running).
#[derive(Default)]
pub struct FrostmodProcess(pub Mutex<Option<std::process::Child>>);

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrostmodStatus {
    /// Whether `frostmod.exe` is present in our managed folder.
    pub installed: bool,
    /// Installed release tag, if known.
    pub version: Option<String>,
    /// Latest release tag on GitHub (None if the check failed / offline).
    pub latest: Option<String>,
    /// The binaries on disk aren't the ones the recorded version ships — an install
    /// that didn't fully apply. Reinstalling is the fix.
    pub needs_repair: bool,
    /// Whether FrostMod is currently running (its reload event exists).
    pub running: bool,
}

fn frostmod_dir(app: &AppHandle) -> PathBuf {
    // Local app-data dir (Windows: `%LOCALAPPDATA%\com.frost.mxbikes\frostmod`).
    app.path()
        .app_local_data_dir()
        .expect("could not resolve app local data dir")
        .join("frostmod")
}

fn exe_path(app: &AppHandle) -> PathBuf {
    frostmod_dir(app).join("frostmod.exe")
}

fn version_path(app: &AppHandle) -> PathBuf {
    frostmod_dir(app).join("version.txt")
}

/// FrostMod's server-browser filter file (its stock default hides Kaizo).
const SERVERFILTER_FILE: &str = "frostmod_serverfilter.yaml";

/// Curated filter: v4 sentinel kept, spam regex kept, Kaizo rules removed.
const CURATED_SERVERFILTER: &str = "# frostmod-filter v4
# FrostMod server filter - hide spam/ad servers from the online browser.
# Hidden if the name contains any 'names' entry or matches any 'regex'.
hideUnjoinable: false   # ping '---' - unreliable at list time, keep off
hideEmpty: false        # hide 0-player servers (many legit ones are just empty)
hideLocked: false       # hide password-locked servers
maxPerIP: 0             # 0 = off; else hide servers past N from one IP per refresh
names:                  # case-insensitive substrings
  - che4ts
regex:                  # ECMAScript regex; single-quote to keep backslashes literal
  - '(che[a4]ts|\\.pr0\\b)'
";

/// FrostMod's stock v4 default (the one that hides Kaizo).
const STOCK_SERVERFILTER: &str = "# frostmod-filter v4
# FrostMod server filter - hide spam/ad servers from the online browser.
# Hidden if the name contains any 'names' entry or matches any 'regex'.
hideUnjoinable: false   # ping '---' - unreliable at list time, keep off
hideEmpty: false        # hide 0-player servers (many legit ones are just empty)
hideLocked: false       # hide password-locked servers
maxPerIP: 0             # 0 = off; else hide servers past N from one IP per refresh
names:                  # case-insensitive substrings
  - che4ts
  - kaizo
  - kalz0
regex:                  # ECMAScript regex; single-quote to keep backslashes literal
  - '(che[a4]ts|k[a4][il1]z[o0]|\\.pr0\\b)'
";

fn serverfilter_path(app: &AppHandle) -> PathBuf {
    frostmod_dir(app).join(SERVERFILTER_FILE)
}

/// Compare filter text ignoring line endings (CRLF) and trailing blank space.
fn filter_eq(a: &str, b: &str) -> bool {
    a.replace('\r', "").trim_end() == b.replace('\r', "").trim_end()
}

/// Write our curated server filter, unless the user has edited it. Best-effort.
pub fn ensure_serverfilter(app: &AppHandle) {
    let path = serverfilter_path(app);
    let should_write = match std::fs::read_to_string(&path) {
        Ok(cur) => filter_eq(&cur, STOCK_SERVERFILTER),
        Err(_) => true, // missing / unreadable -> lay down our copy
    };
    if !should_write {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&path, CURATED_SERVERFILTER) {
        Ok(()) => log::info!("wrote curated FrostMod server filter (Kaizo unhidden): {}", path.display()),
        Err(e) => log::warn!("could not write FrostMod server filter {}: {e}", path.display()),
    }
}

fn installed_version(app: &AppHandle) -> Option<String> {
    std::fs::read_to_string(version_path(app))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn is_installed(app: &AppHandle) -> bool {
    exe_path(app).exists()
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    /// Byte length GitHub reports for the asset.
    size: u64,
    /// `sha256:<hex>`, when the release carries one (older releases may not).
    digest: Option<String>,
}

async fn latest_release() -> anyhow::Result<Release> {
    let client = reqwest::Client::builder().user_agent(UA).build()?;
    let rel = client
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json::<Release>()
        .await?;
    Ok(rel)
}

/// Does the file at `path` hold exactly what the release says the asset is?
///
/// Size first because it settles almost every mismatch without reading anything; the
/// digest then makes it exact. A release that advertises no digest gets the size check
/// alone rather than a free pass on nothing.
fn file_matches_asset(path: &Path, asset: &Asset) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if meta.len() != asset.size {
        return false;
    }
    let Some(want) = asset.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")) else {
        return true;
    };
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    use sha2::{Digest, Sha256};
    let got = Sha256::digest(&bytes);
    format!("{got:x}").eq_ignore_ascii_case(want)
}

/// Are both installed binaries actually the ones `rel` ships?
fn install_matches_release(dir: &Path, rel: &Release) -> bool {
    BINARIES.iter().all(|name| {
        rel.assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
            .is_some_and(|asset| file_matches_asset(&dir.join(name), asset))
    })
}

/// Current install + latest-available snapshot. `latest` is best-effort (network).
pub async fn status(app: &AppHandle) -> FrostmodStatus {
    let rel = latest_release().await.ok();
    let installed = is_installed(app);
    let version = installed_version(app);

    // Only worth checking when we already claim to be on the latest tag: any other
    // state is a plain update, and the button offers that anyway. This is what rescues
    // an install that recorded a version it never finished applying — before the
    // all-or-nothing swap existed, a locked `frostmod.dll` could leave exactly that,
    // and "Up to date" then had no way out.
    let needs_repair = match (&rel, &version) {
        (Some(rel), Some(version)) if installed && *version == rel.tag_name => {
            !install_matches_release(&frostmod_dir(app), rel)
        }
        _ => false,
    };

    FrostmodStatus {
        installed,
        version,
        latest: rel.map(|r| r.tag_name),
        needs_repair,
        running: crate::frostmod::is_running(),
    }
}

/// What an install actually did, beyond succeeding.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallReport {
    /// The release tag now on disk.
    pub version: String,
    /// The previous FrostMod is still mapped into a running MX Bikes, so the new
    /// one only takes over once the game is restarted.
    pub needs_game_restart: bool,
}

/// Where downloads land before anything live is touched.
fn staging_dir(dir: &Path) -> PathBuf {
    dir.join(".staging")
}

/// Delete leftover `*.in-use-*` copies. Best-effort — one that's still mapped into
/// a live process won't go, and gets another chance next time.
fn sweep_retired(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_str()
            .is_some_and(|n| n.contains(RETIRED_MARK))
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// First free `<target>.in-use-<n>` name beside `target`.
fn retired_path(target: &Path) -> PathBuf {
    let base = target.as_os_str().to_string_lossy().into_owned();
    (0..)
        .map(|n| PathBuf::from(format!("{base}{RETIRED_MARK}{n}")))
        .find(|p| !p.exists())
        .expect("the range is unbounded")
}

/// Rename, retrying briefly — a virus scanner that grabbed a just-downloaded binary
/// lets go within a moment, and that shouldn't fail an update.
fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last = None;
    for _ in 0..15 {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
    Err(last.expect("loop runs at least once"))
}

/// Move `staged` onto `target`, leaving the displaced binary aside for the caller
/// to dispose of. Returns where the old one went (`None` if there wasn't one).
///
/// The old file is renamed out of the way rather than overwritten because Windows
/// won't open a mapped image for writing — that's the `os error 32` an in-place
/// overwrite hits while MX Bikes has `frostmod.dll` loaded. Renaming it *is*
/// allowed (the loader opens images with `FILE_SHARE_DELETE`, and the section keeps
/// pointing at the moved file), so the update applies without closing the game.
fn swap_in(target: &Path, staged: &Path) -> std::io::Result<Option<PathBuf>> {
    if !target.exists() {
        std::fs::rename(staged, target)?;
        return Ok(None);
    }
    let retired = retired_path(target);
    rename_with_retry(target, &retired)?;
    if let Err(e) = std::fs::rename(staged, target) {
        // Put the old binary back rather than leave the install without one.
        let _ = std::fs::rename(&retired, target);
        return Err(e);
    }
    Ok(Some(retired))
}

/// Undo a `swap_in`: the new binary goes back to staging, the old one to its name.
fn undo_swap(target: &Path, retired: Option<&Path>, staged: &Path) {
    let _ = std::fs::rename(target, staged);
    if let Some(retired) = retired {
        let _ = std::fs::rename(retired, target);
    }
}

fn locked_file_error(name: &str, e: &std::io::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "Couldn't replace {name} — something still has it open. Close MX Bikes and try again. \
         Nothing was changed. ({e})"
    )
}

/// Move every staged binary into place, rolling back the ones already moved if any
/// of them fails. Returns whether a displaced binary is still in use, which is the
/// signal that the running game is on the old FrostMod until it restarts.
fn apply_staged(dir: &Path, staging: &Path) -> anyhow::Result<bool> {
    let mut done: Vec<(&str, Option<PathBuf>)> = Vec::new();
    for name in BINARIES {
        match swap_in(&dir.join(name), &staging.join(name)) {
            Ok(retired) => done.push((name, retired)),
            Err(e) => {
                for (applied, retired) in &done {
                    undo_swap(
                        &dir.join(applied),
                        retired.as_deref(),
                        &staging.join(applied),
                    );
                }
                return Err(locked_file_error(name, &e));
            }
        }
    }
    // Everything landed, so the displaced copies can go. One that refuses to delete
    // is still backing a loaded image — i.e. the game is running the old FrostMod.
    let mut needs_game_restart = false;
    for (_, retired) in &done {
        if let Some(retired) = retired {
            needs_game_restart |= std::fs::remove_file(retired).is_err();
        }
    }
    Ok(needs_game_restart)
}

/// Download URLs for both binaries, or an error naming what the release is missing.
///
/// A release short one binary used to be installed anyway, which stamped the new tag
/// into `version.txt` over a binary that had never been replaced — the app then
/// reported a version it wasn't running.
fn release_binaries(rel: &Release) -> anyhow::Result<Vec<(&'static str, &Asset)>> {
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for want in BINARIES {
        match rel.assets.iter().find(|a| a.name.eq_ignore_ascii_case(want)) {
            Some(asset) => found.push((want, asset)),
            None => missing.push(want),
        }
    }
    if !missing.is_empty() {
        anyhow::bail!(
            "FrostMod release {} is missing {} — nothing was installed.",
            rel.tag_name,
            missing.join(" and ")
        );
    }
    Ok(found)
}

/// Download `frostmod.exe` + `frostmod.dll` from the latest release and put them in
/// place as one unit.
///
/// Refused off Windows: FrostMod is a Win32 DLL injected into the game, so downloading it
/// elsewhere just parks two unusable binaries in the app's data dir before `start()` bails.
pub async fn install(app: &AppHandle) -> anyhow::Result<InstallReport> {
    if cfg!(not(windows)) {
        anyhow::bail!("FrostMod runs on Windows only");
    }
    let rel = latest_release().await?;
    let assets = release_binaries(&rel)?;

    let dir = frostmod_dir(app);
    std::fs::create_dir_all(&dir)?;
    sweep_retired(&dir);

    // Download both before touching either live file: a download that dies halfway
    // then costs nothing instead of leaving a mismatched pair behind.
    let staging = staging_dir(&dir);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let client = reqwest::Client::builder().user_agent(UA).build()?;
    for (name, asset) in &assets {
        let bytes = client
            .get(&asset.browser_download_url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let staged = staging.join(name);
        std::fs::write(&staged, &bytes)?;
        // A truncated download is worth catching here, where the previous install is
        // still untouched, rather than after it's been replaced with a broken binary.
        if !file_matches_asset(&staged, asset) {
            let _ = std::fs::remove_dir_all(&staging);
            anyhow::bail!(
                "The download of {name} didn't match what the release advertises — \
                 nothing was changed. Try again."
            );
        }
    }

    let applied = apply_staged(&dir, &staging);
    let _ = std::fs::remove_dir_all(&staging);
    let needs_game_restart = applied?;

    // Written last, and only once both binaries are actually in place, so the version
    // we report can never describe an install that didn't happen.
    std::fs::write(version_path(app), &rel.tag_name)?;
    // Ship our curated server filter. Best-effort.
    ensure_serverfilter(app);
    Ok(InstallReport {
        version: rel.tag_name,
        needs_game_restart,
    })
}

/// Launch `frostmod.exe` hidden as a managed child. Windows-only.
#[cfg(windows)]
pub fn start(app: &AppHandle, state: &FrostmodProcess) -> anyhow::Result<bool> {
    use std::os::windows::process::CommandExt;
    /// Don't pop a console window for the headless reloader.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    if crate::frostmod::is_running() {
        return Ok(false);
    }
    let exe = exe_path(app);
    if !exe.exists() {
        anyhow::bail!("FrostMod isn't installed yet");
    }
    // Refresh the curated filter before FrostMod loads it.
    ensure_serverfilter(app);
    // Nothing holds the previous binaries once the game that mapped them is gone,
    // so a start is a good moment to clear what the last update had to leave behind.
    sweep_retired(&frostmod_dir(app));
    let child = std::process::Command::new(&exe)
        .current_dir(frostmod_dir(app))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    *state.0.lock().unwrap() = Some(child);
    Ok(true)
}

#[cfg(not(windows))]
pub fn start(_app: &AppHandle, _state: &FrostmodProcess) -> anyhow::Result<bool> {
    anyhow::bail!("FrostMod runs on Windows only")
}

/// Kill the managed FrostMod child, if we started one.
pub fn stop(state: &FrostmodProcess) {
    if let Some(mut child) = state.0.lock().unwrap().take() {
        let _ = child.kill();
    }
}

/// Force-terminate any running `frostmod.exe` (even one we didn't spawn). Best-effort.
#[cfg(windows)]
pub fn force_stop_exe() {
    use std::os::windows::process::CommandExt;
    /// Don't flash a console window for the kill.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/IM", "frostmod.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

#[cfg(not(windows))]
pub fn force_stop_exe() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("frostmod-inst-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A managed folder holding the old pair, plus a staging folder holding the new
    /// one. Returns `(dir, staging)`.
    fn installed_pair(tag: &str) -> (PathBuf, PathBuf) {
        let dir = temp_dir(tag);
        let staging = staging_dir(&dir);
        std::fs::create_dir_all(&staging).unwrap();
        for name in BINARIES {
            std::fs::write(dir.join(name), b"old").unwrap();
            std::fs::write(staging.join(name), b"new").unwrap();
        }
        (dir, staging)
    }

    fn read(path: impl AsRef<Path>) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    fn asset(name: &str) -> Asset {
        asset_for(name, b"")
    }

    /// An asset advertising exactly `body` — size and sha256 as GitHub reports them.
    fn asset_for(name: &str, body: &[u8]) -> Asset {
        use sha2::{Digest, Sha256};
        Asset {
            name: name.to_string(),
            browser_download_url: format!("https://example.invalid/{name}"),
            size: body.len() as u64,
            digest: Some(format!("sha256:{:x}", Sha256::digest(body))),
        }
    }

    #[test]
    fn a_release_missing_a_binary_installs_nothing() {
        // Skipping the missing one and installing the rest is what used to stamp a new
        // tag into version.txt over a binary that had never been replaced.
        let rel = Release {
            tag_name: "v0.9.9".into(),
            assets: vec![asset("frostmod.exe"), asset("Release.zip")],
        };
        let err = format!("{:#}", release_binaries(&rel).expect_err("half a pair is no pair"));
        assert!(err.contains("frostmod.dll"), "names what's missing: {err}");
        assert!(err.contains("v0.9.9"), "names the release: {err}");
    }

    #[test]
    fn a_complete_release_yields_both_download_urls() {
        // Real releases carry more than the two binaries, and have shipped mixed case.
        let rel = Release {
            tag_name: "v0.9.9".into(),
            assets: vec![
                asset("FrostServer.zip"),
                asset("FrostMod.DLL"),
                asset("frostmod.exe"),
            ],
        };
        let found = release_binaries(&rel).expect("both binaries are there");
        let names: Vec<&str> = found.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, vec!["frostmod.exe", "frostmod.dll"]);
        assert!(
            found[1].1.browser_download_url.ends_with("FrostMod.DLL"),
            "keeps the asset's own url"
        );
    }

    #[test]
    fn applying_swaps_both_binaries_and_leaves_nothing_behind() {
        let (dir, staging) = installed_pair("apply");

        let needs_restart = apply_staged(&dir, &staging).expect("nothing holds these files");

        for name in BINARIES {
            assert_eq!(read(dir.join(name)), "new", "{name} was replaced");
        }
        // Nothing had the old pair open, so it's gone rather than parked aside.
        assert!(!needs_restart);
        assert_eq!(std::fs::read_dir(&staging).unwrap().count(), 0);
        assert!(
            !std::fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().contains(RETIRED_MARK)),
            "no retired copies left over"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The bug this whole path exists for: a failure on the second binary used to
    /// leave a new exe beside an old dll.
    #[test]
    fn a_binary_that_cant_land_puts_the_earlier_one_back() {
        let (dir, staging) = installed_pair("rollback");
        // Nothing to move into place for the dll — stands in for the locked target
        // that `swap_in` can't complete.
        std::fs::remove_file(staging.join("frostmod.dll")).unwrap();

        let err = format!(
            "{:#}",
            apply_staged(&dir, &staging).expect_err("a missing staged binary can't land")
        );
        assert!(err.contains("frostmod.dll"), "names the binary: {err}");
        assert!(err.contains("MX Bikes"), "says how to fix it: {err}");

        for name in BINARIES {
            assert_eq!(read(dir.join(name)), "old", "{name} is back as it was");
        }
        assert_eq!(
            read(staging.join("frostmod.exe")),
            "new",
            "the new exe went back to staging"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_displaced_binary_that_wont_delete_asks_for_a_game_restart() {
        let dir = temp_dir("retired");
        let target = dir.join("frostmod.dll");
        std::fs::write(&target, b"old").unwrap();
        std::fs::write(dir.join("staged.dll"), b"new").unwrap();

        let retired = swap_in(&target, &dir.join("staged.dll"))
            .expect("a rename works even on a mapped image")
            .expect("the old binary was moved aside, not overwritten");

        assert_eq!(read(&target), "new");
        // Windows can't delete this while it backs a loaded image; that failure is
        // exactly what tells us the game is still on the old FrostMod.
        assert_eq!(read(&retired), "old");
        assert!(retired.to_string_lossy().contains(RETIRED_MARK));

        // A second swap doesn't fight the first for the name.
        std::fs::write(dir.join("staged.dll"), b"newer").unwrap();
        let second = swap_in(&target, &dir.join("staged.dll")).unwrap().unwrap();
        assert_ne!(second, retired, "each displaced copy gets its own slot");

        sweep_retired(&dir);
        assert!(!retired.exists() && !second.exists(), "the sweep clears them");
        assert_eq!(read(target), "newer", "and leaves the live binary alone");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The state the old installer could strand people in: `version.txt` says v0.9.9
    /// because the exe landed, but the dll is still the v0.9.8 one the running game had
    /// locked. Settings called that "Up to date" and disabled the button, so there was
    /// no way out — spotting the mismatch is what turns it back into a repair.
    #[test]
    fn a_binary_that_isnt_what_the_release_ships_is_a_mismatch() {
        let dir = temp_dir("verify");
        std::fs::write(dir.join("frostmod.exe"), b"the 0.9.9 exe").unwrap();
        std::fs::write(dir.join("frostmod.dll"), b"the 0.9.8 dll").unwrap();

        let rel = Release {
            tag_name: "v0.9.9".into(),
            assets: vec![
                asset_for("frostmod.exe", b"the 0.9.9 exe"),
                asset_for("frostmod.dll", b"the 0.9.9 dll"),
            ],
        };
        assert!(!install_matches_release(&dir, &rel), "the stale dll is caught");

        // Same length, different bytes — size alone would wave this through.
        std::fs::write(dir.join("frostmod.dll"), b"the 0.9.9 dll").unwrap();
        assert!(install_matches_release(&dir, &rel), "a matching pair verifies");

        // A binary that never landed at all is a mismatch, not a crash.
        std::fs::remove_file(dir.join("frostmod.dll")).unwrap();
        assert!(!install_matches_release(&dir, &rel));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_release_without_digests_still_gets_a_size_check() {
        let dir = temp_dir("verify-nodigest");
        std::fs::write(dir.join("frostmod.exe"), b"exe").unwrap();
        std::fs::write(dir.join("frostmod.dll"), b"dll-but-longer").unwrap();

        let mut assets = vec![
            asset_for("frostmod.exe", b"exe"),
            asset_for("frostmod.dll", b"dll"),
        ];
        for a in &mut assets {
            a.digest = None;
        }
        let rel = Release {
            tag_name: "v0.9.9".into(),
            assets,
        };
        assert!(
            !install_matches_release(&dir, &rel),
            "the wrong-length dll is caught without a digest"
        );

        std::fs::write(dir.join("frostmod.dll"), b"dll").unwrap();
        assert!(install_matches_release(&dir, &rel));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_sweep_only_touches_retired_copies() {
        let dir = temp_dir("sweep");
        std::fs::write(dir.join("frostmod.exe"), b"live").unwrap();
        std::fs::write(dir.join("version.txt"), b"v0.9.9").unwrap();
        std::fs::write(dir.join(format!("frostmod.dll{RETIRED_MARK}0")), b"old").unwrap();

        sweep_retired(&dir);

        assert!(dir.join("frostmod.exe").exists());
        assert!(dir.join("version.txt").exists());
        assert!(!dir.join(format!("frostmod.dll{RETIRED_MARK}0")).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn curated_filter_unhides_kaizo_but_keeps_sentinel() {
        // FrostMod only respects a config whose first line is the v4 sentinel.
        assert!(CURATED_SERVERFILTER.starts_with("# frostmod-filter v4"));
        // Kaizo must no longer be matched, by name or the spam regex.
        let lc = CURATED_SERVERFILTER.to_lowercase();
        assert!(!lc.contains("kaizo"));
        assert!(!lc.contains("kalz0"));
        assert!(!CURATED_SERVERFILTER.contains("k[a4][il1]z[o0]"));
        // Spam rules we keep.
        assert!(CURATED_SERVERFILTER.contains("che4ts"));
        assert!(CURATED_SERVERFILTER.contains(r"\.pr0\b"));
    }

    #[test]
    fn stock_default_is_the_kaizo_blocking_one() {
        // Guards our overwrite trigger: the stock text must actually block Kaizo.
        assert!(STOCK_SERVERFILTER.contains("- kaizo"));
        assert!(STOCK_SERVERFILTER.contains("k[a4][il1]z[o0]"));
    }

    #[test]
    fn filter_eq_ignores_line_endings_and_trailing_space() {
        let crlf = STOCK_SERVERFILTER.replace('\n', "\r\n");
        assert!(filter_eq(&crlf, STOCK_SERVERFILTER));
        assert!(filter_eq(&format!("{STOCK_SERVERFILTER}\n\n"), STOCK_SERVERFILTER));
        // A real edit (curated vs stock) must NOT compare equal.
        assert!(!filter_eq(CURATED_SERVERFILTER, STOCK_SERVERFILTER));
    }
}
