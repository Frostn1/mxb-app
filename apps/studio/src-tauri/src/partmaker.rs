//! The Part Maker (phase F): a part made from nothing. A template with sliders builds it in
//! Blender; a brief in words picks the template and sets the sliders, through the model the
//! rider set up for tracks or, with none, by reading the words here. For what no template
//! covers, the model writes Blender Python, which Blender checks before running it. A part
//! that's kept goes into the library with its role, like any other.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::bikeassemble::{self as asm, V3};
use crate::bikecmd::{anyhow_str, app_dir, blender_exe, cache_dir, err, lock};
use crate::bikeparts::{self, Role};
use crate::{blender, trackmodel};

/// The templates Blender offers, asked once per run: they're written into the script.
static TEMPLATES: Mutex<Option<Value>> = Mutex::new(None);

fn templates(exe: &Path, cache: &Path) -> Result<Value, String> {
    if let Some(t) = TEMPLATES.lock().unwrap_or_else(|p| p.into_inner()).clone() {
        return Ok(t);
    }
    let answer =
        blender::job(exe, cache, "templates", |_| json!({ "op": "templates" })).map_err(anyhow_str)?;
    let t = answer["templates"].clone();
    *TEMPLATES.lock().unwrap_or_else(|p| p.into_inner()) = Some(t.clone());
    Ok(t)
}

#[tauri::command]
pub async fn bike_make_templates(app: tauri::AppHandle) -> Result<Value, String> {
    let exe = blender_exe(&app)?;
    let cache = cache_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || templates(&exe, &cache)).await.map_err(err)?
}

/// The last preview's part file, which "keep" copies into the library.
static LAST_MADE: Mutex<Option<(PathBuf, Role)>> = Mutex::new(None);

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MadePreview {
    thumb: Option<String>,
    role: Role,
    tris: u64,
    size: Option<V3>,
}

/// Build a part from a template and its sliders, or from code the model wrote (checked in
/// Blender before it runs), and picture it. Nothing reaches the library until it's kept.
#[tauri::command]
pub async fn bike_make_preview(
    app: tauri::AppHandle,
    template: Option<String>,
    params: Option<Value>,
    code: Option<String>,
    role: Option<Role>,
) -> Result<MadePreview, String> {
    let exe = blender_exe(&app)?;
    let cache = cache_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let stashed = cache.join("last_made.blend");
        let make = |work: &Path| {
            let mut job = json!({
                "op": "make",
                "blend": work.join("part.blend"),
                "thumb": work.join("thumb.png"),
                "thumbSize": 384,
            });
            match &code {
                Some(c) if !c.trim().is_empty() => {
                    job["code"] = json!(c);
                    job["role"] = json!(role.unwrap_or(Role::Handguards));
                }
                _ => {
                    job["template"] = json!(template);
                    job["params"] = params.clone().unwrap_or(json!({}));
                }
            }
            job
        };
        let keep = |answer: Value| -> anyhow::Result<MadePreview> {
            use base64::Engine;
            let role: Role = serde_json::from_value(answer["role"].clone())?;
            let thumb = answer["thumb"]
                .as_str()
                .and_then(|p| std::fs::read(p).ok())
                .map(|b| format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(b)));
            // Stashed out of the job's own folder, still under the job lock: the next preview
            // (or placeholder, or catalog — anything of a kind sharing this cache) clears the
            // folder this answer's blend file lives in as soon as the lock is released.
            std::fs::copy(answer["blend"].as_str().unwrap_or_default(), &stashed)?;
            *LAST_MADE.lock().unwrap_or_else(|p| p.into_inner()) = Some((stashed.clone(), role));
            let size = answer["bounds"]["min"].as_array().zip(answer["bounds"]["max"].as_array()).map(|(lo, hi)| {
                let d = |i: usize| hi[i].as_f64().unwrap_or(0.0) - lo[i].as_f64().unwrap_or(0.0);
                [d(0), d(1), d(2)]
            });
            Ok(MadePreview { thumb, role, tris: answer["tris"].as_u64().unwrap_or(0), size })
        };
        blender::job_then(&exe, &cache, "make", make, keep).map_err(anyhow_str)
    })
    .await
    .map_err(err)?
}

