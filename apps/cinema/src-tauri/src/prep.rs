//! What Blender is handed: the track, the bike and the rider, read with the core's own
//! readers and written as plain arrays and RGBA sheets the bundled scripts load with numpy.
//!
//! Each is cached under the app's data folder by what it was made from (the track's archive and
//! its time, the bike's archive and the paint and tyres asked for, the profile's loadout), so a
//! second shot on the same track and bike starts Blender at once.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{anyhow, Context, Result};
use mxb_core::config::AppConfig;
use mxb_core::{edf, heightfield, library, map, presets, texstore, track, tracksource, viewer};
use serde_json::{json, Value};

/// The terrain grid is read at most this many samples across. Woodlinesands is 2049; a big
/// venue's 8193 would be a quarter of a gigabyte of floats for a few metres of ground.
const TERRAIN_MAX: u32 = 4097;

fn wf32(p: &Path, v: &[f32]) -> Result<()> {
    let mut f = std::io::BufWriter::new(File::create(p)?);
    for x in v {
        f.write_all(&x.to_le_bytes())?;
    }
    Ok(())
}

fn wu32(p: &Path, v: &[u32]) -> Result<()> {
    let mut f = std::io::BufWriter::new(File::create(p)?);
    for x in v {
        f.write_all(&x.to_le_bytes())?;
    }
    Ok(())
}

fn safe(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

fn stamp(p: &Path) -> u64 {
    fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

/// A cache folder that only counts once `done` is in it: a run killed halfway is redone.
/// Bumped whenever what the prep writes changes, so a cache from an older build is redone.
const CACHE_VERSION: &str = "2";

fn cached(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("done")).is_ok_and(|v| v.trim() == CACHE_VERSION)
}

fn finish(dir: &Path) -> Result<()> {
    fs::write(dir.join("done"), CACHE_VERSION)?;
    Ok(())
}

fn fresh(dir: &Path) -> Result<()> {
    if dir.exists() && fs::remove_dir_all(dir).is_err() {
        // Something still has a file open (a Blender reading the last shot's sheets): set the old
        // one aside rather than fail; it goes the next time the cache is tidied.
        let aside = dir.with_extension(format!("old{}", std::time::SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())));
        fs::rename(dir, &aside).with_context(|| format!("clearing {dir:?}"))?;
        let _ = fs::remove_dir_all(&aside);
    }
    fs::create_dir_all(dir)?;
    Ok(())
}

// ---------------------------------------------------------------- track

