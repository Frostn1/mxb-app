use std::path::Path;

fn main() {
    // Optional local-only module: enable cfg when its file is present.
    println!("cargo::rustc-check-cfg=cfg(pkz_ext)");
    if Path::new("src/pkz_ext.rs").exists() {
        println!("cargo::rustc-cfg=pkz_ext");
    }
    println!("cargo::rerun-if-changed=src/pkz_ext.rs");

    tauri_build::build()
}