/// Keep the last preview: its part file moves to Studio's own parts folder, and it joins the
/// library with its role, in that role's slot if the slot is empty.
#[tauri::command]
pub async fn bike_make_keep(app: tauri::AppHandle, name: String) -> Result<crate::BikePartView, String> {
    let exe = blender_exe(&app)?;
    let lib = crate::bike_library(&app)?;
    let made = app_dir(&app)?.join("bike-made");
    let cache = cache_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (from, role) = LAST_MADE
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .ok_or("make a preview first")?;
        if !from.is_file() {
            return Err("that preview is gone; make it again".into());
        }
        std::fs::create_dir_all(&made).map_err(err)?;
        // Picking a free name and copying into it, together under the library's lock: two
        // Keep presses at once can't both see the same name free and land on the same file.
        let dest = {
            let _one = lock();
            let base = asm::folder_name(&name);
            let mut dest = made.join(format!("{base}.blend"));
            let mut n = 2;
            while dest.exists() {
                dest = made.join(format!("{base}_{n}.blend"));
                n += 1;
            }
            std::fs::copy(&from, &dest).map_err(err)?;
            dest
        };
        let stamp = bikeparts::file_stamp(&dest);
        let make = |work: &Path| {
            json!({ "op": "catalog", "part": dest, "thumb": work.join("thumb.png"), "thumbSize": 256, "glb": work.join("part.glb") })
        };
        let keep = |answer: Value| {
            let _one = lock();
            let part = lib.add_as(&dest, &answer, stamp, Some(role))?;
            if !lib.slots().contains_key(&role) {
                lib.set_slot(role, Some(&part.id))?;
            }
            Ok(crate::bike_part_view(&lib, part))
        };
        blender::job_then(&exe, &cache, "catalog", make, keep).map_err(anyhow_str)
    })
    .await
    .map_err(err)?
}

/// What the model decided: a template and its sliders (the usual answer), or code.
#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MakeAnswer {
    pub template: Option<String>,
    pub params: Value,
    pub code: Option<String>,
    pub role: Option<Role>,
    pub name: String,
    pub reply: String,
    /// Which model answered, or "words" when no model was set up and the brief was read here.
    pub by: String,
}

const MAKE_SYSTEM: &str = include_str!("partmaker.system.md");

/// Colour words a brief might use, for reading one without a model.
const COLOURS: &[(&str, &str)] = &[
    ("orange", "#ff6600"), ("ktm", "#ff6600"), ("red", "#d4121b"), ("honda", "#d4121b"),
    ("blue", "#1f4fbf"), ("yamaha", "#1f4fbf"), ("yellow", "#f2c500"), ("suzuki", "#f2c500"),
    ("green", "#3fa535"), ("kawasaki", "#3fa535"), ("white", "#f2f2f2"), ("black", "#151515"),
    ("husqvarna", "#f2f2f2"), ("gasgas", "#c8102e"), ("purple", "#6a2c91"), ("pink", "#e84393"),
    ("grey", "#808080"), ("gray", "#808080"),
];

/// Read a brief without a model: which template it names, its colour, and the few shape
/// words that map straight onto sliders. The words path, so the Part Maker works with no key.
pub fn read_brief(brief: &str, templates: &Value) -> MakeAnswer {
    let b = brief.to_ascii_lowercase();
    let template = ["handguard", "plate", "grip"]
        .iter()
        .zip(["handguards", "plate", "grips"])
        .find(|(k, _)| b.contains(*k))
        .map(|(_, t)| t.to_string())
        .or_else(|| templates.as_object().and_then(|o| o.keys().next().cloned()));
    let mut params = serde_json::Map::new();
    if let Some((_, hex)) = COLOURS.iter().find(|(w, _)| b.contains(w)) {
        params.insert("color".into(), json!(hex));
    }
    let t = template.as_deref().unwrap_or("");
    let slider = |name: &str, pick: fn(f64, f64, f64) -> f64| -> Option<(String, Value)> {
        let s = templates[t]["sliders"].as_array()?.iter().find(|s| s["name"] == name)?;
        let (d, lo, hi) = (s["default"].as_f64()?, s["min"].as_f64()?, s["max"].as_f64()?);
        Some((name.to_string(), json!((pick(d, lo, hi) * 10_000.0).round() / 10_000.0)))
    };
    let mut set = |k: Option<(String, Value)>| {
        if let Some((k, v)) = k {
            params.insert(k, v);
        }
    };
    if b.contains("low profile") || b.contains("low-profile") || b.contains("small") || b.contains("slim") {
        set(slider("height", |d, lo, _| (d + lo) / 2.0));
    }
    if b.contains("tall") || b.contains("big") || b.contains("large") {
        set(slider("height", |d, _, hi| (d + hi) / 2.0));
    }
    if b.contains("wide") || b.contains("wraparound") || b.contains("wrap-around") {
        set(slider("reach", |d, _, hi| (d + hi) / 2.0));
    }
    if t == "handguards" {
        let open = b.contains("open") || b.contains("flag") || b.contains("deflector");
        params.insert("mount".into(), json!(if open { "open" } else { "wrap" }));
    }
    let role = templates[t]["role"].as_str().and_then(|r| serde_json::from_value(json!(r)).ok());
    MakeAnswer {
        name: brief.trim().chars().take(40).collect(),
        reply: format!("Read without a model: the {t} template{}.", if params.is_empty() { "" } else { ", with what the words set" }),
        template,
        params: Value::Object(params),
        code: None,
        role,
        by: "words".into(),
    }
}