/// The track a ride was on: terrain grid, `.map` scenery and its sheets, the ground layers,
/// the light (`.amb`) and the sky dome's picture.
pub fn track(cfg: &AppConfig, cache: &Path, track_id: &str, log: &dyn Fn(&str)) -> Result<PathBuf> {
    let src = tracksource::resolve(cfg, track_id).ok_or_else(|| anyhow!("trackNotInstalled"))?;
    if src.locked {
        return Err(anyhow!("trackLocked"));
    }
    let path = Path::new(&src.path);
    let dir = cache.join("track").join(format!("{}-{}", safe(track_id), stamp(path)));
    if cached(&dir) {
        log(&format!("track: cached {}", src.name));
        return Ok(dir);
    }
    log(&format!("track: reading {}", src.name));
    fresh(&dir)?;
    fs::create_dir_all(dir.join("tex"))?;
    let prefix = src.prefix.as_deref();
    let names = track::entry_names_under(path, prefix)?;
    let find = |ext: &str| names.iter().find(|n| n.to_ascii_lowercase().ends_with(ext)).cloned();

    // terrain
    let mut layout = None;
    let mut grid = (0, 0, Vec::new());
    for entry in track::heightfield_entries(&names) {
        let Ok(bytes) = track::read_entry(path, &entry) else { continue };
        if let Some(l) = heightfield::probe(&bytes, None) {
            grid = heightfield::read_grid(&bytes, &l, TERRAIN_MAX);
            layout = Some(l);
            break;
        }
    }
    let lay = layout.ok_or_else(|| anyhow!("trackNoTerrain"))?;
    let (w, h, heights) = grid;
    wf32(&dir.join("terrain.f32"), &heights)?;
    let reduction = lay.width as f32 / w.max(1) as f32;
    let mps = lay.metres_per_sample.unwrap_or(1.0) * reduction;
    let mut j = json!({"w": w, "h": h, "src_w": lay.width, "src_h": lay.height, "mps": mps, "source": lay.source,
        "name": src.name, "track_id": track_id});

    // scenery
    let map_name = find(".map").ok_or_else(|| anyhow!("trackNoMap"))?;
    let mb = track::read_entry(path, &map_name)?;
    let m = map::parse(&mb).ok_or_else(|| anyhow!("trackNoMap"))?;
    wf32(&dir.join("pos.f32"), &m.positions)?;
    wf32(&dir.join("nrm.f32"), &m.normals)?;
    wf32(&dir.join("uv.f32"), &m.uvs)?;
    wu32(&dir.join("idx.u32"), &m.indices)?;
    j["groups"] = json!(m.groups.iter().map(|g| json!([g.material, g.tri_start, g.tri_count])).collect::<Vec<_>>());
    let mut texs = vec![];
    for t in map::textures(&mb, 2048) {
        let f = format!("m{}_{}x{}.rgba", t.material, t.width, t.height);
        fs::write(dir.join("tex").join(&f), &t.rgba)?;
        texs.push(json!({"material": t.material, "name": t.name, "w": t.width, "h": t.height, "alpha": t.alpha, "file": f}));
    }
    j["textures"] = json!(texs);
    let mut layers = vec![];
    for (i, l) in map::ground_layers(&mb).iter().enumerate() {
        let mut e = json!({"name": l.sheet.name, "tile": [l.tile_u, l.tile_v], "bump_tile": [l.bump_tile_u, l.bump_tile_v],
            "sheet_w": l.sheet.width, "sheet_h": l.sheet.height});
        let f = format!("layer{i}_sheet_{}x{}.rgba", l.sheet.width, l.sheet.height);
        fs::write(dir.join("tex").join(&f), &l.sheet.rgba)?;
        e["sheet"] = f.into();
        if let Some(mk) = &l.mask {
            let f = format!("layer{i}_mask_{}x{}.r8", mk.width, mk.height);
            fs::write(dir.join("tex").join(&f), &mk.coverage)?;
            e["mask"] = f.into();
            e["mask_w"] = mk.width.into();
            e["mask_h"] = mk.height.into();
        }
        if let Some(b) = &l.bump {
            let f = format!("layer{i}_bump_{}x{}.rgba", b.width, b.height);
            fs::write(dir.join("tex").join(&f), &b.rgba)?;
            e["bump"] = f.into();
            e["bump_w"] = b.width.into();
            e["bump_h"] = b.height.into();
        }
        layers.push(e);
    }
    j["layers"] = json!(layers);
    drop(mb);

    // light: the clear-weather sun, and the picture on the sky dome the game draws behind it
    if let Some(amb_name) = find(".amb") {
        if let Ok(b) = track::read_entry(path, &amb_name) {
            fs::write(dir.join("track.amb"), &b)?;
        }
    }
    if let Ok((_, (_, Some(pic)), _)) = mxb_core::scenery::backdrop(&src.path, prefix) {
        if pic.rgba.len() == (pic.width * pic.height * 4) as usize {
            fs::write(dir.join("sky.rgba"), &pic.rgba)?;
            j["sky"] = json!({"w": pic.width, "h": pic.height, "name": pic.name, "file": "sky.rgba"});
        }
    }
    fs::write(dir.join("track.json"), serde_json::to_vec_pretty(&j)?)?;
    finish(&dir)?;
    log(&format!("track: {w}x{h} terrain at {mps:.3} m, {} scenery triangles", m.indices.len() / 3));
    Ok(dir)
}

// ---------------------------------------------------------------- bike

fn write_textures(dir: &Path, sub: &str, texs: &[mxb_core::paint::PaintTexture]) -> Result<Vec<Value>> {
    fs::create_dir_all(dir.join(sub))?;
    let mut out = vec![];
    let mut seen = std::collections::HashSet::new();
    for (i, t) in texs.iter().enumerate() {
        if !seen.insert(t.name.to_ascii_lowercase()) {
            continue;
        }
        let Some(px) = texstore::get(&t.token) else { continue };
        if px.len() != (t.width * t.height * 4) as usize {
            continue;
        }
        let f = format!("{sub}/{i}_{}_{}x{}.rgba", safe(&t.name), t.width, t.height);
        fs::write(dir.join(&f), px.as_slice())?;
        out.push(json!({"name": t.name, "w": t.width, "h": t.height, "file": f}));
    }
    Ok(out)
}

