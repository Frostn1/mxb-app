//! View-only paints, the receiving side: see them while you ride, don't keep them.
//!
//! An owner can mark a paint view-only (`control-plane/src/paintpolicy.ts`). This app tells the
//! control plane it honours that (`caps: ["viewOnly"]` on the join), and then:
//!
//! - **A separate store.** The bytes go to `paintstore-session/` in the app data folder, never
//!   to the week-long `paintstore/` other synced paints are kept in.
//! - **A journal.** The game only lists a paint that sits in its own paint folder under the
//!   owner's file name, so that is where the session copy has to go. Every one written is
//!   recorded in `mxbapp_viewonly.json` beside the sync's own manifest, so it can always be told
//!   apart from the player's own files.
//! - **Gone at exit.** When the game exits, and again at app start in case the app was killed
//!   or the machine crashed, [`sweep`] deletes every journaled file still holding the bytes it
//!   was written with, and the whole session store.
//! - **Never offered.** No synced paint, view-only or not, is listed in the library or put in
//!   an export, a share or a published look ([`SyncedSet`]).
//!
//! What this is honestly worth: it stops casual keeping (a synced paint does not accumulate in
//! your folders, and the app never hands it on). It does not stop a determined rider who copies
//! the file while the game has it open, or who captures the textures off the GPU. The game must
//! hold a paint to draw it, and nothing short of DRM changes that.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::paintsync::{self, safe_dest, Manifest};

/// Sits beside `mxbapp_synced.json`, in the mods folder.
pub const JOURNAL_NAME: &str = "mxbapp_viewonly.json";

/// The session store's folder, a sibling of the week-long `paintstore`.
pub const SESSION_STORE: &str = "paintstore-session";

/// The view-only files on disk: `rel_dest` (lowercased) → the digest written there.
#[derive(Debug, Default)]
pub struct Journal {
    pub entries: HashMap<String, String>,
    dirty: bool,
}

impl Journal {
    pub fn read(mods_dir: &Path) -> Self {
        let entries = std::fs::read_to_string(mods_dir.join(JOURNAL_NAME))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Journal { entries, dirty: false }
    }

    pub fn claim(&mut self, rel_dest: &str, sha: &str) {
        let key = rel_dest.to_ascii_lowercase();
        if self.entries.get(&key).map(String::as_str) != Some(sha) {
            self.entries.insert(key, sha.to_string());
            self.dirty = true;
        }
    }

    pub fn forget(&mut self, rel_dest: &str) {
        if self.entries.remove(&rel_dest.to_ascii_lowercase()).is_some() {
            self.dirty = true;
        }
    }

    pub fn holds(&self, rel_dest: &str, sha: &str) -> bool {
        self.entries.get(&rel_dest.to_ascii_lowercase()).map(String::as_str) == Some(sha)
    }

    pub fn write(&mut self, mods_dir: &Path) {
        if !self.dirty {
            return;
        }
        let path = mods_dir.join(JOURNAL_NAME);
        if self.entries.is_empty() {
            let _ = std::fs::remove_file(&path);
            self.dirty = false;
            return;
        }
        match serde_json::to_string_pretty(&self.entries) {
            Ok(text) => match std::fs::write(&path, text) {
                Ok(()) => self.dirty = false,
                // The bytes are still in the manifest, so a sweep that misses them now still
                // finds them through `remove_installed`; but say so.
                Err(e) => log::warn!("[viewonly] couldn't write {}: {e}", path.display()),
            },
            Err(e) => log::warn!("[viewonly] couldn't serialize the journal: {e}"),
        }
    }
}

/// The session store's folder for an app data folder.
pub fn session_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(SESSION_STORE)
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepOutcome {
    /// View-only paints deleted from the mods folder.
    pub removed: usize,
    /// Journaled files left alone because their bytes changed: the player wrote over them.
    pub kept_yours: usize,
}

