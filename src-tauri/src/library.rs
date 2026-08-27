use crate::game::GameProfile;
use crate::linkwalk;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMod {
    pub name: String,
    pub path: String,
    pub folder: String,
    pub size: u64,
}

/// The child of `dir` named `seg`, matching case-insensitively when the exact name isn't
/// there.
///
/// Windows and macOS don't care about case, so a hardcoded `"mods"` always resolved. Linux
/// does: MX Bikes runs under Proton on a case-sensitive filesystem, where a folder the game
/// or a mod archive created as `Mods` is a different path from `mods`, and every lookup
/// silently missed. Falls back to the literal name so paths we're about to *create* still
/// come out as written.
pub fn resolve_child(dir: &Path, seg: &str) -> PathBuf {
    let exact = dir.join(seg);
    if exact.exists() {
        return exact;
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().eq_ignore_ascii_case(seg) {
                return e.path();
            }
        }
    }
    exact
}

/// Whether `dir` is itself a mods tree — the folder the game reads `bikes/`, `tracks/`
/// and `rider/` straight out of.
///
/// Judged by its contents first and its name second, in that order of trust. A tree
/// holding type folders is one no matter what it's called (`mxbikes.ini` lets the folder
/// be named and placed freely, and extracted archives arrive under any name); a folder
/// *called* `mods` is one even while it's still empty, which is what makes a fresh
/// relocated folder recognisable before anything has been installed into it.
pub fn is_mods_tree(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    if crate::game::ALL_MODS_DIRS.iter().any(|k| resolve_child(dir, k).is_dir()) {
        return true;
    }
    dir.file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("mods"))
}

/// The folder the game's content actually lives in, given whatever the player pointed the
/// app at.
///
/// `mods_path` has two legal shapes, and this is the one place that tells them apart:
///
/// * the game's **user folder** — the usual `Documents\PiBoSo\MX Bikes`, holding `mods/`
///   and `profiles/` side by side. The content root is its `mods` child.
/// * the **mods tree itself**, e.g. `C:\mods`. `mxbikes.ini`'s `[mods] folder` lets the
///   game read its content from anywhere, and players use it: junctioning one rider paint
///   into six model folders needs short paths off OneDrive, so the tree gets moved to the
///   drive root while `profiles/` stays behind in `Documents`. There is no useful folder
///   above such a tree — the parent is `C:\` — so it has to be usable as the root itself.
///
/// An existing `mods` child wins, so the ordinary layout is decided without touching the
/// second rule, and a user folder that is *itself* named `mods` still resolves to the
/// child rather than to itself. When neither applies the answer is `<mods_path>/mods`:
/// the path to create, which is what a first install needs.
pub fn mods_root(mods_path: &str) -> PathBuf {
    let base = Path::new(mods_path.trim());
    let child = resolve_child(base, "mods");
    if child.is_dir() {
        return child;
    }
    if is_mods_tree(base) {
        return base.to_path_buf();
    }
    child
}

/// Resolve `mods/bikes`-style relative paths under the MX Bikes folder. The single funnel
/// for these lookups, so making it case-tolerant covers the whole app at once.
///
/// A leading `mods` segment is resolved through [`mods_root`] rather than joined blindly,
/// which is what lets every caller keep writing `"mods/bikes"` while the player's
/// `mods_path` may already *be* the mods tree.
pub fn mods_subdir(mods_path: &str, subpath: &str) -> PathBuf {
    let mut segs = subpath.split(['/', '\\']).filter(|s| !s.is_empty()).peekable();
    let mut p = match segs.peek() {
        Some(first) if first.eq_ignore_ascii_case("mods") => {
            segs.next();
            mods_root(mods_path)
        }
        _ => PathBuf::from(mods_path.trim()),
    };
    for seg in segs {
        p = resolve_child(&p, seg);
    }
    p
}

/// True when `s` names one folder or file and nothing else — no separators, no drive letter,
/// no `.`/`..`.
///
/// The guard every "name came from the frontend, now join it onto a path" call site needs, so
/// a crafted name can't walk out of the folder it's meant to stay in.
pub fn is_simple_name(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && !s.contains('/')
        && !s.contains('\\')
        && !s.contains(':')
}

/// True when `rel` is a relative path that cannot leave the folder it is joined onto —
/// no `..`, no absolute or UNC root, no drive letter, no control characters.
///
/// [`is_simple_name`] guards a single segment; this guards a whole `mods/`-relative path,
/// which is the shape a share code carries (`tracks/EU/RedBud.pkz`). A code is written by
/// whoever hands it to you, so every path out of one is checked with this before it reaches
/// anything that joins it onto the mods root.
pub fn is_safe_rel(rel: &str) -> bool {
    // A trailing separator is just how some senders write a folder; a leading one is a path
    // that means to start from the root, which this must never accept.
    let rel = rel.trim().trim_end_matches(['/', '\\']);
    if rel.is_empty() || rel.chars().any(|c| c.is_control()) {
        return false;
    }
    // Split on both separators so a Windows-shaped path is judged the same way a POSIX one
    // is: any empty segment left is a leading separator (absolute, or a UNC root).
    rel.split(['/', '\\']).all(is_simple_name)
}