/// Caches of this bike built from an older install (a model swapped in and later restored, an
/// updated archive). The key already moves when any file under the bike folder changes, so
/// these are never read again; they go now so nothing can pick a stale model up by its path.
fn drop_stale_bikes(cache: &Path, bike_id: &str, stamp_hex: &str, log: &dyn Fn(&str)) {
    let prefix = format!("{}-", safe(bike_id));
    let Ok(rd) = fs::read_dir(cache.join("bike")) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(rest) = name.strip_prefix(&prefix) else { continue };
        let stamp = rest.split(['-', '.']).next().unwrap_or("");
        // Another bike whose id starts with this one ("..._SX-F" vs "..._SX-F_2"): not a stamp.
        if stamp.is_empty() || !stamp.chars().all(|c| c.is_ascii_hexdigit()) || stamp == stamp_hex {
            continue;
        }
        if fs::remove_dir_all(e.path()).is_ok() {
            log(&format!("bike: dropped stale cache {name}"));
        }
    }
}

/// What the cache was built from: each file the bike folder (and its archive) held, with size
/// and write time, so a shot can be traced back to the exact install.
fn source_files(root: &Path) -> Value {
    let mut rows = vec![];
    let mut add = |p: &Path| {
        if let Ok(m) = fs::metadata(p) {
            let t = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
            rows.push(json!({"path": p.to_string_lossy(), "len": m.len(), "mtime": t}));
        }
    };
    if let Ok(rd) = fs::read_dir(root) {
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
        files.sort();
        files.iter().for_each(|p| add(p));
    }
    if let Some(pkz) = viewer::packed_bike(root) {
        add(&pkz);
    }
    json!(rows)
}

/// The bike the ride was on, assembled the way Coach's replay draws it, in the paint and on
/// the tyres the rider's profile has for it.
pub fn bike(cfg: &AppConfig, cache: &Path, bike_id: &str, lo: &presets::Loadout, log: &dyn Fn(&str)) -> Result<PathBuf> {
    let root = library::mods_subdir(cfg.mods_path.trim(), "mods/bikes").join(bike_id);
    let stamp_text = format!("{}{}{:x}", viewer::folder_stamp(&root), viewer::packed_stamp(&root), viewer::paints_stamp(&root));
    let stamp_hex = format!("{:x}", viewer::fnv1a(&[stamp_text]));
    let key = format!("{}-{}-{}-{}", safe(bike_id), stamp_hex, safe(&lo.paint), safe(&lo.tyres));
    let dir = cache.join("bike").join(key);
    if cached(&dir) {
        log(&format!("bike: cached {bike_id}"));
        return Ok(dir);
    }
    log(&format!("bike: reading {bike_id}"));
    drop_stale_bikes(cache, bike_id, &stamp_hex, log);
    fresh(&dir)?;
    // What the game draws: the archive with the bike folder's loose files over it, so a model
    // swap the rider rides with is the bike in the shot.
    let files = viewer::gather_bike_files(&root).map_err(|_| anyhow!("bikeNotInstalled"))?;
    if files.is_empty() {
        return Err(anyhow!("bikeNotInstalled"));
    }
    let geom = files.iter().rev().find(|(n, _)| n.to_ascii_lowercase().ends_with(".geom")).map(|(_, b)| b.clone());
    // The grips the game puts the hands on (`steer { leftgrip/rightgrip }`).
    if let Some((_, g)) = files.iter().rev().find(|(n, _)| n.to_ascii_lowercase().ends_with("gfx.cfg")) {
        fs::write(dir.join("gfx.cfg"), g)?;
    }
    drop(files);
    let tyres = (!lo.tyres.is_empty()).then(|| lo.tyres.clone());
    let model = viewer::load_bike_model_blocking(root.to_string_lossy().into_owned(), tyres).map_err(|e| anyhow!("bikeUnreadable: {e}"))?;
    let rig = model.rig.ok_or_else(|| anyhow!("bikeUnassembled"))?;
    // The rider's paint for this bike over the model's own sheets, by name.
    let mut sheets = model.base.clone();
    let want = lo.paint.trim().trim_end_matches(".pnt").to_ascii_lowercase();
    let paint = model.paints.iter().find(|p| !want.is_empty() && p.name.to_ascii_lowercase() == want);
    log(&format!("bike: paints {:?}, profile wants {:?}", model.paints.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), lo.paint));
    if let Some(p) = paint {
        let names: std::collections::HashSet<String> = p.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        sheets.retain(|t| !names.contains(&t.name.to_ascii_lowercase()));
        sheets.splice(0..0, p.textures.iter().cloned());
    }
    let textures = write_textures(&dir, "tex", &sheets)?;
    if let Some(g) = &geom {
        fs::write(dir.join("bike.geom"), g)?;
    }
    let j = json!({
        "bike": bike_id,
        "paint": paint.map(|p| p.name.clone()),
        "tyres": model.tyres,
        "tyre_textures": model.tyre_textures,
        "rig": rig,
        "nodes": model.nodes,
        "textures": textures,
        "geom": geom.is_some().then_some("bike.geom"),
        "source": source_files(&root),
    });
    fs::write(dir.join("bike.json"), serde_json::to_vec(&j)?)?;
    finish(&dir)?;
    log(&format!("bike: {} parts, paint {}", model.nodes.len(), paint.map_or("stock", |p| p.name.as_str())));
    Ok(dir)
}

