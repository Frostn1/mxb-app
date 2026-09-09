// Windows: no console window behind the app in a release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Frost's Studio — where MX Bikes content is made.
//!
//! Deliberately thin for now. Every command it answers comes from `mxb_core`, which is the
//! whole point of standing this up before the studio's own 26k lines move in: it proves two
//! binaries build off one core, and that a command registered by path across a crate
//! boundary actually reaches the webview.

// The studio's own modules: making a track, packing a paint, sealing content for a buyer.
mod edfwrite;
mod gearrepair;
mod paintstudio;
mod trackbuild;
mod tracklayout;
mod trackline;
mod trackllm;
mod trackobjects;
mod trackprog;
mod trackscenery;
mod trackshot;
mod trackspeed;
mod trackstats;
mod tracksynth;

/// Sealing content to a buyer. Gitignored, like the module it builds on.
#[cfg(sidecar)]
mod sidecar_lock;

// Re-exported at the root so the `crate::edf::…` call sites throughout the modules above
// keep resolving, exactly as they do in the manager.
use mxb_core::config::AppConfig;
use tauri::{Emitter, Manager};

pub(crate) use mxb_core::{
    bikefiles, presets, cloudfiles, config, edf, game, heightfield, library, linkwalk, map, paint, pkz, texstore, track, usage, viewer, winehost,
};
#[cfg(sidecar)]
pub(crate) use mxb_core::sidecar;
#[cfg(mxbsecure)]
pub(crate) use mxb_core::mxbsecure;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            mxb_core::viewer::app_platform,
            // The studio's own: making a track, packing a paint, sealing content.
            get_config,
            list_games,
            bike_preview_available,
            reveal_in_explorer,
            scan_library,
            scan_bike_targets,
            scan_rider_targets,
            presets_save,
            experimental_state,
            set_guid,
            content_lock_available,
            mxbsecure_generate,
            photo_save,
            psd_read,
            psd_save,
            paint_studio_target,
            paint_studio_hints,
            set_track_tools,
            scan_gear_repairs,
            repair_gear,
            generate_track,
            base_track_program,
            blank_track_program,
            fit_track_budget,
            close_track_lap,
            check_track,
            preview_track,
            export_track_source,
            track_tools_status,
            download_track_tools,
            build_track,
            paint_studio_load,
            paint_studio_pixels,
            paint_studio_stage,
            paint_studio_save,
            paint_studio_extract,
            content_lock_plan,
            content_lock_run,
            // Reading a track, and the archive metadata behind it. The studio previews the
            // track it is building with the same code the manager shows one with.
            mxb_core::trackview::read_track_info,
            mxb_core::trackview::diagnose_track,
            mxb_core::trackview::load_track_terrain,
            mxb_core::trackview::load_track_overview,
            mxb_core::trackview::load_track_scenery,
            mxb_core::trackview::load_track_surfaces,
            mxb_core::trackview::load_track_backdrop,
            mxb_core::trackview::load_track_ground,
            mxb_core::trackview::load_track_ground_layers,
            mxb_core::trackview::read_track_placements,
            mxb_core::trackview::get_pkz_meta,
            mxb_core::trackview::get_pkz_meta_cached,
            mxb_core::trackview::get_pkz_preview,
            // Drawing a bike, a rider and their gear — what the Designer's preview needs.
            mxb_core::viewer::load_bike_model,
            mxb_core::viewer::load_rider_model,
            mxb_core::viewer::load_rider_body_model,
            mxb_core::viewer::load_gear_model,
            mxb_core::viewer::load_stock_gear_model,
            mxb_core::viewer::list_gear_paints,
            mxb_core::viewer::list_installed_gear_paints,
            mxb_core::viewer::texture_bytes,
            mxb_core::viewer::unpack_pkz,
            mxb_core::viewer::watch_paint_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Frost's Studio");
}

/// Gear models the game can't reach where they are: files loose in an area root, or a package
/// buried a folder deep. See [`gearrepair`] for how each happened and what moves.
#[tauri::command]
async fn scan_gear_repairs(app: tauri::AppHandle) -> Result<Vec<gearrepair::GearRepair>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(gearrepair::plan(&cfg.mods_path))
    })
    .await
    .map_err(|e| format!("scan_gear_repairs task failed: {e}"))?
}

/// Carry out one repair, by the `id` its plan carries. Returns how many entries moved.
#[tauri::command]
async fn repair_gear(app: tauri::AppHandle, id: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        gearrepair::apply_one(&cfg.mods_path, &id).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("repair_gear task failed: {e}"))?
}

fn track_program(value: serde_json::Value) -> Result<trackprog::TrackProgram, String> {
    serde_json::from_value(value).map_err(|e| format!("that isn't a track program: {e}"))
}

/// Ask the control plane for a track program, and keep asking until it measures like a track.
///
/// The key lives there, not here. Everything that comes back is synthesised and measured
/// before this returns — see `trackllm` — so a program reaching the studio has already been
/// built once.
#[tauri::command]
async fn generate_track(app: tauri::AppHandle, brief: String) -> Result<serde_json::Value, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    let base = mxb_core::names::control_plane();
    // A debug build pointed at a local control plane is someone testing this, and a local
    // `wrangler dev` has no accounts to enroll with. Anywhere else, the token is what says
    // whose Anthropic spend this is.
    let local = cfg!(debug_assertions) && !base.starts_with("https://");
    if cfg.cp_token.trim().is_empty() && !local {
        return Err(
            "Track generation goes through your MXB account — enroll with an invite code in \
             Settings first. To test against a local control plane, run `wrangler dev` in \
             control-plane/ with ANTHROPIC_API_KEY in .dev.vars and start the app with \
             MXB_CONTROL_PLANE=http://localhost:8787."
                .into(),
        );
    }
    let ask = trackllm::ControlPlane {
        base,
        token: cfg.cp_token.clone(),
    };
    // Four attempts: one to write it, one to fix the numbers, one for the thing the fix
    // broke, and one more because they are cheap now. Three was set when this called Opus at
    // $5/$25 per MTok; it calls Haiku at $1/$5, most of what used to come back wrong is
    // repaired without asking, and an attempt costs a fraction of a cent and twenty seconds.
    let prog = trackllm::generate(brief.trim(), &ask, 4)
        .await
        .map_err(|e| format!("{e:#}"))?;
    usage::track("track.generate");
    serde_json::to_value(&prog).map_err(|e| e.to_string())
}

/// A track to start from, without asking anyone for one.
///
/// The studio's first screen used to be a prompt and nothing else, which is a bad place to
/// start from when the model isn't configured — and a worse one when you just want to change
/// two jumps on something that already works.
#[tauri::command]
async fn base_track_program() -> Result<serde_json::Value, String> {
    serde_json::from_str::<trackprog::TrackProgram>(trackprog::EXAMPLE)
        .and_then(|p| serde_json::to_value(&p))
        .map_err(|e| format!("the built-in track didn't load: {e}"))
}

/// A lap with nothing on it: somewhere to start from scratch.
///
/// Answered from the type, not from the source text, exactly as the base track is. The
/// literal leaves out every field that has a default — `blend`, `elevation`, the ground's
/// `wear` — and handing those absences to the studio put an undefined into a number field
/// the moment a blank track loaded.
#[tauri::command]
async fn blank_track_program() -> Result<serde_json::Value, String> {
    serde_json::from_str::<trackprog::TrackProgram>(trackprog::BLANK)
        .and_then(|p| serde_json::to_value(&p))
        .map_err(|e| format!("the blank track didn't load: {e}"))
}