/// Delete every view-only paint this app wrote, and the session store.
///
/// Only a file still holding the exact bytes written is removed; one the player has written
/// over since is theirs and stays (and is forgotten). Safe to run any time the game is not
/// running, as often as anyone likes: with nothing journaled it is two stat calls.
pub fn sweep(mods_dir: &Path, session_store: Option<&Path>) -> SweepOutcome {
    let mut out = SweepOutcome::default();
    let mut journal = Journal::read(mods_dir);
    if !journal.entries.is_empty() {
        let mut manifest = Manifest::read(mods_dir);
        for (rel_dest, sha) in std::mem::take(&mut journal.entries) {
            journal.dirty = true;
            let Some(dest) = safe_dest(mods_dir, &rel_dest) else { continue };
            let Some(dest) = dest
                .is_file()
                .then_some(dest)
                .or_else(|| paintsync::resolve_ignoring_case(mods_dir, &rel_dest))
            else {
                manifest.forget(&rel_dest);
                continue;
            };
            if paintsync::sha256_file(&dest).ok().as_deref() != Some(sha.as_str()) {
                out.kept_yours += 1;
                continue;
            }
            paintsync::note_sync_write(&dest);
            match std::fs::remove_file(&dest) {
                Ok(()) => {
                    out.removed += 1;
                    manifest.forget(&rel_dest);
                    paintsync::prune_empty(mods_dir, dest.parent());
                }
                Err(e) => {
                    // Locked by something: try again next time rather than lose track of it.
                    log::warn!("[viewonly] couldn't remove {}: {e}", dest.display());
                    journal.entries.insert(rel_dest, sha);
                }
            }
        }
        manifest.write(mods_dir);
        journal.write(mods_dir);
    }
    if let Some(dir) = session_store {
        if dir.is_dir() {
            if let Err(e) = std::fs::remove_dir_all(dir) {
                log::warn!("[viewonly] couldn't clear {}: {e}", dir.display());
            }
        }
    }
    out
}

/// [`sweep`] for this app's mods folder and session store, unless the game is running (it may
/// have the files open, and they are what it is drawing).
pub fn sweep_if_idle(app: &tauri::AppHandle, why: &str) -> Option<SweepOutcome> {
    if crate::gameproc::is_game_running() {
        return None;
    }
    let cfg = crate::config::load_or_detect(app).unwrap_or_default();
    if cfg.mods_path.trim().is_empty() {
        return None;
    }
    let mods_dir = crate::library::mods_root(&cfg.mods_path);
    let session = crate::config::data_dir(app).map(|d| session_dir(&d));
    let out = sweep(&mods_dir, session.as_deref());
    if out.removed > 0 || out.kept_yours > 0 {
        log::info!(
            "[viewonly] {why}: removed {} view-only paints, left {} the player wrote over",
            out.removed,
            out.kept_yours
        );
    }
    Some(out)
}

// ── Never offered ─────────────────────────────────────────────────────────────

/// Every file paint sync put in the mods folder, view-only or not, by lowercased `rel_dest`.
///
/// What the library, exports, shares and the published look leave out: another rider's
/// paint is theirs, and this app is not the way it gets passed on.
#[derive(Debug, Default, Clone)]
pub struct SyncedSet {
    mods_dir: String,
    rels: HashSet<String>,
}

fn fold(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

impl SyncedSet {
    pub fn read(mods_dir: &Path) -> Self {
        let mut rels: HashSet<String> = Manifest::read(mods_dir).installed.into_keys().collect();
        rels.extend(Journal::read(mods_dir).entries.into_keys());
        SyncedSet { mods_dir: fold(&mods_dir.to_string_lossy()), rels }
    }

    pub fn is_empty(&self) -> bool {
        self.rels.is_empty()
    }

    /// Whether `rel_dest` (relative to the mods folder) is a synced file.
    pub fn has_rel(&self, rel_dest: &str) -> bool {
        self.rels.contains(&fold(rel_dest))
    }

    /// Whether the absolute `path` is a synced file.
    pub fn has_path(&self, path: &str) -> bool {
        if self.rels.is_empty() {
            return false;
        }
        let p = fold(path);
        match p.strip_prefix(&self.mods_dir).and_then(|r| r.strip_prefix('/')) {
            Some(rel) => self.rels.contains(rel),
            None => false,
        }
    }
}

// ── The owner's side ─────────────────────────────────────────────────────────

/// One of the rider's own paints, with what they have said about it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnPaint {
    pub sha256: String,
    pub file_name: String,
    pub rel_dest: String,
    #[serde(default)]
    pub bike_id: String,
    #[serde(default)]
    pub slot: Option<String>,
    /// In the look the rider last published (otherwise a policy kept on a paint they took off).
    #[serde(default)]
    pub worn: bool,
    #[serde(default)]
    pub view_only: bool,
    #[serde(default)]
    pub locked: bool,
    /// A bike paint: the only kind a server can refuse, so the only kind a lock means anything on.
    #[serde(default)]
    pub lockable: bool,
    #[serde(default)]
    pub team: Vec<TeamEntry>,
}

/// Someone else allowed to wear a locked paint: an MXB App rider by name, or a GUID.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TeamEntry {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnPaints {
    /// Steam sign-in proved this rider's GUID, which a lock needs.
    pub can_lock: bool,
    /// The control plane has view-only/locked paints switched on. Absent (an older control plane)
    /// is read as off: the app shows no controls.
    #[serde(default)]
    pub view_only_available: bool,
    pub paints: Vec<OwnPaint>,
}

/// The rider's own paints and their settings, from the control plane.
pub async fn own_paints(token: &str) -> anyhow::Result<OwnPaints> {
    let resp = paintsync::client()?
        .get(format!("{}/v1/paints/policies", paintsync::control_plane()))
        .bearer_auth(token)
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        anyhow::bail!("this control plane doesn't support view-only paints yet");
    }
    let resp = resp.error_for_status()?;
    Ok(resp.json().await?)
}

