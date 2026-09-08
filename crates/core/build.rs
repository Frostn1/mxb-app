use std::path::Path;

fn main() {
    // The optional local-only module lives in this crate now, so this is the only build
    // script that can see whether it is present.
    println!("cargo::rustc-check-cfg=cfg(sidecar)");
    let have = Path::new("src/sidecar.rs").exists();
    if have {
        println!("cargo::rustc-cfg=sidecar");
    }
    println!("cargo::rerun-if-changed=src/sidecar.rs");

    // Publish the decision to dependent crates. `apps/manager` still holds `sidecar_lock`,
    // which is gated on the same cfg and calls into `mxb_core::sidecar` — and its own build
    // script cannot see this tree. Without this the two would disagree and the failure would
    // land as a compile error deep inside sidecar_lock with nothing pointing at the cause.
    // Delivered as DEP_MXBCORE_SIDECAR, which the `links` key in Cargo.toml enables.
    println!("cargo::metadata=sidecar={}", if have { "1" } else { "0" });
}