/// Give a programme a height budget that fits it.
///
/// The budget only exists because samples are quantised against it, and there is no reason a
/// person should be told to guess a number the synthesiser already knows.
#[tauri::command]
async fn fit_track_budget(program: serde_json::Value) -> Result<serde_json::Value, String> {
    let prog = track_program(program)?;
    let fitted = tauri::async_runtime::spawn_blocking(move || {
        tracksynth::with_fitted_budget(&prog).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("fit_track_budget task failed: {e}"))??;
    serde_json::to_value(&fitted).map_err(|e| e.to_string())
}

/// Bring an open lap back to its start.
#[tauri::command]
async fn close_track_lap(program: serde_json::Value) -> Result<serde_json::Value, String> {
    let mut prog = track_program(program)?;
    // A turn no tighter than the lap's own tightest, so the join doesn't need a corner
    // sharper than anything already on the track.
    let radius = prog
        .segments
        .iter()
        .filter_map(|s| match s {
            trackprog::Segment::Arc { radius, .. } => Some(radius.abs()),
            _ => None,
        })
        .fold(f32::MAX, f32::min);
    let radius = if radius.is_finite() { radius } else { 25.0 };
    match prog.closing_segments(radius) {
        Some(add) => prog.segments.extend(add),
        None => return Err("The lap already meets itself.".into()),
    }
    serde_json::to_value(&prog).map_err(|e| e.to_string())
}

/// Everything wrong with a program, without asking anyone. The studio calls this as edits are
/// made, so a hand-edited track is held to the same corpus a generated one is.
#[tauri::command]
async fn check_track(program: serde_json::Value) -> Result<trackllm::Review, String> {
    let prog = track_program(program)?;
    tauri::async_runtime::spawn_blocking(move || trackllm::review(&prog))
        .await
        .map_err(|e| format!("check_track task failed: {e}"))
}

/// Build a program into terrain and write it where the track viewer can open it.
#[tauri::command]
async fn preview_track(
    app: tauri::AppHandle,
    program: serde_json::Value,
) -> Result<TrackPreview, String> {
    let prog = track_program(program)?;
    tauri::async_runtime::spawn_blocking(move || {
        let syn = tracksynth::synthesise(&prog).map_err(|e| format!("{e:#}"))?;
        let dir = app
            .path()
            .app_cache_dir()
            .map_err(|e| format!("no cache directory: {e}"))?
            .join("track-preview");
        std::fs::create_dir_all(&dir).map_err(|e| format!("{e}"))?;
        // One file, overwritten. A studio session generates many tracks and none of them are
        // worth keeping until someone installs one.
        let path = dir.join("preview.pkz");
        tracksynth::write_pkz(&prog, &syn, &path, true).map_err(|e| format!("{e:#}"))?;

        let c = trackstats::measure("synth", &syn.corridor, &syn.heights, syn.gw, syn.gh, syn.mps);
        Ok(TrackPreview {
            path: path.to_string_lossy().into_owned(),
            name: prog.name.clone(),
            lap_m: prog.lap_length(),
            width_m: prog.width,
            features: prog.features.len(),
            closure_m: prog.closure_error(),
            used_m: syn.used_m,
            budget_m: syn.budget_m,
            measured_width_m: c.width_from_mean_m,
            measured_length_m: c.length_m,
            lips: c.lips,
            lips_per_km: c.lips_per_km,
            slope_p99_deg: c.slope_deg.p99,
            relief_p90_m: c.feature_relief_m.p90,
        })
    })
    .await
    .map_err(|e| format!("preview_track task failed: {e}"))?
}

/// Where an installed track goes: the mods tree's `tracks` folder.
///
/// Not `game_path` — that is the folder with the executable in it, which the game never
/// reads content from, and which is empty on a machine that only has the mods folder
/// configured. And `mods` is resolved rather than joined on: a player who relocated the
/// tree with `mxbikes.ini` has `mods_path` already pointing at it.

/// Write the folder TerrainEd compiles: the heightmap, the masks, and every config file.
#[tauri::command]
async fn export_track_source(
    program: serde_json::Value,
    dir: String,
) -> Result<Vec<String>, String> {
    let prog = track_program(program)?;
    tauri::async_runtime::spawn_blocking(move || {
        let syn = tracksynth::synthesise(&prog).map_err(|e| format!("{e:#}"))?;
        tracksynth::write_source(&prog, &syn, std::path::Path::new(&dir))
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("export_track_source task failed: {e}"))?
}


/// Whether the app can compile a track here, and what with.

#[tauri::command]
async fn track_tools_status(app: tauri::AppHandle) -> Result<TrackToolsStatus, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    let path = cfg.track_tools_path.clone();
    let tools = (!path.trim().is_empty())
        .then(|| trackbuild::find(std::path::Path::new(&path)))
        .flatten();
    Ok(TrackToolsStatus {
        path,
        found: tools.is_some(),
        has_tracked: tools.map(|t| t.tracked.is_some()).unwrap_or(false),
    })
}

/// Remember where PiBoSo's track editing tools live.

/// Fetch PiBoSo's track tools, so nobody has to leave the app to find them.
///
/// They are a public download and not ours to ship, so the app gets them on request rather
/// than carrying them. This was the last step of building a track that needed a browser.
#[tauri::command]
async fn download_track_tools(app: tauri::AppHandle) -> Result<TrackToolsStatus, String> {
    const URL: &str = "https://www.kartracing-pro.com/downloads/tt.zip";
    let dir = track_tools_dir(&app)?;

    let bytes = reqwest::Client::new()
        .get(URL)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("couldn't reach {URL}: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("the download stopped early: {e}"))?;

    let extracted = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| format!("that download isn't a zip: {e}"))?;
        // Flat: the archive nests each tool in its own folder, and `find` looks one level
        // down anyway — but the fonts `tracked.exe` reads sit beside it, so keep the shape.
        zip.extract(&dir).map_err(|e| format!("{e}"))?;
        Ok(())
    })
    .await
    .map_err(|e| format!("download_track_tools task failed: {e}"))?;
    extracted?;

    let dir = track_tools_dir(&app)?;
    if trackbuild::find(&dir).is_none() {
        return Err("the download arrived but there's no terrained.exe in it".into());
    }
    set_track_tools(app, dir.to_string_lossy().into_owned()).await
}

/// The compilers, fetched if this machine hasn't got them yet.
///
/// A track is only a track once `terrained.exe` has been over it — there is no second way to
/// produce a `.map` the game will ride. So the download belongs to the build rather than to a
/// step someone has to know to take first.
async fn ensure_track_tools(app: &tauri::AppHandle) -> Result<String, String> {
    let at = config::load_or_detect(app)
        .unwrap_or_default()
        .track_tools_path;
    if !at.trim().is_empty() && trackbuild::find(std::path::Path::new(&at)).is_some() {
        return Ok(at);
    }
    let got = download_track_tools(app.clone()).await?;
    if !got.found {
        return Err("PiBoSo's track tools downloaded but there's no terrained.exe in them".into());
    }
    Ok(got.path)
}

/// How far a track build has got.
///
/// A build is minutes of work with nothing to look at, so it reports where it is rather than
/// only what it produced. Keyed by slug: the studio's bar belongs to one track, and a second
/// build must not drive the first one's.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildProgress {
    slug: String,
    #[serde(flatten)]
    at: trackbuild::Progress,
}

/// The event a build reports itself on.

/// Everything a build produced, and where it ended up.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildResult {
    /// Each compiler run, in order.
    steps: Vec<trackbuild::Step>,
    /// The folder the source and the compiled files are in.
    dir: String,
    /// The archive, once every step has succeeded.
    pkz: Option<String>,
    /// Where it was installed, when it was asked for and worked.
    installed: Option<String>,
}