// ---------------------------------------------------------------- rider

const DEFAULT_GLOVES: &str = "27 ASTARS Supertech (Black)";

/// Which `profile.ini` the ride's rider is: the profile folder of that name, else the only one.
pub fn profile_for(cfg: &AppConfig, rider: &str) -> String {
    let dir = cfg.profiles_dir();
    if !rider.is_empty() && dir.join(rider).join("profile.ini").is_file() {
        return rider.to_string();
    }
    let mut found: Vec<(u64, String)> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("profile.ini").is_file())
        .map(|e| (stamp(&e.path().join("profile.ini")), e.file_name().to_string_lossy().into_owned()))
        .collect();
    found.sort();
    found.pop().map(|(_, n)| n).unwrap_or_else(|| rider.to_string())
}

pub fn loadout(cfg: &AppConfig, profile: &str, bike_id: &str) -> presets::Loadout {
    presets::read_loadout(&cfg.profiles_dir(), profile, bike_id).unwrap_or_else(|_| presets::Loadout {
        rider: "default_mx".into(),
        ..Default::default()
    })
}

/// One file of a rider model (`rider.edf`, `gfx.cfg`), where the game would find it: an
/// installed model's folder or `.pkz`, else the stock one in `rider.pkz`.
fn rider_file(cfg: &AppConfig, rider: &str, file: &str) -> Option<Vec<u8>> {
    let riders = library::mods_subdir(&cfg.mods_path, "mods/rider/riders");
    let loose = riders.join(rider).join(file);
    if loose.is_file() {
        return fs::read(loose).ok();
    }
    let packed = riders.join(format!("{rider}.pkz"));
    if packed.is_file() {
        return viewer::read_pkz_entry(&packed, &format!("rider/riders/{rider}/{file}"))
            .or_else(|| viewer::read_pkz_basename(&packed, file));
    }
    let game = viewer::resolve_game_pkz(cfg, "rider.pkz")?;
    viewer::read_pkz_entry(&game, &format!("rider/riders/{rider}/{file}"))
        .or_else(|| viewer::read_pkz_entry(&game, &format!("rider/riders/default_mx/{file}")))
}

fn rider_edf(cfg: &AppConfig, rider: &str) -> Option<Vec<u8>> {
    rider_file(cfg, rider, "rider.edf")
}

/// Where each gear model's mesh is, for its embedded bump maps: the game's own
/// `helmets|boots/<model>` folder or `.pkz`, else the stock one in `rider.pkz`.
fn gear_edfs(cfg: &AppConfig, lo: &presets::Loadout) -> Vec<(String, Vec<u8>)> {
    let rd = library::mods_subdir(&cfg.mods_path, "mods/rider");
    let game = viewer::resolve_game_pkz(cfg, "rider.pkz");
    let mut out = vec![];
    for (kind, name, mesh) in [
        ("helmets", lo.helmet.as_str(), "helmet.edf"),
        ("boots", lo.boots.as_str(), "boots.edf"),
        ("protections", lo.protection.as_str(), "protection.edf"),
    ] {
        let name = name.trim_end_matches(".pkz");
        if kind == "protections" && name.is_empty() {
            continue; // no protection in the loadout: the game draws none
        }
        let bytes = if name.is_empty() {
            game.as_ref().and_then(|g| viewer::read_pkz_entry(g, &format!("rider/{kind}/default/{mesh}")))
        } else {
            let d = rd.join(kind).join(name);
            let p = rd.join(kind).join(format!("{name}.pkz"));
            if d.is_dir() {
                fs::read(d.join(mesh)).ok()
            } else {
                viewer::read_pkz_entry(&p, &format!("{name}/{mesh}")).or_else(|| viewer::read_pkz_basename(&p, mesh))
            }
        };
        if let Some(b) = bytes {
            out.push((format!("{kind}_{}", if name.is_empty() { "default" } else { name }), b));
        }
    }
    out
}