fn sanitize_seg(seg: &str) -> String {
    seg.chars()
        .map(|c| match c {
            ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect()
}

pub fn move_mod(
    mods_path: &str,
    from_path: &str,
    to_folder: &str,
    subpath: &str,
) -> anyhow::Result<()> {
    let from = PathBuf::from(from_path);
    if !from.is_file() {
        anyhow::bail!("file not found: {from_path}");
    }
    let type_dir = mods_subdir(mods_path, subpath);
    if !from.starts_with(&type_dir) {
        anyhow::bail!("refusing to move a file outside the {subpath} folder");
    }

    let mut dest_dir = type_dir;
    for seg in to_folder.split(['/', '\\']).filter(|s| !s.is_empty()) {
        dest_dir.push(sanitize_seg(seg));
    }
    fs::create_dir_all(&dest_dir)?;

    let name = from
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("bad file name"))?;
    let dest = dest_dir.join(name);
    if dest == from {
        return Ok(());
    }
    if dest.exists() {
        anyhow::bail!("a mod named '{}' is already in that folder", name.to_string_lossy());
    }

    // Rename when possible; fall back to copy+remove across volumes.
    if fs::rename(&from, &dest).is_err() {
        fs::copy(&from, &dest)?;
        fs::remove_file(&from)?;
    }
    Ok(())
}

/// Where the Trash put something, when we can tell — so it can be put back.
///
/// macOS can answer with a path, because the folders are ordinary directories we can look
/// in. Windows renames everything it recycles to `$R…`, so there is no path worth keeping and
/// the answer is `None`; [`restore_from_trash`] asks the OS instead. Either way the ledger
/// only has to record what it was given.
pub type TrashedAt = Option<String>;

/// Move `path` to the Trash, reporting where it went.
///
/// Not plain `trash::delete`: on macOS that drives Finder over AppleScript, and Finder refuses
/// a file iCloud has evicted — *"the item needs to be downloaded"*, error -8013. A mods folder
/// under `Documents` is exactly where eviction happens, and with most of a library dataless
/// this made uninstalling fail on nearly every mod. `NSFileManager` has no such objection, and
/// wants no automation permission either.
///
/// Calling it directly rather than through the crate, which passes `None` for the resulting
/// URL and throws the answer away. That answer is the only reliable way to learn where the
/// file went: the Trash cannot simply be listed afterwards, because macOS refuses `~/.Trash`
/// to an app without Full Disk Access, and a mods folder in iCloud and one outside it land in
/// different Trash folders anyway.
#[cfg(target_os = "macos")]
pub fn move_to_trash(path: &Path) -> anyhow::Result<TrashedAt> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    let path_str = path.to_string_lossy();
    // SAFETY: every argument outlives the call, and `out` is a valid out-pointer for the
    // resulting URL. This is the same call the `trash` crate makes, asking for the one extra
    // value it declines to request.
    unsafe {
        let mgr = NSFileManager::defaultManager();
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path_str));
        let mut out = None;
        mgr.trashItemAtURL_resultingItemURL_error(&url, Some(&mut out))
            .map_err(|e| anyhow::anyhow!("could not move to Trash: {e}"))?;
        Ok(out.and_then(|u| u.path()).map(|p| p.to_string()))
    }
}

/// Windows and Linux have no Finder problem, and their own Trash reports nothing useful
/// about where an item landed — see [`TrashedAt`].
#[cfg(not(target_os = "macos"))]
pub fn move_to_trash(path: &Path) -> anyhow::Result<TrashedAt> {
    trash::delete(path)?;
    Ok(None)
}

/// Put a mod back where it came from.
///
/// `trashed_at` is whatever [`move_to_trash`] reported. When it names a path — macOS — the
/// restore is a plain move. When it doesn't, the OS is asked to undo its own recycle, matching
/// on the path the mod used to occupy.
///
/// Refuses to overwrite: if something is already sitting at `original`, the mod on disk wins
/// and the caller is told, rather than a restore quietly replacing a newer copy.
pub fn restore_from_trash(original: &Path, trashed_at: Option<&str>) -> anyhow::Result<()> {
    if original.exists() {
        anyhow::bail!("something is already installed there");
    }
    if let Some(parent) = original.parent() {
        fs::create_dir_all(parent)?;
    }

    if let Some(from) = trashed_at {
        let from = Path::new(from);
        if !from.exists() {
            anyhow::bail!("it is no longer in the Trash");
        }
        fs::rename(from, original)?;
        return Ok(());
    }

    restore_via_os(original)
}

/// Windows and Linux keep an index of what they recycled and where it came from, so the item
/// can be found by the path it used to have.
#[cfg(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))
))]
fn restore_via_os(original: &Path) -> anyhow::Result<()> {
    use trash::os_limited;

    let name = original
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parent = original.parent().unwrap_or(Path::new(""));

    let item = os_limited::list()?
        .into_iter()
        .filter(|i| i.name.to_string_lossy() == name && Path::new(&i.original_parent) == parent)
        .max_by_key(|i| i.time_deleted)
        .ok_or_else(|| anyhow::anyhow!("it is no longer in the Recycle Bin"))?;

    os_limited::restore_all([item])?;
    Ok(())
}

#[cfg(not(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))
)))]
fn restore_via_os(_original: &Path) -> anyhow::Result<()> {
    anyhow::bail!("this system can't put files back from the Trash")
}

pub fn uninstall_mod(mods_path: &str, from_path: &str, subpath: &str) -> anyhow::Result<TrashedAt> {
    let from = PathBuf::from(from_path);
    if !from.exists() {
        anyhow::bail!("path not found: {from_path}");
    }
    let type_dir = mods_subdir(mods_path, subpath);
    if !from.starts_with(&type_dir) {
        anyhow::bail!("refusing to uninstall a file outside the {subpath} folder");
    }
    move_to_trash(&from)
}

pub fn reveal_in_explorer(path: &str) -> anyhow::Result<()> {
    let p = PathBuf::from(path);
    if !p.exists() {
        anyhow::bail!("path not found: {path}");
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&p)
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg("-R").arg(&p).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // No portable "select the file" on Linux — open its parent folder.
        let target = p.parent().unwrap_or(&p);
        std::process::Command::new("xdg-open").arg(target).spawn()?;
    }
    Ok(())
}

/// Open a folder in the OS file manager.
///
/// The sibling of [`reveal_in_explorer`] for when there is no file to select — an empty
/// log folder, say. Explorer's `/select,` on a folder highlights it in its *parent*, which
/// is not what "open this folder" means, so Windows gets the plain form here.
pub fn open_folder(path: &str) -> anyhow::Result<()> {
    let p = PathBuf::from(path);
    if !p.is_dir() {
        anyhow::bail!("folder not found: {path}");
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(&p).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(&p).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(&p).spawn()?;
    }
    Ok(())
}

pub fn scan_mods(mods_path: &str, subpath: &str) -> anyhow::Result<Vec<InstalledMod>> {
    let dir = mods_subdir(mods_path, subpath);
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut items = Vec::new();
    for entry in linkwalk::walk(&dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let is_pkz = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("pkz"))
            .unwrap_or(false);
        if !is_pkz {
            continue;
        }

        let folder = path
            .parent()
            .and_then(|p| p.strip_prefix(&dir).ok())
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();

        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        items.push(InstalledMod {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: path.to_string_lossy().into_owned(),
            folder,
            size,
        });
    }

    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(items)
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiderTargets {
    pub helmets: Vec<String>,
    pub boots: Vec<String>,
    pub protection: Vec<String>,
    /// Riding-style animations. Both titles read `mods/rider/animations/<name>/` and record
    /// the pick in `profile.ini`'s `[riding_style]`.
    pub animations: Vec<String>,
    pub profiles: Vec<String>,
}

/// The `<Model>.pkz` that sits beside a model's folder, whether or not either exists.
///
/// Not `with_extension("pkz")`: a mod named `Fox Instinct 2.0 by Aeffertz` has
/// `0 by Aeffertz` for an extension, so replacing it asks for `Fox Instinct 2.pkz` — a file
/// nobody has. A model's name is the whole name; the archive appends to it, which is also
/// how `models_in` reads one back.
pub fn sibling_pkz(model_dir: &Path) -> PathBuf {
    let mut name = model_dir.file_name().unwrap_or_default().to_os_string();
    name.push(".pkz");
    model_dir.with_file_name(name)
}