/// Export a track, run the compilers over it, and put the result where the game reads it.
///
/// The whole way, because a folder of source is homework and a compiled folder is still
/// homework — a track you can ride is a `.pkz` in the mods tree. The compilers are PiBoSo's
/// and Windows-only; on macOS they run through the same Wine prefix the game does.
#[tauri::command]
async fn build_track(
    app: tauri::AppHandle,
    program: serde_json::Value,
    dir: Option<String>,
    install: bool,
) -> Result<BuildResult, String> {
    let prog = track_program(program)?;
    let tools_at = ensure_track_tools(&app).await?;
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    let slug = tracksynth::slug(&prog.name);
    // Somewhere of its own when nobody picked a folder, so building is one press.
    let root = match dir {
        Some(d) if !d.trim().is_empty() => std::path::PathBuf::from(d),
        _ => app
            .path()
            .app_data_dir()
            .map_err(|e| format!("no data directory: {e}"))?
            .join("track-builds")
            .join(&slug),
    };
    let tracks = install.then(|| track_install_dir(&cfg)).transpose()?;

    tauri::async_runtime::spawn_blocking(move || {
        let tools = trackbuild::find(std::path::Path::new(&tools_at))
            .ok_or("There's no terrained.exe in that folder.".to_string())?;
        let mut plan = trackbuild::Plan::new(
            prog.terrain.samples,
            tools.tracked.is_some(),
            tracks.is_some(),
        );
        let slug_for_events = slug.clone();
        let say = |at: trackbuild::Progress| {
            let _ = app.emit(
                BUILD_EVENT,
                BuildProgress { slug: slug_for_events.clone(), at },
            );
        };

        say(plan.start("synthesising"));
        let syn = tracksynth::synthesise(&prog).map_err(|e| format!("{e:#}"))?;

        say(plan.start("writing"));
        std::fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
        tracksynth::write_source(&prog, &syn, &root).map_err(|e| format!("{e:#}"))?;

        let steps = trackbuild::compile(&tools, &root, &slug, &cfg.game_path, &mut |phase| {
            say(plan.start(phase))
        })
        .map_err(|e| format!("{e:#}"))?;

        let mut out = BuildResult {
            dir: root.to_string_lossy().into_owned(),
            pkz: None,
            installed: None,
            steps,
        };
        // Only a build that got all the way through is worth packaging: a `.pkz` missing its
        // `.map` is a track the game lists and then refuses to load.
        if out.steps.iter().all(|s| s.ok) {
            say(plan.start("packaging"));
            let pkz = root.join(format!("{slug}.pkz"));
            trackbuild::package(&root, &slug, &pkz).map_err(|e| format!("{e:#}"))?;
            out.pkz = Some(pkz.to_string_lossy().into_owned());
            if let Some(tracks) = tracks {
                say(plan.start("installing"));
                let at = trackbuild::install(&pkz, &tracks).map_err(|e| format!("{e:#}"))?;
                usage::track("track.build.install");
                out.installed = Some(at.to_string_lossy().into_owned());
            }
        }
        // Closes the last phase, so what it cost is remembered and the next build is paced
        // by this machine rather than by the one the defaults were measured on.
        plan.finish();
        Ok(out)
    })
    .await
    .map_err(|e| format!("build_track task failed: {e}"))?
}


/// The models a track ships that a prop can be placed by name.

/// Read source images for the studio — the pixels land in the texture store, so the UI
/// previews them through exactly the same path as a decoded paint's.
#[tauri::command]
async fn paint_studio_load(paths: Vec<String>) -> Result<Vec<paintstudio::StudioImage>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .iter()
            .map(|p| paintstudio::inspect(std::path::Path::new(p)).map_err(|e| format!("{e:#}")))
            .collect()
    })
    .await
    .map_err(|e| format!("paint_studio_load task failed: {e}"))?
}

/// Read one image at its full size, for the Designer to composite with.
///
/// Separate from `paint_studio_load` because that one answers "describe this file" with a
/// thumbnail, and the editor needs the pixels themselves — see `paintstudio::pixels`.
#[tauri::command]
async fn paint_studio_pixels(path: String) -> Result<paint::PaintTexture, String> {
    tauri::async_runtime::spawn_blocking(move || {
        paintstudio::pixels(std::path::Path::new(&path)).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("paint_studio_pixels task failed: {e}"))?
}

/// Stage a composited sheet, returning the file `paint_studio_save` should pack.
///
/// Takes the PNG as a raw request body rather than an argument: a 4096² sheet is megabytes,
/// and JSON would send it as a list of numbers. The sheet's texture name rides in a header
/// for the same reason — the body has to be the bytes and nothing else.
///
/// One staging directory per call. The caller saves immediately after staging every sheet, so
/// these are short-lived; they sit in the OS temp dir either way, which is where an editor
/// that's closed mid-flight should leave its scratch files.
#[tauri::command]
async fn paint_studio_stage(request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(png) = request.body() else {
        return Err("paint_studio_stage expects the sheet's PNG bytes as the request body".into());
    };
    let name = request
        .headers()
        .get("x-sheet-name")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let png = png.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let dir = mxb_core::names::staging_dir("paint");
        paintstudio::stage_sheet(&dir, &name, &png)
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("paint_studio_stage task failed: {e}"))?
}

/// Write a photo of the 3D preview to a path the user picked in a save dialog.
///
/// Raw body and a header, for the same reason [`paint_studio_stage`] takes one: a 4K frame is
/// megabytes and JSON would send it as a list of numbers. The path is percent-encoded, because
/// a header has to be ASCII and a Windows user's pictures folder is under their name.
///
/// Nothing is resolved or relocated here — the dialog already asked, and the file goes exactly
/// where it said. A `.png` is enforced so a typed name can't quietly write PNG bytes to
/// something that isn't one.

/// Build a `.pnt` from the chosen images and write it.
#[tauri::command]
async fn paint_studio_save(
    app: tauri::AppHandle,
    name: String,
    file_name: String,
    textures: Vec<paintstudio::BuildTexture>,
    dest: PaintDest,
    overwrite: bool,
) -> Result<SavedPaint, String> {
    let target = resolve_paint_dest(&app, &file_name, &dest)?;
    tauri::async_runtime::spawn_blocking(move || {
        if !overwrite && target.exists() {
            return Err(format!("{} is already there.", target.display()));
        }
        // The paint's own name is the one the game shows; an empty one falls back to the
        // file name, which is what every paint on disk is picked by anyway.
        let title = if name.trim().is_empty() {
            target.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
        } else {
            name.trim().to_string()
        };
        let bytes = paintstudio::build(&title, &textures).map_err(|e| format!("{e:#}"))?;
        let names = paint::texture_names(&bytes).map_err(|e| format!("{e:#}"))?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        std::fs::write(&target, &bytes)
            .map_err(|e| format!("write {}: {e}", target.display()))?;
        log::info!(
            "[paint studio] wrote {} ({} bytes, textures: {})",
            target.display(),
            bytes.len(),
            names.join(", ")
        );
        usage::track("paint.save");
        Ok(SavedPaint {
            path: target.to_string_lossy().into_owned(),
            textures: names,
            bytes: bytes.len() as u64,
        })
    })
    .await
    .map_err(|e| format!("paint_studio_save task failed: {e}"))?
}

/// Write a paint's sheets out as `.tga` files to edit — the way to start from a livery
/// that already fits the model instead of from a blank sheet.
#[tauri::command]
async fn paint_studio_extract(
    app: tauri::AppHandle,
    path: String,
    dest: Option<String>,
) -> Result<PaintTemplate, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let src = std::path::PathBuf::from(&path);
        let stem = src
            .file_stem()
            .map(|s| mxb_core::names::sanitize(&s.to_string_lossy()))
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "paint".to_string());
        let dir = match dest {
            Some(d) => std::path::PathBuf::from(d).join(&stem),
            None => templates_root(&app).join(&stem),
        };
        let bytes = std::fs::read(&src).map_err(|e| format!("read {}: {e}", src.display()))?;
        let files = paintstudio::extract(&bytes, &dir).map_err(|e| format!("{e:#}"))?;
        let textures = files
            .iter()
            .filter_map(|f| f.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .collect();
        Ok(PaintTemplate {
            dir: dir.to_string_lossy().into_owned(),
            files: files.iter().map(|f| f.to_string_lossy().into_owned()).collect(),
            textures,
        })
    })
    .await
    .map_err(|e| format!("paint_studio_extract task failed: {e}"))?
}

