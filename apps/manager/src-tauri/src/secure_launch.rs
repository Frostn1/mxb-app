//! Arming secure content in the running game — the app half of the injected client, so a
//! player does none of it by hand.
//!
//! When you lock a file and bind it to your account, the app remembers the mapping (below).
//! When the game starts, this writes the manifest next to `mxbsecure.dll` and injects the DLL
//! into the running process. No environment to set, no manifest to write, no `inject.exe`.
//!
//! Injection is into the **running** game rather than at launch, because MX Bikes is usually
//! started by Steam and we don't create that process. The DLL goes in shortly after the game
//! appears — at the menu, before a track is loaded — which is in time for the reads that
//! matter. The DLL reads its manifest from its own directory (no inherited environment), which
//! is why the manifest is written there.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// One secured asset the app knows how to serve: the name the game opens, the blob, and the
/// key sealed to the buyer (`.mxbkey`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureAsset {
    /// The file name the game opens (e.g. `pinehill.pkz`).
    pub game_name: String,
    pub blob_path: String,
    pub mxbkey_path: String,
}

/// Where the registry of secured assets lives.
fn registry_path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_local_data_dir().ok()?.join("secure_assets.json"))
}

/// The assets provisioned on this machine, or an empty list.
pub fn load_assets(app: &AppHandle) -> Vec<SecureAsset> {
    registry_path(app)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// The secured assets actually present on disk: every `*.mxbsecure` under the mods trees that
/// has a sibling `.mxbkey`. Secured content can be any asset type — a track, a bike paint, rider
/// gear, a whole bike — so every tree the game loads from is walked, not just tracks. This is how
/// a **buyer** works with no provisioning: they drop the two files into the right folder and the
/// manifest is built from what's there, the same way MX Bikes discovers content by scanning.
/// Anything provisioned on this machine is folded in.
pub fn scan_secured(app: &AppHandle) -> Vec<SecureAsset> {
    let mut found: Vec<SecureAsset> = Vec::new();
    if let Ok(cfg) = crate::config::load(app) {
        // The whole mods tree: secured content is any asset type and a buyer may drop it in any
        // sub-folder (a `mxbsecure` folder of their own included), so walk all of `mods` rather
        // than only tracks/bikes/rider.
        let root = crate::library::mods_subdir(&cfg.mods_path, "mods");
        collect_mxbsecure(app, &root, &mut found);
    }
    for a in load_assets(app) {
        if !found.iter().any(|f| f.blob_path.eq_ignore_ascii_case(&a.blob_path)) {
            found.push(a);
        }
    }
    found
}

/// Walk `dir` for `*.mxbsecure` blobs that have a key beside them, pushing a [`SecureAsset`] for
/// each. Recursive, because content lives in sub-folders (tracks, bikes and their paints, rider
/// gear). A blob whose key was deleted gets it back from the vault here
/// ([`ensure_key_present`]), so arming a game after a tidy-up is not a re-provision.
fn collect_mxbsecure(app: &AppHandle, dir: &std::path::Path, out: &mut Vec<SecureAsset>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_mxbsecure(app, &path, out);
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !name.ends_with(".mxbsecure") {
            continue; // key siblings end in .mxbsecurekey / .mxbkey, so they're skipped here
        }
        let blob_path = path.to_string_lossy().to_string();
        let Some(mxbkey) = ensure_key_present(app, &blob_path) else {
            continue; // no key here and none vaulted — it can't be opened, so don't list it
        };
        out.push(SecureAsset {
            game_name: game_name_of(&path, name),
            blob_path,
            mxbkey_path: mxbkey,
        });
    }
}

/// The key file the app writes beside a new blob: `X.mxbsecure` → `X.mxbsecurekey`. Older blobs
/// were named `X.pkz.mxbsecure` with a `X.pkz.mxbsecure.mxbkey` sibling; that legacy form is still
/// read (see [`existing_key_path`]), but every fresh provision writes the short name.
pub fn key_path_for(blob_path: &str) -> String {
    match blob_path.strip_suffix(".mxbsecure") {
        Some(stem) => format!("{stem}.mxbsecurekey"),
        None => format!("{blob_path}.mxbkey"), // not a .mxbsecure name; keep the old shape
    }
}