/// The rider: the body's skinned mesh and its bones as the game authors them, and the
/// outfit as MXB App and Coach dress it (helmet, boots, protection, suit and gloves paints).
pub fn rider(app: &tauri::AppHandle, cfg: &AppConfig, cache: &Path, profile: &str, bike_id: &str, log: &dyn Fn(&str)) -> Result<PathBuf> {
    let lo = loadout(cfg, profile, bike_id);
    let ini = cfg.profiles_dir().join(profile).join("profile.ini");
    let key = format!("{}-{}-{}", safe(profile), safe(bike_id), stamp(&ini));
    let dir = cache.join("rider").join(key);
    if cached(&dir) {
        log(&format!("rider: cached {profile}"));
        return Ok(dir);
    }
    log(&format!("rider: dressing {profile}"));
    fresh(&dir)?;
    let rider_name = if lo.rider.is_empty() { "default_mx".to_string() } else { lo.rider.clone() };

    // body: the mesh as authored, its skeleton and skin
    let b = rider_edf(cfg, &rider_name).ok_or_else(|| anyhow!("riderMissing"))?;
    let body = dir.join("body");
    fs::create_dir_all(&body)?;
    let nodes = edf::parse(&b);
    let bones = edf::parse_skeleton(&b);
    if bones.is_empty() {
        return Err(anyhow!("riderNoRig"));
    }
    let skin = edf::skin_mesh(&nodes, &bones);
    let (mut p, mut n, mut u, mut ix) = (vec![], vec![], vec![], vec![]);
    for nd in &nodes {
        let base = (p.len() / 3) as u32;
        p.extend_from_slice(&nd.positions);
        n.extend_from_slice(&nd.normals);
        u.extend_from_slice(&nd.uvs);
        ix.extend(nd.indices.iter().map(|i| i + base));
    }
    wf32(&body.join("pos.f32"), &p)?;
    wf32(&body.join("nrm.f32"), &n)?;
    wf32(&body.join("uv.f32"), &u)?;
    wu32(&body.join("idx.u32"), &ix)?;
    wu32(&body.join("skin_idx.u32"), &skin.indices.iter().map(|&i| i as u32).collect::<Vec<_>>())?;
    wf32(&body.join("skin_w.f32"), &skin.weights)?;
    let bj: Vec<Value> = bones.iter().map(|bn| json!({"name": bn.name, "parent": bn.parent, "bind": bn.bind.to_vec()})).collect();
    fs::write(body.join("rider.json"), serde_json::to_vec(&json!({"bones": bj, "rider": rider_name}))?)?;
    // Where the hands go on the grips (`lefthand`/`righthand`: wrist bone and offset).
    if let Some(g) = rider_file(cfg, &rider_name, "gfx.cfg") {
        fs::write(body.join("gfx.cfg"), g)?;
    }
    drop(b);

    // outfit, through the same loader MXB App's garage uses. A profile with no gloves paint wears
    // the default rider's black Supertech, as the approved stills do, when it is installed.
    let mut lo = lo;
    if lo.gloves_paint.is_empty() {
        let g = library::mods_subdir(&cfg.mods_path, "mods/rider/riders").join(&rider_name).join("gloves").join(format!("{DEFAULT_GLOVES}.pnt"));
        if g.is_file() {
            lo.gloves_paint = DEFAULT_GLOVES.into();
        }
    }
    let model = viewer::load_rider_model_blocking(app.clone(), lo.clone()).map_err(|e| anyhow!("riderOutfit: {e}"))?;
    let out = dir.join("outfit");
    fs::create_dir_all(&out)?;
    let mut parts = vec![];
    for (pi, part) in model.parts.iter().enumerate() {
        let textures = write_textures(&out, &format!("p{pi}"), &part.textures)?;
        parts.push(json!({"part": part.part, "nodes": part.nodes, "textures": textures}));
    }
    fs::write(out.join("outfit.json"), serde_json::to_vec(&json!({"loadout": lo, "parts": parts}))?)?;

    // the gear's own shading maps, which the viewer drops: `_n` bump (+ spec in alpha) and the
    // `_r` reflection map the game scales its environment reflection by (outfit.py reads both)
    let edfdir = dir.join("edf");
    fs::create_dir_all(&edfdir)?;
    let mut idx = vec![];
    let mut srcs = gear_edfs(cfg, &lo);
    if let Some(b) = rider_edf(cfg, &rider_name) {
        srcs.push((format!("rider_{rider_name}"), b));
    }
    for (tag, bytes) in &srcs {
        let sheets = edf::embedded_textures(bytes);
        for t in &sheets {
            let lower = t.name.to_ascii_lowercase();
            if !(lower.ends_with("_n") || lower.ends_with("_r")) {
                continue;
            }
            let Some(px) = edf::inflate_texture(bytes, t) else { continue };
            write_map(&edfdir, &mut idx, tag, &t.name, t.width, t.height, &px)?;
        }
        // The single-channel maps (`_r` reflection, `_s` specular) are L8 records the colour
        // scan does not take: found here by the name of each colour sheet they belong to.
        for t in &sheets {
            if ["_n", "_r", "_s"].iter().any(|s| t.name.to_ascii_lowercase().ends_with(s)) {
                continue;
            }
            for sfx in ["_r", "_s"] {
                let name = format!("{}{sfx}", t.name);
                if let Some((w, h, px)) = l8_map(bytes, &name) {
                    write_map(&edfdir, &mut idx, tag, &name, w, h, &px)?;
                }
            }
        }
    }
    fs::write(edfdir.join("index.json"), serde_json::to_vec_pretty(&idx)?)?;
    finish(&dir)?;
    log(&format!(
        "rider: {} bones, helmet {}, suit {}",
        bones.len(),
        if lo.helmet.is_empty() { "stock" } else { &lo.helmet },
        if lo.suit_paint.is_empty() { "stock" } else { &lo.suit_paint }
    ));
    Ok(dir)
}

