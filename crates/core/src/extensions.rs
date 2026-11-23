//! Extensions: third-party packages that change how an app looks and add sandboxed panels.
//!
//! This is the untrusted counterpart of `plugins.rs`. A paid plugin runs with the app's own
//! privileges because it only ever arrives over a verified license. An extension can come
//! from anybody, so nothing in it ever runs with those privileges:
//!
//!   * **Declarative looks.** Themes, button styles, backgrounds and fonts are data. The
//!     webview reads them through [`ext_read_text`] / [`ext_read_asset`] and turns them into
//!     validated CSS variables; no stylesheet or script from a package is applied to the app.
//!   * **Sandboxed code.** A view is HTML/JS that the webview runs in a sandboxed iframe with
//!     an opaque origin and a CSP of its own. Its only way out is a `postMessage` bridge that
//!     answers a fixed list of read-mostly calls, each behind a permission granted at install.
//!   * **Nothing native.** A package may only contain web files and media (see
//!     [`ALLOWED_EXTENSIONS`]). There is no payload step, no file access, no commands of its own.
//!
//! What this module owns is everything that touches the disk: reading a `.mxbext` (a zip),
//! checking its signature, keeping the per-extension state (enabled, granted permissions,
//! pinned signing key) and confining every read to the extension's own folder.
//!
//! ## Signatures
//!
//! `signature.json` at the package root holds an Ed25519 public key and a signature over
//! [`digest_input`]: every other file's path and SHA-256, sorted. Four trust levels follow:
//!
//!   * `builtin`: shipped inside the app (the Style Pack, the colorways). Never on disk here.
//!   * `verified`: signed by a key in [`TRUSTED_PUBLISHERS`], i.e. reviewed by mxbsecure.
//!   * `signed`: a valid signature by a key we don't know. Installs with a warning, and the
//!     key is pinned: an update signed by any other key is refused.
//!   * `unsigned`: no signature at all. Only with developer mode on.
//!
//! A signature that is present and wrong is always fatal, at every level.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::plugins::{resolve_in, safe_path, sha256_hex, version_at_least};

/// Keys whose signature makes an extension `verified`. The private half of each lives with
/// its publisher and never in this repository.
pub const TRUSTED_PUBLISHERS: &[(&str, &str)] = &[(
    "mxbsecure",
    "/3AG7vxFAfroKRBSjZ01d8qGZoGfjOvpNNDP3Wws5FM=",
)];

/// The only file types a package may carry. No executables, no DLLs, no game plugins
/// (`.dlo`): a package with anything else in it is refused before a byte is written.
pub const ALLOWED_EXTENSIONS: &[&str] = &[
    "json", "html", "htm", "js", "mjs", "css", "png", "jpg", "jpeg", "webp", "gif", "svg",
    "woff", "woff2", "ttf", "otf", "txt", "md",
];

/// Every permission a manifest may ask for. Mirrored by `extensions/permissions.ts`, which
/// holds the wording the install dialog shows. An unknown permission fails the install:
/// silently dropping one would install something that then cannot do its job.
pub const PERMISSIONS: &[&str] = &[
    "mods.read",
    "servers.read",
    "friends.presence",
    "game.state",
    "presets.read",
    "coach.read",
    "studio.launch",
    "network",
    "openExternal",
    "mcp.tools",
];

pub const MAX_PACKAGE_BYTES: u64 = 25 * 1024 * 1024;
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 60 * 1024 * 1024;
pub const MAX_FILES: usize = 2000;
const SIGNATURE_FILE: &str = "signature.json";
const MANIFEST_FILE: &str = "extension.json";
const STAGE_TTL: Duration = Duration::from_secs(15 * 60);

/// How much we trust a package. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Trust {
    Verified,
    Signed,
    Unsigned,
}

/// The parts of `extension.json` this side reads. The rest (`contributes`) is passed to the
/// webview whole, after [`validate_manifest`] has checked every file it names exists.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[serde(default = "one")]
    pub manifest_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub min_app_version: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    /// `https://host[:port]` origins the views may `fetch`. Needs the `network` permission.
    #[serde(default)]
    pub network: Vec<String>,
    /// Where to look for a newer version: JSON `{ "version", "url" }`, https only.
    #[serde(default)]
    pub update_url: Option<String>,
    #[serde(default)]
    pub contributes: Value,
    /// Marketplace pictures, inside the package. Shown in the catalog, never by the app itself.
    #[serde(default)]
    pub screenshots: Vec<String>,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingUpdate {
    pub version: String,
    pub url: String,
    /// Permissions the new version asks for that this install hasn't granted.
    pub new_permissions: Vec<String>,
    /// Network origins added by the new version.
    #[serde(default)]
    pub new_hosts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledExt {
    pub version: String,
    pub enabled: bool,
    pub trust: Trust,
    /// Fingerprint of the signing key, pinned for updates. None for unsigned.
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    pub granted: Vec<String>,
    #[serde(default = "yes")]
    pub auto_update: bool,
    #[serde(default)]
    pub pending_update: Option<PendingUpdate>,
    pub installed_at: i64,
    /// `file`, `url`, `folder` or `catalog`. Shown, not trusted.
    pub source: String,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtState {
    #[serde(default)]
    pub developer_mode: bool,
    /// "Let AI agents connect" (`extmcp.rs`). Off until the person turns it on.
    #[serde(default)]
    pub mcp_enabled: bool,
    #[serde(default)]
    pub extensions: BTreeMap<String, InstalledExt>,
}

/// A package that has been read and checked, as it is about to be installed.
#[derive(Debug, Clone)]
pub struct Package {
    pub manifest: Manifest,
    pub manifest_raw: Value,
    pub trust: Trust,
    pub key: Option<String>,
    pub publisher: Option<String>,
    pub files: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, Clone, Deserialize)]
struct SignatureFile {
    #[serde(rename = "publicKey")]
    public_key: String,
    signature: String,
}

/// What the signature covers: a fixed header, then `path\nsha256\n` for every file but the
/// signature itself, sorted by path. Paths are the normalised, forward-slash ones.
pub fn digest_input(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut out = b"mxbext-v1\n".to_vec();
    for (path, bytes) in files {
        if path == SIGNATURE_FILE {
            continue;
        }
        out.extend_from_slice(path.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(sha256_hex(bytes).as_bytes());
        out.push(b'\n');
    }
    out
}

/// First 16 hex digits of the key's SHA-256, which is how a key is shown and pinned.
pub fn key_fingerprint(key: &[u8]) -> String {
    sha256_hex(key)[..16].to_string()
}

fn file_ext(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Read a zip into memory with every limit applied. Nothing is written anywhere.
pub fn read_zip(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    if bytes.len() as u64 > MAX_PACKAGE_BYTES {
        bail!("that package is larger than {} MB", MAX_PACKAGE_BYTES / 1024 / 1024);
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .context("that file is not an extension package (.mxbext)")?;
    if zip.len() > MAX_FILES {
        bail!("that package has more than {MAX_FILES} files");
    }
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name()?.into_owned();
        let rel = safe_path(&name).ok_or_else(|| anyhow!("that package contains an unsafe path: {name}"))?;
        let rel = rel.to_string_lossy().replace('\\', "/");
        if !ALLOWED_EXTENSIONS.contains(&file_ext(&rel).as_str()) {
            bail!("that package contains a file type extensions may not carry: {rel}");
        }
        if entry.size() > MAX_FILE_BYTES {
            bail!("{rel} is larger than {} MB", MAX_FILE_BYTES / 1024 / 1024);
        }
        // The declared size can lie; `take` is what actually bounds a zip bomb.
        let mut buf = Vec::new();
        entry.take(MAX_FILE_BYTES + 1).read_to_end(&mut buf)?;
        if buf.len() as u64 > MAX_FILE_BYTES {
            bail!("{rel} is larger than {} MB", MAX_FILE_BYTES / 1024 / 1024);
        }
        total += buf.len() as u64;
        if total > MAX_TOTAL_BYTES {
            bail!("that package unpacks to more than {} MB", MAX_TOTAL_BYTES / 1024 / 1024);
        }
        if files.insert(rel.clone(), buf).is_some() {
            bail!("that package names {rel} twice");
        }
    }
    Ok(files)
}

/// Read a development folder the same way a package is read, for developer mode.
pub fn read_folder(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        // Editor and VCS litter is skipped rather than refused: it's a working folder.
        if rel.starts_with('.') || rel.contains("/.") || rel.starts_with("node_modules/") {
            continue;
        }
        if !ALLOWED_EXTENSIONS.contains(&file_ext(&rel).as_str()) {
            continue;
        }
        let len = entry.metadata()?.len();
        if len > MAX_FILE_BYTES {
            bail!("{rel} is larger than {} MB", MAX_FILE_BYTES / 1024 / 1024);
        }
        total += len;
        if total > MAX_TOTAL_BYTES || files.len() >= MAX_FILES {
            bail!("that folder is too large to load as an extension");
        }
        files.insert(rel, std::fs::read(entry.path())?);
    }
    Ok(files)
}

fn valid_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    (2..=4).contains(&parts.len())
        && id.len() <= 80
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 40
                && p.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                && !p.starts_with('-')
        })
}