/// Installable content sitting directly in `dir`, by the name you'd address it as: a
/// sub-folder verbatim, a `.pkz` by its stem. Sorted, case-insensitively deduped.
fn models_in(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                if let Some(n) = e.file_name().to_str() {
                    out.push(n.to_string());
                }
            } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("pkz")) {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    out.push(stem.to_string());
                }
            }
        }
    }
    sort_dedup(&mut out);
    out
}

/// Sort case-insensitively and drop repeats that differ only in case — the same bike
/// reached as a folder and as a bike id must not be offered twice.
fn sort_dedup(v: &mut Vec<String>) {
    v.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    v.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
}

/// Everything installed across a slot's folders, as one list.
fn models_in_areas(base: &Path, areas: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = areas.iter().flat_map(|a| models_in(&base.join(a))).collect();
    sort_dedup(&mut out);
    out
}

pub fn scan_rider_targets(mods_path: &str) -> RiderTargets {
    let base = mods_subdir(mods_path, "mods/rider");
    RiderTargets {
        helmets: models_in(&base.join("helmets")),
        // Absent for GP Bikes, which bakes boots and protection into the rider model —
        // the folders simply aren't there, and `models_in` returns empty for those.
        boots: models_in(&base.join("boots")),
        protection: models_in_areas(&base, crate::game::PROTECTION_AREAS),
        // Riding-style animations, which both titles keep here.
        animations: models_in(&base.join("animations")),
        // A rider model can be packed as `riders/<name>.pkz` just as gear can, and a
        // profile the picker never lists is a model nobody can wear.
        profiles: models_in(&base.join("riders")),
    }
}

/// Every bike a paint could be installed for.
///
/// Two sources, because neither is complete on its own. `mods/bikes` has the mod bikes —
/// a `.pkz` package or an unpacked folder. OEM bikes have neither: their files live inside
/// the game's locked archive, so until someone installs a paint for one there is nothing of
/// it on disk at all. The profile is where their ids can still be read, since the game
/// writes a line per bike it knows.
pub fn scan_bike_targets(mods_path: &str, profiles_dir: &Path) -> Vec<String> {
    let mut out = models_in(&mods_subdir(mods_path, "mods/bikes"));
    for profile in crate::presets::list_profiles(profiles_dir) {
        // A profile we can't read is one source short, never an error — the folders above
        // still stand on their own.
        if let Ok(bikes) = crate::presets::list_bikes(profiles_dir, &profile) {
            out.extend(bikes);
        }
    }
    sort_dedup(&mut out);
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntry {
    pub name: String,
    pub path: String,
    pub folder: String,
    pub size: u64,
    /// Unix milliseconds, so the library can be sorted by what arrived most recently — the only
    /// answer available for mods installed before the download history existed.
    pub modified: u64,
    pub kind: String,
    pub category: String,
    pub parent: Option<String>,
}

fn has_ext(p: &Path, ext: &str) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
}

fn strip_ext(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    for ext in [".pkz", ".pnt", ".zip"] {
        if lower.ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    name.to_string()
}

fn rel_folder(base: &Path, path: &Path) -> String {
    path.parent()
        .and_then(|p| p.strip_prefix(base).ok())
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

/// Unix milliseconds, or 0 when the filesystem won't say — a mod with no time sorts last under
/// "recently added" rather than jumping to the top of it.
pub(crate) fn mtime_ms(m: &fs::Metadata) -> u64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Total bytes and the newest mtime among a folder's immediate files, taking the folder's own
/// mtime as the floor. The files matter as well as the folder because a copy can carry the
/// original folder timestamp across while the files it wrote are stamped now.
fn dir_size_and_mtime(dir: &Path) -> (u64, u64) {
    let mut total = 0;
    let mut newest = fs::metadata(dir).map(|m| mtime_ms(&m)).unwrap_or(0);
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            if let Ok(m) = e.metadata() {
                if m.is_file() {
                    total += m.len();
                    newest = newest.max(mtime_ms(&m));
                }
            }
        }
    }
    (total, newest)
}

fn immediate_dirs(base: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(base) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                if let Some(n) = e.file_name().to_str() {
                    out.push(n.to_string());
                }
            }
        }
    }
    out.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    out
}

fn make_entry(base: &Path, p: &Path, category: &str, parent: Option<String>) -> LibraryEntry {
    let is_dir = p.is_dir();
    let kind = if is_dir {
        "folder"
    } else if has_ext(p, "pkz") {
        "pkz"
    } else {
        "loose"
    };
    let (size, modified) = if is_dir {
        dir_size_and_mtime(p)
    } else {
        fs::metadata(p)
            .map(|m| (m.len(), mtime_ms(&m)))
            .unwrap_or((0, 0))
    };
    LibraryEntry {
        name: p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: p.to_string_lossy().into_owned(),
        folder: rel_folder(base, p),
        size,
        modified,
        kind: kind.to_string(),
        category: category.to_string(),
        parent,
    }
}

const TRACK_MARKERS: [&str; 5] = ["map", "trh", "tsc", "rdf", "ssc"];

pub(crate) fn dir_has_track_markers(dir: &Path) -> bool {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() {
                if let Some(ext) = p.extension().and_then(|x| x.to_str()) {
                    if TRACK_MARKERS.contains(&ext.to_ascii_lowercase().as_str()) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn collect_loose(
    base: &Path,
    dir: &Path,
    category: &str,
    parent: Option<&str>,
    out: &mut Vec<LibraryEntry>,
) {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() && (has_ext(&p, "pnt") || has_ext(&p, "pkz")) {
                out.push(make_entry(base, &p, category, parent.map(str::to_string)));
            }
        }
    }
}

fn collect_pkz_shallow(base: &Path, dir: &Path, category: &str, out: &mut Vec<LibraryEntry>) {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() && has_ext(&p, "pkz") {
                out.push(make_entry(base, &p, category, None));
            }
        }
    }
}

fn sort_entries(v: &mut [LibraryEntry]) {
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
}