/// Save View-only, Locked and the team list for one of the rider's paints.
pub async fn set_policy(token: &str, sha256: &str, view_only: bool, locked: bool, team: &[TeamEntry]) -> anyhow::Result<()> {
    if !crate::paintroom::valid_sha(sha256) {
        anyhow::bail!("that isn't a paint hash");
    }
    let resp = paintsync::client()?
        .put(format!("{}/v1/paints/{sha256}/policy", paintsync::control_plane()))
        .bearer_auth(token)
        .json(&serde_json::json!({ "viewOnly": view_only, "locked": locked, "team": team }))
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or_else(|| status.to_string());
        anyhow::bail!(detail);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mxb-viewonly-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(mods: &Path, rel: &str, bytes: &[u8]) -> String {
        let path = mods.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        paintsync::sha256_bytes(bytes)
    }

    /// The game has exited (or the app is starting after a crash): every view-only paint this
    /// app wrote goes, the session store goes, and the player's own files stay.
    #[test]
    fn sweep_removes_view_only_paints_and_the_session_store() {
        let root = scratch("sweep");
        let mods = root.join("mods");
        let session = root.join(SESSION_STORE);
        std::fs::create_dir_all(&session).unwrap();
        std::fs::write(session.join(format!("{}.pnt", "a".repeat(64))), b"x").unwrap();

        let theirs = put(&mods, "bikes/KTM450/paints/Theirs.pnt", b"view only bytes");
        let shared = put(&mods, "bikes/KTM450/paints/Shared.pnt", b"ordinary synced");
        let mine = put(&mods, "bikes/KTM450/paints/Mine.pnt", b"my own livery");
        let mut manifest = Manifest::default();
        manifest.claim("bikes/KTM450/paints/Theirs.pnt", &theirs);
        manifest.claim("bikes/KTM450/paints/Shared.pnt", &shared);
        manifest.write(&mods);
        let mut journal = Journal::default();
        journal.claim("bikes/KTM450/paints/Theirs.pnt", &theirs);
        journal.write(&mods);
        let _ = mine;

        let out = sweep(&mods, Some(&session));
        assert_eq!(out, SweepOutcome { removed: 1, kept_yours: 0 });
        assert!(!mods.join("bikes/KTM450/paints/Theirs.pnt").exists(), "view-only paint deleted");
        assert!(mods.join("bikes/KTM450/paints/Shared.pnt").exists(), "an ordinary synced paint stays");
        assert!(mods.join("bikes/KTM450/paints/Mine.pnt").exists(), "the player's own paint stays");
        assert!(!session.exists(), "the session store is gone");
        assert!(!mods.join(JOURNAL_NAME).exists(), "nothing left to journal");
        let manifest = Manifest::read(&mods);
        assert!(!manifest.installed.contains_key("bikes/ktm450/paints/theirs.pnt"));
        assert!(manifest.installed.contains_key("bikes/ktm450/paints/shared.pnt"));

        // Again, as at the next app start: nothing to do, nothing breaks.
        assert_eq!(sweep(&mods, Some(&session)), SweepOutcome::default());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A journaled file the player has since written over is theirs: it stays.
    #[test]
    fn sweep_leaves_a_file_the_player_wrote_over() {
        let root = scratch("edited");
        let mods = root.join("mods");
        let sha = put(&mods, "bikes/KTM450/paints/Theirs.pnt", b"view only bytes");
        let mut journal = Journal::default();
        journal.claim("bikes/KTM450/paints/Theirs.pnt", &sha);
        journal.write(&mods);
        put(&mods, "bikes/KTM450/paints/Theirs.pnt", b"repainted by the player");

        let out = sweep(&mods, None);
        assert_eq!(out, SweepOutcome { removed: 0, kept_yours: 1 });
        assert!(mods.join("bikes/KTM450/paints/Theirs.pnt").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_synced_set_matches_by_rel_and_by_absolute_path() {
        let root = scratch("set");
        let mods = root.join("mods");
        let sha = put(&mods, "bikes/KTM450/paints/Theirs.pnt", b"x");
        let mut journal = Journal::default();
        journal.claim("bikes/KTM450/paints/Theirs.pnt", &sha);
        journal.write(&mods);
        let set = SyncedSet::read(&mods);
        assert!(set.has_rel("bikes/ktm450/paints/THEIRS.pnt"));
        let abs = mods.join("bikes").join("KTM450").join("paints").join("Theirs.pnt");
        assert!(set.has_path(&abs.to_string_lossy()));
        assert!(!set.has_path(&mods.join("bikes/KTM450/paints/Mine.pnt").to_string_lossy()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
