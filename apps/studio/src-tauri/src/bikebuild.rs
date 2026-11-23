//! The bike builder's output (phase E): the slotted parts put together in Blender and
//! written out as a bike folder, `model.fbx` and its shadow with the `.hrc`s and `gfx.cfg`,
//! and the `.edf`s the game loads when a converter is set up.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::bikeassemble::{self as asm, BuildSettings, Group, TemplateSource};
use crate::bikecmd::{anyhow_str, app_dir, blender_exe, cache_dir, err, lock, template_or_placeholder};
use crate::bikeparts::{self, Role};
use crate::blender;

/// The converter a developer points Studio at. It is private and never ships with Studio or
/// lives in this repository: without it the build stops at FBX, for fbx3edf.
const CONVERTER_ENV: &str = "FROST_FBX2EDF";

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildReport {
    folder: String,
    files: Vec<String>,
    /// Triangles per game part, the model's and the shadow's.
    tris: Value,
    shadow_tris: Value,
    /// Whether model.edf and model_shadow.edf were written here.
    converted: bool,
    /// What the converter said, or what to do instead.
    converter: String,
    rideable: bool,
    notes: Vec<String>,
}

/// What the build says when the chosen template can't be read and the placeholder stood in,
/// as it does in the preview: an OEM `.pkz` is protected, so a base bike picked from one
/// builds on the placeholder's mounts rather than not at all.
fn template_note(problem: Option<&str>) -> Option<String> {
    problem.map(|p| format!("The template bike couldn't be read, so this was built on the placeholder: {p}"))
}

fn run_converter(exe: &Path, fbx: &Path, edf: &Path) -> Result<String, String> {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg(fbx).arg(edf).arg("--parts").stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("couldn't run the converter: {e}"))?;
    let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if out.status.success() && edf.is_file() {
        Ok(said.trim().lines().last().unwrap_or("").to_string())
    } else {
        Err(format!("the converter failed on {}: {}", fbx.display(), said.trim()))
    }
}