/// Where templates go when the player doesn't pick somewhere: their Documents folder, not
/// the mods folder — the game scans that, and a folder of loose sheets isn't a mod.

/// What a run over `paths` would touch — every file under the selection, with the ones it
/// would leave alone flagged and why. Folders are walked; a file is taken as itself.
#[tauri::command]
async fn content_lock_plan(paths: Vec<String>) -> Result<serde_json::Value, String> {
    #[cfg(sidecar)]
    {
        let roots: Vec<std::path::PathBuf> =
            paths.into_iter().map(std::path::PathBuf::from).collect();
        let items = tauri::async_runtime::spawn_blocking(move || sidecar_lock::plan(&roots))
            .await
            .map_err(|e| format!("content_lock_plan task failed: {e}"))?
            .map_err(|e| format!("{e:#}"))?;
        return serde_json::to_value(items).map_err(|e| e.to_string());
    }
    #[cfg(not(sidecar))]
    {
        let _ = paths;
        Err("this build can't lock content".into())
    }
}

/// Write a copy of every file in `paths`, locked to each GUID in `guids`, under
/// `out_dir/<GUID>/`. Reports progress on `content-lock://progress`.
///
/// The sources are only ever read. A creator's plaintext is the one thing they can't get
/// back, so the tool that hands out locked copies is not also the tool that could eat the
/// original.
#[tauri::command]
async fn content_lock_run(
    app: tauri::AppHandle,
    paths: Vec<String>,
    guids: Vec<String>,
    out_dir: String,
) -> Result<serde_json::Value, String> {
    #[cfg(sidecar)]
    {
        let roots: Vec<std::path::PathBuf> =
            paths.into_iter().map(std::path::PathBuf::from).collect();
        let out = std::path::PathBuf::from(out_dir);
        let outcome = tauri::async_runtime::spawn_blocking(move || {
            sidecar_lock::run(&app, &roots, &guids, &out)
        })
        .await
        .map_err(|e| format!("content_lock_run task failed: {e}"))?
        .map_err(|e| format!("{e:#}"))?;
        usage::track("content.protect");
        return serde_json::to_value(outcome).map_err(|e| e.to_string());
    }
    #[cfg(not(sidecar))]
    {
        let _ = (app, paths, guids, out_dir);
        Err("this build can't lock content".into())
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackToolsStatus {
    /// The folder someone pointed at, if they have.
    path: String,
    /// Whether `terrained.exe` was actually found in it.
    found: bool,
    /// Whether `tracked.exe` was too — without it the terrain still builds, it just has no
    /// centreline yet.
    has_tracked: bool,
}

/// Where PiBoSo's track tools live once the app has fetched them.
fn track_tools_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no data directory: {e}"))?
        .join("track-tools"))
}

const BUILD_EVENT: &str = "track-build-progress";

#[tauri::command]
async fn set_track_tools(app: tauri::AppHandle, dir: String) -> Result<TrackToolsStatus, String> {
    let mut cfg = config::load_or_detect(&app).unwrap_or_default();
    cfg.track_tools_path = dir.trim().to_string();
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    track_tools_status(app).await
}

/// What a generated track measures, so the studio can show it rather than assert it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackPreview {
    /// The `.pkz` written for the viewer. Terrain and surfaces only — no graphics, no
    /// scenery — so it previews and does not play.
    path: String,
    name: String,
    lap_m: f32,
    width_m: f32,
    features: usize,
    closure_m: f32,
    /// Height used against the budget it was given. A track using a tenth of its budget is
    /// quantising ten times coarser than it needs to.
    used_m: f32,
    budget_m: f32,
    /// The same measurements `trackstats` takes of published tracks.
    measured_width_m: f32,
    measured_length_m: f32,
    lips: usize,
    lips_per_km: f32,
    slope_p99_deg: f32,
    relief_p90_m: f32,
}

fn track_install_dir(cfg: &AppConfig) -> Result<std::path::PathBuf, String> {
    if cfg.mods_path.trim().is_empty() {
        return Err(format!(
            "No {} folder is configured yet — set it in Settings.",
            cfg.game().display
        ));
    }
    Ok(library::mods_subdir(&cfg.mods_path, "mods/tracks"))
}

/// Where a built paint is written.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum PaintDest {
    /// Under the game's `mods` folder — `bikes/<Bike>/paints`,
    /// `rider/helmets/<Helmet>/paints`, `rider/riders/<Profile>/gloves`…
    Mods { rel: String },
    /// A folder the player picked themselves, for a paint they mean to share rather than
    /// install.
    Folder { path: String },
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedPaint {
    path: String,
    textures: Vec<String>,
    bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PaintTemplate {
    dir: String,
    files: Vec<String>,
    textures: Vec<String>,
}

/// `<dir>/<name>.pnt` for a destination, refusing anything that isn't a paint sitting where
/// a paint belongs.
///
/// The `Mods` arm goes through [`mxb_core::names::safe_dest`] — the same check that vets a
/// destination sent by another player over paint sync. Nothing here is remote, but the rule
/// it enforces (a relative path, no traversal, at least two segments deep, ending in
/// `.pnt`) is exactly the rule a paint destination has to satisfy, and one boundary with
/// tests beats a second one written from memory.
fn resolve_paint_dest(
    app: &tauri::AppHandle,
    file_name: &str,
    dest: &PaintDest,
) -> Result<std::path::PathBuf, String> {
    let stem = mxb_core::names::sanitize(file_name.trim())
        .trim()
        .trim_end_matches('.')
        .trim_end_matches(".pnt")
        .trim()
        .to_string();
    if stem.is_empty() {
        return Err("Name this paint before saving it.".into());
    }
    let file = format!("{stem}.pnt");
    match dest {
        PaintDest::Mods { rel } => {
            let cfg = config::load(app).map_err(|e| format!("{e:#}"))?;
            let mods_dir = library::mods_subdir(&cfg.mods_path, "mods");
            let rel = rel.replace('\\', "/");
            let rel = rel.trim_matches('/');
            mxb_core::names::safe_dest(&mods_dir, &format!("{rel}/{file}"))
                .ok_or_else(|| format!("'{rel}' isn't a folder a paint can be installed into"))
        }
        PaintDest::Folder { path } => {
            let dir = std::path::PathBuf::from(path);
            if !dir.is_dir() {
                return Err(format!("{} isn't a folder", dir.display()));
            }
            Ok(dir.join(file))
        }
    }
}

fn templates_root(app: &tauri::AppHandle) -> std::path::PathBuf {
    dirs_next::document_dir()
        .or_else(|| app.path().app_data_dir().ok())
        .unwrap_or_else(std::env::temp_dir)
        // NOT renamed with the product: this folder already exists on players' machines
        // and holds templates they exported. Moving it orphans them.
        .join("MXB App")
        .join("Paint Templates")
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PaintTarget {
    path: String,
    exists: bool,
}

#[tauri::command]
async fn photo_save(request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(png) = request.body() else {
        return Err("photo_save expects the PNG bytes as the request body".into());
    };
    let raw = request
        .headers()
        .get("x-dest")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let dest = percent_encoding::percent_decode_str(raw).decode_utf8_lossy().into_owned();
    if dest.is_empty() {
        return Err("photo_save needs a destination".into());
    }
    let png = png.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut path = std::path::PathBuf::from(&dest);
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
            path.set_extension("png");
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{dir:?}: {e}"))?;
        }
        std::fs::write(&path, &png).map_err(|e| format!("{path:?}: {e}"))?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| format!("photo_save task failed: {e}"))?
}

