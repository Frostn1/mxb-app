//! Sharing installed content directly — a track, a paint, a handful of both.
//!
//! [`crate::bundle`] shares a *look*: a preset code names slots, and the full bundle packs
//! whatever those slots resolved to. That covers the rider and never the rest, so handing
//! someone the track you just rode still meant a Discord upload and a link in chat.
//!
//! This shares the files themselves. Anything the Library lists can go in a code — the same
//! catbox upload, the same slicing for anything past one part, the same `mods/`-shaped zip
//! that [`crate::install::place_mod`] already knows how to lay back down. What a code
//! carries is a list of `mods/`-relative paths, so a track picked out of `tracks/EU/` lands
//! in `tracks/EU/` on the other machine.

use crate::bundle;
use crate::config::AppConfig;
use crate::install;
use crate::library;
use crate::presets::BundleRef;
use crate::upload;
use anyhow::Context;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

/// Phase updates ride their own event, so the Library's dialog and the Presets one never
/// hear each other's progress.
pub const EVENT: &str = "file-share-progress";

const SLUG: &str = "__file_share__";

const CODE_PREFIX: &str = "MXBS1-";

/// The preset code prefix, recognised only to say where it belongs.
const PRESET_PREFIX: &str = "MXBP1-";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShareItem {
    pub name: String,
    /// Where it sits under the mods root, forward-slashed (`tracks/EU/RedBud.pkz`). This is
    /// the whole portability story: it survives the sender's mods folder living on another
    /// drive, and it puts a rider paint back under `rider/`, not wherever the importer's
    /// Library tab happened to be pointing.
    pub rel: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub path: String,
    pub reason: String,
}

/// What a share would carry, before anything is packed or uploaded.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharePlan {
    pub items: Vec<ShareItem>,
    pub skipped: Vec<Skipped>,
    pub total_size: u64,
}

/// The payload a `MXBS1-` code decodes to.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileShare {
    pub items: Vec<ShareItem>,
    /// Size of the packed zip — what the importer is about to download, which is not the
    /// sum of `items` once the zip's own bookkeeping is counted.
    pub total_size: u64,
    pub bundle: BundleRef,
}

/// A decoded code, and what importing it would land on top of.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharePreview {
    #[serde(flatten)]
    pub share: FileShare,
    /// The rels the importer already has. [`import`] places with
    /// [`install::OnConflict::Overwrite`], so these are replaced without being asked —
    /// which is worth saying before the download, not after.
    pub existing: Vec<String>,
}

/// Read a code and check what it carries against the mods tree.
pub fn preview(cfg: &AppConfig, text: &str) -> anyhow::Result<SharePreview> {
    let share = decode(text)?;
    let existing = share
        .items
        .iter()
        // Through `mods_subdir` rather than a plain join: it resolves each segment against
        // what is really on disk, so a sender whose folder is `Tracks` still matches ours.
        .filter(|i| library::mods_subdir(&cfg.mods_path, &format!("mods/{}", i.rel)).exists())
        .map(|i| i.rel.clone())
        .collect();
    Ok(SharePreview { share, existing })
}

