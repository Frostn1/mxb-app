fn main() {
    // windows-gnu links WebView2 through WebView2Loader.dll (MSVC links it statically), so the
    // installer has to ship it next to the exe. webview2-com-sys's build script, which runs
    // before this one, leaves a copy in its OUT_DIR; it goes beside this manifest, where
    // tauri.conf.json's bundle resources pick it up. This app is only built with windows-gnu.
    let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86") => "x86",
        _ => "x64",
    };
    // OUT_DIR is target/<profile>/build/<this>-<hash>/out.
    let build = out.parent().and_then(|p| p.parent()).expect("build dir").to_path_buf();
    let found = std::fs::read_dir(&build)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("webview2-com-sys-"))
        .map(|e| e.path().join("out").join(arch).join("WebView2Loader.dll"))
        .filter(|p| p.is_file())
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    let src = found.unwrap_or_else(|| panic!("WebView2Loader.dll not found under {}", build.display()));
    std::fs::copy(&src, manifest.join("WebView2Loader.dll")).expect("copy WebView2Loader.dll");
    tauri_build::build()
}
