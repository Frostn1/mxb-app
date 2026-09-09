use crate::config::AppConfig;
use crate::install;
use crate::library::{self, LibraryEntry};
use crate::presets::{self, BundleRef, Loadout, Preset};
use crate::upload;
use anyhow::Context;
use futures_util::{StreamExt, TryStreamExt};
use serde::Serialize;
use std::io::Seek;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRef {
    pub slot: String,
    pub value: String,
    pub name: String,
    /// Destination path relative to `<MX Bikes>/mods` (forward slashes).
    pub rel_dest: String,
    pub abs_path: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedSlot {
    pub slot: String,
    pub value: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundlePlan {
    pub assets: Vec<AssetRef>,
    pub unresolved: Vec<UnresolvedSlot>,
    pub total_size: u64,
}

#[derive(Clone, Copy)]
enum Scan {
    Bikes,
    Rider,
    Tyres,
}

struct Spec {
    slot: &'static str,
    value: String,
    scan: Scan,
    cats: &'static [&'static str],
    owner: Owner,
}

/// Which installed thing a slot's file has to sit under.
///
/// A bike livery and a gear paint both name a file that lives inside something else, but they
/// differ in how much that containment can be trusted, so they get different rules rather than
/// one flag that has to be remembered at every call site.
enum Owner {
    /// Nothing to check — the value names the thing itself.
    Any,
    /// Prefer this owner, fall back to a match elsewhere. A rider model's folder name and the
    /// profile's value for it do not always agree, and refusing the paint over that would lose
    /// a livery the game itself finds.
    Prefer(String),
    /// Must be this owner. A bike livery belongs to one bike: `Race.pnt` under another bike is
    /// a different file with a different destination, so a near miss is worse than a miss.
    Require(String),
}

impl Owner {
    fn name(&self) -> Option<&str> {
        match self {
            Owner::Any => None,
            Owner::Prefer(p) | Owner::Require(p) => Some(p.trim()).filter(|p| !p.is_empty()),
        }
    }
}

fn strip_ext(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    for ext in [".pnt", ".pkz", ".zip"] {
        if lower.ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    name.to_string()
}

fn is_builtin(slot: &str, value: &str) -> bool {
    let v = value.to_ascii_lowercase();
    match slot {
        "helmet" | "boots" => v == "default",
        "protection" => v == "full" || v == "neck",
        "riding_style" => v == "mx" || v == "sm",
        "tyres" => v == "p_mx",
        _ => false,
    }
}

fn rel_dest(type_folder: &str, e: &LibraryEntry) -> String {
    let folder = e.folder.trim_matches('/');
    if folder.is_empty() {
        format!("{type_folder}/{}", e.name)
    } else {
        format!("{type_folder}/{folder}/{}", e.name)
    }
}

pub fn plan(cfg: &AppConfig, loadout: &Loadout) -> anyhow::Result<BundlePlan> {
    let mut p = resolve(cfg, loadout, None)?;
    dedup_assets(&mut p.assets);
    p.total_size = p.assets.iter().map(|a| a.size).sum();
    Ok(p)
}

/// Every bike in a profile, resolved for publishing, paying for the library walk once.
///
/// Resolving a slot means matching it against every installed file, and gathering those files
/// means a full recursive walk of `mods/bikes` — which on a real install is every livery the
/// player owns. One walk per loadout is fine for the one-at-a-time callers; it is not fine for
/// "publish this rider's whole profile", where the loadout count is the number of bikes they
/// have ever sat on. Same resolution, same order, one scan.
///
/// Two things differ from [`plan`], both because the publisher uploads file by file rather
/// than zipping a folder: each loadout's livery is pinned to the bike it was read under, and
/// assets stay individually addressed as they do in [`plan_detailed`] — collapsing a gear
/// paint into the model folder containing it would drop it from a publish entirely.
pub fn plan_profile(cfg: &AppConfig, loadouts: &[(String, Loadout)]) -> Vec<BundlePlan> {
    if loadouts.is_empty() {
        return Vec::new();
    }
    let libs = Libraries::scan(cfg);
    loadouts
        .iter()
        .map(|(bike, loadout)| resolve_with(cfg, &libs, loadout, Some(bike)))
        .collect()
}

/// The same resolution as [`plan`], with every asset still addressed in its own right.
///
/// [`plan`] collapses an asset into the folder that already contains it, because a zip that
/// carries `rider/helmets/AGV` carries the liveries under it for free. Manage needs the
/// opposite: it keeps that helmet by moving nothing at all, and decides livery by livery
/// which ones the game still gets to offer — so the paint has to be named, not implied.
pub fn plan_detailed(
    cfg: &AppConfig,
    loadout: &Loadout,
    bike: Option<&str>,
) -> anyhow::Result<BundlePlan> {
    resolve(cfg, loadout, bike)
}

/// The three scans a resolution reads from, gathered once.
///
/// Exists so [`plan_profile`] can hand the same walk to every loadout. A scan is infallible
/// from the caller's point of view — an unreadable folder resolves to nothing, exactly as it did
/// when each `resolve` did its own `unwrap_or_default`.
struct Libraries {
    bikes: Vec<LibraryEntry>,
    rider: Vec<LibraryEntry>,
    tyres: Vec<LibraryEntry>,
}

impl Libraries {
    fn scan(cfg: &AppConfig) -> Self {
        Libraries {
            bikes: library::scan_library(&cfg.mods_path, "mods/bikes", &[], cfg.game())
                .unwrap_or_default(),
            rider: library::scan_library(&cfg.mods_path, "mods/rider", &[], cfg.game())
                .unwrap_or_default(),
            tyres: library::scan_library(&cfg.mods_path, "mods/tyres", &[], cfg.game())
                .unwrap_or_default(),
        }
    }
}

fn resolve(cfg: &AppConfig, loadout: &Loadout, bike: Option<&str>) -> anyhow::Result<BundlePlan> {
    Ok(resolve_with(cfg, &Libraries::scan(cfg), loadout, bike))
}

/// `bike` is the bike id the loadout was read under, where the caller knows it. A preset does
/// not have one — it dresses whichever bike it is applied to — so the livery stays loose there.
fn resolve_with(
    cfg: &AppConfig,
    libs: &Libraries,
    loadout: &Loadout,
    bike: Option<&str>,
) -> BundlePlan {
    let Libraries { bikes, rider, tyres } = libs;

    let specs = vec![
        Spec { slot: "paint", value: loadout.paint.clone(), scan: Scan::Bikes, cats: &["bikePaint"], owner: bike.map_or(Owner::Any, |b| Owner::Require(b.to_string())) },
        Spec { slot: "helmet", value: loadout.helmet.clone(), scan: Scan::Rider, cats: &["helmet"], owner: Owner::Any },
        Spec { slot: "helmet_paint", value: loadout.helmet_paint.clone(), scan: Scan::Rider, cats: &["helmetPaint"], owner: Owner::Prefer(loadout.helmet.clone()) },
        Spec { slot: "goggles_paint", value: loadout.goggles_paint.clone(), scan: Scan::Rider, cats: &["goggles"], owner: Owner::Prefer(loadout.helmet.clone()) },
        Spec { slot: "suit_paint", value: loadout.suit_paint.clone(), scan: Scan::Rider, cats: &["outfit"], owner: Owner::Prefer(loadout.rider.clone()) },
        Spec { slot: "gloves_paint", value: loadout.gloves_paint.clone(), scan: Scan::Rider, cats: &["gloves"], owner: Owner::Any },
        Spec { slot: "boots", value: loadout.boots.clone(), scan: Scan::Rider, cats: &["boots"], owner: Owner::Any },
        Spec { slot: "boots_paint", value: loadout.boots_paint.clone(), scan: Scan::Rider, cats: &["bootPaint"], owner: Owner::Prefer(loadout.boots.clone()) },
        Spec { slot: "protection", value: loadout.protection.clone(), scan: Scan::Rider, cats: &["protection"], owner: Owner::Any },
        Spec { slot: "protection_paint", value: loadout.protection_paint.clone(), scan: Scan::Rider, cats: &["protectionPaint"], owner: Owner::Prefer(loadout.protection.clone()) },
        // A custom riding style is a mod like any other. The two stock ones live in
        // `rider.pkz` and leave nothing on disk, which `is_builtin` skips rather than
        // reporting unresolved.
        Spec { slot: "riding_style", value: loadout.riding_style.clone(), scan: Scan::Rider, cats: &["animation"], owner: Owner::Any },
        Spec { slot: "tyres", value: loadout.tyres.clone(), scan: Scan::Tyres, cats: &["misc"], owner: Owner::Any },
    ];

    let mut assets: Vec<AssetRef> = Vec::new();
    let mut unresolved: Vec<UnresolvedSlot> = Vec::new();

    for spec in &specs {
        let value = spec.value.trim();
        if value.is_empty() || is_builtin(spec.slot, value) {
            continue;
        }
        let (entries, type_folder) = match spec.scan {
            Scan::Bikes => (bikes, "bikes"),
            Scan::Rider => (rider, "rider"),
            Scan::Tyres => (tyres, "tyres"),
        };

        let mut matches: Vec<&LibraryEntry> = entries
            .iter()
            .filter(|e| {
                spec.cats.contains(&e.category.as_str())
                    && strip_ext(&e.name).eq_ignore_ascii_case(value)
            })
            .collect();

        if let Some(owner) = spec.owner.name() {
            let under_owner = |e: &LibraryEntry| {
                e.parent.as_deref().map(|p| p.eq_ignore_ascii_case(owner)).unwrap_or(false)
            };
            // `Require` keeps the filter even when it empties the list. Reporting the slot
            // unresolved is the honest answer there; falling back would hand back another
            // bike's livery, which also carries that bike's destination path.
            if matches!(spec.owner, Owner::Require(_)) || matches.iter().any(|e| under_owner(e)) {
                matches.retain(|e| under_owner(e));
            }
        }

        if matches.is_empty() {
            unresolved.push(UnresolvedSlot {
                slot: spec.slot.to_string(),
                value: value.to_string(),
                reason: "not installed — can't be bundled".to_string(),
            });
            continue;
        }
        for e in matches {
            assets.push(AssetRef {
                slot: spec.slot.to_string(),
                value: value.to_string(),
                name: e.name.clone(),
                rel_dest: rel_dest(type_folder, e),
                abs_path: e.path.clone(),
                size: e.size,
                is_dir: e.kind == "folder",
            });
        }
    }

    resolve_model_swap(cfg, loadout, &mut assets, &mut unresolved);

    for (slot, value) in [("bike_font", &loadout.bike_font), ("suit_font", &loadout.suit_font)] {
        let v = value.trim();
        if !v.is_empty() && !v.eq_ignore_ascii_case("default_black") && !v.eq_ignore_ascii_case("default_white") {
            unresolved.push(UnresolvedSlot {
                slot: slot.to_string(),
                value: v.to_string(),
                reason: "custom font — bundle it manually if needed".to_string(),
            });
        }
    }

    let total_size = assets.iter().map(|a| a.size).sum();
    BundlePlan { assets, unresolved, total_size }
}

fn resolve_model_swap(
    cfg: &AppConfig,
    loadout: &Loadout,
    assets: &mut Vec<AssetRef>,
    unresolved: &mut Vec<UnresolvedSlot>,
) {
    let value = loadout.model_swap.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("Original") {
        return;
    }
    let bikes_root = library::mods_subdir(&cfg.mods_path, "mods/bikes");
    let mut found = false;
    if let Ok(rd) = std::fs::read_dir(&bikes_root) {
        for e in rd.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let bike = e.file_name().to_string_lossy().into_owned();
            let variant = e.path().join("FrostMod Models").join(value);
            if variant.is_dir() {
                assets.push(AssetRef {
                    slot: "model_swap".to_string(),
                    value: value.to_string(),
                    name: value.to_string(),
                    rel_dest: format!("bikes/{bike}/FrostMod Models/{value}"),
                    abs_path: variant.to_string_lossy().into_owned(),
                    size: dir_size_deep(&variant),
                    is_dir: true,
                });
                found = true;
            }
        }
    }
    if !found {
        unresolved.push(UnresolvedSlot {
            slot: "model_swap".to_string(),
            value: value.to_string(),
            reason: "model variant not parked in the library (it may be the active model)".to_string(),
        });
    }
}

fn dedup_assets(assets: &mut Vec<AssetRef>) {
    let dirs: Vec<String> = assets
        .iter()
        .filter(|a| a.is_dir)
        .map(|a| a.rel_dest.trim_end_matches('/').to_string())
        .collect();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    assets.retain(|a| {
        if !seen.insert(a.rel_dest.clone()) {
            return false;
        }
        !dirs.iter().any(|d| {
            a.rel_dest != *d && a.rel_dest.starts_with(&format!("{d}/"))
        })
    });
}

/// Total bytes under `dir`, following the links a mods tree is full of. Shared with
/// [`crate::fileshare`], which sizes a picked folder the same way this sizes a model variant.
pub(crate) fn dir_size_deep(dir: &Path) -> u64 {
    let mut total = 0;
    for e in crate::linkwalk::walk(dir).into_iter().flatten() {
        if e.file_type().is_file() {
            total += e.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    total
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BundleProgress {
    phase: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

pub const BUNDLE_SLUG: &str = "__preset_bundle__";

pub const BUNDLE_EVENT: &str = "preset-bundle-progress";

/// Emit one phase update on `event`. The event name is a parameter because the same
/// create/download machinery serves two flows — the preset bundle here and the file share
/// in [`crate::fileshare`] — and each has its own dialog listening.
pub(crate) fn emit(app: &AppHandle, event: &str, phase: &'static str, message: Option<String>) {
    let _ = app.emit(event, BundleProgress { phase, message });
}

fn phase(app: &AppHandle, phase: &'static str, message: Option<String>) {
    emit(app, BUNDLE_EVENT, phase, message);
}

pub async fn create(
    app: &AppHandle,
    cfg: &AppConfig,
    presets_dir: &Path,
    name: &str,
) -> anyhow::Result<String> {
    let mut preset = presets::find_preset(presets_dir, name)
        .ok_or_else(|| anyhow::anyhow!("no preset named '{name}'"))?;

    phase(app, "bundling", None);
    let plan = plan(cfg, &preset.loadout)?;
    if plan.assets.is_empty() {
        anyhow::bail!(
            "This preset has no installed assets to bundle — share the plain code instead."
        );
    }

    let work = std::env::temp_dir().join(format!("mxb-bundle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;

    // Library to zip in one pass. The assets used to be copied into a staging tree and read
    // back out of it, which moved every byte of the bundle twice before it went anywhere.
    let mut entries: Vec<ZipEntry> = Vec::new();
    for a in &plan.assets {
        entries.extend(entries_under(&format!("mods/{}", a.rel_dest), Path::new(&a.abs_path)));
    }

    let mut meta = preset.clone();
    meta.bundle = None;
    let meta_path = work.join("preset.json");
    std::fs::write(&meta_path, serde_json::to_vec_pretty(&meta)?)?;
    entries.push(ZipEntry { rel: "preset.json".to_string(), src: meta_path });

    let zip_path = work.join(format!("{}.zip", sanitize_file(name)));
    zip_entries(&entries, &zip_path)?;

    let total = human_size(file_size(&zip_path));
    phase(app, "uploading", Some(format!("Uploading {total}…")));
    let client = install::build_client()?;
    let up = upload::upload_file(&client, &zip_path, |i, n| {
        let msg = if n > 1 {
            format!("Uploading part {i} of {n} ({total})…")
        } else {
            format!("Uploading {total}…")
        };
        phase(app, "uploading", Some(msg));
    })
    .await?;

    let _ = std::fs::remove_dir_all(&work);

    let first = up
        .parts
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("the upload returned no link"))?;
    // `url` stays the first slice so a one-part bundle reads exactly as it always has;
    // `parts` is only carried when there's more than one to stitch.
    let multi = up.parts.len() > 1;
    let parts = if multi { up.parts } else { Vec::new() };
    let part_sizes = if multi { up.part_sizes } else { Vec::new() };
    preset.bundle =
        Some(BundleRef { url: first, host: up.host, size: up.size, parts, part_sizes });
    let code = presets::encode_code_public(&preset);
    phase(app, "done", None);
    Ok(code)
}

pub async fn import(
    app: &AppHandle,
    cfg: &AppConfig,
    presets_dir: &Path,
    text: &str,
) -> anyhow::Result<Preset> {
    let preset = presets::decode_code(text)?;
    let bundle = preset
        .bundle
        .clone()
        .ok_or_else(|| anyhow::anyhow!("This code has no asset bundle — use plain Import."))?;

    let work = scratch_dir(cfg, "bundle-import");

    let fetched = fetch(app, BUNDLE_EVENT, BUNDLE_SLUG, &bundle, &work).await?;

    phase(app, "installing", None);
    let extracted = work.join("extracted");
    std::fs::create_dir_all(&extracted)?;
    fetched.extract(&extracted)?;
    let mods_dir = library::mods_subdir(&cfg.mods_path, "mods");
    // Anything the receiver already has wins: a bundle ships whole asset folders, so
    // overwriting would swap their helmet mesh and their liveries for the sender's.
    install::place_mod_with(
        &extracted,
        &mods_dir,
        "bikes",
        "",
        BUNDLE_SLUG,
        install::OnConflict::Keep,
        // Staged under our own `work`, deleted at the end of this function. Files the
        // receiver already has are skipped and simply go with it.
        install::Staging::Consume,
    )?;

    presets::save_preset(presets_dir, preset.clone())?;

    let _ = std::fs::remove_dir_all(&work);
    install::notify_frostmod(app, BUNDLE_SLUG);
    phase(app, "done", None);

    Ok(preset)
}

/// A bundle as it came down: one file, or the slices it arrived in.
pub(crate) enum Fetched {
    File(PathBuf),
    Parts(Vec<PathBuf>),
}

impl Fetched {
    /// Unpack into `dest`.
    pub(crate) fn extract(&self, dest: &Path) -> anyhow::Result<()> {
        match self {
            Fetched::File(p) => install::extract_archive(p, dest),
            // Read where they lie. Joining the slices into one file first wrote the whole
            // bundle to disk a second time and read it back a third, which on a track is a
            // minute of pure copying for a file we are about to unpack anyway.
            Fetched::Parts(parts) => install::extract_zip_from(
                std::io::BufReader::with_capacity(ZIP_BUF, PartsReader::open(parts)?),
                dest,
                Path::new("bundle.zip"),
            ),
        }
    }
}

/// Bring a hosted bundle down to `work`, whatever shape it was uploaded in: a MEGA link that
/// decrypts in-app, a sliced upload, or a plain single file. Shared with
/// [`crate::fileshare`], which hosts its payload the same way.
pub(crate) async fn fetch(
    app: &AppHandle,
    event: &str,
    slug: &str,
    bundle: &BundleRef,
    work: &Path,
) -> anyhow::Result<Fetched> {
    emit(app, event, "downloading", None);
    let client = install::build_client()?;
    let h = bundle.host.to_lowercase();
    let u = bundle.url.to_lowercase();
    if h.contains("mega") || u.contains("mega.nz") || u.contains("mega.co") {
        Ok(Fetched::File(install::download_mega(app, &client, slug, &bundle.url, work).await?))
    } else if bundle.parts.len() > 1 {
        download_parts(app, event, slug, &client, bundle, work).await
    } else {
        let direct = install::resolve_direct_url(&client, &bundle.url, &bundle.host).await?;
        Ok(Fetched::File(install::download(app, &client, slug, &direct, work).await?))
    }
}

/// A read over a bundle's slices as if they were the one file they add up to.
///
/// The slices are raw byte ranges of the zip, in order, so reading them end to end *is*
/// reading the zip — there is nothing to join.
pub(crate) struct PartsReader {
    files: Vec<std::fs::File>,
    /// Where each part starts within the whole, with the total on the end.
    bounds: Vec<u64>,
    pos: u64,
}

impl PartsReader {
    pub(crate) fn open(paths: &[PathBuf]) -> anyhow::Result<Self> {
        let mut files = Vec::with_capacity(paths.len());
        let mut bounds = vec![0u64];
        for p in paths {
            let f = std::fs::File::open(p)
                .with_context(|| format!("reading {}", p.display()))?;
            let len = f.metadata().map(|m| m.len()).unwrap_or(0);
            bounds.push(bounds[bounds.len() - 1] + len);
            files.push(f);
        }
        Ok(Self { files, bounds, pos: 0 })
    }

    fn len(&self) -> u64 {
        self.bounds.last().copied().unwrap_or(0)
    }

    /// The part `at` falls in. Parts are few — a 2 GB ceiling over 24 MB slices — so a scan
    /// is cheaper than anything cleverer.
    fn part_at(&self, at: u64) -> Option<usize> {
        (0..self.files.len()).find(|&i| at < self.bounds[i + 1])
    }
}

impl std::io::Read for PartsReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() || self.pos >= self.len() {
            return Ok(0);
        }
        let Some(i) = self.part_at(self.pos) else {
            return Ok(0);
        };
        let start = self.bounds[i];
        // Stops at the part boundary; the next call picks up in the one after it.
        let room = (self.bounds[i + 1] - self.pos) as usize;
        let take = buf.len().min(room);
        let f = &mut self.files[i];
        f.seek(std::io::SeekFrom::Start(self.pos - start))?;
        let n = std::io::Read::read(f, &mut buf[..take])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl std::io::Seek for PartsReader {
    fn seek(&mut self, from: std::io::SeekFrom) -> std::io::Result<u64> {
        let end = self.len() as i64;
        let to = match from {
            std::io::SeekFrom::Start(n) => n as i64,
            std::io::SeekFrom::End(n) => end + n,
            std::io::SeekFrom::Current(n) => self.pos as i64 + n,
        };
        if to < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek before the start of the bundle",
            ));
        }
        self.pos = to as u64;
        Ok(self.pos)
    }
}

/// Tries per slice when the one that arrives is not the size the code says it should be.
/// Two, not more: a host that is only holding half a file will keep saying so, and the point
/// of retrying at all is to ride out the case where the transfer, not the upload, was short.
const PART_ATTEMPTS: u32 = 2;

/// How many slices come down at once.
///
/// One at a time left the link half idle for the length of a bundle, and the slices are
/// independent files on the same host — the same reason the upload side sends three
/// (`upload::UPLOAD_CONCURRENCY`). They share one progress bar, so what the player sees is
/// still the whole download filling once.
const PART_CONCURRENCY: usize = 3;

/// Fetch every slice of a multi-part bundle. The slices are raw byte ranges of the zip, so
/// reading them in order reproduces the original file exactly — see [`PartsReader`].
async fn download_parts(
    app: &AppHandle,
    event: &str,
    slug: &str,
    client: &reqwest::Client,
    bundle: &BundleRef,
    work: &Path,
) -> anyhow::Result<Fetched> {
    let n = bundle.parts.len();
    let dir = work.join("parts");
    let shared = install::SharedProgress::new(n, Some(bundle.size));
    let done = std::sync::atomic::AtomicUsize::new(0);
    emit(app, event, "downloading", Some(format!("Downloading {n} parts…")));

    let jobs: Vec<_> = bundle
        .parts
        .iter()
        .enumerate()
        .map(|(i, url)| {
            // Each part lands in its own folder: the host names the file, and two parts of the
            // same bundle can easily come back under the same name.
            let into = dir.join(format!("part{}", i + 1));
            let (shared, done) = (&shared, &done);
            async move {
                let path =
                    download_part(app, event, slug, client, bundle, i, url, &into, shared).await?;
                let k = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                emit(app, event, "downloading", Some(format!("Downloaded {k} of {n} parts…")));
                Ok::<PathBuf, anyhow::Error>(path)
            }
        })
        .collect();

    let paths: Vec<PathBuf> = futures_util::stream::iter(jobs)
        .buffered(PART_CONCURRENCY)
        .try_collect()
        .await?;

    // The whole has to add up even when no single part could be checked — a code made before
    // the app recorded part sizes carries none, and a host that won't answer a range request
    // leaves nothing to compare a slice against.
    let got: u64 = paths.iter().map(|p| file_size(p)).sum();
    if got != bundle.size {
        anyhow::bail!(
            "This bundle came back as {} instead of {} — one of its {n} parts is incomplete or \
             no longer hosted. Ask whoever shared it for a fresh code.",
            human_size(got),
            human_size(bundle.size)
        );
    }
    Ok(Fetched::Parts(paths))
}

/// One slice, retried when it comes back shorter than it should be.
#[allow(clippy::too_many_arguments)]
async fn download_part(
    app: &AppHandle,
    event: &str,
    slug: &str,
    client: &reqwest::Client,
    bundle: &BundleRef,
    i: usize,
    url: &str,
    into: &Path,
    shared: &install::SharedProgress,
) -> anyhow::Result<PathBuf> {
    let n = bundle.parts.len();
    let direct = install::resolve_direct_url(client, url, &bundle.host).await?;
    // What this slice should weigh. A code made before the app recorded part sizes has none,
    // and then a part that came back short only surfaced when the joined file did not add up
    // — by which point nothing knew which part it was and the only advice left was to ask for
    // a fresh code. Falling back to what the host says it is holding puts those codes back
    // inside the same retry.
    let expect = match bundle.part_sizes.get(i).copied() {
        Some(want) => Some(want),
        None => crate::upload::hosted_len(client, &direct).await,
    };

    for attempt in 1..=PART_ATTEMPTS {
        let _ = std::fs::remove_dir_all(into);
        std::fs::create_dir_all(into)?;
        let path = install::download_into(app, client, slug, &direct, into, Some((shared, i)))
            .await
            .with_context(|| format!("part {} of {n} couldn't be downloaded", i + 1))?;
        let have = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        match expect {
            Some(want) if have != want => {
                if attempt == PART_ATTEMPTS {
                    anyhow::bail!(
                        "Part {} of {n} is {} where it should be {} — the host is only \
                         holding some of it. Ask whoever shared it for a fresh code.",
                        i + 1,
                        human_size(have),
                        human_size(want)
                    )
                }
                emit(
                    app,
                    event,
                    "downloading",
                    Some(format!("Part {} of {n} came back short — retrying…", i + 1)),
                );
            }
            _ => return Ok(path),
        }
    }
    unreachable!("the loop returns a path or bails on its last attempt")
}

/// A scratch folder on the same volume as the mods tree.
///
/// What an import unpacks is *moved* into place when the two sit on one drive and copied
/// byte by byte when they don't — see [`install::Staging`]. `%TEMP%` is on the system drive
/// and a player's mods folder very often isn't, which quietly turned the last step of every
/// import into a full copy of the track. Falls back to the temp folder when the mods tree
/// won't take a folder of ours.
pub(crate) fn scratch_dir(cfg: &AppConfig, name: &str) -> PathBuf {
    let leaf = format!(".mxb-{name}-{}", std::process::id());
    let root = library::mods_root(&cfg.mods_path);
    if let Some(near) = root.is_dir().then(|| root.parent()).flatten().map(|p| p.join(&leaf)) {
        let _ = std::fs::remove_dir_all(&near);
        if std::fs::create_dir_all(&near).is_ok() {
            return near;
        }
    }
    let fallback = std::env::temp_dir().join(leaf);
    let _ = std::fs::remove_dir_all(&fallback);
    let _ = std::fs::create_dir_all(&fallback);
    fallback
}

/// One file on its way into an archive: where it is now, and the name it takes inside.
///
/// Naming the source rather than staging a copy of it is the point. A share used to be
/// written out twice — once into a temp tree, once into the zip — and the zip is `Stored`,
/// so the first pass moved every byte of a track for no gain.
pub(crate) struct ZipEntry {
    pub rel: String,
    pub src: PathBuf,
}

/// Every file under `src`, named inside the archive beneath `rel`.
///
/// Links are followed, through [`crate::linkwalk`]: an archive is for someone else's machine,
/// where the far end of the sender's junction doesn't exist.
pub(crate) fn entries_under(rel: &str, src: &Path) -> Vec<ZipEntry> {
    let base = rel.trim_matches('/').to_string();
    // Anything that isn't a folder is one entry, missing files included: a source that has
    // gone away has to fail the zip, the way copying it used to, rather than walk to nothing.
    if !src.is_dir() {
        return vec![ZipEntry { rel: base, src: src.to_path_buf() }];
    }
    let mut out: Vec<ZipEntry> = crate::linkwalk::walk(src)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            let inner = e
                .path()
                .strip_prefix(src)
                .unwrap_or(e.path())
                .to_string_lossy()
                .replace('\\', "/");
            let rel = match (base.as_str(), inner.as_str()) {
                ("", _) => inner.clone(),
                (b, "") => b.to_string(),
                (b, i) => format!("{b}/{i}"),
            };
            ZipEntry { rel, src: e.path().to_path_buf() }
        })
        .collect();
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    out
}

/// Read and write in megabyte bites. The old code read each file whole, so a 400 MB track
/// meant a 400 MB allocation, and wrote to an unbuffered handle.
const ZIP_BUF: usize = 1024 * 1024;

/// Write `entries` into a `Stored` zip, streaming each source file straight in.
pub(crate) fn zip_entries(entries: &[ZipEntry], zip_path: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::create(zip_path)?;
    let mut zip = zip::ZipWriter::new(std::io::BufWriter::with_capacity(ZIP_BUF, file));
    // Stored (no re-compression): payload is mostly already-compressed `.pkz`/`.pnt`.
    let opts: zip::write::SimpleFileOptions =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let mut buf = vec![0u8; ZIP_BUF];
    for e in entries {
        if e.rel.is_empty() {
            continue;
        }
        let mut input = std::fs::File::open(&e.src)
            .with_context(|| format!("reading {}", e.src.display()))?;
        let len = input.metadata().map(|m| m.len()).unwrap_or(0);
        // Zip64 past 4 GB: without the flag the entry's size field wraps and the archive
        // reads back as garbage on the far end.
        zip.start_file(&e.rel, opts.large_file(len >= u64::from(u32::MAX)))?;
        loop {
            let n = match std::io::Read::read(&mut input, &mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(err) => {
                    return Err(err).with_context(|| format!("reading {}", e.src.display()))
                }
            };
            std::io::Write::write_all(&mut zip, &buf[..n])?;
        }
    }
    std::io::Write::flush(&mut zip.finish()?)?;
    Ok(())
}

/// A whole folder, named inside the zip by each file's path within it. Only the tests build
/// a tree first; the real callers hand over a list and skip that copy.
#[cfg(test)]
pub(crate) fn zip_dir(root: &Path, zip_path: &Path) -> anyhow::Result<()> {
    zip_entries(&entries_under("", root), zip_path)
}

pub(crate) fn file_size(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

pub(crate) fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

pub(crate) fn sanitize_file(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();
    let t = s.trim();
    if t.is_empty() { "preset-bundle".to_string() } else { t.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mxb-bundle-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn slices(dir: &Path, bytes: &[u8], each: usize) -> Vec<PathBuf> {
        bytes
            .chunks(each)
            .enumerate()
            .map(|(i, chunk)| {
                let p = dir.join(format!("part{i}"));
                std::fs::write(&p, chunk).unwrap();
                p
            })
            .collect()
    }

    /// The whole multi-part scheme rests on this: slices are raw byte ranges, so reading
    /// them in order has to give the original zip byte for byte.
    #[test]
    fn reading_slices_in_order_rebuilds_the_zip() {
        let dir = tmp("parts-read");
        let original: Vec<u8> = (0..9000u32).map(|i| (i % 251) as u8).collect();
        let paths = slices(&dir, &original, 2048);
        assert_eq!(paths.len(), 5, "expected a short final slice");

        let mut got = Vec::new();
        let mut r = PartsReader::open(&paths).unwrap();
        // Through a small buffer on purpose: a read has to stop at the slice boundary and
        // the next one pick up in the slice after it.
        std::io::copy(&mut std::io::BufReader::with_capacity(64, &mut r), &mut got).unwrap();

        assert_eq!(got, original);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `zip` reads its central directory from the end and then seeks back to each entry, so
    /// a reader that could only go forwards would unpack nothing.
    #[test]
    fn slices_can_be_read_out_of_order() {
        let dir = tmp("parts-seek");
        let original: Vec<u8> = (0..9000u32).map(|i| (i % 251) as u8).collect();
        let mut r = PartsReader::open(&slices(&dir, &original, 2048)).unwrap();

        assert_eq!(r.seek(std::io::SeekFrom::End(0)).unwrap(), 9000);
        for at in [8999u64, 0, 2047, 2048, 4100] {
            r.seek(std::io::SeekFrom::Start(at)).unwrap();
            let mut one = [0u8; 1];
            r.read_exact(&mut one).unwrap();
            assert_eq!(one[0], original[at as usize], "byte {at}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_slice_fails_instead_of_truncating() {
        let dir = tmp("parts-missing");
        let present = dir.join("part0");
        std::fs::write(&present, b"abc").unwrap();
        let paths = vec![present, dir.join("part1-never-downloaded")];

        assert!(PartsReader::open(&paths).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The staging copy used to decide where a file sat inside the archive by putting it
    /// there. Nothing is copied now, so the naming is this function's job alone.
    #[test]
    fn a_picked_folder_keeps_its_shape_inside_the_archive() {
        let root = tmp("entries");
        touch(&root.join("MyTrack/track.pkz"));
        touch(&root.join("MyTrack/maps/ground.tga"));

        let rels: Vec<String> = entries_under("mods/tracks/EU/MyTrack", &root.join("MyTrack"))
            .into_iter()
            .map(|e| e.rel)
            .collect();
        assert_eq!(
            rels,
            ["mods/tracks/EU/MyTrack/maps/ground.tga", "mods/tracks/EU/MyTrack/track.pkz"]
        );

        // A single file is named exactly what it was given, folder or not.
        let one = entries_under("mods/tracks/RedBud.pkz", &root.join("MyTrack/track.pkz"));
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].rel, "mods/tracks/RedBud.pkz");

        // And a source that has gone away is still an entry, so the zip fails on it rather
        // than quietly shipping without it.
        let gone = entries_under("mods/tracks/Gone.pkz", &root.join("nothing-here"));
        assert_eq!(gone.len(), 1);
        assert!(zip_entries(&gone, &root.join("out.zip")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// What the staging copy cost, measured rather than assumed.
    ///
    /// `fs::copy` on APFS clones the file and reports a few milliseconds for any size, which
    /// is not what Windows does — so the "before" arm copies byte by byte, which is what the
    /// player's machine actually did.
    ///
    /// ```text
    /// cargo test -- --ignored --nocapture bench_packing
    /// ```
    #[test]
    #[ignore = "writes a few hundred MB and times it"]
    fn bench_packing() {
        use std::time::Instant;
        let root = tmp("bench");
        let src = root.join("track/RedBud.pkz");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        // Incompressible, like a real `.pkz`, and big enough to see.
        let mut seed = 0x2545F4914F6CDD1Du64;
        let mut blob = vec![0u8; 256 * 1024 * 1024];
        for c in blob.chunks_mut(8) {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            c.copy_from_slice(&seed.to_le_bytes()[..c.len()]);
        }
        std::fs::write(&src, &blob).unwrap();
        drop(blob);

        // Before: copy into a staging tree, then read it back whole into the zip.
        let t = Instant::now();
        let staged = root.join("staged/mods/tracks");
        std::fs::create_dir_all(&staged).unwrap();
        {
            let mut i = std::fs::File::open(&src).unwrap();
            let mut o = std::fs::File::create(staged.join("RedBud.pkz")).unwrap();
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = i.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                std::io::Write::write_all(&mut o, &buf[..n]).unwrap();
            }
            std::io::Write::flush(&mut o).unwrap();
        }
        {
            let f = std::fs::File::create(root.join("before.zip")).unwrap();
            let mut zip = zip::ZipWriter::new(f);
            let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("mods/tracks/RedBud.pkz", opts).unwrap();
            let bytes = std::fs::read(staged.join("RedBud.pkz")).unwrap();
            std::io::Write::write_all(&mut zip, &bytes).unwrap();
            zip.finish().unwrap();
        }
        let before = t.elapsed();

        // After: straight from the mods tree into the zip.
        let t = Instant::now();
        zip_entries(
            &entries_under("mods/tracks/RedBud.pkz", &src),
            &root.join("after.zip"),
        )
        .unwrap();
        let after = t.elapsed();

        assert_eq!(
            file_size(&root.join("before.zip")),
            file_size(&root.join("after.zip")),
            "the two archives have to weigh the same"
        );
        println!(
            "pack 256 MB: staged {:?}, direct {:?} ({:.1}x)",
            before,
            after,
            before.as_secs_f64() / after.as_secs_f64()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn plan_resolves_slots_to_rel_dests() {
        let root = tmp("plan");
        touch(&root.join("mods/bikes/KTM450/paints/RedBud.pnt"));
        touch(&root.join("mods/rider/helmets/AGV/model.edf"));
        touch(&root.join("mods/rider/helmets/AGV/paints/Blue.pnt"));
        touch(&root.join("mods/tyres/oem_mx.pkz"));

        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut lo = Loadout::default();
        lo.paint = "RedBud".into();
        lo.helmet = "AGV".into();
        lo.helmet_paint = "Blue".into();
        lo.tyres = "oem_mx".into();
        lo.suit_font = "MyFont".into(); // free text → unresolved

        let plan = plan(&cfg, &lo).unwrap();
        let dest = |slot: &str| plan.assets.iter().find(|a| a.slot == slot).map(|a| a.rel_dest.clone());
        assert_eq!(dest("paint").as_deref(), Some("bikes/KTM450/paints/RedBud.pnt"));
        assert_eq!(dest("helmet").as_deref(), Some("rider/helmets/AGV"));
        assert_eq!(dest("tyres").as_deref(), Some("tyres/oem_mx.pkz"));
        assert!(dest("helmet_paint").is_none());
        assert!(plan.unresolved.iter().any(|u| u.slot == "suit_font"));
        let _ = std::fs::remove_dir_all(&root);
    }

    // Publishing a rider's whole profile plans every bike they own. `plan_profile` exists to
    // make that one library walk instead of one per bike, so the thing worth pinning is that
    // it still answers exactly what planning them separately would have.
    #[test]
    fn planning_many_at_once_answers_the_same_as_planning_each() {
        let root = tmp("plan-many");
        touch(&root.join("mods/bikes/KTM450/paints/RedBud.pnt"));
        touch(&root.join("mods/bikes/YZ250/paints/Southwick.pnt"));
        touch(&root.join("mods/rider/helmets/AGV/model.edf"));

        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut ktm = Loadout::default();
        ktm.paint = "RedBud".into();
        ktm.helmet = "AGV".into();
        let mut yam = Loadout::default();
        yam.paint = "Southwick".into();

        let loadouts =
            vec![("KTM450".to_string(), ktm.clone()), ("YZ250".to_string(), yam.clone())];
        let many = plan_profile(&cfg, &loadouts);
        assert_eq!(many.len(), 2);
        let each = [
            plan_detailed(&cfg, &ktm, Some("KTM450")).unwrap(),
            plan_detailed(&cfg, &yam, Some("YZ250")).unwrap(),
        ];
        for (batched, one) in many.iter().zip(each) {
            let dests = |p: &BundlePlan| {
                p.assets.iter().map(|a| a.rel_dest.clone()).collect::<Vec<_>>()
            };
            assert_eq!(dests(batched), dests(&one));
            assert_eq!(batched.total_size, one.total_size);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    // Livery names repeat across bikes — `default`, `race`, a team name. Matching on the
    // filename alone published whichever one the walk reached first, and `rel_dest` carries
    // that bike's folder, so the receiver installed it onto the wrong bike too.
    #[test]
    fn a_livery_resolves_under_the_bike_that_wears_it() {
        let root = tmp("livery-owner");
        touch(&root.join("mods/bikes/KTM450/paints/Race.pnt"));
        touch(&root.join("mods/bikes/YZ250/paints/Race.pnt"));

        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut lo = Loadout::default();
        lo.paint = "Race".into();

        for bike in ["YZ250", "KTM450"] {
            let plans = plan_profile(&cfg, &[(bike.to_string(), lo.clone())]);
            let paints = plans[0]
                .assets
                .iter()
                .filter(|a| a.slot == "paint")
                .map(|a| a.rel_dest.as_str())
                .collect::<Vec<_>>();
            assert_eq!(paints, vec![format!("bikes/{bike}/paints/Race.pnt")]);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    // The other half of that rule: when the owning bike has no such livery, the answer is
    // "unresolved", not "here is someone else's". Unlike a rider model — whose folder name and
    // profile value genuinely disagree sometimes — a bike livery has one correct home.
    #[test]
    fn a_livery_missing_from_its_own_bike_is_never_borrowed() {
        let root = tmp("livery-no-borrow");
        touch(&root.join("mods/bikes/KTM450/paints/Race.pnt"));
        touch(&root.join("mods/bikes/YZ250/paints/Southwick.pnt"));

        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut lo = Loadout::default();
        lo.paint = "Race".into();

        let plans = plan_profile(&cfg, &[("YZ250".to_string(), lo)]);
        assert!(!plans[0].assets.iter().any(|a| a.slot == "paint"));
        assert!(plans[0].unresolved.iter().any(|u| u.slot == "paint"));
        let _ = std::fs::remove_dir_all(&root);
    }

    // A gear paint usually lives inside the model folder that binds it. `plan` folds it into
    // that folder on purpose — a zip carrying `rider/helmets/AGV` carries the liveries free —
    // but a publish uploads `.pnt` files one at a time and skips folders, so folding them in
    // meant every helmet, boot and protection paint silently went unshared.
    #[test]
    fn gear_paints_nested_in_their_model_stay_addressable() {
        let root = tmp("nested-gear");
        touch(&root.join("mods/rider/helmets/AGV/model.edf"));
        touch(&root.join("mods/rider/helmets/AGV/paints/Blue.pnt"));
        touch(&root.join("mods/rider/helmets/AGV/goggles/Smoke.pnt"));
        touch(&root.join("mods/rider/boots/Tech10/model.edf"));
        touch(&root.join("mods/rider/boots/Tech10/paints/White.pnt"));
        touch(&root.join("mods/rider/protections/Leatt/model.edf"));
        touch(&root.join("mods/rider/protections/Leatt/paints/Carbon.pnt"));

        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut lo = Loadout::default();
        lo.helmet = "AGV".into();
        lo.helmet_paint = "Blue".into();
        lo.goggles_paint = "Smoke".into();
        lo.boots = "Tech10".into();
        lo.boots_paint = "White".into();
        lo.protection = "Leatt".into();
        lo.protection_paint = "Carbon".into();

        let plans = plan_profile(&cfg, &[("YZ250".to_string(), lo.clone())]);
        let dest = |slot: &str| {
            plans[0]
                .assets
                .iter()
                .find(|a| a.slot == slot)
                .map(|a| a.rel_dest.as_str())
        };
        assert_eq!(dest("helmet_paint"), Some("rider/helmets/AGV/paints/Blue.pnt"));
        assert_eq!(dest("goggles_paint"), Some("rider/helmets/AGV/goggles/Smoke.pnt"));
        assert_eq!(dest("boots_paint"), Some("rider/boots/Tech10/paints/White.pnt"));
        assert_eq!(dest("protection_paint"), Some("rider/protections/Leatt/paints/Carbon.pnt"));

        // The zip path still collapses them, which is what makes it a smaller archive.
        let zipped = plan(&cfg, &lo).unwrap();
        assert!(!zipped.assets.iter().any(|a| a.slot == "helmet_paint"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn planning_nothing_walks_nothing() {
        // Guards the early return: a profile with no bikes must not pay for a library scan.
        let cfg = AppConfig { mods_path: "/nowhere".into(), ..Default::default() };
        assert!(plan_profile(&cfg, &[]).is_empty());
    }

    #[test]
    fn plan_skips_builtins() {
        let root = tmp("builtins");
        touch(&root.join("mods/bikes/x.txt"));
        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut lo = Loadout::default();
        lo.helmet = "default".into();
        lo.tyres = "p_mx".into();
        lo.riding_style = "mx".into();
        let plan = plan(&cfg, &lo).unwrap();
        assert!(plan.assets.is_empty());
        assert!(plan.unresolved.is_empty(), "a stock style ships in rider.pkz, nothing to pack");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A shared preset has to carry the riding style, or it lands on the other player's
    /// machine naming a style they have no way to get.
    #[test]
    fn plan_packs_a_custom_riding_style() {
        let root = tmp("riding-style");
        touch(&root.join("mods/rider/animations/Scrub/Scrub.ini"));
        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut lo = Loadout::default();
        lo.riding_style = "Scrub".into();

        let plan = plan(&cfg, &lo).unwrap();
        let asset = plan.assets.iter().find(|a| a.slot == "riding_style");
        assert_eq!(
            asset.map(|a| a.rel_dest.as_str()),
            Some("rider/animations/Scrub"),
            "assets: {:?}",
            plan.assets.iter().map(|a| &a.rel_dest).collect::<Vec<_>>(),
        );
        assert!(plan.unresolved.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn bundle_zip_place_round_trips() {
        let root = tmp("roundtrip");
        let src = root.join("bundle");
        touch(&src.join("mods/bikes/KTM450/paints/RedBud.pnt"));
        touch(&src.join("mods/rider/helmets/AGV/model.edf"));
        touch(&src.join("preset.json"));

        let zip_path = root.join("b.zip");
        zip_dir(&src, &zip_path).unwrap();

        let extracted = root.join("extracted");
        std::fs::create_dir_all(&extracted).unwrap();
        install::extract_archive(&zip_path, &extracted).unwrap();
        let mods = root.join("game/mods");
        install::place_mod(&extracted, &mods, "bikes", "", "slug").unwrap();

        assert!(mods.join("bikes/KTM450/paints/RedBud.pnt").exists());
        assert!(mods.join("rider/helmets/AGV/model.edf").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An import fills gaps, it doesn't trade. Sharing one helmet paint ships the whole
    /// helmet — `dedup_assets` collapses the paint into its parent folder — so a receiver
    /// who already owns that helmet would otherwise have their mesh and their own liveries
    /// replaced by the sender's copies.
    #[test]
    fn importing_keeps_what_the_receiver_already_has() {
        let root = tmp("keep-existing");
        let src = root.join("bundle");
        std::fs::create_dir_all(src.join("mods/rider/helmets/AGV/paints")).unwrap();
        std::fs::write(src.join("mods/rider/helmets/AGV/model.edf"), b"theirs").unwrap();
        std::fs::write(src.join("mods/rider/helmets/AGV/paints/Theirs.pnt"), b"theirs").unwrap();

        let zip_path = root.join("b.zip");
        zip_dir(&src, &zip_path).unwrap();
        let extracted = root.join("extracted");
        std::fs::create_dir_all(&extracted).unwrap();
        install::extract_archive(&zip_path, &extracted).unwrap();

        let mods = root.join("game/mods");
        std::fs::create_dir_all(mods.join("rider/helmets/AGV/paints")).unwrap();
        std::fs::write(mods.join("rider/helmets/AGV/model.edf"), b"mine").unwrap();
        std::fs::write(mods.join("rider/helmets/AGV/paints/Mine.pnt"), b"mine").unwrap();

        let written = install::place_mod_with(
            &extracted,
            &mods,
            "bikes",
            "",
            "slug",
            install::OnConflict::Keep,
            install::Staging::Preserve,
        )
        .unwrap();

        let read = |p: &str| std::fs::read(mods.join(p)).unwrap();
        assert_eq!(read("rider/helmets/AGV/model.edf"), b"mine", "their mesh stays");
        assert_eq!(read("rider/helmets/AGV/paints/Mine.pnt"), b"mine", "their paint stays");
        assert_eq!(read("rider/helmets/AGV/paints/Theirs.pnt"), b"theirs", "the new paint lands");
        assert_eq!(written, 1, "only the file they were missing was written");
        let _ = std::fs::remove_dir_all(&root);
    }
}