/// Put the slotted parts together and write a bike folder: model.fbx and its shadow, the
/// `.hrc`s, a `gfx.cfg`, the template's setup files when it's a real bike, and, when a
/// converter is set up, the `.edf`s the game loads.
#[tauri::command]
pub async fn bike_build(app: tauri::AppHandle) -> Result<BuildReport, String> {
    let exe = blender_exe(&app)?;
    let lib = crate::bike_library(&app)?;
    let out_root = app_dir(&app)?.join("bike-builds");
    let cache = cache_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (settings, template, problem, parts) = {
            let _one = lock();
            let settings = BuildSettings::load(lib.root());
            let (template, problem) = template_or_placeholder(&settings);
            let parts: BTreeMap<Role, bikeparts::Part> =
                lib.slots().into_iter().filter_map(|(r, id)| lib.get(&id).ok().map(|p| (r, p))).collect();
            (settings, template, problem, parts)
        };
        if !parts.contains_key(&Role::Chassis) {
            return Err("a build needs a chassis in its slot".into());
        }
        let slots: BTreeMap<Role, &bikeparts::Part> = parts.iter().map(|(r, p)| (*r, p)).collect();
        let nodes = crate::bikecmd::base_nodes(&lib, &parts);
        let assembly = asm::place_on(&template.frames, &nodes, &slots, &settings.nudges, &settings.rotations);

        let mut notes: Vec<String> = template_note(problem.as_deref()).into_iter().collect();
        let mut job_parts = Vec::new();
        let mut groups: BTreeMap<&str, [[f64; 4]; 4]> = BTreeMap::new();
        let mut root_turns: BTreeMap<&str, f64> = BTreeMap::new();
        for placed in &assembly.placed {
            let part = &parts[&placed.role];
            let Some(group) = placed.group else {
                notes.push(format!(
                    "{} is in the preview only: the game takes wheels from the bike's tyres mod.",
                    part.name
                ));
                continue;
            };
            if !Path::new(&part.source).is_file() {
                return Err(format!("{}'s file is gone: {}", part.name, part.source));
            }
            groups.insert(group.name(), template.frames.into_part(group));
            root_turns.insert(group.name(), group.root_turn());
            job_parts.push(json!({
                "source": part.source,
                "role": placed.role,
                "group": group.name(),
                "offset": placed.offset,
                "rotation": placed.rotation,
                "pivot": placed.pivot,
                "basis": placed.basis,
            }));
        }

        let folder_name = asm::folder_name(&settings.name);
        let folder = out_root.join(&folder_name);
        let make = |work: &Path| {
            json!({
                "op": "assemble",
                "parts": job_parts,
                "groups": groups,
                "rootTurns": root_turns,
                "fbx": work.join("model.fbx"),
                "shadowFbx": work.join("model_shadow.fbx"),
                "shadowTris": 600,
            })
        };
        // Kept while the job's folder is still ours, like a catalogued part.
        let keep = |answer: Value| -> anyhow::Result<(Value, Vec<Group>)> {
            if folder.exists() {
                std::fs::remove_dir_all(&folder)?;
            }
            std::fs::create_dir_all(&folder)?;
            for f in ["fbx", "shadowFbx"] {
                let from = PathBuf::from(answer[f].as_str().unwrap_or_default());
                std::fs::copy(&from, folder.join(from.file_name().unwrap_or_default()))?;
            }
            let built: Vec<Group> = Group::ALL
                .into_iter()
                .filter(|g| answer["tris"].get(g.name()).is_some())
                .collect();
            Ok((answer, built))
        };
        let (answer, built) =
            blender::job_then(&exe, &cache, "assemble", make, keep).map_err(anyhow_str)?;

        // The setup: the template's own textures and paints, at their own relative paths so
        // nothing that references them by path breaks. Never its `gfx.cfg` or `.hrc`s: they
        // name parts and files by the template's own convention, which may not be ours, and a
        // stale reference would point at a file this build never wrote. Studio's own gfx.cfg
        // and the `.hrc`s below always match the model.edf this build just made.
        let rideable = matches!(template.source, TemplateSource::Bike { .. });
        for (name, bytes) in &template.files {
            let lower = name.to_ascii_lowercase();
            if mxb_core::bikefiles::is_mesh(&lower)
                || lower.ends_with(".hrc")
                || lower.ends_with(".pnt")
                || lower == "gfx.cfg"
            {
                continue;
            }
            let dest = folder.join(name);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(err)?;
            }
            std::fs::write(dest, bytes).map_err(err)?;
        }
        std::fs::write(folder.join("gfx.cfg"), asm::default_gfx("oem_mx")).map_err(err)?;
        for g in &built {
            std::fs::write(folder.join(format!("{}.hrc", g.name())), asm::hrc(*g)).map_err(err)?;
        }
        if !rideable {
            notes.push(
                "Built on the placeholder template: the parts and files are all there, but only a real bike's \
                 .geom and physics make it rideable. Choose an installed bike as the template for that."
                    .into(),
            );
        }

        let (converted, converter) = match std::env::var_os(CONVERTER_ENV).map(PathBuf::from) {
            Some(exe) if exe.is_file() => {
                let a = run_converter(&exe, &folder.join("model.fbx"), &folder.join("model.edf"));
                let b = run_converter(&exe, &folder.join("model_shadow.fbx"), &folder.join("model_shadow.edf"));
                match (a, b) {
                    (Ok(a), Ok(_)) => (true, format!("Converted with the local fbx2edf (developer setup). {a}")),
                    (Err(e), _) | (_, Err(e)) => (false, e),
                }
            }
            _ => (
                false,
                "Convert model.fbx and model_shadow.fbx with fbx3edf (mxbsecure.com/converter), layout \"parts\", \
                 and put the two .edf files in this folder."
                    .into(),
            ),
        };

        let mut files: Vec<String> = std::fs::read_dir(&folder)
            .map_err(err)?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        files.sort();
        Ok(BuildReport {
            folder: folder.to_string_lossy().into_owned(),
            files,
            tris: answer["tris"].clone(),
            shadow_tris: answer["shadowTris"].clone(),
            converted,
            converter,
            rideable,
            notes,
        })
    })
    .await
    .map_err(err)?
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A base picked from a bike Studio can't open (an OEM `.pkz`) used to stop the build with
    /// "the template bike can't be read", while the preview and the base-bike popover said the
    /// placeholder stands in. The build now takes the placeholder too, and says why.
    #[test]
    fn unreadable_template_builds_on_the_placeholder() {
        let dir = std::env::temp_dir().join(format!("frost-template-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pkz = dir.join("MX1OEM_2023_KTM_350_SX-F.pkz");
        std::fs::write(&pkz, b"not a pkz Studio can open").unwrap();
        let settings = BuildSettings {
            template: Some(TemplateSource::Bike { path: pkz.to_string_lossy().into_owned() }),
            ..Default::default()
        };
        let (template, problem) = template_or_placeholder(&settings);
        assert!(matches!(template.source, TemplateSource::Placeholder));
        let note = template_note(problem.as_deref()).expect("a note says the placeholder stood in");
        assert!(note.contains("placeholder") && note.contains("MX1OEM_2023_KTM_350_SX-F"), "{note}");
        assert_eq!(template_note(None), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The whole builder on a real Blender, without the app: the placeholder bike into a
/// library, put together on its template, built to FBX, and, when `FROST_FBX2EDF` names the
/// private converter, on to EDF. `cargo test bike_end_to_end -- --ignored --nocapture`.
#[cfg(test)]
mod real {
    use super::*;
    use crate::bikeparts::Library;

    #[test]
    #[ignore = "needs Blender installed"]
    fn bike_end_to_end() {
        let exe = PathBuf::from(blender::detect("").expect("a Blender").path);
        let root = std::env::temp_dir().join(format!("frost-bike-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let lib = Library::new(root.join("lib"));
        let cache = root.join("cache");
        let dir = root.join("made");

        // The placeholder bike, catalogued and slotted.
        blender::job_then(&exe, &cache, "placeholder", |w| json!({ "op": "placeholder", "dir": dir, "work": w, "thumbSize": 64 }), |a| {
            for p in a["parts"].as_array().unwrap() {
                let role: Role = serde_json::from_value(p["role"].clone())?;
                let src = PathBuf::from(p["part"].as_str().unwrap());
                let part = lib.add_as(&src, p, bikeparts::file_stamp(&src), Some(role))?;
                lib.set_slot(role, Some(&part.id))?;
            }
            Ok(())
        })
        .expect("the placeholder bike");
        assert_eq!(lib.slots().len(), 8);

        // Put together: every placeholder part lands on its anchor as it was built there.
        let frames = asm::Frames::placeholder();
        let parts: BTreeMap<Role, bikeparts::Part> =
            lib.slots().into_iter().map(|(r, id)| (r, lib.get(&id).unwrap())).collect();
        let slots: BTreeMap<Role, &bikeparts::Part> = parts.iter().map(|(r, p)| (*r, p)).collect();
        let assembly = asm::place(&frames, &slots, &BTreeMap::new());
        for p in &assembly.placed {
            assert!(p.offset.iter().all(|x| x.abs() < 1e-5), "{:?} by {} at {:?}", p.role, p.by, p.offset);
        }

        // Built.
        let mut groups = BTreeMap::new();
        let mut root_turns = BTreeMap::new();
        let job_parts: Vec<Value> = assembly
            .placed
            .iter()
            .filter_map(|p| {
                let g = p.group?;
                groups.insert(g.name(), frames.into_part(g));
                root_turns.insert(g.name(), g.root_turn());
                Some(json!({ "source": parts[&p.role].source, "role": p.role, "group": g.name(), "offset": p.offset }))
            })
            .collect();
        let out = root.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let built = blender::job(&exe, &cache, "assemble", |_| {
            json!({ "op": "assemble", "parts": job_parts, "groups": groups, "rootTurns": root_turns,
                    "fbx": out.join("model.fbx"), "shadowFbx": out.join("model_shadow.fbx"), "shadowTris": 600 })
        })
        .expect("assemble");
        eprintln!("built: {built}");
        for g in ["chassis", "steer", "fsusp", "rsusp"] {
            assert!(built["tris"][g].as_u64().unwrap() > 0, "{g} has geometry: {built}");
            assert!(built["shadowTris"][g].as_u64().unwrap() <= 700, "{g}'s shadow is low: {built}");
        }
        assert!(out.join("model.fbx").is_file() && out.join("model_shadow.fbx").is_file());

        if let Some(conv) = std::env::var_os(CONVERTER_ENV).map(PathBuf::from) {
            for (f, e) in [("model.fbx", "model.edf"), ("model_shadow.fbx", "model_shadow.edf")] {
                let said = run_converter(&conv, &out.join(f), &out.join(e)).expect("converted");
                eprintln!("{f}: {said}");
            }
            let inspect = std::process::Command::new(&conv).arg("--inspect").arg(out.join("model.edf")).output().unwrap();
            let text = String::from_utf8_lossy(&inspect.stdout).into_owned();
            eprintln!("{text}");
            for g in ["steer", "fsusp", "rsusp"] {
                assert!(text.contains(g), "model.edf has a {g} object");
            }

            // The build, read back the way the viewer reads every installed bike: placed by a
            // .geom with the template's mounts. Where each part's landmarks land says whether
            // the frames on the way out were the right ones.
            std::fs::write(out.join("bike.geom"), frames.geom_mounts()).unwrap();
            std::fs::write(out.join("gfx.cfg"), asm::default_gfx("none")).unwrap();
            for g in Group::ALL {
                std::fs::write(out.join(format!("{}.hrc", g.name())), asm::hrc(g)).unwrap();
            }
            let model = mxb_core::viewer::load_bike_model_blocking(out.to_string_lossy().into_owned(), None)
                .expect("the viewer reads the build");
            let rig = model.rig.expect("placed by the .geom");
            // The viewer merges each part into one node, its objects kept as submeshes.
            let centroid = |name: &str| -> [f32; 3] {
                let (n, sm) = model
                    .nodes
                    .iter()
                    .find_map(|n| n.submeshes.iter().find(|s| s.name == name).map(|s| (n, s)))
                    .unwrap_or_else(|| panic!("no submesh {name}"));
                let mut seen = std::collections::BTreeSet::new();
                for t in sm.tri_start..sm.tri_start + sm.tri_count {
                    for k in 0..3 {
                        seen.insert(n.indices[(t * 3 + k) as usize] as usize);
                    }
                }
                let mut c = [0.0f32; 3];
                for &v in &seen {
                    for i in 0..3 {
                        c[i] += n.positions[v * 3 + i] / seen.len() as f32;
                    }
                }
                c
            };
            let near = |a: [f32; 3], b: [f32; 3], what: &str| {
                let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
                eprintln!("{what}: {a:?} vs {b:?} ({:.1} mm)", d * 1000.0);
                assert!(d < 0.005, "{what} is {:.1} mm off", d * 1000.0);
            };
            // Bolts through the pivot and both axles, modelled centred on them in Blender.
            near(centroid("pivot_bolt"), rig.pivot, "the pivot bolt on the pivot");
            near(centroid("rear_axle_bolt"), rig.rear_axle.unwrap(), "the rear axle bolt on the rear axle");
            near(centroid("front_axle_bolt"), rig.front_axle.unwrap(), "the front axle bolt on the front axle");
            eprintln!("kept at {}", out.display());
        } else {
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    /// The bug that started `op_split`: a full bike FBX, brought in as one part, used to be
    /// guessed "levers" (four small lever meshes outvoting the one big chassis mesh — see
    /// `guess_role`'s regression test in `bikeparts.rs`). This asks Blender to actually cut
    /// it into its parts and checks the ones that matter come out. Point
    /// `FROST_SPLIT_SAMPLE` at a full-bike file to run it — there's no such file checked
    /// into the repo, so this needs one on disk. `cargo test full_bike_splits_into_its_parts
    /// -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs Blender installed, and a full-bike FBX named by FROST_SPLIT_SAMPLE"]
    fn full_bike_splits_into_its_parts() {
        let Some(src) = std::env::var_os("FROST_SPLIT_SAMPLE").map(PathBuf::from) else {
            eprintln!("set FROST_SPLIT_SAMPLE to a full-bike .fbx to run this");
            return;
        };
        assert!(src.is_file(), "{}: not a file", src.display());
        let exe = PathBuf::from(blender::detect("").expect("a Blender").path);
        let root = std::env::temp_dir().join(format!("frost-bike-split-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cache = root.join("cache");
        let parts_dir = root.join("parts");

        let answer = blender::job(&exe, &cache, "split", |w| {
            json!({ "op": "split", "part": src, "thumbSize": 64, "workDir": w, "partsDir": parts_dir })
        })
        .expect("split");
        let groups = answer["groups"].as_array().expect("groups");
        let tags: Vec<&str> = groups.iter().map(|g| g["tag"].as_str().unwrap()).collect();
        eprintln!("groups: {tags:?}");
        for want in ["chassis", "steer", "fsusp", "rsusp", "levers"] {
            assert!(tags.contains(&want), "no {want} group, got {tags:?}");
        }
        for g in groups {
            assert!(g["glb"].is_string(), "{}: no glb ({g})", g["tag"]);
            assert!(g["tris"].as_u64().unwrap() > 0, "{}: no geometry ({g})", g["tag"]);
            // Its own FBX, not just a preview GLB: a build imports this fresh, and it must
            // hold only this group's geometry, not the whole bike it was cut from.
            let fbx = PathBuf::from(g["source"].as_str().expect("a source fbx"));
            assert!(fbx.is_file(), "{}: {} isn't a file", g["tag"], fbx.display());
        }

        // Spot-check the one this bug was about: re-import the levers group's own FBX and
        // make sure it's just the levers, not the whole bike still riding along inside it —
        // exactly the failure a shared `source` used to cause before each group got its own.
        let levers = groups.iter().find(|g| g["tag"] == "levers").expect("a levers group");
        let levers_fbx = PathBuf::from(levers["source"].as_str().unwrap());
        let reimported = blender::job(&exe, &cache, "inspect", |w| {
            json!({ "op": "inspect", "part": levers_fbx, "fbx": w.join("check.fbx") })
        })
        .expect("re-inspecting the levers fbx");
        let names: Vec<&str> =
            reimported["objects"].as_array().unwrap().iter().map(|o| o["name"].as_str().unwrap()).collect();
        eprintln!("levers.fbx holds: {names:?}");
        assert!(!names.is_empty(), "the levers fbx re-imported empty");
        assert!(
            names.iter().all(|n| !n.to_ascii_lowercase().contains("chassis")),
            "the levers fbx still has the chassis in it: {names:?}",
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// "Split into parts" on a file that is one mesh: no kit nodes or role names to go by, so
    /// it comes apart by material, else by loose piece, each piece named from its material and
    /// none given a role. A file that is one piece whichever way answers `nothing`, never a
    /// Python error. Point `FROST_ONE_MESH_SAMPLE` at a one-mesh part with several materials or
    /// shells, and `FROST_ONE_PIECE_SAMPLE` at one that is a single shell with one material (a
    /// plain cube): `cargo test one_mesh_part_splits_by_geometry -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs Blender installed, and the files named by FROST_ONE_MESH_SAMPLE / FROST_ONE_PIECE_SAMPLE"]
    fn one_mesh_part_splits_by_geometry() {
        let exe = match std::env::var_os("FROST_BLENDER") {
            Some(p) => PathBuf::from(p),
            None => PathBuf::from(blender::detect("").expect("a Blender").path),
        };
        let root = std::env::temp_dir().join(format!("frost-one-mesh-split-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cache = root.join("cache");
        let split = |src: &std::path::Path, by: &str, parts: &std::path::Path| {
            blender::job(&exe, &cache, "split", |w| {
                json!({ "op": "split", "by": by, "part": src, "thumbSize": 64, "workDir": w, "partsDir": parts })
            })
            .expect("split")
        };

        if let Some(src) = std::env::var_os("FROST_ONE_MESH_SAMPLE").map(PathBuf::from) {
            let answer = split(&src, "auto", &root.join("parts"));
            let groups = answer["groups"].as_array().expect("groups");
            let tags: Vec<&str> = groups.iter().map(|g| g["tag"].as_str().unwrap()).collect();
            eprintln!("split by {}: {tags:?}", answer["by"]);
            assert!(["material", "loose"].contains(&answer["by"].as_str().unwrap_or("")), "{answer}");
            assert!(groups.len() >= 2 && groups.len() <= 24, "{} pieces", groups.len());
            let total: u64 = groups.iter().map(|g| g["tris"].as_u64().unwrap()).sum();
            assert!(total > 0);
            for g in groups {
                assert!(g["role"].is_null(), "{}: a piece was given a role", g["tag"]);
                assert!(g["tris"].as_u64().unwrap() > 0, "{}: no geometry", g["tag"]);
                let fbx = PathBuf::from(g["source"].as_str().expect("a source fbx"));
                assert!(fbx.is_file(), "{}: {} isn't a file", g["tag"], fbx.display());
            }
            // Asked for loose pieces only, it still answers rather than fails.
            let loose = split(&src, "loose", &root.join("parts-loose"));
            eprintln!("loose: {} pieces", loose["groups"].as_array().map_or(0, |g| g.len()));
            assert!(loose.get("error").is_none(), "{loose}");
        }

        if let Some(src) = std::env::var_os("FROST_ONE_PIECE_SAMPLE").map(PathBuf::from) {
            let answer = split(&src, "auto", &root.join("parts-one"));
            assert_eq!(answer["nothing"], json!(true), "{answer}");
            assert_eq!(answer["groups"], json!([]));
            let catalogued = blender::job(&exe, &cache, "catalog", |w| {
                json!({ "op": "catalog", "part": src, "glb": w.join("p.glb") })
            })
            .expect("catalog");
            assert!(catalogued["splitBy"].is_null(), "{}", catalogued["splitBy"]);
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A customer's report: the WP 7448 AER Fork's game file (LOD0, LOD1, LOD2 and SHADOW
    /// collections) came in as one 137,388-tri part. Adding it now brings LOD0 only, split by
    /// the kit's `mxb_node` into a steer part and an fsusp part. Point `FROST_LOD_SAMPLE` at a
    /// copy of `wp7448_game.blend` (and `FROST_BLENDER` at blender.exe if Studio wouldn't find
    /// it) to run it: `cargo test kit_game_blend_comes_in_as_lod0_by_node -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs Blender installed, and a Frost Blend's game file named by FROST_LOD_SAMPLE"]
    fn kit_game_blend_comes_in_as_lod0_by_node() {
        let Some(src) = std::env::var_os("FROST_LOD_SAMPLE").map(PathBuf::from) else {
            eprintln!("set FROST_LOD_SAMPLE to a Frost Blend's *_game.blend to run this");
            return;
        };
        // FROST_BLENDER: a blender.exe Studio wouldn't find by itself (one unpacked by hand).
        let saved = std::env::var("FROST_BLENDER").unwrap_or_default();
        let exe = PathBuf::from(blender::detect(&saved).expect("a Blender").path);
        let root = std::env::temp_dir().join(format!("frost-bike-lod-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cache = root.join("cache");
        let parts_dir = root.join("parts");

        let answer = blender::job(&exe, &cache, "catalog", |w| {
            json!({
                "op": "catalog", "part": src, "thumb": w.join("thumb.png"), "thumbSize": 64,
                "glb": w.join("part.glb"), "workDir": w, "partsDir": parts_dir,
            })
        })
        .expect("catalog");
        eprintln!("lod: {}", answer["lod"]);
        assert_eq!(answer["lod"]["leftOut"], json!(["LOD1", "LOD2", "SHADOW"]));
        let lod0 = answer["lod"]["tris"].as_u64().unwrap();
        let groups = answer["groups"].as_array().expect("split by node");
        let mut sum = 0;
        for g in groups {
            eprintln!("{}: {} tris, {} objects", g["tag"], g["tris"], g["objects"].as_array().unwrap().len());
            assert_eq!(g["role"], g["tag"], "a kit node is its own role");
            for o in g["objects"].as_array().unwrap() {
                let name = o["name"].as_str().unwrap();
                assert!(!name.ends_with("_shadow") && !name.ends_with('b') && !name.ends_with('c'), "{name}: not LOD0");
            }
            sum += g["tris"].as_u64().unwrap();
        }
        assert_eq!(sum, lod0, "the groups together are LOD0, no more and no less");
        let tags: Vec<&str> = groups.iter().map(|g| g["tag"].as_str().unwrap()).collect();
        assert!(tags.contains(&"steer") && tags.contains(&"fsusp"), "{tags:?}");

        // Its packed textures come out beside the split parts, so the converter finds them:
        // a build of the fork used to be grey, its colour map wired through a Mix and every
        // map's recorded path the seller's own folder.
        for t in ["wp7448.tga", "wp7448_n.tga", "wp7448_r.tga"] {
            let f = parts_dir.join("textures").join(t);
            assert!(f.metadata().map(|m| m.len() > 1 << 20).unwrap_or(false), "{} written", f.display());
        }
        let steer = std::fs::read(parts_dir.join("steer.fbx")).unwrap();
        let has = |s: &str| steer.windows(s.len()).any(|w| w == s.as_bytes());
        assert!(has("DiffuseColor") && has("wp7448.tga"), "steer.fbx carries its colour map");

        // A base bike asks for it whole: no partsDir, one part, still LOD0 only.
        let whole = blender::job(&exe, &cache, "catalog", |w| json!({ "op": "catalog", "part": src, "workDir": w }))
            .expect("catalog whole");
        assert!(whole["groups"].is_null());
        assert_eq!(whole["tris"].as_u64().unwrap(), lod0);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Sean's actual report: he added the full-bike FBX as a plain part (not split) and it
    /// didn't show in the 3D viewport. This runs exactly that path — the "catalog" op, the
    /// same one `bike_part_add` calls — headlessly against the real sample, and prints every
    /// number the preview pipeline depends on: whether the GLB export raised anything, its
    /// size, and the bounds the camera would frame around. `cargo test
    /// catalogued_full_bike_gets_a_working_glb -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs Blender installed, and a full-bike FBX named by FROST_SPLIT_SAMPLE"]
    fn catalogued_full_bike_gets_a_working_glb() {
        let Some(src) = std::env::var_os("FROST_SPLIT_SAMPLE").map(PathBuf::from) else {
            eprintln!("set FROST_SPLIT_SAMPLE to a full-bike .fbx to run this");
            return;
        };
        assert!(src.is_file(), "{}: not a file", src.display());
        let exe = PathBuf::from(blender::detect("").expect("a Blender").path);
        let root = std::env::temp_dir().join(format!("frost-bike-catalog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cache = root.join("cache");

        let answer = blender::job(&exe, &cache, "catalog", |w| {
            json!({
                "op": "catalog", "part": src, "thumb": w.join("thumb.png"), "thumbSize": 256,
                "glb": w.join("part.glb"),
            })
        })
        .expect("catalog");
        eprintln!("catalog answer: {answer}");
        // A thumbnail failing is worth knowing about, but it isn't what this test is
        // asking: the tray already treats it as "no picture", never "can't add the part",
        // and the GLB and bounds below are what the viewport actually depends on.
        if !answer["thumbError"].is_null() {
            eprintln!("thumbnail failed (not fatal): {}", answer["thumbError"]);
        }
        let glb = PathBuf::from(answer["glb"].as_str().expect("a glb path"));
        assert!(glb.is_file(), "no glb written at all");
        let bytes = std::fs::metadata(&glb).unwrap().len();
        eprintln!("glb: {} bytes", bytes);
        assert!(bytes > 1000, "the glb is basically empty ({bytes} bytes)");
        let bounds = &answer["bounds"];
        eprintln!("bounds: {bounds}");
        assert!(!bounds.is_null(), "no bounds — the camera has nothing to frame the preview around");
        let min = bounds["min"].as_array().unwrap();
        let max = bounds["max"].as_array().unwrap();
        let size: f64 = (0..3).map(|i| max[i].as_f64().unwrap() - min[i].as_f64().unwrap()).sum();
        eprintln!("bounds span (sum of 3 axes): {size:.3} m");
        assert!(size > 0.05, "the bike's bounds are near zero — nothing for the camera to frame");
        assert!(size < 50.0, "the bike's bounds are enormous ({size:.1} m) — a scale or unit problem");

        let _ = std::fs::remove_dir_all(&root);
    }
}