/// The model the Part Maker asks: the one set up for tracks. On Anthropic it asks Sonnet 5,
/// which writes Blender Python well for its cost; elsewhere the model named there.
fn part_model(app: &tauri::AppHandle) -> Option<trackmodel::TrackModel> {
    let mut m = mxb_core::config::data_dir(app).and_then(|d| trackmodel::load(&d))?;
    if m.kind == trackmodel::Kind::Anthropic {
        m.model = "claude-sonnet-5".into();
    }
    Some(m)
}

fn image_block(path: &str, anthropic: bool) -> Result<Value, String> {
    use base64::Engine;
    let lower = path.to_ascii_lowercase();
    let media = if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        return Err(format!("{path}: a reference image must be PNG, JPEG or WebP"));
    };
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    if bytes.len() > 5 * 1024 * 1024 {
        return Err(format!("{path} is over 5 MB"));
    }
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(if anthropic {
        json!({ "type": "image", "source": { "type": "base64", "media_type": media, "data": data } })
    } else {
        json!({ "type": "image_url", "image_url": { "url": format!("data:{media};base64,{data}") } })
    })
}

/// The first JSON object in a model's answer, however it was wrapped.
fn first_object(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in text[start..].char_indices() {
        match c {
            _ if esc => esc = false,
            '\\' if in_str => esc = true,
            '"' => in_str = !in_str,
            '{' if !in_str => depth += 1,
            '}' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&text[start..=start + i]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

/// Turn a brief (and the part on screen, and any reference pictures) into a template and
/// sliders, or code. With no model set up, the brief is read for its template and colour.
#[tauri::command]
pub async fn bike_make_ask(
    app: tauri::AppHandle,
    brief: String,
    current: Option<Value>,
    images: Option<Vec<String>>,
) -> Result<MakeAnswer, String> {
    let exe = blender_exe(&app)?;
    let cache = cache_dir(&app)?;
    let t = tauri::async_runtime::spawn_blocking(move || templates(&exe, &cache)).await.map_err(err)??;
    let Some(model) = part_model(&app) else {
        return Ok(read_brief(&brief, &t));
    };
    let anthropic = model.kind == trackmodel::Kind::Anthropic;
    let mut content = vec![json!({
        "type": "text",
        "text": format!(
            "Templates:\n{}\n\nThe part on screen now (edit it rather than start again, unless asked):\n{}\n\nBrief:\n{}",
            serde_json::to_string_pretty(&t).unwrap_or_default(),
            current.map(|c| c.to_string()).unwrap_or_else(|| "none yet".into()),
            brief.trim()
        ),
    })];
    for p in images.unwrap_or_default().iter().take(3) {
        content.push(image_block(p, anthropic)?);
    }
    let body = if anthropic {
        json!({ "model": model.model, "max_tokens": 8000, "system": MAKE_SYSTEM, "messages": [{ "role": "user", "content": content }] })
    } else {
        json!({ "model": model.model, "messages": [{ "role": "system", "content": MAKE_SYSTEM }, { "role": "user", "content": content }] })
    };
    let text = trackmodel::post_plain(&model, &body).await.map_err(anyhow_str)?;
    let v = first_object(&text).ok_or("the model's answer had no JSON in it")?;
    let mut answer: MakeAnswer = serde_json::from_value(v).map_err(|e| format!("the model's answer didn't fit: {e}"))?;
    if answer.template.is_none() && answer.code.as_deref().is_none_or(|c| c.trim().is_empty()) {
        return Err("the model named no template and wrote no code".into());
    }
    if let Some(name) = &answer.template {
        if t.get(name).is_none() {
            return Err(format!("the model picked a template that doesn't exist: {name}"));
        }
        if answer.role.is_none() {
            answer.role = t[name]["role"].as_str().and_then(|r| serde_json::from_value(json!(r)).ok());
        }
    }
    answer.by = model.model.clone();
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn templates() -> Value {
        json!({
            "handguards": { "role": "handguards", "sliders": [
                { "name": "reach", "default": 0.09, "min": 0.04, "max": 0.16 },
                { "name": "height", "default": 0.09, "min": 0.04, "max": 0.16 } ], "choices": { "mount": ["wrap", "open"] } },
            "plate": { "role": "plate", "sliders": [], "choices": {} },
        })
    }

    #[test]
    fn a_brief_reads_without_a_model() {
        let a = read_brief("KTM-style wraparound handguards, orange, low profile", &templates());
        assert_eq!(a.template.as_deref(), Some("handguards"));
        assert_eq!(a.params["color"], "#ff6600");
        assert_eq!(a.params["mount"], "wrap");
        assert!(a.params["height"].as_f64().unwrap() < 0.09);
        assert!(a.params["reach"].as_f64().unwrap() > 0.09);
        assert_eq!(a.role, Some(Role::Handguards));
        assert_eq!(a.by, "words");
        let p = read_brief("white number plate", &templates());
        assert_eq!(p.template.as_deref(), Some("plate"));
        assert_eq!(p.params["color"], "#f2f2f2");
    }

    #[test]
    fn json_is_found_in_a_wrapped_answer() {
        let v = first_object("Sure!\n```json\n{\"template\": \"plate\", \"reply\": \"a {curly} one\"}\n```").unwrap();
        assert_eq!(v["template"], "plate");
        assert_eq!(v["reply"], "a {curly} one");
        assert!(first_object("no json").is_none());
    }
}

/// The Part Maker on a real Blender: `cargo test part_maker_real -- --ignored`.
#[cfg(test)]
mod real {
    use super::*;

    #[test]
    #[ignore = "needs Blender installed"]
    fn part_maker_real() {
        let exe = PathBuf::from(blender::detect("").expect("a Blender").path);
        let root = std::env::temp_dir().join(format!("frost-partmaker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        let t = templates(&exe, &root).expect("the templates");
        for name in ["handguards", "plate", "grips"] {
            assert!(t[name]["sliders"].as_array().is_some_and(|s| !s.is_empty()), "{name}: {t}");
        }

        // KTM handguards, from the words alone, as the app reads them with no model set up.
        let brief = read_brief("KTM-style wraparound handguards, orange, low profile", &t);
        let made = blender::job(&exe, &root, "make", |w| {
            json!({ "op": "make", "template": brief.template, "params": brief.params,
                    "blend": w.join("p.blend"), "thumb": w.join("t.png"), "thumbSize": 64 })
        })
        .expect("handguards");
        assert_eq!(made["role"], "handguards");
        assert!(made["empties"].as_array().unwrap().iter().any(|e| e["name"] == "handlebar"), "{made}");
        assert!(made["tris"].as_u64().unwrap() > 100);
        assert!(Path::new(made["thumb"].as_str().unwrap()).is_file());

        // Code that stays inside geometry runs, and gets its mount empty.
        let ok = "m = material('orange', hex_rgba('#ff6600'))\nfor s in (-1, 1):\n    box('guard_%d' % s, (s * 0.4, -0.1, 0.03), (0.1, 0.01, 0.08), m)\n";
        let made = blender::job(&exe, &root, "make", |w| {
            json!({ "op": "make", "code": ok, "role": "handguards", "blend": w.join("p.blend") })
        })
        .expect("checked code runs");
        assert!(made["empties"].as_array().unwrap().iter().any(|e| e["name"] == "handlebar"), "{made}");

        // Code that reaches outside is refused before it runs.
        for bad in [
            "import os\nos.remove('x')",
            "open('x', 'w')",
            "bpy.ops.wm.save_as_mainfile(filepath='x.blend')",
            "(1).__class__",
            "getattr(bpy, 'app')",
            "__import__('subprocess')",
            "x = '{0.__class__}'.format(1)",
            "from random import __builtins__ as b\nb['open']('x', 'w')",
        ] {
            let err = blender::job(&exe, &root, "make", |w| {
                json!({ "op": "make", "code": bad, "role": "handguards", "blend": w.join("p.blend") })
            })
            .unwrap_err();
            assert!(format!("{err:#}").contains("ValueError"), "{bad}: {err:#}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