/// Extracted tracks and packaged `.pkz`, in one pass.
///
/// An extracted track's folder *is* the mod, so once its markers are found we stop
/// descending: the interior can hold thousands of files, and any `.pkz` down there
/// belongs to the track rather than being a mod of its own. Pruning is what keeps a
/// large tracks folder from being walked twice over and every path compared against
/// every track found so far.
fn scan_tracks(dir: &Path) -> Vec<LibraryEntry> {
    let mut out = Vec::new();
    let mut walk = linkwalk::walk(dir).into_iter();

    loop {
        let entry = match walk.next() {
            None => break,
            Some(Err(_)) => continue,
            Some(Ok(e)) => e,
        };
        let p = entry.path();

        if entry.file_type().is_dir() {
            if p != dir && dir_has_track_markers(p) {
                out.push(make_entry(dir, p, "track", None));
                walk.skip_current_dir();
            }
        } else if entry.file_type().is_file() && has_ext(p, "pkz") {
            out.push(make_entry(dir, p, "track", None));
        }
    }

    sort_entries(&mut out);
    out
}

const SOUND_MARKERS: [&str; 2] = ["engine.scl", "sfx.cfg"];

fn dir_has_sound_markers(dir: &Path) -> bool {
    let mut found = [false; SOUND_MARKERS.len()];
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.path().is_file() {
                let name = e.file_name();
                let name = name.to_string_lossy();
                for (i, m) in SOUND_MARKERS.iter().enumerate() {
                    if name.eq_ignore_ascii_case(m) {
                        found[i] = true;
                    }
                }
            }
        }
    }
    found.iter().all(|&f| f)
}

/// Whether these folder segments name a bike livery, and which bike owns it — `Some(None)`
/// for a livery loose at the bikes root, `None` when it isn't a livery at all.
///
/// Liveries normally live in `<Bike>/paints/`, but a model swap can own them, and then they
/// sit under `FrostMod Models/` instead — on the shelf (`_paints/`) while that model is
/// inactive, or in the variant's own `paints/` if a model pack shipped them there and
/// nothing has adopted them yet. Both are still *the bike's* liveries: attributing them to
/// the variant folder is what stopped `bundle`'s `Owner::Require` resolving a livery that
/// came in with a model pack, so a share code shipped without it.
fn paint_owner(segs: &[&str]) -> Option<Option<String>> {
    let owner_at = |i: usize| segs.get(i).map(|s| s.to_string());
    let is_lib =
        |i: usize| segs.get(i).is_some_and(|s| s.eq_ignore_ascii_case(crate::modelswap::LIB_DIR));

    if let Some(pos) = segs.iter().position(|s| s.eq_ignore_ascii_case("paints")) {
        // `<Bike>/FrostMod Models/<Variant>/paints/…` — owner is the bike, three up.
        if pos >= 2 && is_lib(pos - 2) {
            return Some(pos.checked_sub(3).and_then(owner_at));
        }
        // `<Bike>/paints/…` — owner is the segment before `paints`.
        return Some(pos.checked_sub(1).and_then(owner_at));
    }

    // `<Bike>/FrostMod Models/_paints/…` — shelved while its model is off the bike. Still
    // listed, or assigning a livery would look like losing it.
    let pos = segs
        .iter()
        .position(|s| s.eq_ignore_ascii_case(crate::modelswap::PAINT_SHELF))?;
    if !is_lib(pos.checked_sub(1)?) {
        return None;
    }
    Some(pos.checked_sub(2).and_then(owner_at))
}

fn scan_bikes(dir: &Path, sound_bikes: &[String]) -> Vec<LibraryEntry> {
    let mut out = Vec::new();

    for name in sound_bikes {
        let folder = dir.join(name);
        if folder.is_dir() && dir_has_sound_markers(&folder) {
            out.push(make_entry(dir, &folder, "sound", None));
        }
    }

    for entry in linkwalk::walk(dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        let is_pnt = has_ext(p, "pnt");
        let is_pkz = has_ext(p, "pkz");
        if !is_pnt && !is_pkz {
            continue;
        }
        let folder = rel_folder(dir, p);
        let segs: Vec<&str> = folder.split('/').filter(|s| !s.is_empty()).collect();

        if let Some(owner) = paint_owner(&segs) {
            out.push(make_entry(dir, p, "bikePaint", owner));
        } else if is_pkz {
            out.push(make_entry(dir, p, "bike", None));
        }
        // A loose `.pnt` outside any `paints` folder is a stray — ignore it.
    }

    let bike_names: HashSet<String> = out
        .iter()
        .filter(|e| e.category == "bike" && e.folder.is_empty())
        .map(|e| strip_ext(&e.name).to_lowercase())
        .collect();
    for e in out.iter_mut() {
        if e.category != "bike" || e.folder.is_empty() {
            continue;
        }
        if let Some(last) = e.folder.rsplit('/').next() {
            if bike_names.contains(&last.to_lowercase()) {
                e.category = "bikeModelSwap".to_string();
                e.parent = Some(last.to_string());
            }
        }
    }

    sort_entries(&mut out);
    out
}

fn scan_rider(dir: &Path, game: &GameProfile) -> Vec<LibraryEntry> {
    let mut out = Vec::new();

    for area in game.rider.areas {
        let abase = dir.join(area.folder);
        for model in immediate_dirs(&abase) {
            let mpath = abase.join(&model);
            out.push(make_entry(dir, &mpath, area.model_cat, None));
            if let Some(paint_cat) = area.paint_cat {
                collect_loose(dir, &mpath.join("paints"), paint_cat, Some(&model), &mut out);
            }
            if area.goggles {
                collect_loose(dir, &mpath.join("goggles"), "goggles", Some(&model), &mut out);
            }
        }
        // A model packaged as a bare `.pkz` directly under the area folder.
        collect_pkz_shallow(dir, &abase, area.model_cat, &mut out);
        if let Some(paint_cat) = area.paint_cat {
            if let Ok(rd) = fs::read_dir(&abase) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_file() && has_ext(&p, "pnt") {
                        out.push(make_entry(dir, &p, paint_cat, None));
                    }
                }
            }
        }
    }

    // Gloves installed directly under rider/gloves. MX Bikes only — GP Bikes bakes them
    // into the rider model.
    if game.rider.gloves {
        collect_loose(dir, &dir.join("gloves"), "gloves", None, &mut out);
        collect_pkz_shallow(dir, &dir.join("gloves"), "gloves", &mut out);
    }

    // Rider profiles: outfit/kit paints always, plus whatever else the title keeps
    // per profile (MX Bikes: gloves and goggles).
    for profile in immediate_dirs(&dir.join("riders")) {
        let pbase = dir.join("riders").join(&profile);
        collect_loose(dir, &pbase.join("paints"), "outfit", Some(&profile), &mut out);
        for (folder, cat) in game.rider.profile_extras {
            collect_loose(dir, &pbase.join(folder), cat, Some(&profile), &mut out);
        }
    }

    sort_entries(&mut out);
    out
}

