use std::path::Path;

fn main() {
    // Optional local-only module: enable cfg when its file is present.
    println!("cargo::rustc-check-cfg=cfg(sidecar)");
    if Path::new("src/sidecar.rs").exists() {
        println!("cargo::rustc-cfg=sidecar");
    }
    println!("cargo::rerun-if-changed=src/sidecar.rs");

    tauri_build::build()
}