/// The bytes of a `.psd`, for the Designer to take apart in the webview.
///
/// Parsing happens up there rather than here, because that is where the pixels have to end
/// up: a layer becomes an `ImageBitmap` on a canvas, and a Rust-side decode would only mean
/// re-encoding every layer to cross back. So this is the whole of the backend's part —
/// hand over the file.
///
/// Restricted to the two Photoshop extensions on purpose. Nothing else has any business
/// being read wholesale into the webview, and a command that would do it for any path is a
/// wider door than this feature needs.
#[tauri::command]
async fn psd_read(path: String) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = std::path::PathBuf::from(&path);
        let ok = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("psd") || e.eq_ignore_ascii_case("psb"));
        if !ok {
            return Err(format!("{path:?} is not a .psd"));
        }
        let len = std::fs::metadata(&path).map_err(|e| format!("{path:?}: {e}"))?.len();
        if len > PSD_LIMIT {
            return Err(format!("{path:?} is {} MB — too large to open", len / (1024 * 1024)));
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("{path:?}: {e}"))?;
        Ok(tauri::ipc::Response::new(bytes))
    })
    .await
    .map_err(|e| format!("psd_read task failed: {e}"))?
}

/// Write one sheet's `.psd` to a path the user picked.
///
/// Same shape as [`photo_save`], and for the same reason: a 4096² document with its layers
/// still separate runs to tens of megabytes, so the file is the request body and the
/// destination rides in a percent-encoded header.
///
/// Nothing is resolved or relocated — the dialog already asked. The extension is enforced so
/// a typed name can't leave PSD bytes in a file Photoshop won't offer to open.
#[tauri::command]
async fn psd_save(request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(psd) = request.body() else {
        return Err("psd_save expects the PSD bytes as the request body".into());
    };
    let raw = request
        .headers()
        .get("x-dest")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let dest = percent_encoding::percent_decode_str(raw).decode_utf8_lossy().into_owned();
    if dest.is_empty() {
        return Err("psd_save needs a destination".into());
    }
    let psd = psd.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut path = std::path::PathBuf::from(&dest);
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("psd")) {
            path.set_extension("psd");
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{dir:?}: {e}"))?;
        }
        std::fs::write(&path, &psd).map_err(|e| format!("{path:?}: {e}"))?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| format!("psd_save task failed: {e}"))?
}

/// The file a save would write, resolved but not written — so the UI can ask before
/// replacing a paint that's already there.
#[tauri::command]
fn paint_studio_target(
    app: tauri::AppHandle,
    file_name: String,
    dest: PaintDest,
) -> Result<PaintTarget, String> {
    let path = resolve_paint_dest(&app, &file_name, &dest)?;
    Ok(PaintTarget { exists: path.is_file(), path: path.to_string_lossy().into_owned() })
}

/// The texture names a destination can paint.
///
/// A paint binds by name: call a sheet `livery` and it lands on the bodywork that asked for
/// `livery`, call it `my_livery` and it lands nowhere. Two sources answer it, and both are
/// needed. The paints already installed name what *they* replace — read from their headers,
/// so that half costs no pixels. The model's own mesh names everything it draws, which is
/// the half a paint can't supply: the OEM bikes ship a stock paint that replaces the wheels
/// and the chain and nothing else, so on a stock Husqvarna the sheets on offer were `chain`,
/// `wheel`, `wheels` — and `plastics`, the one anybody opens the Designer for, was missing.
#[tauri::command]
async fn paint_studio_hints(app: tauri::AppHandle, rel: String) -> Result<Vec<String>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let rel = rel.replace('\\', "/").trim_matches('/').to_string();
        Ok(paint_hints(&library::mods_subdir(&cfg.mods_path, &format!("mods/{rel}"))))
    })
    .await
    .map_err(|e| format!("paint_studio_hints task failed: {e}"))?
}

/// The texture names of the `.pnt` files sitting loose in `dir`, sampled.
fn loose_paint_names(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = 0usize;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if seen >= PAINT_SAMPLE || !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pnt")) {
            continue;
        }
        // Seeked, not read: a bike's paints are tens of megabytes each and the names are
        // in their headers. Reading eight of them whole put nineteen seconds between
        // picking a model and being told what it wants.
        if let Ok(found) = paint::texture_names_at(&p) {
            out.extend(found);
            seen += 1;
        }
    }
    out
}

/// Every texture a model's own mesh carries, by name.
///
/// Read from the mesh's texture records — names and dimensions, never the pixels beside
/// them — so a 54 MB bike is answered by a walk over its bytes rather than by inflating it.
/// A model that ships as a folder is read from there, and one that ships packed from the
/// `<Model>.pkz` beside it; a sealed file is unwrapped the way the viewer unwraps it.
fn mesh_texture_names(model_dir: &std::path::Path) -> Vec<String> {
    let mut meshes: Vec<Vec<u8>> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(model_dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_file() && p.file_name().and_then(|n| n.to_str()).is_some_and(bikefiles::is_mesh)
            {
                // Reading one back from iCloud or OneDrive costs minutes, and this is a
                // convenience: a rider profile whose two 67 MB meshes had been evicted put
                // 84 seconds between picking it and seeing any sheet names. The preview
                // fetches the model when it actually draws it — that wait buys a picture.
                if cloudfiles::is_placeholder(&p) {
                    log::info!("[paint studio] skipping evicted mesh {}", p.display());
                    continue;
                }
                meshes.extend(viewer::read_gear_file(&p));
            }
        }
    }
    if meshes.is_empty() {
        let pkz = library::sibling_pkz(model_dir);
        if pkz.is_file() && !cloudfiles::is_placeholder(&pkz) {
            for (_, d) in pkz::read_selected(&pkz, bikefiles::is_mesh).unwrap_or_default() {
                meshes.push(pkz::read_sidecar_blob(&d).unwrap_or(d));
            }
        }
    }
    let mut names: Vec<String> = Vec::new();
    for mesh in &meshes {
        for t in edf::embedded_textures(mesh) {
            if !names.iter().any(|s| s.eq_ignore_ascii_case(&t.name)) {
                names.push(t.name);
            }
        }
    }
    names
}

#[cfg(test)]
mod build_progress_tests {
    use super::*;

    /// The studio reads these five names off the event. `#[serde(flatten)]` is the only thing
    /// holding the phase's own fields at the top level, and nothing about that is checked by
    /// the compiler — get it wrong and the bar simply never moves.
    #[test]
    fn a_build_reports_one_flat_object() {
        let at = trackbuild::Plan::new(2049, true, true).start("map");
        let json = serde_json::to_value(BuildProgress {
            slug: "corpus_national".into(),
            at,
        })
        .unwrap();
        let obj = json.as_object().expect("an object");
        let mut names: Vec<&str> = obj.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, ["expect", "from", "phase", "slug", "to"]);
        assert_eq!(obj["phase"], "map");
        assert_eq!(obj["slug"], "corpus_national");
        assert!(obj["to"].as_f64().unwrap() > obj["from"].as_f64().unwrap());
    }
}

