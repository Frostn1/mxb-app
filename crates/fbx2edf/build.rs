//! Builds the private converter in when its sources are here (see Cargo.toml).
//!
//! `private/src/lib.rs` is the private crate's root. It can't be this crate's root as it
//! stands, and it can't be a module of it either: its modules name each other `crate::…`. So
//! its body is included at this crate's root, with each `mod x;` given the path of its file.
//! Its inner doc comments are dropped, since an included file can't carry them.

use std::path::Path;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(fbx2edf)");
    // The private crate's own features. Never on here; named so their cfgs aren't warnings.
    println!("cargo::rustc-check-cfg=cfg(feature, values(\"web\", \"testkit\"))");
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=private");

    let src = Path::new("private/src");
    let Ok(root) = std::fs::read_to_string(src.join("lib.rs")) else { return };
    let dir = std::path::absolute(src).expect("private/src has a path");

    let mut out = String::new();
    for line in root.lines() {
        let t = line.trim_start();
        if t.starts_with("//!") || t.starts_with("#![") {
            continue;
        }
        if let Some(name) = module_decl(t) {
            let file = dir.join(format!("{name}.rs"));
            out.push_str(&format!("#[path = {:?}]\n", file.to_string_lossy()));
        }
        out.push_str(line);
        out.push('\n');
    }
    let dest = Path::new(&std::env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("private_root.rs");
    std::fs::write(dest, out).expect("writing the converter's root");
    println!("cargo::rustc-cfg=fbx2edf");
}

/// `mod x;` or `pub mod x;` (any visibility): the module's name.
fn module_decl(line: &str) -> Option<&str> {
    let rest = line.strip_suffix(';')?.trim_end();
    let rest = match rest.find("mod ") {
        Some(0) => rest,
        Some(i) if rest[..i].trim_start().starts_with("pub") => &rest[i..],
        _ => return None,
    };
    let name = rest.strip_prefix("mod ")?.trim();
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_').then_some(name)
}