/// The key file that actually exists beside `blob_path`, preferring the new `.mxbsecurekey` name
/// and falling back to the legacy `.mxbkey` sibling, or `None` if neither is present.
pub fn existing_key_path(blob_path: &str) -> Option<String> {
    let new = key_path_for(blob_path);
    if std::path::Path::new(&new).exists() {
        return Some(new);
    }
    let legacy = format!("{blob_path}.mxbkey");
    std::path::Path::new(&legacy).exists().then_some(legacy)
}

// ── the key vault ──────────────────────────────────────────────────────────────────────────
//
// The `.mxbsecurekey` beside a blob lives in the mods tree, which is exactly where a player
// tidies up, unzips over the top, or moves a folder — and a key deleted by accident used to
// mean the asset silently stopped appearing in game. So every key the app provisions is also
// copied into the app's own data directory, keyed by the account and asset it belongs to, and
// the sibling is put back from that copy whenever it goes missing. That restore is a file copy:
// no server, no network, no re-grant. Losing both copies is still free (the entitlement is
// perpetual and `/v1/keys/grant` re-issues the same key and secret), it just needs to be online.

/// `<app-data>/secure/keys/` — the app's own copy of every key it has provisioned. Outside the
/// mods tree on purpose: nothing a player cleans up, unzips into, or syncs touches it.
#[cfg_attr(not(mxbsecure), allow(dead_code))]
pub fn key_vault_dir(app: &AppHandle) -> Option<PathBuf> {
    Some(secure_dir(app)?.join("keys"))
}

/// One path component, made safe to join: an `asset_id` or Steam ID comes from a blob header or
/// a VDF, so it is treated as untrusted. Everything outside `[A-Za-z0-9._-]` becomes `_`, and a
/// name that is empty or only dots (`.`, `..`) is refused rather than escaping the directory.
#[cfg_attr(not(mxbsecure), allow(dead_code))]
fn vault_component(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
        .collect();
    (!cleaned.is_empty() && !cleaned.chars().all(|c| c == '.')).then_some(cleaned)
}

/// Where this account's copy of an asset's key lives: `keys/<steam_id>/<asset_id>.mxbsecurekey`.
/// Per account as well as per asset, because a key is sealed to one Steam ID — two accounts on
/// the same PC each own their own copy and must not overwrite each other's.
#[cfg_attr(not(mxbsecure), allow(dead_code))]
pub fn vault_key_path(app: &AppHandle, steam_id: &str, asset_id: &str) -> Option<PathBuf> {
    let steam = vault_component(steam_id)?;
    let asset = vault_component(asset_id)?;
    Some(key_vault_dir(app)?.join(steam).join(format!("{asset}.mxbsecurekey")))
}

/// Keep a copy of a freshly provisioned key. Best-effort: the sibling beside the blob is what
/// plays, so a vault write that fails is logged and nothing else changes.
#[cfg_attr(not(mxbsecure), allow(dead_code))]
pub fn vault_store(app: &AppHandle, steam_id: &str, asset_id: &str, sealed: &[u8]) {
    let Some(path) = vault_key_path(app, steam_id, asset_id) else {
        log::warn!("[secure] no vault path for asset {asset_id:?} — key not backed up");
        return;
    };
    let wrote = path
        .parent()
        .map(std::fs::create_dir_all)
        .transpose()
        .and_then(|_| std::fs::write(&path, sealed));
    match wrote {
        Ok(()) => log::info!("[secure] key for {asset_id} backed up to the vault"),
        Err(e) => log::warn!("[secure] couldn't back up the key for {asset_id}: {e}"),
    }
}