/// Turn one pick into the file it names and the rel a code would carry, or the reason it
/// can't be shared.
///
/// Two callers, two ways of naming a thing. The Library holds absolute paths from its scan;
/// Manage and the Locker name content the way the rest of the app does, by its
/// `mods/`-relative rel (`mods/tracks/EU/RedBud.pkz`, or bare `tracks/…`). Both land here.
///
/// This is also the guard on input from the frontend, and it has exactly two ways to say
/// yes: the mods tree, and the shadow tree a disabled mod is parked in. A `..` is refused
/// outright — `starts_with` compares components, so `mods/../../secrets` would otherwise
/// pass a prefix check while naming a file well outside.
fn resolve_pick(
    cfg: &AppConfig,
    root: &Path,
    raw: &str,
) -> Result<(PathBuf, String), &'static str> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("empty path");
    }
    let given = Path::new(raw);
    if given.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("path steps outside the mods folder");
    }

    let abs = if given.is_absolute() {
        given.to_path_buf()
    } else {
        // A rel names a place in the mods tree whether or not it says `mods/` first.
        let first_is_mods = raw
            .split(['/', '\\'])
            .find(|s| !s.is_empty())
            .is_some_and(|s| s.eq_ignore_ascii_case("mods"));
        let rel = if first_is_mods { raw.to_string() } else { format!("mods/{raw}") };
        let enabled = library::mods_subdir(&cfg.mods_path, &rel);
        // Manage lists mods it has switched *off* too, and those are parked outside the
        // content folder. Sharing one is still sharing the file — and the rel a code
        // carries is where it goes when enabled, which is the same either way.
        if enabled.exists() { enabled } else { crate::modstate::disabled_path(&cfg.mods_path, &rel) }
    };

    if !abs.exists() {
        return Err("no longer on disk");
    }

    let shadow = crate::modstate::shadow_root(&cfg.mods_path);
    let rel = abs
        .strip_prefix(root)
        .or_else(|_| abs.strip_prefix(&shadow))
        .map_err(|_| "outside the mods folder")?
        .to_string_lossy()
        .replace('\\', "/");
    let rel = rel.trim_matches('/').to_string();
    if rel.is_empty() {
        return Err("that's the mods folder itself");
    }
    Ok((abs, rel))
}

/// A resolved pick: what a code will say about it, and where it's actually read from.
///
/// The two are not the same file for a mod Manage has switched off — it says
/// `mods/tracks/RedBud.pkz`, because that is where it goes on the far end, while `src`
/// points into the shadow tree it's parked in today.
struct Pick {
    item: ShareItem,
    src: PathBuf,
}

/// Resolve picks, saying what was left out and why. Deduped and ordered by rel.
///
/// The rel path is the whole portability story — see [`ShareItem::rel`] — so anything that
/// can't be given one can't be shared. See [`resolve_pick`] for what counts.
fn picks(cfg: &AppConfig, paths: &[String]) -> (Vec<Pick>, Vec<Skipped>) {
    let root = library::mods_root(&cfg.mods_path);
    let mut picks: Vec<Pick> = Vec::new();
    let mut skipped: Vec<Skipped> = Vec::new();

    for raw in paths {
        let (src, rel) = match resolve_pick(cfg, &root, raw) {
            Ok(resolved) => resolved,
            Err(reason) => {
                skipped.push(Skipped { path: raw.clone(), reason: reason.to_string() });
                continue;
            }
        };

        let is_dir = src.is_dir();
        picks.push(Pick {
            item: ShareItem {
                name: src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                rel,
                size: if is_dir { bundle::dir_size_deep(&src) } else { bundle::file_size(&src) },
                is_dir,
            },
            src,
        });
    }

    dedup(&mut picks);
    picks.sort_by(|a, b| a.item.rel.to_lowercase().cmp(&b.item.rel.to_lowercase()));
    (picks, skipped)
}

pub fn plan(cfg: &AppConfig, paths: &[String]) -> SharePlan {
    let (picks, skipped) = picks(cfg, paths);
    let items: Vec<ShareItem> = picks.into_iter().map(|p| p.item).collect();
    let total_size = items.iter().map(|i| i.size).sum();
    SharePlan { items, skipped, total_size }
}

/// Drop repeats, and anything already carried by a folder that's also in the list — picking
/// a track folder *and* a file inside it must not pack that file twice.
fn dedup(picks: &mut Vec<Pick>) {
    let dirs: Vec<String> = picks
        .iter()
        .filter(|p| p.item.is_dir)
        .map(|p| p.item.rel.trim_end_matches('/').to_lowercase())
        .collect();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    picks.retain(|p| {
        let key = p.item.rel.to_lowercase();
        if !seen.insert(key.clone()) {
            return false;
        }
        !dirs.iter().any(|d| key != *d && key.starts_with(&format!("{d}/")))
    });
}

