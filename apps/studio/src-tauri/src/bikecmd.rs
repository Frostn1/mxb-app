//! The bike builder's commands past the part library: the placeholder bike, putting parts
//! together (phase C) and the preview's data (D). The build is `bikebuild`, the Part Maker
//! `partmaker`; the library's own commands (add, role, slot) are in `main.rs`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tauri::Manager;

use crate::bikeassemble::{self as asm, BuildSettings, TemplateSource, V3};
use crate::bikeparts::{self, Library, Role};
use crate::blender;

pub(crate) fn app_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| format!("no app data directory: {e}"))
}

pub(crate) fn cache_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_cache_dir().map_err(|e| format!("no cache directory: {e}"))?.join("bike-builder"))
}

/// Blender, ready to run, or why not.
pub(crate) fn blender_exe(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let saved = mxb_core::config::load_or_detect(app).unwrap_or_default().blender_path;
    Ok(PathBuf::from(crate::supported_blender(&saved)?.path))
}

pub(crate) fn lock() -> std::sync::MutexGuard<'static, ()> {
    crate::BIKE_LIBRARY.lock().unwrap_or_else(|p| p.into_inner())
}

pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub(crate) fn anyhow_str(e: anyhow::Error) -> String {
    format!("{e:#}")
}

// ---------------------------------------------------------------------------
// the placeholder bike

/// Make Studio's placeholder bike, add its eight parts to the library and put each in its
/// slot: the whole builder, ready to try, with no parts of one's own.
#[tauri::command]
pub async fn bike_placeholder_add(app: tauri::AppHandle) -> Result<crate::BikeParts, String> {
    let exe = blender_exe(&app)?;
    let lib = crate::bike_library(&app)?;
    let dir = app_dir(&app)?.join("bike-made").join("placeholder");
    let cache = cache_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let make = |work: &Path| json!({ "op": "placeholder", "dir": dir, "work": work, "thumbSize": 256 });
        let keep = |answer: Value| {
            let _one = lock();
            for p in answer["parts"].as_array().cloned().unwrap_or_default() {
                let role: Role = serde_json::from_value(p["role"].clone())?;
                let source = PathBuf::from(p["part"].as_str().unwrap_or_default());
                let part = lib.add_as(&source, &p, bikeparts::file_stamp(&source), Some(role))?;
                lib.set_slot(role, Some(&part.id))?;
            }
            let parts = lib.list().into_iter().map(|p| crate::bike_part_view(&lib, p)).collect();
            Ok(crate::BikeParts { parts, slots: lib.slots() })
        };
        blender::job_then(&exe, &cache, "placeholder", make, keep).map_err(anyhow_str)
    })
    .await
    .map_err(err)?
}

// ---------------------------------------------------------------------------
// putting it together

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateView {
    source: TemplateSource,
    name: String,
    /// Whether a build from it can be ridden: only a real bike brings the physics.
    rideable: bool,
    /// Why the chosen template couldn't be read, when the placeholder stood in for it.
    problem: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssemblyView {
    template: TemplateView,
    assembly: asm::Assembly,
    name: String,
}

/// The template the settings name, or the placeholder with the reason it had to stand in.
pub(crate) fn template_or_placeholder(settings: &BuildSettings) -> (asm::Template, Option<String>) {
    match asm::Template::load(&settings.template()) {
        Ok(t) => (t, None),
        Err(e) => (
            asm::Template::load(&TemplateSource::Placeholder).expect("the placeholder always loads"),
            Some(format!("{e:#}")),
        ),
    }
}

fn assembly_view(lib: &Library, settings: &BuildSettings) -> AssemblyView {
    let (template, problem) = template_or_placeholder(settings);
    let parts: BTreeMap<Role, bikeparts::Part> =
        lib.slots().into_iter().filter_map(|(r, id)| lib.get(&id).ok().map(|p| (r, p))).collect();
    let slots: BTreeMap<Role, &bikeparts::Part> = parts.iter().map(|(r, p)| (*r, p)).collect();
    let assembly = asm::place(&template.frames, &slots, &settings.nudges);
    AssemblyView {
        template: TemplateView {
            rideable: matches!(template.source, TemplateSource::Bike { .. }),
            source: template.source,
            name: template.name,
            problem,
        },
        assembly,
        name: settings.name.clone(),
    }
}

#[tauri::command]
pub async fn bike_assembly(app: tauri::AppHandle) -> Result<AssemblyView, String> {
    let lib = crate::bike_library(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _one = lock();
        let settings = BuildSettings::load(lib.root());
        Ok(assembly_view(&lib, &settings))
    })
    .await
    .map_err(err)?
}

/// Move a role's part (and whatever hangs from it) by `delta` metres, in Blender's frame.
/// No delta puts it back where it snapped.
#[tauri::command]
pub async fn bike_nudge(app: tauri::AppHandle, role: Role, delta: Option<V3>) -> Result<AssemblyView, String> {
    let lib = crate::bike_library(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _one = lock();
        let mut settings = BuildSettings::load(lib.root());
        match delta {
            Some(d) if d.iter().all(|x| x.is_finite() && x.abs() <= 0.5) => {
                let n = settings.nudges.entry(role).or_insert([0.0; 3]);
                for i in 0..3 {
                    // Kept to a tenth of a millimetre, so repeated steps don't drift.
                    n[i] = ((n[i] + d[i]) * 10_000.0).round() / 10_000.0;
                }
            }
            Some(_) => return Err("a nudge is at most half a metre".into()),
            None => {
                settings.nudges.remove(&role);
            }
        }
        settings.save(lib.root()).map_err(anyhow_str)?;
        Ok(assembly_view(&lib, &settings))
    })
    .await
    .map_err(err)?
}

/// Build for an installed bike (its folder or `.pkz`), or for the placeholder with no path.
#[tauri::command]
pub async fn bike_template_set(app: tauri::AppHandle, path: Option<String>) -> Result<AssemblyView, String> {
    let lib = crate::bike_library(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let source = match path.filter(|p| !p.trim().is_empty()) {
            Some(p) => TemplateSource::Bike { path: p.trim().to_string() },
            None => TemplateSource::Placeholder,
        };
        // Refused up front rather than saved and quietly replaced by the placeholder.
        asm::Template::load(&source).map_err(anyhow_str)?;
        let _one = lock();
        let mut settings = BuildSettings::load(lib.root());
        settings.template = Some(source);
        settings.save(lib.root()).map_err(anyhow_str)?;
        Ok(assembly_view(&lib, &settings))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub async fn bike_build_name_set(app: tauri::AppHandle, name: String) -> Result<AssemblyView, String> {
    let lib = crate::bike_library(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _one = lock();
        let mut settings = BuildSettings::load(lib.root());
        settings.name = name.trim().chars().take(60).collect();
        settings.save(lib.root()).map_err(anyhow_str)?;
        Ok(assembly_view(&lib, &settings))
    })
    .await
    .map_err(err)?
}

/// A part's GLB, as bytes, for the 3D preview.
#[tauri::command]
pub async fn bike_part_glb(app: tauri::AppHandle, id: String) -> Result<tauri::ipc::Response, String> {
    let lib = crate::bike_library(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let part = lib.get(&id).map_err(anyhow_str)?;
        if !part.has_glb {
            return Err(format!("{} has no preview model; refresh it", part.name));
        }
        std::fs::read(lib.part_dir(&part.id).join("part.glb"))
            .map(tauri::ipc::Response::new)
            .map_err(|e| format!("reading {}'s preview: {e}", part.name))
    })
    .await
    .map_err(err)?
}