/// This account's vaulted key for an asset, if there is one.
#[cfg_attr(not(mxbsecure), allow(dead_code))]
pub fn vault_read(app: &AppHandle, steam_id: &str, asset_id: &str) -> Option<Vec<u8>> {
    std::fs::read(vault_key_path(app, steam_id, asset_id)?).ok()
}

/// Write a vaulted key back beside its blob, returning where it landed. Used both to replace a
/// deleted sibling and to overwrite one that no longer opens.
#[cfg_attr(not(mxbsecure), allow(dead_code))]
pub fn write_key_beside(blob_path: &str, sealed: &[u8]) -> Result<String, String> {
    let out = key_path_for(blob_path);
    std::fs::write(&out, sealed).map_err(|e| format!("restoring the key beside the blob: {e}"))?;
    Ok(out)
}

/// The key beside `blob_path`, restoring it from the vault first if it has gone missing.
///
/// This is the accidental-deletion path, and it is deliberately offline: the asset id comes from
/// the blob's own (authenticated) header and the bytes come from `<app-data>`, so a player who
/// deleted the key file gets it back the next time the app looks at that blob, with no server
/// call and nothing to click. `None` means there is no key here and none vaulted — that asset
/// needs a re-provision (see the app's repair pass), which is free but needs to be online.
pub fn ensure_key_present(app: &AppHandle, blob_path: &str) -> Option<String> {
    if let Some(p) = existing_key_path(blob_path) {
        return Some(p);
    }
    #[cfg(mxbsecure)]
    {
        let steam_id = crate::steamid::current_steam_id64()?;
        let asset_id = header_asset_id(std::path::Path::new(blob_path))?;
        let sealed = vault_read(app, &steam_id, &asset_id)?;
        match write_key_beside(blob_path, &sealed) {
            Ok(p) => {
                log::info!("[secure] restored the deleted key for {asset_id} from the vault");
                return Some(p);
            }
            Err(e) => log::warn!("[secure] couldn't restore the key for {asset_id}: {e}"),
        }
    }
    let _ = app;
    None
}

/// The `asset_id` from a blob's authenticated header — what a key is vaulted under. Reads only
/// the header prefix, not the whole (often ~200 MB) file.
#[cfg(mxbsecure)]
pub fn header_asset_id(path: &std::path::Path) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = vec![0u8; 8192];
    let n = f.read(&mut head).ok()?;
    head.truncate(n);
    let (asset_id, _key, _len, _orig) = crate::mxbsecure::header_of(&head).ok()?;
    (!asset_id.is_empty()).then_some(asset_id)
}

/// The filename the game opens for a blob. New `X.mxbsecure` blobs carry the original name
/// (`X.pkz`) in their v2 header; older `X.pkz.mxbsecure` blobs don't, so fall back to stripping the
/// `.mxbsecure` suffix, which recovers `X.pkz` for them.
pub fn game_name_of(path: &std::path::Path, file_name: &str) -> String {
    #[cfg(mxbsecure)]
    if let Some(orig) = header_orig_name(path) {
        if !orig.is_empty() {
            return orig;
        }
    }
    let _ = path;
    file_name.trim_end_matches(".mxbsecure").to_string()
}

/// The original game filename from a blob's v2 header, or `None` for a v1 blob (no name) or an
/// unreadable file. Reads only the header prefix, not the whole blob.
#[cfg(mxbsecure)]
fn header_orig_name(path: &std::path::Path) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = vec![0u8; 8192];
    let n = f.read(&mut head).ok()?;
    head.truncate(n);
    let (_asset, _key, _len, orig) = crate::mxbsecure::header_of(&head).ok()?;
    Some(orig)
}

/// Every `*.mxbsecure` blob under the mods trees, whether or not it has a key beside it — what
/// auto-unlock walks to find files that still need a key. ([`scan_secured`] lists only ones that
/// already have one.)
#[cfg_attr(not(mxbsecure), allow(dead_code))]
pub fn scan_blobs(app: &AppHandle) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(cfg) = crate::config::load(app) {
        let root = crate::library::mods_subdir(&cfg.mods_path, "mods");
        collect_blobs(&root, &mut out);
    }
    out
}

