//! Besides Tauri's own build step: the bundled Blender scripts must match the prototype they come
//! from (`blender/upstream.json`, see `blender/upstream.py`). Where the prototype folder exists, a
//! script that has fallen behind fails the build here, saying which; `python blender/upstream.py
//! --write` brings the copied and patched ones up to date, and a ported one is re-ported by hand.
//! On CI (the runner PC has the folder too) it only warns, so a prototype edit never blocks a
//! release build.

use std::path::{Path, PathBuf};

fn read(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok().map(|s| s.replace("\r\n", "\n"))
}

fn fnv(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h = (h ^ *b as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn check_upstream() {
    let here = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("blender");
    let src = PathBuf::from(std::env::var("CINEMA_PROTOTYPE_DIR").unwrap_or_else(|_| r"C:\dev\MXB\claude-work\cinema".into()));
    println!("cargo::rerun-if-env-changed=CINEMA_PROTOTYPE_DIR");
    println!("cargo::rerun-if-changed=blender");
    if !src.is_dir() {
        return;
    }
    let Some(spec) = read(&here.join("upstream.json")) else { return };
    let spec: serde_json::Value = serde_json::from_str(&spec).expect("blender/upstream.json");
    let mut behind = Vec::new();
    for e in spec.as_array().into_iter().flatten() {
        let (file, from, kind) = (e["file"].as_str().unwrap_or(""), e["from"].as_str().unwrap_or(""), e["kind"].as_str().unwrap_or(""));
        let up_path = src.join(from.replace('\\', "/"));
        println!("cargo::rerun-if-changed={}", up_path.display());
        let Some(mut up) = read(&up_path) else {
            behind.push(format!("{file}: {from} is missing from the prototype"));
            continue;
        };
        if kind == "port" {
            if fnv(&up) != e["hash"].as_str().unwrap_or("") {
                behind.push(format!("{file}: the prototype's {from} changed since it was ported"));
            }
            continue;
        }
        let mut ok = true;
        for p in e["patches"].as_array().into_iter().flatten() {
            let (old, new) = (p[0].as_str().unwrap_or(""), p[1].as_str().unwrap_or(""));
            if !up.contains(old) {
                behind.push(format!("{file}: the prototype no longer has a patched line; re-patch it"));
                ok = false;
                break;
            }
            up = up.replacen(old, new, 1);
        }
        if ok && read(&here.join(file)).as_deref() != Some(up.as_str()) {
            behind.push(format!("{file}: differs from {from} + its patches (python blender/upstream.py --write)"));
        }
    }
    if behind.is_empty() {
        return;
    }
    if std::env::var_os("CI").is_some() {
        for b in &behind {
            println!("cargo::warning=bundled Blender script behind the prototype: {b}");
        }
    } else {
        panic!("bundled Blender scripts are behind the prototype:\n  {}", behind.join("\n  "));
    }
}

fn main() {
    check_upstream();
    tauri_build::build()
}