#[cfg(test)]
mod track_install_tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("frost-trackdest-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_track_installs_into_the_mods_tree_not_the_install_dir() {
        let user = tmp("user");
        std::fs::create_dir_all(user.join("mods").join("tracks")).unwrap();
        let cfg = AppConfig {
            mods_path: user.to_string_lossy().into_owned(),
            // Deliberately set, and deliberately not where the track goes.
            game_path: "/somewhere/else/MX Bikes".into(),
            ..Default::default()
        };
        assert_eq!(
            track_install_dir(&cfg).unwrap(),
            user.join("mods").join("tracks")
        );
        let _ = std::fs::remove_dir_all(&user);
    }

    /// `mxbikes.ini` lets a player point the game at `C:\mods`, and then `mods_path` *is*
    /// the tree — joining `mods` on by hand would write to `C:\mods\mods\tracks`.
    #[test]
    fn a_relocated_tree_is_the_mods_folder_itself() {
        let tree = tmp("tree");
        for d in ["bikes", "tracks", "rider"] {
            std::fs::create_dir_all(tree.join(d)).unwrap();
        }
        let cfg = AppConfig {
            mods_path: tree.to_string_lossy().into_owned(),
            ..Default::default()
        };
        assert_eq!(track_install_dir(&cfg).unwrap(), tree.join("tracks"));
        let _ = std::fs::remove_dir_all(&tree);
    }

    /// With no folder configured the old code joined onto an empty `game_path` and wrote
    /// `mods/tracks` relative to the working directory — a track installed into thin air.
    #[test]
    fn no_configured_folder_is_an_error_not_a_relative_path() {
        let cfg = AppConfig { mods_path: String::new(), ..Default::default() };
        let err = track_install_dir(&cfg).unwrap_err();
        assert!(err.contains("configured"), "{err}");
    }
}

/// [`paint_studio_hints`] for a destination folder that's already been resolved.
fn paint_hints(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    fn add(names: &mut Vec<String>, found: Vec<String>) {
        for n in found {
            if !n.is_empty() && !names.iter().any(|s| s.eq_ignore_ascii_case(&n)) {
                names.push(n);
            }
        }
    }

    add(&mut names, loose_paint_names(dir));
    // Nothing installed loose: the model may be packed, and its own paints are the
    // same evidence. `<Model>.pkz` sits beside the `<Model>` folder this destination
    // lives in — for a bike as much as for a helmet.
    if names.is_empty() {
        if let (Some(sub), Some(model_dir)) = (dir.file_name(), dir.parent()) {
            let pkz = library::sibling_pkz(model_dir);
            let tail = format!("/{}/", sub.to_string_lossy().to_ascii_lowercase());
            if pkz.is_file() {
                let want = |n: &str| {
                    let n = n.replace('\\', "/").to_ascii_lowercase();
                    n.contains(&tail) && n.ends_with(".pnt")
                };
                let packed = pkz::read_selected(&pkz, want).unwrap_or_default();
                for (_, bytes) in packed.iter().take(PAINT_SAMPLE) {
                    add(&mut names, paint::texture_names_any(bytes).unwrap_or_default());
                }
            }
        }
    }
    // A rider profile that ships its folders empty wears the stock profile's kits.
    //
    // `Rider+` and `Rider+RolledUp` do exactly that on purpose — the kits installed under
    // `default_mx` are meant to work on them, which is why `read_rider_paint_file` reaches
    // there to render one. The names are the same names, so the hints have to reach there
    // too, or painting for one of those profiles starts with nothing to call a sheet. It
    // also spares the walk over the profile's own mesh, which for `Rider+` is 67 MB of
    // rider read to learn what nine installed kits already say.
    if names.is_empty() {
        if let Some((sub, riders)) = dir.file_name().zip(dir.parent().and_then(|p| p.parent())) {
            if riders.file_name().is_some_and(|n| n.eq_ignore_ascii_case(game::RIDERS_DIR)) {
                for stock in game::active().rider.stock_profiles {
                    add(&mut names, loose_paint_names(&riders.join(stock).join(&sub)));
                    if !names.is_empty() {
                        break;
                    }
                }
            }
        }
    }
    // What the model itself draws, whether or not a paint has ever replaced it.
    //
    // Only for the model's own `paints` folder. A mesh names every texture on the item
    // without saying which of them belong to the goggles hanging off it — the paints are
    // the only thing that says that (see `on_goggle_side`) — and offering a helmet's shell
    // sheet to somebody painting its goggles would put the shell in the wrong file.
    let main_paints = dir.file_name().is_some_and(|s| s.eq_ignore_ascii_case("paints"));
    let mesh = match (main_paints, dir.parent()) {
        (true, Some(model_dir)) => mesh_texture_names(model_dir),
        _ => Vec::new(),
    };
    add(&mut names, mesh.clone());

    // Drop a name that is another paint's misspelling of one the model actually binds.
    //
    // A `.pnt` supplies textures by name, so a sheet the model never asks for changes
    // nothing — and these names come from paints as much as from the mesh, misspellings and
    // all. The KTM 250 SX-F binds `plastics_n`; a paint installed beside it calls its own
    // sheet `plastics-n`, and the two sat next to each other in the list, one character
    // apart, with the dead one first. Painting it is work that cannot reach the bike.
    //
    // Only a name that collides with a bound one is dropped, and only by separator or case.
    // A paint is free to ship sheets the mesh never mentions — `tyres` and `wheel` come off
    // the wheels rather than the bike — and those are left alone.
    if !mesh.is_empty() {
        let key = |s: &str| s.to_ascii_lowercase().replace('-', "_");
        let bound: std::collections::HashSet<String> = mesh.iter().map(|n| key(n)).collect();
        names.retain(|n| {
            mesh.iter().any(|m| m.eq_ignore_ascii_case(n)) || !bound.contains(&key(n))
        });
    }
    names.sort_by_key(|n| n.to_lowercase());
    names
}

#[cfg(test)]
mod mesh_texture_tests {
    use super::{mesh_texture_names, paint_hints};