fn valid_version(v: &str) -> bool {
    semver::Version::parse(v).is_ok()
}

/// `https://host[:port]`, nothing else: no path, no wildcard, no credentials.
pub fn valid_origin(o: &str) -> bool {
    let Some(rest) = o.strip_prefix("https://") else {
        return false;
    };
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (rest, None),
    };
    if let Some(p) = port {
        if p.parse::<u16>().is_err() {
            return false;
        }
    }
    !host.is_empty()
        && host.len() <= 253
        && host.contains('.')
        && host
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
        && !host.starts_with('.')
        && !host.ends_with('.')
}

/// Every check that only needs the manifest and the file list. The webview validates the
/// *values* of each contribution again before it applies one; this side makes sure the
/// package is well-formed and that every file it names is really in it.
pub fn validate_manifest(m: &Manifest, files: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    if m.manifest_version != 1 {
        bail!("this app reads manifestVersion 1; that package is {}", m.manifest_version);
    }
    if !valid_id(&m.id) {
        bail!("'{}' is not a valid extension id (use publisher.name, lower-case)", m.id);
    }
    if !valid_version(&m.version) {
        bail!("'{}' is not a version (use x.y.z)", m.version);
    }
    let name = m.name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        bail!("an extension's name must be 1 to 60 characters");
    }
    if m.author.trim().is_empty() || m.author.chars().count() > 60 {
        bail!("an extension's author must be 1 to 60 characters");
    }
    if m.description.chars().count() > 400 {
        bail!("an extension's description must be under 400 characters");
    }
    if let Some(h) = &m.homepage {
        if !h.starts_with("https://") || h.len() > 300 {
            bail!("homepage must be an https:// address");
        }
    }
    let mut seen = BTreeSet::new();
    for p in &m.permissions {
        if !PERMISSIONS.contains(&p.as_str()) {
            bail!("unknown permission '{p}'");
        }
        if !seen.insert(p) {
            bail!("permission '{p}' is listed twice");
        }
    }
    if !m.network.is_empty() && !m.permissions.iter().any(|p| p == "network") {
        bail!("'network' lists hosts but the 'network' permission is not requested");
    }
    if m.network.len() > 10 {
        bail!("at most 10 network hosts");
    }
    for o in &m.network {
        if !valid_origin(o) {
            bail!("'{o}' is not an https://host origin");
        }
    }
    if let Some(u) = &m.update_url {
        if !u.starts_with("https://") {
            bail!("updateUrl must be https://");
        }
    }

    // Every file a contribution names must be in the package, and be the right kind.
    let need = |path: &str, kinds: &[&str], what: &str| -> Result<()> {
        let norm = safe_path(path)
            .ok_or_else(|| anyhow!("{what} '{path}' is not a path inside the package"))?
            .to_string_lossy()
            .replace('\\', "/");
        if !kinds.contains(&file_ext(&norm).as_str()) {
            bail!("{what} '{path}' must be one of: {}", kinds.join(", "));
        }
        if !files.contains_key(&norm) {
            bail!("{what} '{path}' is not in the package");
        }
        Ok(())
    };
    let list = |key: &str| -> Vec<Value> {
        m.contributes
            .get(key)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    };
    let images = ["png", "jpg", "jpeg", "webp", "gif"];
    for v in list("views") {
        let entry = v.get("entry").and_then(|e| e.as_str()).unwrap_or("");
        need(entry, &["html", "htm"], "view entry")?;
    }
    if let Some(main) = m.contributes.get("main").and_then(|v| v.as_str()) {
        need(main, &["html", "htm"], "main")?;
    }
    if m.screenshots.len() > 6 {
        bail!("at most 6 screenshots");
    }
    for shot in &m.screenshots {
        need(shot, &["png", "jpg", "jpeg", "webp"], "screenshot")?;
    }
    for f in list("fonts") {
        let src = f.get("src").and_then(|e| e.as_str()).unwrap_or("");
        need(src, &["woff", "woff2", "ttf", "otf"], "font")?;
    }
    for b in list("backgrounds") {
        if let Some(img) = b.get("image").and_then(|e| e.as_str()) {
            need(img, &images, "background image")?;
        }
    }
    for t in list("themes") {
        if let Some(img) = t
            .get("background")
            .and_then(|b| b.get("image"))
            .and_then(|e| e.as_str())
        {
            need(img, &images, "theme background")?;
        }
        if let Some(logo) = t.get("brand").and_then(|b| b.get("logo")).and_then(|e| e.as_str()) {
            need(logo, &images, "theme logo")?;
            let norm = safe_path(logo).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
            if files.get(&norm).map(|f| f.len()).unwrap_or(0) > 512 * 1024 {
                bail!("theme logo '{logo}' is larger than 512 KB");
            }
        }
    }
    if !m.contributes.is_null() && !m.contributes.is_object() {
        bail!("'contributes' must be an object");
    }
    Ok(())
}

/// Check the signature, if any, and say how far the package is trusted.
pub fn check_signature(
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(Trust, Option<String>, Option<String>)> {
    let Some(raw) = files.get(SIGNATURE_FILE) else {
        return Ok((Trust::Unsigned, None, None));
    };
    let sig: SignatureFile =
        serde_json::from_slice(raw).context("signature.json is not readable")?;
    let key_bytes: [u8; 32] = B64
        .decode(sig.public_key.trim())
        .context("signature.json: publicKey is not base64")?
        .try_into()
        .map_err(|_| anyhow!("signature.json: publicKey is not an Ed25519 key"))?;
    let sig_bytes: [u8; 64] = B64
        .decode(sig.signature.trim())
        .context("signature.json: signature is not base64")?
        .try_into()
        .map_err(|_| anyhow!("signature.json: signature is the wrong length"))?;
    let key = VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_| anyhow!("signature.json: publicKey is not a valid Ed25519 key"))?;
    key.verify_strict(&digest_input(files), &Signature::from_bytes(&sig_bytes))
        .map_err(|_| {
            anyhow!("the package's signature does not match its files - it was changed after signing")
        })?;
    let fp = key_fingerprint(&key_bytes);
    let publisher = TRUSTED_PUBLISHERS.iter().find_map(|(name, k)| {
        (B64.decode(k).ok().as_deref() == Some(&key_bytes[..])).then(|| name.to_string())
    });
    let trust = if publisher.is_some() { Trust::Verified } else { Trust::Signed };
    Ok((trust, Some(fp), publisher))
}

/// Read, validate and signature-check a package. The single entry point for every source.
pub fn open_package(files: BTreeMap<String, Vec<u8>>, app_version: &str) -> Result<Package> {
    let raw = files
        .get(MANIFEST_FILE)
        .ok_or_else(|| anyhow!("that package has no extension.json"))?;
    let manifest_raw: Value =
        serde_json::from_slice(raw).context("extension.json is not valid JSON")?;
    let manifest: Manifest = serde_json::from_value(manifest_raw.clone())
        .map_err(|e| anyhow!("extension.json: {e}"))?;
    validate_manifest(&manifest, &files)?;
    if let Some(need) = &manifest.min_app_version {
        if !version_at_least(app_version, need) {
            bail!("{} needs version {need} or newer of this app; this is {app_version}", manifest.name);
        }
    }
    let (trust, key, publisher) = check_signature(&files)?;
    enforce_reserved_names(&manifest, trust)?;
    Ok(Package { manifest, manifest_raw, trust, key, publisher, files })
}

fn reserved_normalize(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c.to_ascii_lowercase() {
            '0' => Some('o'),
            '1' => Some('l'),
            '3' => Some('e'),
            '4' => Some('a'),
            '5' => Some('s'),
            '7' => Some('t'),
            c if c.is_ascii_lowercase() => Some(c),
            _ => None,
        })
        .collect()
}

fn edit_distance_at_most_one(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len().abs_diff(b.len()) > 1 {
        return false;
    }
    if a.len() == b.len() {
        return a.iter().zip(b).filter(|(x, y)| x != y).count() <= 1;
    }
    let (short, long) = if a.len() < b.len() { (a, b) } else { (b, a) };
    let (mut i, mut j, mut skipped) = (0, 0, false);
    while i < short.len() && j < long.len() {
        if short[i] == long[j] {
            i += 1;
            j += 1;
        } else if skipped {
            return false;
        } else {
            skipped = true;
            j += 1;
        }
    }
    true
}

