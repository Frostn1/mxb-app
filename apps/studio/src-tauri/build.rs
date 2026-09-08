use std::path::Path;

fn main() {
    // `sidecar.rs` lives in mxb-core, which owns the decision and publishes it; this build
    // script cannot see that tree. `sidecar_lock` is ours and is gated on the same cfg, so
    // the two disagreeing would surface as a compile error inside it with nothing naming
    // the cause. DEP_MXBCORE_SIDECAR comes from core's `links = "mxbcore"`.
    println!("cargo::rustc-check-cfg=cfg(sidecar)");
    let core_has_sidecar = std::env::var("DEP_MXBCORE_SIDECAR").as_deref() == Ok("1");
    if core_has_sidecar {
        println!("cargo::rustc-cfg=sidecar");
    }
    if Path::new("src/sidecar_lock.rs").exists() && !core_has_sidecar {
        panic!(
            "src/sidecar_lock.rs is present but mxb-core has no sidecar.rs — the private \
             module was synced into one tree and not the other. Run the private repo's \
             sync.sh again."
        );
    }
    println!("cargo::rerun-if-changed=src/sidecar_lock.rs");

    tauri_build::build()
}