/// Zip name for a share: the single item's name, or how many items there are.
fn archive_stem(items: &[ShareItem]) -> String {
    match items {
        [only] => bundle::sanitize_file(&strip_ext(&only.name)),
        _ => format!("mxb-share-{}-items", items.len()),
    }
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

pub async fn create(
    app: &AppHandle,
    cfg: &AppConfig,
    paths: &[String],
) -> anyhow::Result<String> {
    Ok(encode(&pack(app, cfg, paths).await?))
}

/// Pack the picks and upload them, stopping short of writing a code.
///
/// [`create`] is this plus [`encode`]. Split because a live share needs the same packing and
/// the same upload but does not want the result as a `MXBS1-` string — it posts the
/// `FileShare` to the control plane and hands back a short code instead. Everything that
/// makes sharing work (the `mods/`-relative rels, the slicing, the retry on a dropped part)
/// therefore has one implementation rather than two that drift.
pub async fn pack(
    app: &AppHandle,
    cfg: &AppConfig,
    paths: &[String],
) -> anyhow::Result<FileShare> {
    let (picks, _) = picks(cfg, paths);
    if picks.is_empty() {
        anyhow::bail!(
            "Nothing here can be shared — pick installed files from your mods folder."
        );
    }

    bundle::emit(app, EVENT, "bundling", None);
    let work = std::env::temp_dir().join(format!("mxb-share-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;

    // Named, not copied. Sharing a track used to write the whole `.pkz` into a staging tree
    // and then read it straight back out to build the zip; the zip stores its payload
    // uncompressed, so that first pass only ever cost time. Folders are still resolved
    // rather than linked — a junction into the sender's tree means nothing on the machine
    // this is headed for — which is what `entries_under` walks for.
    let mut entries: Vec<bundle::ZipEntry> = Vec::new();
    for Pick { item, src } in &picks {
        entries.extend(bundle::entries_under(&format!("mods/{}", item.rel), src));
    }

    let items: Vec<ShareItem> = picks.into_iter().map(|p| p.item).collect();

    // A manifest for anyone who unzips the archive by hand rather than pasting the code.
    // `place_mod` routes on the `mods/` child alone, so this sits beside it harmlessly.
    let manifest = work.join("share.json");
    std::fs::write(&manifest, serde_json::to_vec_pretty(&items)?)?;
    entries.push(bundle::ZipEntry { rel: "share.json".to_string(), src: manifest });

    let zip_path = work.join(format!("{}.zip", archive_stem(&items)));
    // Off the runtime: packing a track copies eighty-odd megabytes through a blocking read
    // and write, and doing that on a runtime thread freezes every other async task in the
    // app — the upload that follows included. `file_share_plan` beside it already does this.
    let packed = std::time::Instant::now();
    let zp = zip_path.clone();
    tauri::async_runtime::spawn_blocking(move || bundle::zip_entries(&entries, &zp))
        .await
        .map_err(|e| anyhow::anyhow!("packing the share failed: {e}"))??;

    let size = bundle::file_size(&zip_path);
    log::info!(
        "share: packed {} into {} in {:.1}s",
        bundle::human_size(size),
        zip_path.display(),
        packed.elapsed().as_secs_f32()
    );
    let total = bundle::human_size(size);
    bundle::emit(app, EVENT, "uploading", Some(format!("Uploading {total}…")));
    let client = install::build_client()?;
    let up = upload::upload_file(&client, &zip_path, |i, n| {
        let msg = if n > 1 {
            format!("Uploading part {i} of {n} ({total})…")
        } else {
            format!("Uploading {total}…")
        };
        bundle::emit(app, EVENT, "uploading", Some(msg));
    })
    .await?;

    let _ = std::fs::remove_dir_all(&work);

    let first = up
        .parts
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("the upload returned no link"))?;
    // As in the preset bundle: `url` is the first slice, and `parts` is only carried when
    // there's more than one to stitch back together.
    let multi = up.parts.len() > 1;
    let parts = if multi { up.parts } else { Vec::new() };
    let part_sizes = if multi { up.part_sizes } else { Vec::new() };
    let share = FileShare {
        items,
        total_size: up.size,
        bundle: BundleRef { url: first, host: up.host, size: up.size, parts, part_sizes },
    };
    bundle::emit(app, EVENT, "done", None);
    Ok(share)
}

pub fn encode(share: &FileShare) -> String {
    let json = serde_json::to_vec(share).unwrap_or_default();
    format!("{CODE_PREFIX}{}", STANDARD.encode(json))
}

/// Read a share code. A preset code is recognised on purpose, so pasting one here says
/// where it belongs instead of "bad code".
pub fn decode(text: &str) -> anyhow::Result<FileShare> {
    let t = text.trim();
    if t.starts_with(PRESET_PREFIX) {
        anyhow::bail!("That's a preset code — import it from the Presets tab.");
    }
    // A live code carries nothing itself — what it points at lives on the control plane —
    // so it cannot be decoded here. Named rather than refused as gibberish, for the same
    // reason a preset code is: the paste was not a mistake, it went to the wrong reader.
    if crate::liveshare::is_live_code(t) {
        anyhow::bail!(
            "That's a live share code — it needs to be fetched, not decoded. \
             Import it and it'll stay up to date on its own."
        );
    }
    let body = t.strip_prefix(CODE_PREFIX).unwrap_or(t).trim();
    if body.starts_with('{') {
        return serde_json::from_str(body).context("that JSON isn't a valid share");
    }
    let bytes = STANDARD
        .decode(body)
        .context("that doesn't look like a share code")?;
    let share: FileShare =
        serde_json::from_slice(&bytes).context("share code isn't a valid file share")?;
    if share.items.is_empty() {
        anyhow::bail!("this share code carries no files");
    }
    // Every `rel` is joined onto the receiver's mods root on import, and the first segment
    // of the first one picks the type folder outright — so a code written by hand with
    // `../` in it would install into the game folder itself. Nothing this app produces
    // looks like that: `plan` derives every rel from a real path under the mods root.
    if let Some(bad) = share.items.iter().find(|i| !library::is_safe_rel(&i.rel)) {
        anyhow::bail!(
            "this share code points outside the mods folder ('{}') — don't import it",
            bad.rel
        );
    }
    crate::presets::check_bundle_ref(&share.bundle)?;
    Ok(share)
}

pub async fn import(
    app: &AppHandle,
    cfg: &AppConfig,
    text: &str,
) -> anyhow::Result<FileShare> {
    let share = decode(text)?;

    // Beside the mods tree, not in `%TEMP%`: what lands here is renamed into place a moment
    // later, and a rename only costs nothing when both ends are on one drive.
    let work = bundle::scratch_dir(cfg, "share-import");

    let fetched = bundle::fetch(app, EVENT, SLUG, &share.bundle, &work).await?;

    bundle::emit(app, EVENT, "installing", None);
    let extracted = work.join("extracted");
    std::fs::create_dir_all(&extracted)?;
    fetched.extract(&extracted)?;
    let mods_dir = library::mods_subdir(&cfg.mods_path, "mods");
    // The archive is a `mods/` tree, which routes as a merge — the type folder is only a
    // fallback for shapes this never produces, but naming the real one keeps the log honest.
    // Staged under our own `work`, deleted on the next line — nothing else reads it.
    install::place_mod_with(
        &extracted,
        &mods_dir,
        &type_folder(&share.items),
        "",
        SLUG,
        install::OnConflict::Overwrite,
        install::Staging::Consume,
    )?;

    let _ = std::fs::remove_dir_all(&work);
    install::notify_frostmod(app, SLUG);
    bundle::emit(app, EVENT, "done", None);

    Ok(share)
}

/// The `mods/` child the first item lives under (`tracks`, `bikes`, `rider`, …).
fn type_folder(items: &[ShareItem]) -> String {
    items
        .first()
        .and_then(|i| i.rel.split('/').next())
        .filter(|s| !s.is_empty())
        .unwrap_or("bikes")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mxb-share-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn cfg_at(root: &Path) -> AppConfig {
        AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() }
    }

    /// The point of the whole feature: a track and a rider paint, picked from two different
    /// corners of the tree, keep the folders they were found in.
    #[test]
    fn planning_keeps_each_pick_where_it_lives() {
        let root = tmp("plan");
        touch(&root.join("mods/tracks/EU/RedBud.pkz"));
        touch(&root.join("mods/rider/helmets/AGV/paints/Blue.pnt"));

        let p = plan(
            &cfg_at(&root),
            &[
                root.join("mods/tracks/EU/RedBud.pkz").to_string_lossy().into_owned(),
                root.join("mods/rider/helmets/AGV/paints/Blue.pnt").to_string_lossy().into_owned(),
            ],
        );

        let rels: Vec<&str> = p.items.iter().map(|i| i.rel.as_str()).collect();
        assert_eq!(rels, ["rider/helmets/AGV/paints/Blue.pnt", "tracks/EU/RedBud.pkz"]);
        assert!(p.skipped.is_empty(), "{:?}", p.skipped);
        assert_eq!(p.total_size, 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The one thing an importer can't see for themselves: the code is about to land on
    /// top of a track they already ride. `import` overwrites without asking, so the preview
    /// has to name what it replaces — and match the folder whatever case it was written in.
    #[test]
    fn a_preview_names_what_it_would_replace() {
        let root = tmp("preview");
        touch(&root.join("mods/tracks/EU/RedBud.pkz"));

        let code = encode(&FileShare {
            items: vec![
                ShareItem {
                    name: "RedBud.pkz".into(),
                    rel: "Tracks/EU/RedBud.pkz".into(),
                    size: 1,
                    is_dir: false,
                },
                ShareItem {
                    name: "Hangtown.pkz".into(),
                    rel: "tracks/EU/Hangtown.pkz".into(),
                    size: 1,
                    is_dir: false,
                },
            ],
            total_size: 2,
            bundle: BundleRef {
                url: "https://example.invalid/x.zip".into(),
                host: "example".into(),
                size: 2,
                parts: vec![],
                part_sizes: vec![],
            },
        });

        let p = preview(&cfg_at(&root), &code).unwrap();
        assert_eq!(p.existing, ["Tracks/EU/RedBud.pkz"], "only the one already there");
        assert_eq!(p.share.items.len(), 2, "and the code still carries both");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Paths come in from the frontend, so "is it under the mods root" is a guard, not a
    /// convenience: nothing else may be packed up and uploaded to a public host.
    #[test]
    fn planning_refuses_anything_outside_the_mods_folder() {
        let root = tmp("outside");
        touch(&root.join("mods/tracks/RedBud.pkz"));
        let elsewhere = root.join("secrets.txt");
        touch(&elsewhere);

        let p = plan(
            &cfg_at(&root),
            &[
                elsewhere.to_string_lossy().into_owned(),
                root.join("mods/tracks/Gone.pkz").to_string_lossy().into_owned(),
            ],
        );

        assert!(p.items.is_empty(), "{:?}", p.items);
        assert_eq!(p.skipped.len(), 2);
        assert!(p.skipped[0].reason.contains("outside"));
        assert!(p.skipped[1].reason.contains("no longer on disk"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn planning_drops_a_file_its_own_folder_already_carries() {
        let root = tmp("nested");
        touch(&root.join("mods/tracks/Loose Track/track.trh"));

        let p = plan(
            &cfg_at(&root),
            &[
                root.join("mods/tracks/Loose Track").to_string_lossy().into_owned(),
                root.join("mods/tracks/Loose Track/track.trh").to_string_lossy().into_owned(),
                root.join("mods/tracks/Loose Track").to_string_lossy().into_owned(),
            ],
        );

        assert_eq!(p.items.len(), 1);
        assert_eq!(p.items[0].rel, "tracks/Loose Track");
        assert!(p.items[0].is_dir);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Manage and the Locker name content by rel, not by absolute path — that's how the rest
    /// of the app talks about a mod. Both spellings have to resolve to the same item.
    #[test]
    fn planning_takes_a_rel_as_readily_as_a_path() {
        let root = tmp("rels");
        touch(&root.join("mods/tracks/EU/RedBud.pkz"));
        touch(&root.join("mods/bikes/KTM450/FrostMod Models/Factory OEM/model.edf"));
        let cfg = cfg_at(&root);

        let p = plan(
            &cfg,
            &[
                "mods/tracks/EU/RedBud.pkz".to_string(),
                "bikes/KTM450/FrostMod Models/Factory OEM".to_string(),
            ],
        );

        let rels: Vec<&str> = p.items.iter().map(|i| i.rel.as_str()).collect();
        assert_eq!(rels, ["bikes/KTM450/FrostMod Models/Factory OEM", "tracks/EU/RedBud.pkz"]);
        assert!(p.items[0].is_dir, "a swap variant is a folder");
        assert!(p.skipped.is_empty(), "{:?}", p.skipped);

        // And the absolute spelling of the same track answers identically.
        let abs = plan(&cfg, &[root.join("mods/tracks/EU/RedBud.pkz").to_string_lossy().into_owned()]);
        assert_eq!(abs.items[0], p.items[1]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Manage lists mods it has switched off, and those are parked outside the content
    /// folder. Sharing one has to reach it — and still say where it goes, not where it sits.
    #[test]
    fn planning_reaches_a_mod_manage_switched_off() {
        let root = tmp("parked");
        touch(&root.join("mxbapp_disabled/tracks/Old.pkz"));

        let p = plan(&cfg_at(&root), &["mods/tracks/Old.pkz".to_string()]);

        assert_eq!(p.items.len(), 1, "skipped: {:?}", p.skipped);
        assert_eq!(p.items[0].rel, "tracks/Old.pkz", "the rel is where it lands when enabled");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `starts_with` compares components, so a rel with `..` in it would sail through a
    /// prefix check while naming a file well outside the tree. It never gets that far.
    #[test]
    fn planning_refuses_a_rel_that_climbs_out() {
        let root = tmp("climb");
        touch(&root.join("mods/tracks/RedBud.pkz"));
        touch(&root.join("secrets.txt"));

        let p = plan(&cfg_at(&root), &["mods/../secrets.txt".to_string()]);

        assert!(p.items.is_empty(), "{:?}", p.items);
        assert!(p.skipped[0].reason.contains("steps outside"), "{:?}", p.skipped);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The import target is picked from the first segment of the first item's `rel`, so a
    /// hand-written code with `../` in it would install into the game folder itself.
    #[test]
    fn a_code_that_points_outside_the_mods_folder_is_refused() {
        let item = |rel: &str| ShareItem {
            name: "x.pkz".into(),
            rel: rel.into(),
            size: 1,
            is_dir: false,
        };
        let share = |items: Vec<ShareItem>, url: &str| FileShare {
            items,
            total_size: 1,
            bundle: BundleRef {
                url: url.into(),
                host: "catbox".into(),
                size: 1,
                parts: Vec::new(),
                part_sizes: Vec::new(),
            },
        };
        let good = "https://files.catbox.moe/a.zip";
        for hostile in [
            share(vec![item("../evil.dll")], good),
            share(vec![item("tracks/../../evil.dll")], good),
            share(vec![item("/etc/passwd")], good),
            // The first item routes the install; a climb hiding behind a good one still lands.
            share(vec![item("tracks/EU/RedBud.pkz"), item("../evil.dll")], good),
            share(vec![item("tracks/EU/RedBud.pkz")], "file:///etc/passwd"),
        ] {
            let code = encode(&hostile);
            assert!(decode(&code).is_err(), "should be refused: {:?}", hostile.items);
        }
    }

    #[test]
    fn code_round_trips() {
        let share = FileShare {
            items: vec![ShareItem {
                name: "RedBud.pkz".into(),
                rel: "tracks/EU/RedBud.pkz".into(),
                size: 12,
                is_dir: false,
            }],
            total_size: 40,
            bundle: BundleRef {
                url: "https://files.catbox.moe/abc.zip".into(),
                host: "catbox".into(),
                size: 40,
                parts: Vec::new(),
                part_sizes: Vec::new(),
            },
        };

        let code = encode(&share);
        assert!(code.starts_with(CODE_PREFIX));
        let back = decode(&code).unwrap();
        assert_eq!(back.items, share.items);
        assert_eq!(back.bundle.url, share.bundle.url);
        // Pasted without its prefix — chat clients love to eat the start of a line.
        assert_eq!(decode(code.trim_start_matches(CODE_PREFIX)).unwrap().items, share.items);
    }

    /// The two codes look alike and land in different dialogs. Saying which is which beats
    /// "share code isn't valid".
    #[test]
    fn a_preset_code_says_where_it_belongs() {
        let err = decode("MXBP1-eyJuYW1lIjoiUmVkQnVkIn0=").unwrap_err().to_string();
        assert!(err.contains("Presets tab"), "{err}");
    }

    /// End to end minus the network: what `create` packs has to be what `import` lays down,
    /// in the same folders, on a machine that has none of it.
    ///
    /// Packed the way `create` packs — straight off the sender's mods tree, with no staging
    /// copy in between — so the paths inside the archive are the ones a real share carries.
    #[test]
    fn a_share_lands_back_in_the_same_folders() {
        let root = tmp("roundtrip");
        let sender = root.join("sender/mods");
        touch(&sender.join("tracks/EU/RedBud.pkz"));
        touch(&sender.join("rider/helmets/AGV/paints/Blue.pnt"));
        // A picked folder, to prove its interior travels too.
        touch(&sender.join("tracks/Loose/track.pkz"));
        touch(&sender.join("tracks/Loose/maps/ground.tga"));

        let cfg = AppConfig {
            mods_path: root.join("sender").to_string_lossy().into_owned(),
            ..Default::default()
        };
        let (picks, skipped) = picks(
            &cfg,
            &[
                "tracks/EU/RedBud.pkz".to_string(),
                "rider/helmets/AGV/paints/Blue.pnt".to_string(),
                "tracks/Loose".to_string(),
            ],
        );
        assert!(skipped.is_empty(), "{skipped:?}");
        let entries: Vec<bundle::ZipEntry> = picks
            .iter()
            .flat_map(|p| bundle::entries_under(&format!("mods/{}", p.item.rel), &p.src))
            .collect();

        let zip_path = root.join("share.zip");
        bundle::zip_entries(&entries, &zip_path).unwrap();

        let extracted = root.join("extracted");
        std::fs::create_dir_all(&extracted).unwrap();
        install::extract_archive(&zip_path, &extracted).unwrap();
        let mods = root.join("game/mods");
        install::place_mod(&extracted, &mods, "tracks", "", SLUG).unwrap();

        assert!(mods.join("tracks/EU/RedBud.pkz").exists());
        assert!(mods.join("rider/helmets/AGV/paints/Blue.pnt").exists());
        assert!(mods.join("tracks/Loose/track.pkz").exists());
        assert!(mods.join("tracks/Loose/maps/ground.tga").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_archive_is_named_after_what_it_carries() {
        let item = |name: &str| ShareItem {
            name: name.into(),
            rel: format!("tracks/{name}"),
            size: 1,
            is_dir: false,
        };
        assert_eq!(archive_stem(&[item("RedBud.pkz")]), "RedBud");
        assert_eq!(archive_stem(&[item("A.pkz"), item("B.pkz")]), "mxb-share-2-items");
    }
}