fn reserved_identity(s: &str) -> bool {
    const NAMES: &[&str] = &[
        "mxb",
        "mxbapp",
        "mxbsecure",
        "frost",
        "frostn",
        "frostnl",
        "frosts",
        "frostsstudio",
        "frostmod",
    ];
    let n = reserved_normalize(s);
    NAMES.contains(&n.as_str())
        || n.contains("mxbsecur")
        || edit_distance_at_most_one(&n, "mxbsecure")
}

/// The manifest field that claims a name reserved for packages signed by mxbsecure.
pub fn claims_reserved_name(m: &Manifest) -> Option<String> {
    // Keep the exact namespace rule as well as catching punctuation and digit lookalikes.
    if ["mxb.", "mxbsecure.", "frost."]
        .iter()
        .any(|p| m.id.starts_with(p))
    {
        return Some(m.id.clone());
    }
    let first = m.id.split('.').next().unwrap_or("");
    if reserved_identity(first) {
        return Some(first.to_string());
    }
    if reserved_identity(&m.author) {
        return Some(m.author.trim().to_string());
    }
    if reserved_normalize(&m.name).contains("mxbsecur") {
        return Some(m.name.trim().to_string());
    }
    None
}

fn enforce_reserved_names(m: &Manifest, trust: Trust) -> Result<()> {
    if trust != Trust::Verified {
        if let Some(what) = claims_reserved_name(m) {
            bail!("'{what}' is reserved for extensions signed by mxbsecure");
        }
    }
    Ok(())
}

fn downgrade_error(new: &str, old: &str, developer_mode: bool) -> Option<String> {
    let older = semver::Version::parse(new).ok()? < semver::Version::parse(old).ok()?;
    (older && !developer_mode).then(|| {
        format!("This is an older version ({new}) than the one installed ({old}). Uninstall first if you want it.")
    })
}

/// Whether `pkg` may replace what is installed under its id. A different signing key is
/// a different author, whatever the id says; so is going from signed to unsigned.
pub fn may_replace(installed: &InstalledExt, pkg: &Package, developer_mode: bool) -> Result<()> {
    match (&installed.key, &pkg.key) {
        (Some(old), Some(new)) if old == new => Ok(()),
        // Moving to a key we trust is the publisher being reviewed, which is allowed.
        (_, Some(_)) if pkg.trust == Trust::Verified => Ok(()),
        (None, _) if developer_mode => Ok(()),
        (Some(_), None) if developer_mode => Ok(()),
        (Some(old), Some(new)) => bail!(
            "this update is signed by a different key ({new}) than the installed version ({old}), so it may not be from the same author. Uninstall first if you trust it."
        ),
        (Some(_), None) => bail!("the installed version is signed and this one isn't"),
        (None, _) => bail!("unsigned extensions need developer mode"),
    }
}

/// Write a checked package to `<dir>/<id>`, replacing whatever was there.
pub fn write_package(dir: &Path, pkg: &Package) -> Result<PathBuf> {
    let root = dir.join(&pkg.manifest.id);
    let tmp = dir.join(format!(".{}.tmp", pkg.manifest.id));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp).ok();
    }
    std::fs::create_dir_all(&tmp)?;
    for (rel, bytes) in &pkg.files {
        let out = tmp.join(safe_path(rel).ok_or_else(|| anyhow!("unsafe path {rel}"))?);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::File::create(&out)?.write_all(bytes)?;
    }
    // Swap in one step, so a failed write never leaves half an extension behind.
    if root.exists() {
        std::fs::remove_dir_all(&root).with_context(|| format!("removing {}", root.display()))?;
    }
    std::fs::rename(&tmp, &root)?;
    Ok(root)
}

// ---------------------------------------------------------------------------------------
// Tauri side: state on disk, staging, commands.
// ---------------------------------------------------------------------------------------

pub fn extensions_dir(app: &tauri::AppHandle) -> Result<PathBuf> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_local_data_dir()
        .context("no app data directory")?
        .join("extensions");
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

fn state_path(app: &tauri::AppHandle) -> Result<PathBuf> {
    Ok(extensions_dir(app)?.join("state.json"))
}

fn state_lock() -> &'static Mutex<()> {
    static L: OnceLock<Mutex<()>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(()))
}

