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
use std::path::PathBuf;
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

/// Resolve picked absolute paths into `mods/`-relative items, saying what was left out.
///
/// Everything shared has to live under the mods root: the rel path is what the code carries,
/// and a file from anywhere else has no rel path to give. That check doubles as the guard on
/// paths arriving from the frontend — nothing outside the mods tree can be packed up and
/// uploaded, whatever the caller asks for.
pub fn plan(cfg: &AppConfig, paths: &[String]) -> SharePlan {
    let root = library::mods_root(&cfg.mods_path);
    let mut items: Vec<ShareItem> = Vec::new();
    let mut skipped: Vec<Skipped> = Vec::new();

    for raw in paths {
        let p = PathBuf::from(raw.trim());
        let reason = if !p.exists() {
            Some("no longer on disk")
        } else if !p.starts_with(&root) {
            Some("outside the mods folder")
        } else {
            None
        };
        if let Some(reason) = reason {
            skipped.push(Skipped { path: raw.clone(), reason: reason.to_string() });
            continue;
        }

        let rel = p
            .strip_prefix(&root)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let rel = rel.trim_matches('/').to_string();
        if rel.is_empty() {
            skipped.push(Skipped {
                path: raw.clone(),
                reason: "that's the mods folder itself".to_string(),
            });
            continue;
        }

        let is_dir = p.is_dir();
        items.push(ShareItem {
            name: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            rel,
            size: if is_dir { bundle::dir_size_deep(&p) } else { bundle::file_size(&p) },
            is_dir,
        });
    }

    dedup(&mut items);
    items.sort_by(|a, b| a.rel.to_lowercase().cmp(&b.rel.to_lowercase()));
    let total_size = items.iter().map(|i| i.size).sum();
    SharePlan { items, skipped, total_size }
}

/// Drop repeats, and anything already carried by a folder that's also in the list — picking
/// a track folder *and* a file inside it must not pack that file twice.
fn dedup(items: &mut Vec<ShareItem>) {
    let dirs: Vec<String> = items
        .iter()
        .filter(|i| i.is_dir)
        .map(|i| i.rel.trim_end_matches('/').to_lowercase())
        .collect();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    items.retain(|i| {
        let key = i.rel.to_lowercase();
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
    let plan = plan(cfg, paths);
    if plan.items.is_empty() {
        anyhow::bail!(
            "Nothing here can be shared — pick installed files from your mods folder."
        );
    }

    bundle::emit(app, EVENT, "bundling", None);
    let root_dir = library::mods_root(&cfg.mods_path);
    let work = std::env::temp_dir().join(format!("mxb-share-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    let root = work.join("share");
    std::fs::create_dir_all(&root)?;

    for item in &plan.items {
        let src = root_dir.join(bundle::rel_to_native(&item.rel));
        let dest = root.join("mods").join(bundle::rel_to_native(&item.rel));
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if item.is_dir {
            // Resolved, not linked: a junction into the sender's tree means nothing on the
            // machine this is headed for. Same reason the preset bundle resolves its folders.
            bundle::copy_tree(&src, &dest)?;
        } else {
            std::fs::copy(&src, &dest)
                .with_context(|| format!("copying {}", src.display()))?;
        }
    }

    // A manifest for anyone who unzips the archive by hand rather than pasting the code.
    // `place_mod` routes on the `mods/` child alone, so this sits beside it harmlessly.
    std::fs::write(root.join("share.json"), serde_json::to_vec_pretty(&plan.items)?)?;

    let zip_path = work.join(format!("{}.zip", archive_stem(&plan.items)));
    bundle::zip_dir(&root, &zip_path)?;

    let size = bundle::file_size(&zip_path);
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
    let parts = if up.parts.len() > 1 { up.parts } else { Vec::new() };
    let share = FileShare {
        items: plan.items,
        total_size: up.size,
        bundle: BundleRef { url: first, host: up.host, size: up.size, parts },
    };
    bundle::emit(app, EVENT, "done", None);
    Ok(encode(&share))
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
    Ok(share)
}

pub async fn import(
    app: &AppHandle,
    cfg: &AppConfig,
    text: &str,
) -> anyhow::Result<FileShare> {
    let share = decode(text)?;

    let work = std::env::temp_dir().join(format!("mxb-share-import-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;

    let archive = bundle::fetch(app, EVENT, SLUG, &share.bundle, &work).await?;

    bundle::emit(app, EVENT, "installing", None);
    let extracted = work.join("extracted");
    std::fs::create_dir_all(&extracted)?;
    install::extract_archive(&archive, &extracted)?;
    let mods_dir = library::mods_subdir(&cfg.mods_path, "mods");
    // The archive is a `mods/` tree, which routes as a merge — the type folder is only a
    // fallback for shapes this never produces, but naming the real one keeps the log honest.
    install::place_mod(&extracted, &mods_dir, &type_folder(&share.items), "", SLUG)?;

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
    use std::path::Path;

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

    /// End to end minus the network: what `create` stages has to be what `import` lays down,
    /// in the same folders, on a machine that has none of it.
    #[test]
    fn a_staged_share_lands_back_in_the_same_folders() {
        let root = tmp("roundtrip");
        let staged = root.join("share");
        touch(&staged.join("mods/tracks/EU/RedBud.pkz"));
        touch(&staged.join("mods/rider/helmets/AGV/paints/Blue.pnt"));
        touch(&staged.join("share.json"));

        let zip_path = root.join("share.zip");
        bundle::zip_dir(&staged, &zip_path).unwrap();

        let extracted = root.join("extracted");
        std::fs::create_dir_all(&extracted).unwrap();
        install::extract_archive(&zip_path, &extracted).unwrap();
        let mods = root.join("game/mods");
        install::place_mod(&extracted, &mods, "tracks", "", SLUG).unwrap();

        assert!(mods.join("tracks/EU/RedBud.pkz").exists());
        assert!(mods.join("rider/helmets/AGV/paints/Blue.pnt").exists());
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