fn collect_blobs(dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_blobs(&path, out);
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.ends_with(".mxbsecure") {
                out.push(path.to_string_lossy().to_string());
            }
        }
    }
}

/// Record a newly provisioned asset, replacing any earlier entry for the same game name so a
/// re-lock doesn't leave two. Only the full (mxbsecure) build provisions, so it is otherwise
/// unused.
#[cfg_attr(not(mxbsecure), allow(dead_code))]
pub fn record_asset(app: &AppHandle, asset: SecureAsset) -> Result<(), String> {
    let path = registry_path(app).ok_or("no app data dir")?;
    let mut assets = load_assets(app);
    assets.retain(|a| !a.game_name.eq_ignore_ascii_case(&asset.game_name));
    assets.push(asset);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let json = serde_json::to_vec_pretty(&assets).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// Where the shipped `mxbsecure.dll` is found, in priority order: an explicit override, the
/// Tauri resource dir (a packaged build bundles it there), then beside the app's own
/// executable (a dev build's `build.rs` copies it there). The build places it, so there is
/// nothing to configure.
fn source_dll(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MXB_SECURE_DLL") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(res) = app.path().resource_dir() {
        // The bundler places `resources/*.dll` under `<resource_dir>/resources/`; also accept
        // it at the root, in case a build stages it there.
        for p in [res.join("resources").join("mxbsecure.dll"), res.join("mxbsecure.dll")] {
            if p.exists() {
                return Some(p);
            }
        }
    }
    let exe = std::env::current_exe().ok()?;
    let p = exe.parent()?.join("mxbsecure.dll");
    p.exists().then_some(p)
}

/// The writable directory the DLL is run from — `<app-data>/secure/`. The shipped DLL may sit
/// in a read-only place (a packaged app's resource dir under Program Files), and the DLL needs
/// to read a `manifest.tsv` written beside it, so it is staged here where both can live.
fn run_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = secure_dir(app)?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// `<app-data>/secure/`, without creating it — where the DLL writes `mxbsecure.log`, so the
/// log bundle can pick it up.
pub fn secure_dir(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_local_data_dir().ok()?.join("secure"))
}

/// Stage the DLL into the run dir, copying it only when it isn't already the same bytes — so a
/// running game holding the previous copy open doesn't block a launch, but an updated DLL does
/// replace it.
fn stage_dll(app: &AppHandle, run_dir: &std::path::Path) -> Result<PathBuf, String> {
    let src = source_dll(app).ok_or("no mxbsecure.dll shipped with the app")?;
    let dst = run_dir.join("mxbsecure.dll");
    let bytes = std::fs::read(&src).map_err(|e| format!("reading {}: {e}", src.display()))?;
    // Bytes, not length: a rebuilt DLL is often the exact same size (PE sections are padded),
    // and comparing lengths kept injecting the old one.
    let same = std::fs::read(&dst).is_ok_and(|old| old == bytes);
    if !same {
        std::fs::write(&dst, &bytes).map_err(|e| format!("staging the DLL: {e}"))?;
    }
    log::info!(
        "[secure] DLL from {} ({} bytes, {})",
        src.display(),
        bytes.len(),
        if same { "already staged" } else { "staged" }
    );
    Ok(dst)
}

/// Write the manifest the DLL reads — `manifest.tsv` in the run dir, one tab-separated line per
/// asset: game name, blob, `.mxbkey`.
fn write_manifest(assets: &[SecureAsset], dir: &std::path::Path) -> Result<(), String> {
    let mut out = String::new();
    for a in assets {
        out.push_str(&format!("{}\t{}\t{}\n", a.game_name, a.blob_path, a.mxbkey_path));
    }
    std::fs::write(dir.join("manifest.tsv"), out).map_err(|e| e.to_string())
}