fn write_map(edfdir: &Path, idx: &mut Vec<Value>, tag: &str, name: &str, w: u32, h: u32, px: &[u8]) -> Result<()> {
    let f = format!("{}__{}_{}x{}.rgba", safe(tag), safe(name), w, h);
    fs::write(edfdir.join(&f), px)?;
    idx.push(json!({"src": tag, "name": name, "w": w, "h": h, "file": f}));
    Ok(())
}

/// An EDF's single-channel (L8) map by name, as grey RGBA. Its record is the colour one's
/// shape with the format word in place of the zero: `name | w h | md5[16] | size fmt=2 0 | deflate`.
pub(crate) fn l8_map(b: &[u8], name: &str) -> Option<(u32, u32, Vec<u8>)> {
    use std::io::Read;
    let u = |o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
    let pat = name.as_bytes();
    let mut o = 0;
    while let Some(p) = b[o..].windows(pat.len() + 1).position(|w| &w[..pat.len()] == pat && w[pat.len()] == 0) {
        let at = o + p;
        o = at + 1;
        if at > 0 && (b[at - 1].is_ascii_alphanumeric() || b[at - 1] == b'_') {
            continue;
        }
        for w_off in [100usize, 104] {
            let wa = at + w_off;
            let (Some(w), Some(h), Some(size), Some(fmt)) = (u(wa), u(wa + 4), u(wa + 24), u(wa + 28)) else { continue };
            if !w.is_power_of_two() || !h.is_power_of_two() || w > 16384 || h > 16384 || fmt != 2 || size <= 8 {
                continue;
            }
            let (d0, dl) = (wa + 36, size as usize - 8);
            let Some(data) = b.get(d0..d0 + dl) else { continue };
            let want = (w * h) as usize;
            let mut px = Vec::with_capacity(want);
            if std::io::Read::take(flate2::read::DeflateDecoder::new(data), want as u64).read_to_end(&mut px).is_err() || px.len() != want {
                continue;
            }
            return Some((w, h, px.iter().flat_map(|&v| [v, v, v, 255]).collect()));
        }
    }
    None
}

