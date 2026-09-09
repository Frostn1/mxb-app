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

    // The secure-content packer, gated independently. Both binaries need it — the manager
    // injects the client at launch, the studio seals a track to a buyer — so like `sidecar`
    // it lives here and the decision is published rather than made twice.
    println!("cargo::rustc-check-cfg=cfg(mxbsecure)");
    let secure = Path::new("src/mxbsecure.rs").exists();
    if secure {
        println!("cargo::rustc-cfg=mxbsecure");
    }
    println!("cargo::rerun-if-changed=src/mxbsecure.rs");
    println!("cargo::metadata=mxbsecure={}", if secure { "1" } else { "0" });
}