pub fn load_state(app: &tauri::AppHandle) -> ExtState {
    state_path(app)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_state(app: &tauri::AppHandle, st: &ExtState) -> Result<()> {
    let path = state_path(app)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(st)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Read-modify-write under one lock, so two commands can't lose each other's change.
fn with_state<T>(
    app: &tauri::AppHandle,
    f: impl FnOnce(&mut ExtState) -> Result<T>,
) -> Result<T, String> {
    let _g = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut st = load_state(app);
    let out = f(&mut st).map_err(|e| e.to_string())?;
    save_state(app, &st).map_err(|e| e.to_string())?;
    Ok(out)
}

struct Staged {
    pkg: Package,
    source: String,
    at: Instant,
}

fn staged() -> &'static Mutex<HashMap<String, Staged>> {
    static S: OnceLock<Mutex<HashMap<String, Staged>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn app_version(app: &tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

/// What the install dialog shows before anything is written.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspected {
    pub token: String,
    pub manifest: Value,
    pub source: String,
    pub trust: Trust,
    pub key: Option<String>,
    pub publisher: Option<String>,
    pub new_permissions: Vec<String>,
    pub new_hosts: Vec<String>,
    /// The installed version, when this would replace one.
    pub replaces: Option<String>,
    /// Set when this can't be installed as things stand (unsigned without developer mode,
    /// a different signing key). The dialog shows it instead of an Install button.
    pub blocked: Option<String>,
}

fn stage(app: &tauri::AppHandle, pkg: Package, source: &str) -> Inspected {
    let st = load_state(app);
    let installed = st.extensions.get(&pkg.manifest.id);
    let old_manifest = installed_manifest(app, &pkg.manifest.id)
        .and_then(|m| serde_json::from_value::<Manifest>(m).ok());
    let blocked = match installed {
        Some(i) => downgrade_error(&pkg.manifest.version, &i.version, st.developer_mode)
            .or_else(|| may_replace(i, &pkg, st.developer_mode).err().map(|e| e.to_string())),
        None if pkg.trust == Trust::Unsigned && !st.developer_mode => Some(
            "This extension isn't signed. Unsigned extensions only install with developer mode on.".into(),
        ),
        None => None,
    };
    let token = uuid::Uuid::new_v4().to_string();
    let out = Inspected {
        token: token.clone(),
        manifest: pkg.manifest_raw.clone(),
        source: source.into(),
        trust: pkg.trust,
        key: pkg.key.clone(),
        publisher: pkg.publisher.clone(),
        new_permissions: old_manifest
            .as_ref()
            .map(|old| new_manifest_permissions(old, &pkg.manifest))
            .unwrap_or_default(),
        new_hosts: old_manifest
            .as_ref()
            .map(|old| new_manifest_hosts(old, &pkg.manifest))
            .unwrap_or_default(),
        replaces: installed.map(|i| i.version.clone()),
        blocked,
    };
    let mut map = staged().lock().unwrap_or_else(|e| e.into_inner());
    map.retain(|_, s| s.at.elapsed() < STAGE_TTL);
    map.insert(token, Staged { pkg, source: source.into(), at: Instant::now() });
    out
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("MXB-App-Extensions/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(60))
        .https_only(true)
        .build()
        .unwrap_or_default()
}

/// The catalog's own base. Over https in every build anyone is given; `wrangler dev` (http)
/// only through the debug-only override in `names::control_plane`.
pub(crate) fn catalog_base() -> String {
    crate::names::control_plane()
}

fn catalog_download_id(url: &str, base: &str) -> Option<String> {
    let rest = url.strip_prefix(&format!("{base}/v1/extensions/"))?;
    let id = rest.strip_suffix("/download")?;
    valid_id(id).then(|| id.to_string())
}

fn is_catalog_url(url: &str) -> bool {
    url.starts_with(&format!("{}/v1/extensions/", catalog_base()))
}

pub(crate) fn catalog_http() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("MXB-App-Extensions/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(60))
        .https_only(catalog_base().starts_with("https://"))
        .build()
        .unwrap_or_default()
}

async fn download(url: &str) -> Result<Vec<u8>> {
    let catalog = is_catalog_url(url);
    if !url.starts_with("https://") && !catalog {
        bail!("only https:// addresses can be installed from");
    }
    let client = if catalog { catalog_http() } else { http() };
    let mut resp = client.get(url).send().await?.error_for_status()?;
    // File hosts that put a "are you sure?" page in front of the download answer 200 with
    // HTML. Say that, rather than "not a package" about a file the person knows is one.
    let html = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/html"));
    if html {
        bail!("that address returned a web page, not an extension package; use a direct download link");
    }
    if resp.content_length().unwrap_or(0) > MAX_PACKAGE_BYTES {
        bail!("that package is larger than {} MB", MAX_PACKAGE_BYTES / 1024 / 1024);
    }
    let mut out = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        out.extend_from_slice(&chunk);
        if out.len() as u64 > MAX_PACKAGE_BYTES {
            bail!("that package is larger than {} MB", MAX_PACKAGE_BYTES / 1024 / 1024);
        }
    }
    Ok(out)
}

/// One installed extension, as the Extensions page lists it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtView {
    pub id: String,
    pub manifest: Value,
    pub suspended: bool,
    #[serde(flatten)]
    pub state: InstalledExt,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtList {
    pub developer_mode: bool,
    pub extensions: Vec<ExtView>,
}

fn installed_manifest(app: &tauri::AppHandle, id: &str) -> Option<Value> {
    let p = extensions_dir(app).ok()?.join(id).join(MANIFEST_FILE);
    serde_json::from_slice(&std::fs::read(p).ok()?).ok()
}

#[tauri::command]
pub fn ext_list(app: tauri::AppHandle) -> ExtList {
    let st = load_state(&app);
    let extensions = st
        .extensions
        .iter()
        .filter_map(|(id, s)| {
            Some(ExtView { id: id.clone(), manifest: installed_manifest(&app, id)?, suspended: is_suspended(s.trust, st.developer_mode), state: s.clone() })
        })
        .collect();
    ExtList { developer_mode: st.developer_mode, extensions }
}

#[tauri::command]
pub fn ext_inspect_file(app: tauri::AppHandle, path: String) -> Result<Inspected, String> {
    let p = PathBuf::from(&path);
    let meta = std::fs::metadata(&p).map_err(|e| format!("{path}: {e}"))?;
    let pkg = if meta.is_dir() {
        if !load_state(&app).developer_mode {
            return Err("Loading an extension from a folder needs developer mode.".into());
        }
        let files = read_folder(&p).map_err(|e| e.to_string())?;
        open_package(files, &app_version(&app)).map_err(|e| e.to_string())?
    } else {
        if meta.len() > MAX_PACKAGE_BYTES {
            return Err(format!("that package is larger than {} MB", MAX_PACKAGE_BYTES / 1024 / 1024));
        }
        let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
        let files = read_zip(&bytes).map_err(|e| e.to_string())?;
        open_package(files, &app_version(&app)).map_err(|e| e.to_string())?
    };
    Ok(stage(&app, pkg, if meta.is_dir() { "folder" } else { "file" }))
}

#[tauri::command]
pub async fn ext_inspect_url(app: tauri::AppHandle, url: String) -> Result<Inspected, String> {
    if let Some(id) = catalog_download_id(&url, &catalog_base()) {
        return catalog_inspect(&app, &id).await;
    }
    let bytes = download(&url).await.map_err(|e| e.to_string())?;
    let files = read_zip(&bytes).map_err(|e| e.to_string())?;
    let pkg = open_package(files, &app_version(&app)).map_err(|e| e.to_string())?;
    Ok(stage(&app, pkg, "url"))
}

/// Install what [`ext_inspect_file`] / [`ext_inspect_url`] staged. `granted` is what the
/// person ticked in the dialog; it can only be a subset of what the manifest asked for.
#[tauri::command]
pub fn ext_install(app: tauri::AppHandle, token: String, granted: Vec<String>) -> Result<String, String> {
    let Staged { pkg, source, .. } = staged()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&token)
        .ok_or("That install has expired. Open the package again.")?;
    let dir = extensions_dir(&app).map_err(|e| e.to_string())?;
    let id = pkg.manifest.id.clone();
    with_state(&app, |st| {
        if let Some(i) = st.extensions.get(&id) {
            if let Some(message) =
                downgrade_error(&pkg.manifest.version, &i.version, st.developer_mode)
            {
                bail!(message);
            }
            may_replace(i, &pkg, st.developer_mode)?;
        } else if pkg.trust == Trust::Unsigned && !st.developer_mode {
            bail!("unsigned extensions need developer mode");
        }
        let asked: BTreeSet<&String> = pkg.manifest.permissions.iter().collect();
        let granted: Vec<String> = granted.into_iter().filter(|g| asked.contains(g)).collect();
        write_package(&dir, &pkg)?;
        let auto_update = st.extensions.get(&id).map(|i| i.auto_update).unwrap_or(true);
        st.extensions.insert(
            id.clone(),
            InstalledExt {
                version: pkg.manifest.version.clone(),
                enabled: true,
                trust: pkg.trust,
                key: pkg.key.clone(),
                publisher: pkg.publisher.clone(),
                granted,
                auto_update,
                pending_update: None,
                installed_at: now_secs(),
                source,
            },
        );
        log::info!("[extensions] installed {id} {} ({:?})", pkg.manifest.version, pkg.trust);
        Ok(id.clone())
    })
}

#[tauri::command]
pub fn ext_set_enabled(app: tauri::AppHandle, id: String, enabled: bool) -> Result<(), String> {
    with_state(&app, |st| {
        st.extensions.get_mut(&id).ok_or_else(|| anyhow!("not installed"))?.enabled = enabled;
        Ok(())
    })
}

#[tauri::command]
pub fn ext_set_auto_update(app: tauri::AppHandle, id: String, on: bool) -> Result<(), String> {
    with_state(&app, |st| {
        st.extensions.get_mut(&id).ok_or_else(|| anyhow!("not installed"))?.auto_update = on;
        Ok(())
    })
}

/// Revoke (or re-grant) permissions. Only ones the installed manifest asked for.
#[tauri::command]
pub fn ext_set_permissions(app: tauri::AppHandle, id: String, granted: Vec<String>) -> Result<(), String> {
    let asked: BTreeSet<String> = installed_manifest(&app, &id)
        .and_then(|m| serde_json::from_value::<Manifest>(m).ok())
        .map(|m| m.permissions.into_iter().collect())
        .unwrap_or_default();
    with_state(&app, |st| {
        let e = st.extensions.get_mut(&id).ok_or_else(|| anyhow!("not installed"))?;
        e.granted = granted.into_iter().filter(|g| asked.contains(g)).collect();
        Ok(())
    })
}

#[tauri::command]
pub fn ext_uninstall(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let dir = extensions_dir(&app).map_err(|e| e.to_string())?;
    with_state(&app, |st| {
        st.extensions.remove(&id).ok_or_else(|| anyhow!("not installed"))?;
        // `id` came from the state file, but still: only ever a direct child of the folder.
        if valid_id(&id) {
            let root = dir.join(&id);
            if root.exists() {
                std::fs::remove_dir_all(&root)?;
            }
        }
        Ok(())
    })
}

pub fn set_mcp_enabled(app: &tauri::AppHandle, on: bool) -> Result<(), String> {
    with_state(app, |st| {
        st.mcp_enabled = on;
        Ok(())
    })
}

#[tauri::command]
pub fn ext_set_developer_mode(app: tauri::AppHandle, on: bool) -> Result<(), String> {
    with_state(&app, |st| {
        st.developer_mode = on;
        Ok(())
    })
}

/// The extension's folder, but only while it is installed and enabled.
fn enabled_root(app: &tauri::AppHandle, id: &str) -> Result<PathBuf, String> {
    let st = load_state(app);
    match st.extensions.get(id) {
        Some(e) if is_suspended(e.trust, st.developer_mode) => {
            return Err("Unsigned extensions only run with developer mode on.".into())
        }
        Some(e) if e.enabled => {}
        _ => return Err("that extension isn't enabled".into()),
    }
    if !valid_id(id) {
        return Err("invalid id".into());
    }
    Ok(extensions_dir(app).map_err(|e| e.to_string())?.join(id))
}

fn is_suspended(trust: Trust, developer_mode: bool) -> bool {
    trust == Trust::Unsigned && !developer_mode
}

fn read_inside(app: &tauri::AppHandle, id: &str, path: &str, kinds: &[&str]) -> Result<Vec<u8>, String> {
    let root = enabled_root(app, id)?;
    let full = resolve_in(&root, path).map_err(|_| "that path is outside the extension".to_string())?;
    if !kinds.contains(&file_ext(path).as_str()) {
        return Err("that file type can't be read this way".into());
    }
    let len = std::fs::metadata(&full).map_err(|_| format!("{path} is not in the extension"))?.len();
    if len > MAX_FILE_BYTES {
        return Err("file too large".into());
    }
    std::fs::read(&full).map_err(|e| e.to_string())
}

/// A text file from an enabled extension's own folder: a view's HTML, JS or CSS.
#[tauri::command]
pub fn ext_read_text(app: tauri::AppHandle, id: String, path: String) -> Result<String, String> {
    let bytes = read_inside(&app, &id, &path, &["html", "htm", "js", "mjs", "css", "json", "txt", "md", "svg"])?;
    String::from_utf8(bytes).map_err(|_| format!("{path} is not UTF-8 text"))
}

#[derive(Debug, Serialize)]
pub struct Asset {
    pub mime: &'static str,
    pub base64: String,
}

/// An image or font from an enabled extension's own folder, for a `data:` URL.
#[tauri::command]
pub fn ext_read_asset(app: tauri::AppHandle, id: String, path: String) -> Result<Asset, String> {
    let mime = match file_ext(&path).as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => return Err("that file type can't be read as an asset".into()),
    };
    let bytes = read_inside(&app, &id, &path, &["png", "jpg", "jpeg", "webp", "gif", "svg", "woff", "woff2", "ttf", "otf"])?;
    Ok(Asset { mime, base64: B64.encode(bytes) })
}