/// Watch for the game and, if this install has opted in, inject shortly after it appears —
/// early matters, because MX Bikes reads a track's content to list it, so the hook has to be
/// live before the track browser opens or a protected track won't show.
///
/// Exactly one decision per run of the game, latched. That matters more than it sounds: this
/// used to re-test `!scan_secured(..).is_empty()` on every tick, and since an install with
/// nothing protected never made that true, it never latched — so a player with no locked
/// content at all got a **recursive walk of the whole `mods/tracks` tree every two seconds**
/// for as long as they played. On a cloud-synced mods folder (OneDrive et al, where a single
/// directory read can take seconds) that alone is enough to ruin the session. The old comment
/// here claimed the opposite: "a no-op run to run when nothing is protected". It wasn't.
///
/// So: decide once, latch, and only re-decide when the game has gone away.
pub fn watch(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut decided_this_run = false;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if !crate::gameproc::is_game_running() {
                decided_this_run = false; // game gone: decide again for the next run
                continue;
            }
            if decided_this_run {
                continue;
            }
            // Latch first, whatever we decide below: every path here is a once-per-run
            // decision, and none of them should be retried on a two-second timer.
            decided_this_run = true;

            // The game just started: pull keys for anything owned but not yet unlocked first, so
            // content bought while the app stayed open is playable this session without a restart.
            // `arm` then serves whatever now has a key.
            #[cfg(mxbsecure)]
            {
                crate::auto_unlock_now(&app, true).await;
            }

            // Always arm when there is secured content to serve — `arm` is a no-op when the scan
            // finds nothing, so a player with no locked content pays only a config + scan and
            // never sees an injection. However the game was started — Play or Steam — like FrostMod.
            arm(&app);
        }
    });
}

/// Arm secure content for the session that just started: stage the DLL, write the manifest
/// beside it, and inject. Best-effort and quiet on the common "nothing to secure" — a player
/// with no locked content should see no trace of this.
pub fn arm(app: &AppHandle) {
    let assets = scan_secured(app);
    if assets.is_empty() {
        return;
    }
    let Some(dir) = run_dir(app) else {
        log::warn!("[secure] no writable run dir for secured content");
        return;
    };
    let dll = match stage_dll(app, &dir) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("[secure] {e} (have {} secured asset(s))", assets.len());
            return;
        }
    };
    if let Err(e) = write_manifest(&assets, &dir) {
        log::warn!("[secure] couldn't write the manifest: {e}");
        return;
    }
    // Where the DLL's log is now, so we only read what this injection adds.
    let dll_log = dir.join("mxbsecure.log");
    let log_from = std::fs::metadata(&dll_log).map(|m| m.len()).unwrap_or(0);
    match inject(&dll) {
        Ok(()) => {
            log::info!("[secure] injected mxbsecure.dll for {} asset(s)", assets.len());
            // The game lists its tracks at startup, usually before the DLL is in, so a
            // locked track never shows. Once the hooks are live, have FrostMod re-run the
            // content load so the scan sees it. Off-thread: the wait can take seconds.
            std::thread::spawn(move || {
                if !wait_for_hooks(&dll_log, log_from, std::time::Duration::from_secs(15)) {
                    log::warn!("[secure] the DLL never reported its hooks — see {}", dll_log.display());
                    return;
                }
                let outcome = crate::frostmod::signal_reload();
                log::info!("[secure] hooks live; asked FrostMod to rescan: {outcome:?}");
            });
        }
        Err(e) => log::warn!("[secure] injection failed: {e}"),
    }
}

