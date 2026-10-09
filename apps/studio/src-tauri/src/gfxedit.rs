//! The Gfx tab: open a bike or a FrostMod model's `gfx.cfg`, show it on the model, save it
//! back, and tell FrostMod to re-read it.
//!
//! The editing itself is `mxb_core::gfxedit`; this is the part that knows where a
//! `gfx.cfg` lives. A FrostMod model that is the active one has had its files moved into
//! the bike folder, so the copy the game reads is the bike's, not the one in
//! `FrostMod Models\<Variant>\` (which only holds the manifest while it's active).

use mxb_core::frostmodcmd::{self, CommandOutcome};
use mxb_core::{gfxedit, viewer};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MODELS_DIR: &str = "FrostMod Models";
const ACTIVE_MARKER: &str = "_active.txt";

/// What a path the user picked turned out to be.
#[derive(Debug, Clone, PartialEq)]
struct Resolved {
    /// The `gfx.cfg` to read and write.
    gfx: PathBuf,
    /// The bike folder: the picked folder, or the one above `FrostMod Models`.
    bike_dir: PathBuf,
    /// The FrostMod model, when one was picked.
    variant: Option<String>,
    /// Whether that model is the one in use.
    active: bool,
}

fn eq_ci(a: &std::ffi::OsStr, b: &str) -> bool {
    a.to_string_lossy().eq_ignore_ascii_case(b)
}

fn resolve(path: &str) -> Result<Resolved, String> {
    let mut dir = PathBuf::from(path.trim());
    if dir.file_name().is_some_and(|n| eq_ci(n, "gfx.cfg")) {
        dir.pop();
    }
    if !dir.is_dir() {
        return Err(format!("{} isn't a folder", dir.display()));
    }
    let parent = dir.parent().map(Path::to_path_buf);
    let in_models = parent
        .as_deref()
        .and_then(Path::file_name)
        .is_some_and(|n| eq_ci(n, MODELS_DIR));
    let r = if in_models {
        let models = parent.unwrap();
        let bike_dir = models.parent().map(Path::to_path_buf).ok_or("no bike folder")?;
        let variant = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let active = std::fs::read_to_string(models.join(ACTIVE_MARKER))
            .map(|s| s.trim().eq_ignore_ascii_case(&variant))
            .unwrap_or(false);
        let gfx = if active { bike_dir.join("gfx.cfg") } else { dir.join("gfx.cfg") };
        Resolved { gfx, bike_dir, variant: Some(variant), active }
    } else {
        Resolved { gfx: dir.join("gfx.cfg"), bike_dir: dir, variant: None, active: false }
    };
    if !r.gfx.is_file() {
        return Err("gfx.cfg not found".into());
    }
    Ok(r)
}

/// The bike's folder name, when it sits in a game's `mods\bikes` — what FrostMod knows it by.
fn bike_id(bike_dir: &Path) -> Option<String> {
    let parent = bike_dir.parent()?.file_name()?;
    eq_ci(parent, "bikes").then(|| bike_dir.file_name().map(|n| n.to_string_lossy().into_owned()))?
}