#[derive(Debug, Deserialize)]
struct UpdateInfo {
    version: String,
    url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOutcome {
    pub id: String,
    /// `updated`, `needs-approval`, `current` or `failed`.
    pub status: &'static str,
    pub version: Option<String>,
    pub message: Option<String>,
}

/// Check every installed extension that has an `updateUrl` and auto-update on.
///
/// An update installs on its own only when it is signed by the same key and asks for no
/// permission the person hasn't already granted and adds no network host. Anything more is
/// parked as a `pendingUpdate` for the Extensions page to ask about.
#[tauri::command]
pub async fn ext_check_updates(app: tauri::AppHandle, only: Option<String>) -> Vec<UpdateOutcome> {
    let st = load_state(&app);
    let mut out = Vec::new();
    for (id, ext) in st.extensions.iter() {
        if only.as_deref().is_some_and(|o| o != id) || (only.is_none() && !ext.auto_update) {
            continue;
        }
        let Some(manifest) = installed_manifest(&app, id).and_then(|m| serde_json::from_value::<Manifest>(m).ok()) else {
            continue;
        };
        let Some(source) = update_source(&ext.source, manifest.update_url.as_deref()) else { continue };
        let res = check_one(&app, id, ext, source).await;
        out.push(match res {
            Ok(o) => o,
            Err(e) => UpdateOutcome { id: id.clone(), status: "failed", version: None, message: Some(e.to_string()) },
        });
    }
    out
}

#[derive(Debug, PartialEq)]
enum UpdateSource {
    /// The manifest's own `updateUrl`: JSON `{ version, url }`.
    Url(String),
    /// Installed from the marketplace: its listing says the version and where to get it.
    Catalog,
}

fn update_source(source: &str, update_url: Option<&str>) -> Option<UpdateSource> {
    if source == "catalog" {
        return Some(UpdateSource::Catalog);
    }
    match source {
        "file" | "url" | "folder" => update_url.map(|u| UpdateSource::Url(u.to_string())),
        _ => None,
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogListing {
    version: String,
    download: String,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    key: Option<String>,
}

fn catalog_package_matches(listing: &CatalogListing, bytes: &[u8], pkg: &Package) -> bool {
    pkg.manifest.version == listing.version
        && listing
            .sha256
            .as_ref()
            .is_none_or(|h| sha256_hex(bytes) == *h)
        && listing
            .key
            .as_ref()
            .is_none_or(|k| pkg.key.as_ref() == Some(k))
}

fn new_manifest_permissions(installed: &Manifest, new: &Manifest) -> Vec<String> {
    new.permissions
        .iter()
        .filter(|p| !installed.permissions.contains(p))
        .cloned()
        .collect()
}

fn new_manifest_hosts(installed: &Manifest, new: &Manifest) -> Vec<String> {
    new.network
        .iter()
        .filter(|h| !installed.network.contains(h))
        .cloned()
        .collect()
}

fn needs_consent(
    installed: &Manifest,
    granted: &[String],
    new: &Manifest,
) -> (Vec<String>, Vec<String>) {
    let new_permissions = new
        .permissions
        .iter()
        .filter(|p| !granted.contains(p))
        .cloned()
        .collect();
    let new_hosts = if granted.iter().any(|p| p == "network")
        && new.permissions.iter().any(|p| p == "network")
    {
        new_manifest_hosts(installed, new)
    } else {
        Vec::new()
    };
    (new_permissions, new_hosts)
}

async fn check_one(
    app: &tauri::AppHandle,
    id: &str,
    ext: &InstalledExt,
    source: UpdateSource,
) -> Result<UpdateOutcome> {
    let (info, catalog_listing): (UpdateInfo, Option<CatalogListing>) = match source {
        UpdateSource::Url(u) => (http().get(&u).send().await?.error_for_status()?.json().await?, None),
        UpdateSource::Catalog => {
            let l: CatalogListing = catalog_http()
                .get(format!("{}/v1/extensions/{id}", catalog_base()))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let url = format!("{}{}", catalog_base(), l.download);
            if !l.download.starts_with("/v1/extensions/") || l.download.contains("..") || !is_catalog_url(&url) {
                bail!("the catalog pointed somewhere else for {id}");
            }
            (UpdateInfo { version: l.version.clone(), url }, Some(l))
        }
    };
    let newer = semver::Version::parse(&info.version)? > semver::Version::parse(&ext.version)?;
    if !newer {
        return Ok(UpdateOutcome { id: id.into(), status: "current", version: Some(ext.version.clone()), message: None });
    }
    let bytes = download(&info.url).await?;
    let pkg = open_package(read_zip(&bytes)?, &app_version(app))?;
    if let Some(listing) = &catalog_listing {
        if pkg.manifest.id != id || !catalog_package_matches(listing, &bytes, &pkg) {
            bail!(
                "The marketplace's file for {id} doesn't match its listing. Nothing was installed."
            );
        }
    }
    if pkg.manifest.id != id || pkg.manifest.version != info.version {
        bail!("the update at {} is not {id} {}", info.url, info.version);
    }
    let dev = load_state(app).developer_mode;
    may_replace(ext, &pkg, dev)?;
    let installed = installed_manifest(app, id)
        .and_then(|m| serde_json::from_value::<Manifest>(m).ok())
        .ok_or_else(|| anyhow!("the installed extension is missing its manifest"))?;
    let (new_perms, new_hosts) = needs_consent(&installed, &ext.granted, &pkg.manifest);
    // New access is never granted here, even from the page's own Update button: it goes
    // through the install dialog like any other consent.
    if !new_perms.is_empty() || !new_hosts.is_empty() {
        let pending = PendingUpdate { version: info.version.clone(), url: info.url.clone(), new_permissions: new_perms, new_hosts };
        with_state(app, |st| {
            if let Some(e) = st.extensions.get_mut(id) {
                e.pending_update = Some(pending);
            }
            Ok(())
        })
        .map_err(|e| anyhow!(e))?;
        return Ok(UpdateOutcome { id: id.into(), status: "needs-approval", version: Some(info.version), message: None });
    }
    let dir = extensions_dir(app)?;
    let version = pkg.manifest.version.clone();
    with_state(app, |st| {
        write_package(&dir, &pkg)?;
        if let Some(e) = st.extensions.get_mut(id) {
            e.version = version.clone();
            e.trust = pkg.trust;
            e.key = pkg.key.clone();
            e.publisher = pkg.publisher.clone();
            e.pending_update = None;
            // Permissions the new version dropped are dropped from the grant too.
            e.granted.retain(|g| pkg.manifest.permissions.contains(g));
        }
        Ok(())
    })
    .map_err(|e| anyhow!(e))?;
    log::info!("[extensions] updated {id} to {version}");
    Ok(UpdateOutcome { id: id.into(), status: "updated", version: Some(version), message: None })
}

// ---------------------------------------------------------------------------------------
// The marketplace (control plane `/v1/extensions`). The webview gets the catalog's JSON as
// display data. What gets *installed* goes through the same package checks as a file, and
// nothing the listing says is trusted over what the signed package itself says.
// ---------------------------------------------------------------------------------------

pub(crate) fn catalog_id(id: &str) -> Result<(), String> {
    if valid_id(id) {
        Ok(())
    } else {
        Err("not an extension id".into())
    }
}

pub(crate) const CATALOG_DOWN: &str = "The extension catalog can't be reached right now.";

pub(crate) async fn catalog_json(resp: reqwest::Response) -> Result<Value, String> {
    let status = resp.status();
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(body.get("error").and_then(|e| e.as_str()).unwrap_or("catalog error").to_string());
    }
    Ok(body)
}

/// The catalog's links are paths (`/v1/extensions/...`); join them to the base we called,
/// and drop any that aren't ours, so a listing can only ever point the app back at the catalog.
pub fn absolutize(v: &mut Value, base: &str) {
    let fix = |s: &str| -> Value {
        if s.starts_with("/v1/extensions/") && !s.contains("..") {
            Value::String(format!("{base}{s}"))
        } else {
            Value::Null
        }
    };
    match v {
        Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                match (k.as_str(), &*val) {
                    ("thumbnail" | "download", Value::String(s)) => *val = fix(s),
                    ("screenshots", Value::Array(items)) => {
                        *val = Value::Array(
                            items.iter().filter_map(|i| i.as_str()).map(fix).filter(|x| !x.is_null()).collect(),
                        )
                    }
                    ("manifest", _) => {}
                    _ => absolutize(val, base),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|i| absolutize(i, base)),
        _ => {}
    }
}

async fn catalog_get(path_and_query: &str) -> Result<Value, String> {
    let base = catalog_base();
    let resp = catalog_http()
        .get(format!("{base}{path_and_query}"))
        .send()
        .await
        .map_err(|_| CATALOG_DOWN.to_string())?;
    let mut v = catalog_json(resp).await?;
    absolutize(&mut v, &base);
    Ok(v)
}

fn query_escape(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'.' {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Search the catalog. `category` and `sort` are passed through; the server ignores unknowns.
#[tauri::command]
pub async fn ext_catalog_search(q: String, category: String, sort: String, offset: u32, tier: Option<String>) -> Result<Value, String> {
    let q: String = q.chars().take(80).collect();
    let category: String = category.chars().take(20).collect();
    let sort: String = sort.chars().take(10).collect();
    let mut path = format!(
        "/v1/extensions?q={}&category={}&sort={}&offset={}",
        query_escape(&q),
        query_escape(&category),
        query_escape(&sort),
        offset.min(2000)
    );
    if let Some(tier) = tier {
        let tier: String = tier.chars().take(10).collect();
        path.push_str("&tier=");
        path.push_str(&query_escape(&tier));
    }
    catalog_get(&path).await
}

#[tauri::command]
pub async fn ext_catalog_get(id: String) -> Result<Value, String> {
    catalog_id(&id)?;
    catalog_get(&format!("/v1/extensions/{id}")).await
}

/// Latest listed versions for every installed extension the catalog knows.
#[tauri::command]
pub async fn ext_catalog_latest(app: tauri::AppHandle) -> Result<Value, String> {
    let ids: Vec<String> = load_state(&app).extensions.keys().filter(|i| valid_id(i)).cloned().collect();
    if ids.is_empty() {
        return Ok(serde_json::json!({ "extensions": [] }));
    }
    catalog_get(&format!("/v1/extensions/latest?ids={}", ids.join(","))).await
}

async fn catalog_inspect(app: &tauri::AppHandle, id: &str) -> Result<Inspected, String> {
    let listing: CatalogListing = catalog_http()
        .get(format!("{}/v1/extensions/{id}", catalog_base()))
        .send()
        .await
        .map_err(|_| CATALOG_DOWN.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let url = format!("{}/v1/extensions/{id}/download", catalog_base());
    let bytes = download(&url).await.map_err(|e| e.to_string())?;
    let files = read_zip(&bytes).map_err(|e| e.to_string())?;
    let pkg = open_package(files, &app_version(app)).map_err(|e| e.to_string())?;
    if pkg.manifest.id != id || !catalog_package_matches(&listing, &bytes, &pkg) {
        return Err(format!("The marketplace's file for {id} doesn't match its listing. Nothing was installed."));
    }
    Ok(stage(app, pkg, "catalog"))
}

/// Download a listing's package and stage it for the install dialog, like a file.
#[tauri::command]
pub async fn ext_catalog_inspect(app: tauri::AppHandle, id: String) -> Result<Inspected, String> {
    catalog_id(&id)?;
    catalog_inspect(&app, &id).await
}

/// Count this install once (the server keeps only a digest of the app's install id).
#[tauri::command]
pub async fn ext_catalog_installed(app: tauri::AppHandle, id: String) -> Result<(), String> {
    catalog_id(&id)?;
    let install_id = crate::config::load_or_detect(&app).map(|c| c.install_id).unwrap_or_default();
    if install_id.len() < 8 {
        return Ok(());
    }
    let _ = catalog_http()
        .post(format!("{}/v1/extensions/{id}/installs", catalog_base()))
        .json(&serde_json::json!({ "installId": install_id }))
        .send()
        .await;
    Ok(())
}

pub(crate) fn account_token(app: &tauri::AppHandle) -> Result<String, String> {
    let t = crate::config::load_or_detect(app).map(|c| c.cp_token).unwrap_or_default();
    let t = t.trim().to_string();
    if t.is_empty() {
        return Err("Sign in to rate extensions.".into());
    }
    Ok(t)
}

/// Rate a listing 1 to 5 as the signed-in account. Answers `{ rating, ratings, mine }`.
#[tauri::command]
pub async fn ext_catalog_rate(app: tauri::AppHandle, id: String, stars: u8) -> Result<Value, String> {
    catalog_id(&id)?;
    let tok = account_token(&app)?;
    let resp = catalog_http()
        .post(format!("{}/v1/extensions/{id}/rating", catalog_base()))
        .bearer_auth(tok)
        .json(&serde_json::json!({ "stars": stars }))
        .send()
        .await
        .map_err(|_| CATALOG_DOWN.to_string())?;
    catalog_json(resp).await
}

/// The signed-in account's own rating of a listing, or null.
#[tauri::command]
pub async fn ext_catalog_my_rating(app: tauri::AppHandle, id: String) -> Result<Option<u8>, String> {
    catalog_id(&id)?;
    let Ok(tok) = account_token(&app) else { return Ok(None) };
    let Ok(resp) = catalog_http()
        .get(format!("{}/v1/extensions/{id}/rating", catalog_base()))
        .bearer_auth(tok)
        .send()
        .await
    else {
        return Ok(None);
    };
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    Ok(body.get("mine").and_then(|m| m.as_u64()).map(|m| m as u8))
}

/// One MXB Coach session, as an extension with `coach.read` gets it: what was ridden and the
/// times, never the recording's path or the rider's folders.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CoachSession {
    /// `yyyymmdd-hhmmss`, local time.
    pub started: String,
    pub track: String,
    pub bike: String,
    pub category: String,
    /// `testing`, `race`, `straight-rhythm` or `other`.
    pub event: &'static str,
    /// Laps that started and finished at the line, with no cut, reset or crash.
    pub clean_laps: Vec<i64>,
    pub laps: usize,
    pub best_ms: Option<i64>,
}

/// Coach's session index (`coach/index-v5.json` in the shared data folder), newest first.
/// Read only; written by Coach. An index that isn't there yet is an empty list.
pub fn coach_sessions_from(index: &Value, limit: usize) -> Vec<CoachSession> {
    let s = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").chars().take(120).collect::<String>();
    let mut out: Vec<CoachSession> = index
        .as_object()
        .map(|m| m.values().filter_map(|e| e.get("summary")).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|sum| {
            let laps = sum.get("laps").and_then(|l| l.as_array()).cloned().unwrap_or_default();
            let flag = |l: &Value, k: &str| l.get(k).and_then(|x| x.as_bool()).unwrap_or(false);
            let clean_laps = laps
                .iter()
                .filter(|l| flag(l, "whole") && !flag(l, "invalid") && !flag(l, "cut") && !flag(l, "reset") && !flag(l, "crashed") && !flag(l, "warmup"))
                .filter_map(|l| l.get("timeMs").and_then(|t| t.as_i64()))
                .filter(|t| *t > 0)
                .take(200)
                .collect();
            CoachSession {
                started: s(sum, "started").chars().take(15).collect(),
                track: s(sum, "trackName"),
                bike: s(sum, "bikeName"),
                category: s(sum, "category"),
                event: match sum.get("eventType").and_then(|x| x.as_i64()) {
                    Some(1) => "testing",
                    Some(2) => "race",
                    Some(4) => "straight-rhythm",
                    _ => "other",
                },
                laps: laps.len(),
                best_ms: sum.get("bestMs").and_then(|x| x.as_i64()).filter(|t| *t > 0),
                clean_laps,
            }
        })
        .collect();
    out.sort_by(|a, b| b.started.cmp(&a.started));
    out.truncate(limit);
    out
}

/// The app's own read of Coach's sessions, for the extension bridge (`coach.read`).
#[tauri::command]
pub fn ext_coach_sessions(app: tauri::AppHandle) -> Vec<CoachSession> {
    let Some(path) = crate::config::data_dir(&app).map(|d| d.join("coach").join("index-v5.json")) else {
        return vec![];
    };
    let Ok(bytes) = std::fs::read(path) else { return vec![] };
    let Ok(index) = serde_json::from_slice::<Value>(&bytes) else { return vec![] };
    coach_sessions_from(&index, 50)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn manifest(id: &str, extra: Value) -> Vec<u8> {
        let mut m = serde_json::json!({
            "manifestVersion": 1, "id": id, "name": "Test", "version": "1.0.0", "author": "Me",
            "permissions": [], "contributes": {}
        });
        for (k, v) in extra.as_object().unwrap() {
            m[k] = v.clone();
        }
        serde_json::to_vec(&m).unwrap()
    }

    fn files(id: &str, extra: Value) -> BTreeMap<String, Vec<u8>> {
        let mut f = BTreeMap::new();
        f.insert(MANIFEST_FILE.to_string(), manifest(id, extra));
        f.insert("view/index.html".to_string(), b"<p>hi</p>".to_vec());
        f
    }

    fn parsed_manifest(id: &str, extra: Value) -> Manifest {
        serde_json::from_slice(&manifest(id, extra)).unwrap()
    }

    fn sign(files: &mut BTreeMap<String, Vec<u8>>, key: &SigningKey) {
        files.remove(SIGNATURE_FILE);
        let sig = key.sign(&digest_input(files));
        let body = serde_json::json!({
            "publicKey": B64.encode(key.verifying_key().to_bytes()),
            "signature": B64.encode(sig.to_bytes()),
        });
        files.insert(SIGNATURE_FILE.into(), serde_json::to_vec(&body).unwrap());
    }

    fn zip_of(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            for (p, b) in files {
                z.start_file(p.as_str(), zip::write::SimpleFileOptions::default()).unwrap();
                z.write_all(b).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn unsigned_reads_as_unsigned() {
        let pkg = open_package(files("someone.theme", serde_json::json!({})), "1.0.0").unwrap();
        assert_eq!(pkg.trust, Trust::Unsigned);
        assert!(pkg.key.is_none());
    }

    #[test]
    fn a_valid_signature_is_signed_and_pinned() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let mut f = files("someone.theme", serde_json::json!({}));
        sign(&mut f, &key);
        let pkg = open_package(f, "1.0.0").unwrap();
        assert_eq!(pkg.trust, Trust::Signed);
        assert_eq!(pkg.key.unwrap(), key_fingerprint(&key.verifying_key().to_bytes()));
    }

    #[test]
    fn a_file_changed_after_signing_is_refused() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let mut f = files("someone.theme", serde_json::json!({}));
        sign(&mut f, &key);
        f.insert("view/index.html".into(), b"<script>evil()</script>".to_vec());
        let err = open_package(f, "1.0.0").unwrap_err().to_string();
        assert!(err.contains("changed after signing"), "{err}");
    }

    #[test]
    fn a_file_added_after_signing_is_refused() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let mut f = files("someone.theme", serde_json::json!({}));
        sign(&mut f, &key);
        f.insert("extra.js".into(), b"1".to_vec());
        assert!(open_package(f, "1.0.0").is_err());
    }

    #[test]
    fn reserved_ids_need_our_key() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let mut f = files("mxb.fake", serde_json::json!({}));
        sign(&mut f, &key);
        assert!(open_package(f, "1.0.0").unwrap_err().to_string().contains("reserved"));
    }

    #[test]
    fn reserved_name_lookalikes_need_our_key() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        for id in [
            "mxb-secure.x",
            "mxbsecure-official.x",
            "mxbsecur3.x",
            "frostn1.x",
        ] {
            let mut f = files(id, serde_json::json!({}));
            sign(&mut f, &key);
            assert!(
                open_package(f, "1.0.0")
                    .unwrap_err()
                    .to_string()
                    .contains("reserved"),
                "{id}"
            );
            let m = parsed_manifest(id, serde_json::json!({}));
            assert!(enforce_reserved_names(&m, Trust::Verified).is_ok(), "{id}");
        }
        for extra in [
            serde_json::json!({ "author": "MXB Secure Official" }),
            serde_json::json!({ "name": "mxbsecur3 Tools" }),
        ] {
            let mut f = files("someone.tools", extra.clone());
            sign(&mut f, &key);
            assert!(open_package(f, "1.0.0")
                .unwrap_err()
                .to_string()
                .contains("reserved"));
            assert!(enforce_reserved_names(
                &parsed_manifest("someone.tools", extra),
                Trust::Verified
            )
            .is_ok());
        }
        let mut ordinary = files(
            "someone.mxb-tools",
            serde_json::json!({ "author": "Mike B" }),
        );
        sign(&mut ordinary, &key);
        assert!(open_package(ordinary, "1.0.0").is_ok());
    }

    #[test]
    fn native_code_is_refused() {
        let mut f = files("someone.theme", serde_json::json!({}));
        f.insert("payload/frostmod.dlo".into(), vec![0x4d, 0x5a]);
        let err = read_zip(&zip_of(&f)).unwrap_err().to_string();
        assert!(err.contains("file type"), "{err}");
        let mut f = files("someone.theme", serde_json::json!({}));
        f.insert("run.exe".into(), vec![0x4d, 0x5a]);
        assert!(read_zip(&zip_of(&f)).is_err());
    }

    #[test]
    fn zip_slip_is_refused() {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            z.start_file("../../evil.js", zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(b"x").unwrap();
            z.finish().unwrap();
        }
        assert!(read_zip(&buf.into_inner()).unwrap_err().to_string().contains("unsafe path"));
    }

    #[test]
    fn unknown_permissions_and_bad_hosts_are_refused() {
        let f = files("someone.x", serde_json::json!({ "permissions": ["fs.write"] }));
        assert!(open_package(f, "1.0.0").unwrap_err().to_string().contains("unknown permission"));
        let f = files("someone.x", serde_json::json!({ "permissions": ["network"], "network": ["http://x.com"] }));
        assert!(open_package(f, "1.0.0").is_err());
        let f = files("someone.x", serde_json::json!({ "network": ["https://x.com"] }));
        assert!(open_package(f, "1.0.0").is_err(), "hosts without the permission");
        assert!(valid_origin("https://api.example.com"));
        assert!(valid_origin("https://api.example.com:8443"));
        assert!(!valid_origin("https://*.example.com"));
        assert!(!valid_origin("https://example.com/path"));
        assert!(!valid_origin("https://user@example.com"));
        assert!(!valid_origin("https://localhost"));
    }

    #[test]
    fn contributions_must_name_files_in_the_package() {
        let f = files("someone.x", serde_json::json!({ "contributes": { "views": [{ "id": "a", "title": "A", "entry": "missing.html" }] } }));
        assert!(open_package(f, "1.0.0").unwrap_err().to_string().contains("not in the package"));
        let f = files("someone.x", serde_json::json!({ "contributes": { "views": [{ "id": "a", "title": "A", "entry": "../x.html" }] } }));
        assert!(open_package(f, "1.0.0").is_err());
        let f = files("someone.x", serde_json::json!({ "contributes": { "views": [{ "id": "a", "title": "A", "entry": "view/index.html" }] } }));
        assert!(open_package(f, "1.0.0").is_ok());
    }

    #[test]
    fn ids_and_versions() {
        assert!(valid_id("frost.friends-online"));
        assert!(valid_id("a.b.c"));
        assert!(!valid_id("noseparator"));
        assert!(!valid_id("Upper.case"));
        assert!(!valid_id("a..b"));
        assert!(!valid_id("../x.y"));
        assert!(open_package(files("someone.x", serde_json::json!({ "version": "1.0" })), "1.0.0").is_err());
    }

    #[test]
    fn recognizes_catalog_download_urls() {
        let base = "https://api.example.com";
        assert_eq!(catalog_download_id("https://api.example.com/v1/extensions/someone.x/download", base), Some("someone.x".into()));
        assert_eq!(catalog_download_id("https://other.example.com/v1/extensions/someone.x/download", base), None);
        assert_eq!(catalog_download_id("https://api.example.com/v1/extensions/someone.x/download/extra", base), None);
        assert_eq!(catalog_download_id("https://api.example.com/v1/extensions/../download", base), None);
        assert_eq!(catalog_download_id("https://api.example.com/v1/extensions/not-an-id/download", base), None);
    }

    #[test]
    fn min_app_version_is_enforced() {
        let f = files("someone.x", serde_json::json!({ "minAppVersion": "9.0.0" }));
        assert!(open_package(f, "0.21.0").unwrap_err().to_string().contains("needs version"));
    }

    fn installed(key: Option<&str>) -> InstalledExt {
        InstalledExt {
            version: "1.0.0".into(),
            enabled: true,
            trust: if key.is_some() { Trust::Signed } else { Trust::Unsigned },
            key: key.map(String::from),
            publisher: None,
            granted: vec![],
            auto_update: true,
            pending_update: None,
            installed_at: 0,
            source: "file".into(),
        }
    }

    #[test]
    fn an_update_must_keep_its_key() {
        let a = SigningKey::from_bytes(&[1u8; 32]);
        let b = SigningKey::from_bytes(&[2u8; 32]);
        let mut fa = files("someone.x", serde_json::json!({}));
        sign(&mut fa, &a);
        let mut fb = files("someone.x", serde_json::json!({}));
        sign(&mut fb, &b);
        let pa = open_package(fa, "1.0.0").unwrap();
        let pb = open_package(fb, "1.0.0").unwrap();
        let inst = installed(pa.key.as_deref());
        assert!(may_replace(&inst, &pa, false).is_ok());
        assert!(may_replace(&inst, &pb, false).unwrap_err().to_string().contains("different key"));
        let unsigned = open_package(files("someone.x", serde_json::json!({})), "1.0.0").unwrap();
        assert!(may_replace(&inst, &unsigned, false).is_err());
        assert!(may_replace(&inst, &unsigned, true).is_ok(), "developer mode may overwrite");
    }

    #[test]
    fn catalog_installs_only_update_from_the_catalog() {
        assert_eq!(
            update_source("catalog", Some("https://author.example/latest.json")),
            Some(UpdateSource::Catalog)
        );
        assert_eq!(
            update_source("file", Some("https://author.example/latest.json")),
            Some(UpdateSource::Url(
                "https://author.example/latest.json".into()
            ))
        );
        assert_eq!(update_source("catalog", None), Some(UpdateSource::Catalog));
        assert_eq!(
            update_source("other", Some("https://author.example/latest.json")),
            None
        );
    }

    #[test]
    fn update_consent_covers_permissions_and_hosts() {
        let old = parsed_manifest(
            "someone.x",
            serde_json::json!({ "permissions": ["network", "mods.read"], "network": ["https://one.example.com"] }),
        );
        let granted = vec!["network".into(), "mods.read".into()];
        let new_permission = parsed_manifest(
            "someone.x",
            serde_json::json!({ "permissions": ["network", "mods.read", "coach.read"], "network": ["https://one.example.com"] }),
        );
        assert_eq!(
            needs_consent(&old, &granted, &new_permission),
            (vec!["coach.read".into()], vec![])
        );
        let new_host = parsed_manifest(
            "someone.x",
            serde_json::json!({ "permissions": ["network", "mods.read"], "network": ["https://one.example.com", "https://two.example.com"] }),
        );
        assert_eq!(
            needs_consent(&old, &granted, &new_host),
            (vec![], vec!["https://two.example.com".into()])
        );
        let removed_host = parsed_manifest(
            "someone.x",
            serde_json::json!({ "permissions": ["network", "mods.read"], "network": [] }),
        );
        assert_eq!(
            needs_consent(&old, &granted, &removed_host),
            (vec![], vec![])
        );
        let dropped_permissions =
            parsed_manifest("someone.x", serde_json::json!({ "permissions": [] }));
        assert_eq!(
            needs_consent(&old, &granted, &dropped_permissions),
            (vec![], vec![])
        );
    }

    #[test]
    fn older_versions_need_developer_mode() {
        assert!(downgrade_error("1.9.9", "2.0.0", false)
            .unwrap()
            .contains("older version (1.9.9)"));
        assert!(downgrade_error("1.9.9", "2.0.0", true).is_none());
        assert!(downgrade_error("2.0.0", "2.0.0", false).is_none());
        assert!(downgrade_error("2.0.1", "2.0.0", false).is_none());
    }

    #[test]
    fn catalog_packages_match_hash_version_and_key() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let mut f = files("someone.x", serde_json::json!({ "version": "2.0.0" }));
        sign(&mut f, &key);
        let bytes = zip_of(&f);
        let pkg = open_package(f, "1.0.0").unwrap();
        let listing = CatalogListing {
            version: "2.0.0".into(),
            download: "/v1/extensions/someone.x/download".into(),
            sha256: Some(sha256_hex(&bytes)),
            key: pkg.key.clone(),
        };
        assert!(catalog_package_matches(&listing, &bytes, &pkg));
        let mut wrong = listing.clone();
        wrong.version = "2.0.1".into();
        assert!(!catalog_package_matches(&wrong, &bytes, &pkg));
        let mut wrong = listing.clone();
        wrong.sha256 = Some("00".repeat(32));
        assert!(!catalog_package_matches(&wrong, &bytes, &pkg));
        let mut wrong = listing;
        wrong.key = Some("different".into());
        assert!(!catalog_package_matches(&wrong, &bytes, &pkg));
    }

