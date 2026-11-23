fn main() {
    // The secure-content reader lives in mxb-core and is absent from the public tree. Mirror
    // core's decision, the way the manager does, so a secured track locked to the rider opens.
    println!("cargo::rustc-check-cfg=cfg(mxbsecure)");
    if std::env::var("DEP_MXBCORE_MXBSECURE").as_deref() == Ok("1") {
        println!("cargo::rustc-cfg=mxbsecure");
    }
    tauri_build::build()
}