fn scan_generic(dir: &Path) -> Vec<LibraryEntry> {
    let mut out = Vec::new();
    for entry in linkwalk::walk(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && has_ext(entry.path(), "pkz") {
            out.push(make_entry(dir, entry.path(), "misc", None));
        }
    }
    sort_entries(&mut out);
    out
}

pub fn scan_library(
    mods_path: &str,
    subpath: &str,
    sound_bikes: &[String],
    game: &GameProfile,
) -> anyhow::Result<Vec<LibraryEntry>> {
    let dir = mods_subdir(mods_path, subpath);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let kind = subpath.rsplit(['/', '\\']).find(|s| !s.is_empty()).unwrap_or("");
    Ok(match kind {
        "tracks" => scan_tracks(&dir),
        "bikes" => scan_bikes(&dir, sound_bikes),
        "rider" => scan_rider(&dir, game),
        // `tyres`, and GP Bikes' `misc/{dashes,stands}` — folders of `.pkz` with no
        // internal structure to read, which is exactly what `scan_generic` handles.
        _ => scan_generic(&dir),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-lib-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    /// A mod with a version in its name — `Fox Instinct 2.0 by Aeffertz` — has an
    /// "extension" as far as the path types are concerned, and replacing it asks for an
    /// archive nobody has. The name is the whole name.
    #[test]
    fn a_packed_model_is_found_beside_a_dotted_name() {
        let boots = Path::new("/mods/rider/boots");
        assert_eq!(
            sibling_pkz(&boots.join("Fox Instinct 2.0 by Aeffertz")),
            boots.join("Fox Instinct 2.0 by Aeffertz.pkz"),
        );
        assert_eq!(
            sibling_pkz(&boots.join("TLD SE4 - Oakley Airbrake")),
            boots.join("TLD SE4 - Oakley Airbrake.pkz"),
            "and an undotted one is unchanged",
        );
        // What `models_in` reads back out of the archive is the name we started from.
        assert_eq!(
            sibling_pkz(&boots.join("Fox Instinct 2.0 by Aeffertz"))
                .file_stem()
                .unwrap(),
            "Fox Instinct 2.0 by Aeffertz",
        );
    }

    /// The guard every share code is checked against. A rel that climbs, starts at a root,
    /// or names a drive can never be joined onto the mods folder.
    #[test]
    fn safe_rels_are_the_ones_that_stay_put() {
        for ok in [
            "tracks/EU/RedBud.pkz",
            "rider/helmets/AGV/paints/Blue.pnt",
            "bikes\\KTM\\paints",
            "tracks/EU/",
            " tracks/RedBud.pkz ",
        ] {
            assert!(is_safe_rel(ok), "should be safe: {ok:?}");
        }
        for bad in [
            "",
            "..",
            "../mxbikes.exe",
            "tracks/../../evil",
            "tracks\\..\\evil",
            "/etc/passwd",
            "\\\\server\\share",
            "C:/Windows/System32",
            "tracks/\nRedBud.pkz",
        ] {
            assert!(!is_safe_rel(bad), "should be refused: {bad:?}");
        }
    }

    #[test]
    fn moves_mod_between_folders() {
        let root = tmp("move");
        let old = root.join("mods").join("tracks").join("Old");
        fs::create_dir_all(&old).unwrap();
        let file = old.join("t.pkz");
        fs::write(&file, b"x").unwrap();

        move_mod(
            root.to_str().unwrap(),
            file.to_str().unwrap(),
            "New Folder",
            "mods/tracks",
        )
        .unwrap();

        assert!(!file.exists());
        assert!(root.join("mods/tracks/New Folder/t.pkz").exists());
        let _ = fs::remove_dir_all(&root);
    }

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }

    fn cat<'a>(v: &'a [LibraryEntry], name: &str) -> Option<&'a LibraryEntry> {
        v.iter().find(|e| e.name.eq_ignore_ascii_case(name))
    }

    #[test]
    fn bike_targets_merge_disk_folders_with_profile_bike_ids() {
        let root = tmp("bike-targets");
        let bikes = root.join("mods/bikes");
        touch(&bikes.join("CLUBMX YZ450F.pkz")); // packaged mod bike
        touch(&bikes.join("MX1OEM_2023_KTM_450_SX-F/paints/red.pnt")); // OEM, paints only
        // Same bike again by id — the profile must not double it up.
        touch(
            &root.join("profiles/Rider One/profile.ini"),
        );
        fs::write(
            root.join("profiles/Rider One/profile.ini"),
            "[rider]\nMX1OEM_2023_KTM_450_SX-F=default_mx\nMX2OEM_2023_KTM_250_SX-F=default_mx\n",
        )
        .unwrap();

        let out = scan_bike_targets(root.to_str().unwrap(), &root.join("profiles"));

        assert_eq!(
            out,
            vec![
                "CLUBMX YZ450F".to_string(),
                "MX1OEM_2023_KTM_450_SX-F".to_string(),
                // Never touched the disk — only the profile knows this one exists.
                "MX2OEM_2023_KTM_250_SX-F".to_string(),
            ]
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bike_targets_survive_a_missing_profiles_folder() {
        let root = tmp("bike-targets-noprofiles");
        touch(&root.join("mods/bikes/Some Bike.pkz"));
        let out = scan_bike_targets(root.to_str().unwrap(), &root.join("nope"));
        assert_eq!(out, vec!["Some Bike".to_string()]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scans_extracted_tracks_and_pkz() {
        let root = tmp("lib-tracks");
        let base = root.join("mods/tracks");
        touch(&base.join("Packed.pkz"));
        touch(&base.join("Loose Track/Loose.map"));
        touch(&base.join("Loose Track/Loose.cfg"));
        touch(&base.join("Loose Track/Loose.pkz")); // inside a track folder → skipped

        let v = scan_library(root.to_str().unwrap(), "mods/tracks", &[], &crate::game::MXB).unwrap();
        assert!(cat(&v, "Packed.pkz").is_some());
        let lt = cat(&v, "Loose Track").expect("extracted track surfaced");
        assert_eq!(lt.kind, "folder");
        assert_eq!(lt.category, "track");
        // The .pkz inside the extracted track must not double-count.
        assert!(cat(&v, "Loose.pkz").is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_livery_loose_at_the_bikes_root_has_no_owner() {
        // `paints/` directly under `mods/bikes` — the no-owning-bike branch of the livery
        // classifier, and the one a refactor is most likely to drop.
        let root = tmp("lib-ownerless-paint");
        touch(&root.join("mods/bikes/paints/Red.pnt"));

        let v = scan_library(root.to_str().unwrap(), "mods/bikes", &[], &crate::game::MXB).unwrap();
        let paint = cat(&v, "Red.pnt").unwrap();
        assert_eq!(paint.category, "bikePaint");
        assert_eq!(paint.parent, None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_shelved_livery_is_still_the_bikes_livery() {
        // Assigned to an inactive model swap, so parked out of `paints/` — the Library
        // must still list it, or assigning a livery would look like losing it.
        let root = tmp("lib-shelved-paint");
        let base = root.join("mods/bikes");
        touch(&base.join("KTM450/model.edf"));
        touch(&base.join("KTM450/paints/Red.pnt"));
        touch(&base.join("KTM450/FrostMod Models/_paints/Yami Redbud.pnt"));

        let v = scan_library(root.to_str().unwrap(), "mods/bikes", &[], &crate::game::MXB).unwrap();
        let shelved = cat(&v, "Yami Redbud.pnt").unwrap();
        assert_eq!(shelved.category, "bikePaint");
        assert_eq!(shelved.parent.as_deref(), Some("KTM450"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_livery_inside_a_model_swap_belongs_to_the_bike() {
        // What a model pack that shipped its own `paints/` leaves behind. Attributing it to
        // the variant folder put it in a bucket keyed by a name no `bikeid` ever matches,
        // so `bundle`'s `Owner::Require` couldn't resolve it and a share code shipped
        // without the livery.
        let root = tmp("lib-swap-paint");
        let base = root.join("mods/bikes");
        touch(&base.join("KTM450/model.edf"));
        touch(&base.join("KTM450/FrostMod Models/Yami/model.edf"));
        touch(&base.join("KTM450/FrostMod Models/Yami/paints/Yami Redbud.pnt"));

        let v = scan_library(root.to_str().unwrap(), "mods/bikes", &[], &crate::game::MXB).unwrap();
        let paint = cat(&v, "Yami Redbud.pnt").unwrap();
        assert_eq!(paint.category, "bikePaint");
        assert_eq!(paint.parent.as_deref(), Some("KTM450"), "the bike, not the variant");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn classifies_bike_paints_and_model_swaps() {
        let root = tmp("lib-bikes");
        let base = root.join("mods/bikes");
        touch(&base.join("KTM450.pkz")); // top-level bike
        touch(&base.join("KTM450/paints/Red.pnt")); // livery for it
        touch(&base.join("KTM450/OEM2024.pkz")); // model swap for it

        let v = scan_library(root.to_str().unwrap(), "mods/bikes", &[], &crate::game::MXB).unwrap();
        assert_eq!(cat(&v, "KTM450.pkz").unwrap().category, "bike");
        let paint = cat(&v, "Red.pnt").unwrap();
        assert_eq!(paint.category, "bikePaint");
        assert_eq!(paint.parent.as_deref(), Some("KTM450"));
        let swap = cat(&v, "OEM2024.pkz").unwrap();
        assert_eq!(swap.category, "bikeModelSwap");
        assert_eq!(swap.parent.as_deref(), Some("KTM450"));
        let _ = fs::remove_dir_all(&root);
    }

    /// The shared-paints layout, which is what a player with six rider models and one set
    /// of liveries actually has on disk: the `paints` folders are links to a single folder
    /// living somewhere else entirely. The game reads them; before this the app didn't,
    /// and the only way to use it was six copies of every paint.
    #[cfg(unix)]
    #[test]
    fn finds_paints_in_a_folder_that_is_really_a_link() {
        let root = tmp("lib-linked-paints");
        let shared = root.join("D_drive/Shared Paints");
        touch(&shared.join("Frost.pnt"));

        let bikes = root.join("mods/bikes");
        touch(&bikes.join("KTM450/model.edf"));
        std::os::unix::fs::symlink(&shared, bikes.join("KTM450/paints")).unwrap();

        let v = scan_library(root.to_str().unwrap(), "mods/bikes", &[], &crate::game::MXB).unwrap();
        let paint = cat(&v, "Frost.pnt").expect("the livery behind the link is found");
        assert_eq!(paint.category, "bikePaint");
        assert_eq!(paint.parent.as_deref(), Some("KTM450"));
        assert_eq!(
            paint.folder, "KTM450/paints",
            "and is addressed where it appears in the tree, not where it's stored",
        );

        // Same folder, reached through two rider models — each must list it.
        let riders = root.join("mods/rider/riders");
        for who in ["Male", "Female"] {
            fs::create_dir_all(riders.join(who)).unwrap();
            std::os::unix::fs::symlink(&shared, riders.join(who).join("paints")).unwrap();
        }
        let v = scan_library(root.to_str().unwrap(), "mods/rider", &[], &crate::game::MXB).unwrap();
        let owners: Vec<&str> = v
            .iter()
            .filter(|e| e.name == "Frost.pnt")
            .filter_map(|e| e.parent.as_deref())
            .collect();
        assert_eq!(owners, vec!["Female", "Male"], "one paint, worn by both models");

        let _ = fs::remove_dir_all(&root);
    }

    /// A whole content folder moved to another drive and linked back in — the split
    /// layout, rather than a single shared leaf.
    #[cfg(unix)]
    #[test]
    fn scans_a_type_folder_that_lives_on_another_drive() {
        let root = tmp("lib-linked-tracks");
        let elsewhere = root.join("D_drive/tracks");
        touch(&elsewhere.join("Packed.pkz"));
        touch(&elsewhere.join("Loose Track/Loose.map"));
        fs::create_dir_all(root.join("mods")).unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.join("mods/tracks")).unwrap();

        let v = scan_library(root.to_str().unwrap(), "mods/tracks", &[], &crate::game::MXB).unwrap();
        assert!(cat(&v, "Packed.pkz").is_some());
        assert!(cat(&v, "Loose Track").is_some_and(|e| e.category == "track"));

        // And the plain `.pkz` listing Manage reads is the same tree.
        let mods = scan_mods(root.to_str().unwrap(), "mods/tracks").unwrap();
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name, "Packed.pkz");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn surfaces_recorded_sound_mods() {
        let root = tmp("lib-sound");
        let base = root.join("mods/bikes");
        // A sound-modded OEM bike folder (loose configs, no .pkz).
        touch(&base.join("MX2OEM_2023_KTM_250_SX-F/engine.scl"));
        touch(&base.join("MX2OEM_2023_KTM_250_SX-F/sfx.cfg"));
        touch(&base.join("Stock/model.edf"));

        let recorded = vec![
            "MX2OEM_2023_KTM_250_SX-F".to_string(),
            "Gone".to_string(),
            "Stock".to_string(),
        ];
        let v = scan_library(root.to_str().unwrap(), "mods/bikes", &recorded, &crate::game::MXB).unwrap();
        let s = cat(&v, "MX2OEM_2023_KTM_250_SX-F").expect("sound bike surfaced");
        assert_eq!(s.category, "sound");
        assert_eq!(s.kind, "folder");
        assert!(cat(&v, "Gone").is_none(), "removed bike pruned");
        assert!(
            v.iter().all(|e| e.name != "Stock" || e.category != "sound"),
            "a recorded folder without sound markers isn't a sound entry",
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn surfaces_all_rider_categories() {
        let root = tmp("lib-rider");
        let base = root.join("mods/rider");
        touch(&base.join("helmets/AGV/AGV.pkz"));
        touch(&base.join("helmets/AGV/paints/Blue.pnt"));
        touch(&base.join("helmets/AGV/goggles/Smoke.pnt"));
        touch(&base.join("boots/Tech10/paints/Wht.pnt"));
        touch(&base.join("boots/Purple White Alpinestar Boots.pnt"));
        touch(&base.join("gloves/Flexair.pnt"));
        touch(&base.join("riders/default_mx/paints/Kit.pnt"));
        touch(&base.join("riders/default_mx/gloves/G.pnt"));
        touch(&base.join("animations/Scrub/Scrub.ini"));
        touch(&base.join("animations/Whip.pkz"));

        let v = scan_library(root.to_str().unwrap(), "mods/rider", &[], &crate::game::MXB).unwrap();
        let has = |c: &str| v.iter().any(|e| e.category == c);
        assert!(has("helmet"), "helmet model");
        assert!(has("helmetPaint"), "helmet paint");
        assert!(has("goggles"), "goggles");
        assert!(has("bootPaint"), "boot paint");
        assert!(
            cat(&v, "Purple White Alpinestar Boots.pnt")
                .is_some_and(|e| e.category == "bootPaint" && e.parent.is_none()),
            "loose boot paint under boots/ surfaces as a parentless bootPaint",
        );
        assert!(has("gloves"), "gloves");
        assert!(has("outfit"), "outfit/kit");
        assert_eq!(cat(&v, "Kit.pnt").unwrap().parent.as_deref(), Some("default_mx"));
        // Riding styles are not a GP Bikes exclusive: `mxbikes.exe` reads the same
        // `rider\animations\<name>\` folder, so both packagings have to surface here or the
        // Riding style picker has nothing to offer but the two stock styles.
        assert!(has("animation"), "riding-style animation as a folder");
        assert!(
            cat(&v, "Whip.pkz").is_some_and(|e| e.category == "animation"),
            "a riding style packaged as a bare .pkz counts too",
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// What the Riding style picker is fed. `scan_rider_targets` is game-agnostic, so this
    /// is the same list for either title — the two stock styles (`mx`, `sm`) are not in it
    /// because they live inside `rider.pkz` and leave nothing on disk to find.
    #[test]
    fn rider_targets_list_installed_riding_styles() {
        let root = tmp("targets-animations");
        let base = root.join("mods/rider");
        touch(&base.join("animations/Scrub/Scrub.ini"));
        touch(&base.join("animations/Whip.pkz"));

        let t = scan_rider_targets(root.to_str().unwrap());
        assert_eq!(t.animations, vec!["Scrub", "Whip"], "folder and .pkz both count");
        let _ = fs::remove_dir_all(&root);
    }

    /// GP Bikes' `mods/rider` is a different shape: helmets and riding-style animations and
    /// nothing else — no boots/protection/gloves folders (those are baked into the rider
    /// model), and no goggles (road helmets use visors).
    #[test]
    fn surfaces_gp_bikes_rider_categories() {
        let root = tmp("lib-rider-gp");
        let base = root.join("mods/rider");
        touch(&base.join("helmets/AGV Pista GP RR/AGV Pista GP RR.pkz"));
        touch(&base.join("helmets/AGV Pista GP RR/paints/Rossi.pnt"));
        touch(&base.join("animations/Elbow Down/Elbow Down.pkz"));
        touch(&base.join("riders/(S) Suit 1 + Boots Alpinestars/paints/Team.pnt"));

        let v = scan_library(root.to_str().unwrap(), "mods/rider", &[], &crate::game::GPB).unwrap();
        let has = |c: &str| v.iter().any(|e| e.category == c);
        assert!(has("helmet"), "helmet model");
        assert!(has("helmetPaint"), "helmet paint");
        assert!(has("animation"), "riding-style animation");
        assert!(has("outfit"), "per-profile suit paint");
        assert_eq!(
            cat(&v, "Team.pnt").unwrap().parent.as_deref(),
            Some("(S) Suit 1 + Boots Alpinestars"),
            "a suit paint belongs to the rider model it sits under",
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// The same tree scanned as GP Bikes must not sprout MX-only categories: a `goggles`
    /// folder there is not a thing the game reads, and reporting one would put an item in
    /// the library that can never be selected.
    #[test]
    fn gp_bikes_ignores_mx_only_gear_folders() {
        let root = tmp("lib-rider-gp-strict");
        let base = root.join("mods/rider");
        touch(&base.join("helmets/AGV/goggles/Smoke.pnt"));
        touch(&base.join("boots/Tech10/paints/Wht.pnt"));
        touch(&base.join("gloves/Flexair.pnt"));

        let v = scan_library(root.to_str().unwrap(), "mods/rider", &[], &crate::game::GPB).unwrap();
        let has = |c: &str| v.iter().any(|e| e.category == c);
        assert!(!has("goggles"), "GP helmets have visors, not goggles");
        assert!(!has("boots") && !has("bootPaint"), "boots are baked into the rider");
        assert!(!has("gloves"), "gloves are baked into the rider");
        let _ = fs::remove_dir_all(&root);
    }

    /// `tyres` (both games) and GP Bikes' `misc/{dashes,stands}` have no internal
    /// structure — they fall through to the generic `.pkz` sweep rather than being
    /// silently skipped for want of a dedicated scanner.
    #[test]
    fn structureless_folders_still_list_their_archives() {
        let root = tmp("lib-generic");
        touch(&root.join("mods/tyres/Dunlop_MiniGp/Dunlop_MiniGp.pkz"));
        touch(&root.join("mods/misc/stands/Paddock.pkz"));

        let tyres =
            scan_library(root.to_str().unwrap(), "mods/tyres", &[], &crate::game::GPB).unwrap();
        assert_eq!(tyres.len(), 1, "the tyre archive is listed");
        let misc =
            scan_library(root.to_str().unwrap(), "mods/misc", &[], &crate::game::GPB).unwrap();
        assert_eq!(misc.len(), 1, "the stand archive is listed");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn move_rejects_file_outside_type_dir() {
        let root = tmp("move-guard");
        fs::create_dir_all(&root).unwrap();
        let outside = root.join("outside.pkz");
        fs::write(&outside, b"x").unwrap();

        let res = move_mod(
            root.to_str().unwrap(),
            outside.to_str().unwrap(),
            "X",
            "mods/tracks",
        );
        assert!(res.is_err());
        assert!(outside.exists());
        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod path_case_tests {
    use super::*;

    /// True when the filesystem under `dir` distinguishes case (ext4 does, APFS/NTFS
    /// as shipped do not) — the assertion only means something on the former.
    fn case_sensitive_fs(dir: &Path) -> bool {
        let probe = dir.join("CaseProbe");
        std::fs::create_dir_all(&probe).unwrap();
        !dir.join("caseprobe").exists()
    }

    /// `mods_path` may be the game's user folder or the mods tree itself, and every
    /// `"mods/..."` lookup in the app goes through here — so this is where the two shapes
    /// have to come out the same.
    #[test]
    fn mods_lookups_work_whether_the_path_is_the_user_folder_or_the_tree() {
        let root = std::env::temp_dir().join(format!("frost-modsroot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        // The ordinary layout: `<user folder>/mods/...`.
        let user = root.join("MX Bikes");
        std::fs::create_dir_all(user.join("mods").join("bikes")).unwrap();
        std::fs::create_dir_all(user.join("profiles")).unwrap();
        let user_s = user.to_string_lossy().into_owned();
        assert_eq!(mods_root(&user_s), user.join("mods"));
        assert_eq!(mods_subdir(&user_s, "mods/bikes"), user.join("mods").join("bikes"));

        // The relocated one: `mods_path` *is* the tree, so the leading `mods` is it.
        let tree = root.join("mods");
        std::fs::create_dir_all(tree.join("bikes")).unwrap();
        let tree_s = tree.to_string_lossy().into_owned();
        assert_eq!(mods_root(&tree_s), tree);
        assert_eq!(mods_subdir(&tree_s, "mods/bikes"), tree.join("bikes"));
        assert_eq!(mods_subdir(&tree_s, "mods"), tree, "the bare root resolves too");

        // A tree recognised by its contents rather than its name — an extracted archive,
        // or a folder `mxbikes.ini` points at under any name at all.
        let odd = root.join("MyStuff");
        std::fs::create_dir_all(odd.join("tracks")).unwrap();
        let odd_s = odd.to_string_lossy().into_owned();
        assert!(is_mods_tree(&odd));
        assert_eq!(mods_subdir(&odd_s, "mods/tracks"), odd.join("tracks"));

        // A user folder that is *itself* named `mods` still resolves to its child, because
        // an existing `mods` child outranks the folder's own name.
        let confusing = root.join("weird").join("mods");
        std::fs::create_dir_all(confusing.join("mods").join("rider")).unwrap();
        let confusing_s = confusing.to_string_lossy().into_owned();
        assert_eq!(mods_root(&confusing_s), confusing.join("mods"));

        // Nothing on disk yet → the path to create, exactly as before.
        let fresh = root.join("Fresh");
        std::fs::create_dir_all(&fresh).unwrap();
        let fresh_s = fresh.to_string_lossy().into_owned();
        assert_eq!(mods_root(&fresh_s), fresh.join("mods"));
        assert!(!is_mods_tree(&fresh), "an unrelated empty folder is not a tree");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolves_a_child_whose_case_differs() {
        let root = std::env::temp_dir().join(format!("frost-case-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let sensitive = case_sensitive_fs(&root);
        // What a Proton prefix or a mod archive can leave on a case-sensitive filesystem.
        std::fs::create_dir_all(root.join("Mods").join("Bikes")).unwrap();

        let got = mods_subdir(root.to_str().unwrap(), "mods/bikes");
        assert!(got.is_dir(), "lookup found the real folder: {got:?}");
        if sensitive {
            assert!(got.ends_with("Bikes"), "kept the on-disk casing: {got:?}");
        } else {
            eprintln!("case-insensitive filesystem — the OS resolved it for us");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn falls_back_to_the_literal_name_for_paths_we_create() {
        let root = std::env::temp_dir().join(format!("frost-case-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let got = mods_subdir(root.to_str().unwrap(), "mods/tracks");
        assert!(got.ends_with("tracks"), "nothing on disk yet, so use what we asked for");
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod library_swap_join_tests {
    use std::fs;

    /// The join the Library's model-swap badge relies on.
    ///
    /// A bike row in the library is a `<Bike>.pkz` **file**, so its `name` carries the archive
    /// extension, while `modelswap::scan_model_swaps` keys by the bike **folder** beside it.
    /// Matching the two raw finds nothing — which is exactly the bug that shipped: the badge
    /// could never appear for any bike. The frontend has to strip the extension, and this
    /// pins which side carries it so a rename on either can't quietly break the join again.
    #[test]
    fn a_bike_row_joins_its_swaps_only_once_the_extension_is_stripped() {
        let root = std::env::temp_dir().join(format!("frost-join-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mp = root.to_str().unwrap();
        let bikes = super::mods_subdir(mp, "mods/bikes");
        let bike = "MX1OEM_2023_KTM_450_SX-F";
        fs::create_dir_all(&bikes).unwrap();
        // The bike as the library sees it: a packed archive at the bikes root.
        fs::write(bikes.join(format!("{bike}.pkz")), b"pkz").unwrap();
        // And a registered model set beside it, as the Locker sees it.
        let variant = bikes.join(bike).join(crate::modelswap::LIB_DIR).join("Factory");
        fs::create_dir_all(&variant).unwrap();
        fs::write(variant.join("model.edf"), b"mesh").unwrap();

        let rows = super::scan_library(mp, "mods/bikes", &[], &crate::game::MXB).expect("scan");
        let bike_rows: Vec<&super::LibraryEntry> =
            rows.iter().filter(|e| e.category == "bike").collect();
        assert!(!bike_rows.is_empty(), "the packed bike must be listed");
        assert!(
            bike_rows.iter().all(|e| e.name.ends_with(".pkz")),
            "a bike row is the archive file, extension and all: {:?}",
            bike_rows.iter().map(|e| &e.name).collect::<Vec<_>>(),
        );

        let swaps = crate::modelswap::scan_model_swaps(mp);
        let key = &swaps.iter().find(|b| b.bike == bike).expect("the bike has swaps").bike;
        assert!(!key.ends_with(".pkz"), "a swap is keyed by the folder, not the archive");

        assert!(
            !bike_rows.iter().any(|e| &e.name == key),
            "raw names must not join — if they do, the frontend's strip is wrong",
        );
        assert!(
            bike_rows.iter().any(|e| super::strip_ext(&e.name).eq_ignore_ascii_case(key)),
            "stripped names must join, or the badge can never appear",
        );
        let _ = fs::remove_dir_all(&root);
    }
}