/// Refresh a running game after a mid-session unlock: rewrite the manifest the DLL watches, then
/// ask FrostMod to rescan, so a just-unlocked asset appears without a restart. A no-op unless the
/// game is running with injection on — otherwise `arm` picks the new asset up at the next launch.
#[cfg_attr(not(mxbsecure), allow(dead_code))]
pub fn refresh_running(app: &AppHandle) {
    if !crate::gameproc::is_game_running() {
        return;
    }
    let Some(dir) = run_dir(app) else { return };
    let assets = scan_secured(app);
    if let Err(e) = write_manifest(&assets, &dir) {
        log::warn!("[secure] couldn't rewrite the manifest on unlock: {e}");
        return;
    }
    // The DLL polls the manifest (~2s) and reloads its catalog; give it a moment before the
    // rescan, or the rescan could run before the new asset is in the catalog.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        let outcome = crate::frostmod::signal_reload();
        log::info!("[secure] mid-session unlock: manifest rewritten, asked FrostMod to rescan: {outcome:?}");
    });
}

/// Wait for the DLL to finish installing its hooks. It sets up on its own thread, so
/// `LoadLibraryW` returns first; it says when it's done in its log, past `from`. `false` on
/// a failed install or a timeout.
fn wait_for_hooks(log: &std::path::Path, from: u64, timeout: std::time::Duration) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        // Bytes, not a String: a path in the log needn't be UTF-8, and one bad byte would
        // hide the line we're waiting for.
        let mut bytes = Vec::new();
        if let Ok(mut f) = std::fs::File::open(log) {
            if f.seek(SeekFrom::Start(from)).is_ok() {
                let _ = f.read_to_end(&mut bytes);
            }
        }
        let tail = String::from_utf8_lossy(&bytes);
        if tail.contains("[dll] hooks installed") {
            return true;
        }
        if tail.contains("[dll] install failed") {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

/// Inject `dll` into the running game.
#[cfg(windows)]
fn inject(dll: &std::path::Path) -> Result<(), String> {
    let pid = crate::gameproc::game_pid().ok_or("the game isn't running")?;
    win::inject_into(pid, dll)
}

#[cfg(not(windows))]
fn inject(_dll: &std::path::Path) -> Result<(), String> {
    Err("injection is Windows-only".into())
}

#[cfg(windows)]
mod win {
    use std::ffi::{c_void, OsStr};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;

    const PROCESS_ACCESS: u32 = 0x0002 | 0x0008 | 0x0010 | 0x0020 | 0x0400; // VM ops + create thread + query
    const MEM_COMMIT_RESERVE: u32 = 0x1000 | 0x2000;
    const PAGE_READWRITE: u32 = 0x04;

    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn GetModuleHandleW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const i8) -> *mut c_void;
        fn VirtualAllocEx(p: *mut c_void, addr: *mut c_void, size: usize, typ: u32, prot: u32) -> *mut c_void;
        fn WriteProcessMemory(p: *mut c_void, addr: *mut c_void, buf: *const c_void, size: usize, wrote: *mut usize) -> i32;
        fn CreateRemoteThread(p: *mut c_void, attr: *mut c_void, stack: usize, start: *mut c_void, param: *mut c_void, flags: u32, tid: *mut u32) -> *mut c_void;
        fn WaitForSingleObject(h: *mut c_void, ms: u32) -> u32;
        fn CloseHandle(h: *mut c_void) -> i32;
        fn GetLastError() -> u32;
    }

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Load `dll` into process `pid` via the standard `LoadLibraryW` remote thread.
    pub fn inject_into(pid: u32, dll: &std::path::Path) -> Result<(), String> {
        let dll_w: Vec<u16> = wide(&dll.to_string_lossy());
        // SAFETY: a textbook remote-thread injection into a process we opened for it; every
        // handle is closed, and the one remote allocation holds only the DLL path string.
        unsafe {
            let proc = OpenProcess(PROCESS_ACCESS, 0, pid);
            if proc.is_null() {
                return Err(format!("OpenProcess({pid}) failed: {}", GetLastError()));
            }
            let bytes = dll_w.len() * 2;
            let remote = VirtualAllocEx(proc, null_mut(), bytes, MEM_COMMIT_RESERVE, PAGE_READWRITE);
            if remote.is_null() {
                CloseHandle(proc);
                return Err(format!("VirtualAllocEx failed: {}", GetLastError()));
            }
            if WriteProcessMemory(proc, remote, dll_w.as_ptr() as *const c_void, bytes, null_mut()) == 0 {
                CloseHandle(proc);
                return Err(format!("WriteProcessMemory failed: {}", GetLastError()));
            }
            let k32 = GetModuleHandleW(wide("kernel32.dll").as_ptr());
            let load = GetProcAddress(k32, c"LoadLibraryW".as_ptr());
            if load.is_null() {
                CloseHandle(proc);
                return Err("LoadLibraryW not found".into());
            }
            let thread = CreateRemoteThread(proc, null_mut(), 0, load, remote, 0, null_mut());
            if thread.is_null() {
                CloseHandle(proc);
                return Err(format!("CreateRemoteThread failed: {}", GetLastError()));
            }
            WaitForSingleObject(thread, 10_000);
            CloseHandle(thread);
            CloseHandle(proc);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn waits_for_this_injections_hooks_only() {
        let dir = std::env::temp_dir().join(format!("frost-secure-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("mxbsecure.log");
        // A previous session's success is already in the file; it mustn't count.
        std::fs::write(&log, "[dll] hooks installed — secured reads now served from RAM\n").unwrap();
        let from = std::fs::metadata(&log).unwrap().len();
        assert!(!wait_for_hooks(&log, from, Duration::from_millis(300)));

        let mut body = std::fs::read_to_string(&log).unwrap();
        body.push_str("[dll] attached\n[dll] hooks installed — secured reads now served from RAM\n");
        std::fs::write(&log, &body).unwrap();
        assert!(wait_for_hooks(&log, from, Duration::from_millis(300)));

        // A failed install stops the wait at once rather than running out the clock.
        let from = std::fs::metadata(&log).unwrap().len();
        body.push_str("[dll] install failed: nope\n");
        std::fs::write(&log, &body).unwrap();
        let started = std::time::Instant::now();
        assert!(!wait_for_hooks(&log, from, Duration::from_secs(5)));
        assert!(started.elapsed() < Duration::from_secs(1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_vault_name_cannot_escape_the_vault() {
        // asset_id and Steam ID come from a blob header and a VDF — untrusted input that is
        // joined into a path, so traversal and separators must not survive.
        assert_eq!(vault_component("trk_pinehill").as_deref(), Some("trk_pinehill"));
        assert_eq!(vault_component("  76561198000000001 ").as_deref(), Some("76561198000000001"));
        assert_eq!(vault_component("../../etc/passwd").as_deref(), Some(".._.._etc_passwd"));
        assert_eq!(vault_component(r"a\b").as_deref(), Some("a__b"));
        assert_eq!(vault_component(".."), None, "dots only");
        assert_eq!(vault_component("."), None);
        assert_eq!(vault_component("   "), None, "empty");
    }

    #[test]
    fn a_restored_key_lands_at_the_name_the_scan_looks_for() {
        // The repair path writes the key back beside the blob; discovery has to find exactly
        // that file, or a restore would look like it did nothing.
        let dir = std::env::temp_dir().join(format!("frost-vault-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let blob = dir.join("pinehill.mxbsecure").to_string_lossy().to_string();
        std::fs::write(&blob, b"not a real blob").unwrap();
        assert_eq!(existing_key_path(&blob), None, "no key to start with");

        let wrote = write_key_beside(&blob, b"sealed-key-bytes").unwrap();
        assert_eq!(wrote, key_path_for(&blob));
        assert_eq!(existing_key_path(&blob).as_deref(), Some(wrote.as_str()));
        assert_eq!(std::fs::read(&wrote).unwrap(), b"sealed-key-bytes");

        // Restoring over a key that no longer opens replaces it rather than failing.
        let again = write_key_beside(&blob, b"a-newer-sealed-key").unwrap();
        assert_eq!(std::fs::read(&again).unwrap(), b"a-newer-sealed-key");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