/// The folder holding `mods`, for the swap preview to resolve the bike against.
fn mods_path(bike_dir: &Path) -> Option<String> {
    let bikes = bike_dir.parent()?;
    let mods = bikes.parent()?;
    if !eq_ci(bikes.file_name()?, "bikes") || !eq_ci(mods.file_name()?, "mods") {
        return None;
    }
    Some(mods.parent()?.to_string_lossy().into_owned())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GfxFile {
    pub gfx_path: String,
    pub label: String,
    pub variant: Option<String>,
    pub active: bool,
    /// Set when FrostMod can be told to re-read it.
    pub bike_id: Option<String>,
    pub fields: BTreeMap<String, String>,
}

fn read(r: &Resolved) -> Result<GfxFile, String> {
    let bytes = std::fs::read(&r.gfx).map_err(|e| format!("{}: {e}", r.gfx.display()))?;
    let (text, _) = gfxedit::decode(&bytes);
    let bike = r.bike_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(GfxFile {
        gfx_path: r.gfx.to_string_lossy().into_owned(),
        label: match &r.variant {
            Some(v) => format!("{bike} · {v}"),
            None => bike,
        },
        variant: r.variant.clone(),
        active: r.active,
        bike_id: bike_id(&r.bike_dir),
        fields: gfxedit::read_fields(&text),
    })
}

#[tauri::command]
pub async fn gfx_open(path: String) -> Result<GfxFile, String> {
    tauri::async_runtime::spawn_blocking(move || read(&resolve(&path)?))
        .await
        .map_err(|e| e.to_string())?
}

/// The model to draw under the markers: the bike as it stands, or the picked FrostMod model
/// as applying it would leave the bike.
#[tauri::command]
pub async fn gfx_preview(path: String) -> Result<viewer::BikeModel, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let r = resolve(&path)?;
        match (&r.variant, r.active, mods_path(&r.bike_dir)) {
            (Some(v), false, Some(mods)) => {
                let bike = r.bike_dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
                viewer::preview_model_swap_blocking(&mods, &bike, v, None)
            }
            (Some(_), false, None) => {
                let dir = r.gfx.parent().unwrap_or(&r.bike_dir).to_string_lossy().into_owned();
                viewer::load_bike_model_blocking(dir, None)
            }
            _ => viewer::load_bike_model_blocking(r.bike_dir.to_string_lossy().into_owned(), None),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GfxSaved {
    pub file: GfxFile,
    /// What FrostMod was told, or `None` when the bike isn't one it can reload.
    pub reload: Option<CommandOutcome>,
}

/// Write `edits` (main-copy paths to new values) into the file as it is on disk now, then
/// ask FrostMod to re-read it.
#[tauri::command]
pub async fn gfx_save(path: String, edits: Vec<(String, String)>) -> Result<GfxSaved, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let r = resolve(&path)?;
        let bytes = std::fs::read(&r.gfx).map_err(|e| format!("{}: {e}", r.gfx.display()))?;
        let (text, utf8) = gfxedit::decode(&bytes);
        let out = gfxedit::apply(&text, &edits)?;
        if out != text {
            write_atomic(&r.gfx, &gfxedit::encode(&out, utf8))?;
        }
        let file = read(&r)?;
        let reload = file.bike_id.as_deref().map(signal_reload_gfx);
        Ok(GfxSaved { file, reload })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Beside the target and renamed over it, so a crash mid-write never leaves half a file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("cfg.frost-tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

/// `reload_bike_gfx`, through the same file and event MXB App uses for its verbs.
fn signal_reload_gfx(bike_id: &str) -> CommandOutcome {
    let json = frostmodcmd::command_json("reload_bike_gfx", bike_id);
    #[cfg(windows)]
    {
        frostmodcmd::write_and_signal(json)
    }
    #[cfg(not(windows))]
    {
        // Off Windows the file goes in FrostMod's folder, which only MXB App knows.
        let _ = json;
        CommandOutcome::Unsupported
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn resolves_bike_and_variant_folders() {
        let t = tempfile::tempdir().unwrap();
        let bike = t.path().join("mods").join("bikes").join("KTM250");
        touch(&bike.join("gfx.cfg"), "steer { }");
        touch(&bike.join(MODELS_DIR).join("Blend").join("gfx.cfg"), "chassis { }");
        touch(&bike.join(MODELS_DIR).join("Live").join("_files.txt"), "gfx.cfg");
        touch(&bike.join(MODELS_DIR).join(ACTIVE_MARKER), "Live\r\n");

        let r = resolve(bike.to_str().unwrap()).unwrap();
        assert_eq!((r.gfx.clone(), r.variant.clone(), r.active), (bike.join("gfx.cfg"), None, false));
        assert_eq!(bike_id(&r.bike_dir).as_deref(), Some("KTM250"));
        assert_eq!(mods_path(&r.bike_dir).as_deref(), t.path().to_str());

        // A model not in use: its own copy.
        let r = resolve(bike.join(MODELS_DIR).join("Blend").to_str().unwrap()).unwrap();
        assert_eq!(r.gfx, bike.join(MODELS_DIR).join("Blend").join("gfx.cfg"));
        assert_eq!((r.variant.as_deref(), r.active), (Some("Blend"), false));
        assert_eq!(r.bike_dir, bike);

        // The model in use: its files are in the bike folder.
        let r = resolve(bike.join(MODELS_DIR).join("Live").to_str().unwrap()).unwrap();
        assert_eq!((r.gfx.clone(), r.active), (bike.join("gfx.cfg"), true));

        // A gfx.cfg picked directly.
        let r = resolve(bike.join("gfx.cfg").to_str().unwrap()).unwrap();
        assert_eq!(r.bike_dir, bike);
    }

    #[test]
    fn a_loose_blend_is_not_a_bike_frostmod_knows() {
        let t = tempfile::tempdir().unwrap();
        let blend = t.path().join("Downloads").join("Blend");
        touch(&blend.join("gfx.cfg"), "steer { }");
        let r = resolve(blend.to_str().unwrap()).unwrap();
        assert_eq!(bike_id(&r.bike_dir), None);
        assert_eq!(mods_path(&r.bike_dir), None);
        assert!(resolve(t.path().to_str().unwrap()).is_err());
    }

    #[test]
    fn save_writes_both_copies() {
        let t = tempfile::tempdir().unwrap();
        let blend = t.path().join("Blend");
        let src = "steer\n{\n\tleftgrip { pos {\n x = -0.33\n } }\n}\ncockpit\n{\n\tsteer\n\t{\n\t\tleftgrip\n\t\t{\n\t\t\tpos\n\t\t\t{\n\t\t\t\tx = -0.33\n\t\t\t}\n\t\t}\n\t}\n}\n";
        touch(&blend.join("gfx.cfg"), src);
        let saved = tauri::async_runtime::block_on(gfx_save(
            blend.to_string_lossy().into_owned(),
            vec![("steer/leftgrip/pos/x".into(), "-0.35".into())],
        ))
        .unwrap();
        assert!(saved.reload.is_none());
        assert_eq!(saved.file.fields.get("cockpit/steer/leftgrip/pos/x").map(String::as_str), Some("-0.35"));
        assert_eq!(std::fs::read_to_string(blend.join("gfx.cfg")).unwrap(), src.replace("-0.33", "-0.35"));
    }
}