    /// The smallest thing `edf::embedded_textures` reads as a texture record: a
    /// null-terminated name, then the fields it validates by shape at a fixed offset.
    fn mesh_naming(texture: &str) -> Vec<u8> {
        const W_FROM_NAME: usize = 100;
        let mut b = vec![0u8; W_FROM_NAME];
        b[..texture.len()].copy_from_slice(texture.as_bytes());
        b.extend_from_slice(&64u32.to_le_bytes()); // width, from the fixed size set
        b.extend_from_slice(&64u32.to_le_bytes()); // height
        b.extend_from_slice(&[0u8; 16]); // digest
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&12u32.to_le_bytes()); // payload, counting the pad
        b.extend_from_slice(&[0u8; 8]); // pad
        b.extend_from_slice(&[1u8; 4]); // payload
        b
    }

    #[test]
    fn a_models_own_textures_come_from_its_mesh() {
        let root = std::env::temp_dir().join(format!("frost-mesh-tex-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("MyBike");
        std::fs::create_dir_all(&dir).unwrap();
        // No paint here at all — the mesh is the only thing that can name `plastics`.
        std::fs::write(dir.join("model.edf"), mesh_naming("plastics")).unwrap();
        assert_eq!(mesh_texture_names(&dir), vec!["plastics".to_string()]);

        std::fs::create_dir_all(root.join("Bare")).unwrap();
        assert!(mesh_texture_names(&root.join("Bare")).is_empty(), "no mesh, nothing to say");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A sheet name that only exists because another paint misspelt one the model binds is
    /// not offered. The KTM 250 SX-F binds `plastics_n`; a paint beside it ships
    /// `plastics-n`, which the game asks for on no part of the bike, so painting it is work
    /// that cannot show up. Names the mesh never mentions at all are somebody else's sheet
    /// and are left alone.
    #[test]
    fn a_paints_misspelling_of_a_bound_sheet_is_not_offered() {
        let root = std::env::temp_dir().join(format!("frost-dead-sheet-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bike = root.join("Bike");
        std::fs::create_dir_all(bike.join("paints")).unwrap();
        let mut mesh = mesh_naming("plastics");
        mesh.extend_from_slice(&mesh_naming("plastics_n"));
        std::fs::write(bike.join("model.edf"), mesh).unwrap();
        std::fs::write(
            bike.join("paints").join("Someone.pnt"),
            super::paint::encode(
                "Someone",
                &[
                    super::paint::PntTexture {
                        name: "plastics-n".into(),
                        width: 2,
                        height: 2,
                        rgba: vec![0; 16],
                    },
                    super::paint::PntTexture {
                        name: "tyres".into(),
                        width: 2,
                        height: 2,
                        rgba: vec![0; 16],
                    },
                ],
            )
            .unwrap(),
        )
        .unwrap();

        let names = paint_hints(&bike.join("paints"));
        assert!(names.iter().any(|n| n == "plastics_n"), "the bound spelling stays: {names:?}");
        assert!(!names.iter().any(|n| n == "plastics-n"), "the dead one goes: {names:?}");
        assert!(names.iter().any(|n| n == "tyres"), "a sheet of its own is not ours to drop: {names:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A model's own textures are offered where a paint for it goes, and nowhere else: the
    /// goggles beside it are a different file, painted from a different sheet.
    #[test]
    fn only_the_models_own_paints_folder_is_offered_the_mesh() {
        let root = std::env::temp_dir().join(format!("frost-mesh-hints-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let helmet = root.join("Helmet");
        std::fs::create_dir_all(helmet.join("paints")).unwrap();
        std::fs::create_dir_all(helmet.join("goggles")).unwrap();
        std::fs::write(helmet.join("helmet.edf"), mesh_naming("shell")).unwrap();

        assert_eq!(paint_hints(&helmet.join("paints")), vec!["shell".to_string()]);
        assert!(paint_hints(&helmet.join("goggles")).is_empty(), "the shell is not a goggle sheet");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A packed model whose name carries a version number: `Fox Instinct 2.0 by Aeffertz`
    /// has no folder on disk at all, only the archive beside where one would be, and the
    /// dot in the name is not an extension to be replaced. Getting that wrong asked for
    /// `Fox Instinct 2.pkz`, and the Designer offered no sheet names for the boots — so
    /// nothing suggested `fox`, and a sheet named anything else paints nothing.
    #[test]
    fn a_packed_model_with_a_dot_in_its_name_still_names_its_sheets() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!("frost-dotted-pkz-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let packed = root.join("Fox Instinct 2.0 by Aeffertz.pkz");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&packed).unwrap());
            w.start_file::<_, ()>("boots.edf", zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(&mesh_naming("fox")).unwrap();
            w.finish().unwrap();
        }

        // The destination the picker aims at: a `paints` folder under a model folder that
        // was never unpacked, which is where a paint for a packed mod has to go.
        let dest = root.join("Fox Instinct 2.0 by Aeffertz").join("paints");
        assert_eq!(paint_hints(&dest), vec!["fox".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `Rider+` and `Rider+RolledUp` ship `paints/` and `gloves/` empty on purpose: the kits
    /// installed under the stock profile are the ones meant to be worn on them, which is
    /// what `read_rider_paint_file` already does when it renders one. So the sheet names
    /// have to come from there too — otherwise painting a kit or a pair of gloves for such
    /// a profile starts with nothing to call the sheet, and a sheet named by guesswork
    /// binds to nothing.
    #[test]
    fn a_profile_that_ships_no_paints_borrows_the_stock_ones() {
        let root = std::env::temp_dir().join(format!("frost-stock-kit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let riders = root.join("riders");
        let mine = riders.join("Rider+");
        std::fs::create_dir_all(mine.join("paints")).unwrap();
        std::fs::create_dir_all(mine.join("gloves")).unwrap();

        let pnt = |name: &str| {
            crate::paint::encode(
                "Stock",
                &[crate::paint::PntTexture {
                    name: name.to_string(),
                    width: 4,
                    height: 4,
                    rgba: vec![0u8; 4 * 4 * 4],
                }],
            )
            .unwrap()
        };
        let stock = riders.join("default_mx");
        std::fs::create_dir_all(stock.join("paints")).unwrap();
        std::fs::create_dir_all(stock.join("gloves")).unwrap();
        std::fs::write(stock.join("paints").join("Kit.pnt"), pnt("rider")).unwrap();
        std::fs::write(stock.join("gloves").join("Gloves.pnt"), pnt("gloves")).unwrap();

        assert_eq!(paint_hints(&mine.join("paints")), vec!["rider".to_string()]);
        assert_eq!(
            paint_hints(&mine.join("gloves")),
            vec!["gloves".to_string()],
            "a gloves folder is never offered the mesh's names, so this is its only source",
        );
        // A profile with kits of its own is answered by those, not by the stock ones.
        std::fs::write(mine.join("paints").join("Mine.pnt"), pnt("rider_mine")).unwrap();
        assert_eq!(paint_hints(&mine.join("paints")), vec!["rider_mine".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The whole hint list for a real destination:
    /// `MXB_PAINT_DEST='…/mods/bikes/MX1OEM_2023_Husqvarna_FC_450/paints' \
    ///   cargo test paint_hints_from_env -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn paint_hints_from_env() {
        let Ok(dir) = std::env::var("MXB_PAINT_DEST") else {
            eprintln!("set MXB_PAINT_DEST to run");
            return;
        };
        let t = std::time::Instant::now();
        let names = paint_hints(std::path::Path::new(&dir));
        eprintln!("{dir} expects {names:?} ({:?})", t.elapsed());
        assert!(!names.is_empty(), "a destination with a model behind it expects something");
    }
}

/// How big a `.psd` this will open. A 4096² sheet with a couple of dozen layers is well
/// inside this; the cap exists so a mistyped path at a 4 GB video doesn't try to cross the
/// IPC channel as one allocation.
const PSD_LIMIT: u64 = 512 * 1024 * 1024;

/// How many paints are read for their names. A handful is plenty: paints for one model
/// overwhelmingly supply the same names, and this runs every time the destination changes.
const PAINT_SAMPLE: usize = 8;

/// The viewer builds the model, but burying and raising a gear package is this app's
/// business, so the round trip is tested here rather than in `mxb_core::viewer`.
#[cfg(test)]
mod gear_repair_crossing_tests {
    use crate::gearrepair;
    use mxb_core::viewer::*;
    use std::path::{Path, PathBuf};

    #[test]
    #[ignore]
    fn a_buried_package_loads_once_the_repair_has_raised_it() {
        let Ok(path) = std::env::var("MXB_REAL_GEAR") else {
            eprintln!("set MXB_REAL_GEAR to an installed gear .pkz to run");
            return;
        };
        let src = std::path::Path::new(&path);
        assert!(src.is_file(), "MXB_REAL_GEAR must be a .pkz file for this one");
        let name = src.file_name().unwrap().to_string_lossy().into_owned();

        let root = std::env::temp_dir().join(format!("frost-buried-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let helmets = root.join("mods/rider/helmets");
        let buried = helmets.join("shop-44");
        std::fs::create_dir_all(&buried).unwrap();
        std::fs::hard_link(src, buried.join(&name))
            .or_else(|_| std::fs::copy(src, buried.join(&name)).map(|_| ()))
            .expect("stage the package");

        // Where the install put it: the folder is what the picker offers, and it has no mesh.
        let buried_load = mxb_core::viewer::load_gear_model_blocking(
            buried.to_string_lossy().into_owned(),
            "helmet".into(),
            None,
            None,
            false,
            false,
            Vec::new(),
        );
        let err = buried_load.err().expect("a buried package must not load");
        assert!(err.contains("no gear mesh found"), "unexpected error: {err}");

        let plans = gearrepair::plan(root.to_str().unwrap());
        assert_eq!(plans.len(), 1, "the burial is found");
        let moved = gearrepair::apply_one(root.to_str().unwrap(), &plans[0].id).unwrap();
        assert_eq!(moved, 1);

        let raised = helmets.join(&name);
        assert!(raised.is_file(), "the package is in the area root");
        let part = mxb_core::viewer::load_gear_model_blocking(
            raised.to_string_lossy().into_owned(),
            "helmet".into(),
            None,
            None,
            false,
            false,
            Vec::new(),
        )
        .expect("load the raised package");
        eprintln!(
            "{name} -> {} node(s), textures {:?}",
            part.nodes.len(),
            part.textures.iter().map(|t| &t.name).collect::<Vec<_>>()
        );
        assert!(!part.nodes.is_empty(), "it draws something");
        assert!(!part.textures.is_empty(), "and it is textured");
        let _ = std::fs::remove_dir_all(&root);
    }
}

/// Whether this build can produce protected copies of a creator's files. Same shape as
/// [`bike_preview_available`]: the optional local module carries the format, so a build
/// without it hides the tool rather than offering one that can't do anything.
#[tauri::command]
fn content_lock_available() -> bool {
    cfg!(sidecar)
}

/// Generate a protected copy of a track for a **specific Steam ID**, leaving the original
/// untouched.
///
/// This is the creator's action. Given a track and the buyer's 17-digit Steam ID, it writes two
/// files beside the original:
///
/// - `<track>.mxbsecure` — the encrypted blob.
/// - `<track>.mxbsecure.mxbkey` — the content key sealed to that Steam ID.
///
/// Both are needed, next to each other, to load; the key opens only on the machine signed into
/// that Steam account. The original `.pkz` is never modified — the creator keeps their master,
/// and hands the buyer only the two generated files.
#[tauri::command]
async fn mxbsecure_generate(
    track_path: String,
    steam_id: String,
) -> Result<SecureGenerateOutcome, String> {
    #[cfg(mxbsecure)]
    {
        use std::path::Path;

        let steam_id = steam_id.trim().to_string();
        if steam_id.len() != 17 || !steam_id.bytes().all(|b| b.is_ascii_digit()) {
            return Err("that isn't a Steam ID — it should be 17 digits (a SteamID64)".into());
        }

        let plaintext = tokio::fs::read(&track_path)
            .await
            .map_err(|e| format!("read {track_path}: {e}"))?;
        // Refuse a file that is already one of ours, so a double-encrypt can't seal ciphertext.
        if plaintext.starts_with(b"MXBSEC") {
            return Err("that file is already protected".into());
        }
        let name = Path::new(&track_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .ok_or("not a file")?;

        let mut rnd = [0u8; 6];
        getrandom::getrandom(&mut rnd).map_err(|e| e.to_string())?;
        let suffix: String = rnd.iter().map(|b| format!("{b:02x}")).collect();
        let asset_id = format!("{}-{suffix}", mxb_core::names::sanitize_asset_id(&name));

        let locked = mxbsecure::lock(&plaintext, &asset_id, "k1");
        let sealed = mxbsecure::seal_key_to_identity(&locked.content_key, &steam_id, "");

        // `<track>.mxbsecure` and `<track>.mxbsecure.mxbkey`, beside the original.
        let blob_path = format!("{track_path}.mxbsecure");
        let mxbkey_path = format!("{blob_path}.mxbkey");
        // Blob is written to a temp and renamed, so a crash mid-write leaves no half file.
        let tmp = format!("{blob_path}.writing");
        tokio::fs::write(&tmp, &locked.blob).await.map_err(|e| format!("write blob: {e}"))?;
        tokio::fs::rename(&tmp, &blob_path).await.map_err(|e| format!("finish blob: {e}"))?;
        tokio::fs::write(&mxbkey_path, &sealed).await.map_err(|e| format!("write .mxbkey: {e}"))?;

        Ok(SecureGenerateOutcome {
            game_name: name,
            blob_path,
            mxbkey_path,
            steam_id,
            plain_bytes: plaintext.len() as u64,
        })
    }
    #[cfg(not(mxbsecure))]
    {
        let _ = (track_path, steam_id);
        Err("this build can't generate protected content".into())
    }
}

/// What generating a protected copy produced. The two files a buyer needs, side by side.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SecureGenerateOutcome {
    /// The name the game will list and open (the original file's name, e.g. `FarmSX.pkz`).
    game_name: String,
    /// The encrypted blob: `<original>.mxbsecure`.
    blob_path: String,
    /// The key sealed to the buyer's Steam ID: `<original>.mxbsecure.mxbkey`.
    mxbkey_path: String,
    /// The Steam ID it was sealed to.
    steam_id: String,
    plain_bytes: u64,
}

// ─────────────────────────────────────────────────────────────────────────────────────
// The studio's own wrappers over shared core.
//
// Deliberately not shared command *wrappers*: the manager's versions of these do work that
// only the manager has any business doing — reconciling the install ledger, claiming a GUID
// against the control plane, folding in sound swaps and ReShade presets. Core owns the
// logic; each app wraps the part of it that app actually means.
// ─────────────────────────────────────────────────────────────────────────────────────

#[tauri::command]
fn get_config(app: tauri::AppHandle) -> config::AppConfig {
    config::load(&app).unwrap_or_default()
}

#[tauri::command]
fn list_games() -> Vec<game::GameInfo> {
    game::all_info()
}

/// Whether this build can decode real bike geometry (the optional local module is compiled
/// in). Public builds without it return `false`, so the UI hides the preview rather than
/// offering one that cannot work.
#[tauri::command]
fn bike_preview_available() -> bool {
    cfg!(sidecar)
}

#[tauri::command]
fn reveal_in_explorer(path: String) -> Result<(), String> {
    library::reveal_in_explorer(&path).map_err(|e| format!("{e:#}"))
}

/// The installed library, without the manager's reconciliation.
///
/// The manager's version also folds in sound swaps and ReShade presets and nudges the
/// install ledger — all of which are about *managing* mods. The studio only needs to know
/// what is on disk to paint it.
#[tauri::command]
async fn scan_library(
    app: tauri::AppHandle,
    subpath: String,
) -> Result<Vec<library::LibraryEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        library::scan_library(&cfg.mods_path, &subpath, &[], cfg.game())
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("scan_library task failed: {e}"))?
}

#[tauri::command]
async fn scan_bike_targets(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(library::scan_bike_targets(&cfg.mods_path, &cfg.profiles_dir()))
    })
    .await
    .map_err(|e| format!("scan_bike_targets task failed: {e}"))?
}

#[tauri::command]
async fn scan_rider_targets(app: tauri::AppHandle) -> Result<library::RiderTargets, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(library::scan_rider_targets(&cfg.mods_path))
    })
    .await
    .map_err(|e| format!("scan_rider_targets task failed: {e}"))?
}

#[tauri::command]
fn presets_save(app: tauri::AppHandle, preset: presets::Preset) -> Result<(), String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no data directory: {e}"))?
        .join("presets");
    presets::save_preset(&dir, preset).map_err(|e| format!("{e:#}"))?;
    usage::track("preset.save");
    Ok(())
}

/// What the Protect tab needs to know: whether a GUID has been set, and what it is.
///
/// A narrow slice of the manager's command of the same name. Paint sync, enrolment and the
/// release badge are all its business, not this app's — the shared key is the GUID, which a
/// creator sets here to seal content to themselves.
#[tauri::command]
fn experimental_state(app: tauri::AppHandle) -> serde_json::Value {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    serde_json::json!({
        "version": app.package_info().version.to_string(),
        "guid": cfg.cp_guid,
        "enrolled": !cfg.cp_token.trim().is_empty(),
    })
}

/// Record the GUID a creator typed.
///
/// The manager claims a GUID against the control plane; this only writes it down. Reading it
/// out of the running game is `gameproc`'s job and stays with the manager — the studio asks
/// the creator to type it, exactly as they already do for every buyer.
#[tauri::command]
fn set_guid(app: tauri::AppHandle, guid: String) -> Result<(), String> {
    let mut cfg = config::load_or_detect(&app).unwrap_or_default();
    cfg.cp_guid = guid.trim().to_string();
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