    #[test]
    fn unsigned_extensions_suspend_without_developer_mode() {
        assert!(is_suspended(Trust::Unsigned, false));
        assert!(!is_suspended(Trust::Unsigned, true));
        assert!(!is_suspended(Trust::Signed, false));
        assert!(!is_suspended(Trust::Verified, false));
    }

    #[test]
    fn the_publisher_key_is_a_valid_key() {
        for (_, k) in TRUSTED_PUBLISHERS {
            let bytes: [u8; 32] = B64.decode(k).unwrap().try_into().unwrap();
            VerifyingKey::from_bytes(&bytes).unwrap();
        }
    }

    /// Signed by `scripts/extensions/pack.mjs`, so the JS signer and this verifier agree on
    /// the digest byte for byte.
    #[test]
    fn a_package_from_the_pack_script_verifies() {
        let bytes = include_bytes!("fixtures/ext-template-signed.mxbext");
        let pkg = open_package(read_zip(bytes).unwrap(), "1.0.0").unwrap();
        assert_eq!(pkg.trust, Trust::Signed);
        assert_eq!(pkg.manifest.id, "yourname.my-extension");
        assert_eq!(pkg.key.as_deref(), Some("baba70a6051dd514"));
    }

    #[test]
    fn coach_sessions_keep_times_and_drop_paths() {
        let index = serde_json::json!({
            "C:/Users/x/Documents/PiBoSo/MX Bikes/mxbcoach/sessions/a.mxbc": { "size": 1, "modified": 1, "summary": {
                "path": "C:/Users/x/a.mxbc", "started": "20261101-120000-000", "rider": "x", "trackName": "Club",
                "bikeName": "KTM 450", "category": "MX1", "eventType": 1, "bestMs": 95000,
                "laps": [
                    { "timeMs": 95000, "whole": true, "invalid": false },
                    { "timeMs": 90000, "whole": true, "invalid": false, "cut": true },
                    { "timeMs": 120000, "whole": false, "invalid": false }
                ] } },
            "b": { "summary": { "started": "20261105-090000-000", "trackName": "Dunes", "laps": [], "bestMs": null } }
        });
        let out = coach_sessions_from(&index, 10);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].track, "Dunes", "newest first");
        assert_eq!(out[1].clean_laps, vec![95000]);
        assert_eq!(out[1].best_ms, Some(95000));
        assert_eq!(out[1].event, "testing");
        let json = serde_json::to_string(&out).unwrap();
        assert!(!json.contains("Users"), "no paths: {json}");
    }

    #[test]
    fn catalog_links_only_point_back_at_the_catalog() {
        let mut v = serde_json::json!({ "extensions": [{
            "thumbnail": "/v1/extensions/a.b/screenshots/0",
            "download": "https://evil.example/x.mxbext",
            "screenshots": ["/v1/extensions/a.b/screenshots/0", "/v1/extensions/../../x", "//evil.example/y"],
            "manifest": { "thumbnail": "left alone" }
        }]});
        absolutize(&mut v, "https://api.example.com");
        let e = &v["extensions"][0];
        assert_eq!(e["thumbnail"], "https://api.example.com/v1/extensions/a.b/screenshots/0");
        assert!(e["download"].is_null());
        assert_eq!(e["screenshots"].as_array().unwrap().len(), 1);
        assert_eq!(e["manifest"]["thumbnail"], "left alone");
    }

    #[test]
    fn a_theme_logo_must_be_a_small_image_in_the_package() {
        let theme = |logo: &str| serde_json::json!({ "contributes": { "themes": [{ "id": "t", "label": "T", "brand": { "logo": logo } }] } });
        let mut f = files("someone.x", theme("logo.png"));
        assert!(open_package(f.clone(), "1.0.0").unwrap_err().to_string().contains("not in the package"));
        f.insert("logo.png".into(), vec![0u8; 1024]);
        assert!(open_package(f.clone(), "1.0.0").is_ok());
        f.insert("logo.png".into(), vec![0u8; 600 * 1024]);
        assert!(open_package(f, "1.0.0").unwrap_err().to_string().contains("512 KB"));
        let mut f = files("someone.x", theme("logo.svg"));
        f.insert("logo.svg".into(), b"<svg/>".to_vec());
        assert!(open_package(f, "1.0.0").is_err(), "svg logos are refused");
    }

    #[test]
    fn a_zip_round_trips() {
        let f = files("someone.x", serde_json::json!({}));
        let back = read_zip(&zip_of(&f)).unwrap();
        assert_eq!(back, f);
    }
}
