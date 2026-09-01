use std::collections::HashMap;
use std::path::Path;

/// Keys read from `.env.local` (or the build environment) and baked into the binary.
/// See `src/shop_credentials.rs` for what they're for and why they live here.
const SHOP_KEYS: [&str; 2] = ["MXB_SHOP_API_HEADER", "MXB_SHOP_API_KEY"];

fn main() {
    // Optional local-only module: enable cfg when its file is present.
    println!("cargo::rustc-check-cfg=cfg(sidecar)");
    if Path::new("src/sidecar.rs").exists() {
        println!("cargo::rustc-cfg=sidecar");
    }
    println!("cargo::rerun-if-changed=src/sidecar.rs");
    println!("cargo::rerun-if-changed=src/sidecar_lock.rs");

    // The secure-content packer, gated independently of the sidecar modules above.
    println!("cargo::rustc-check-cfg=cfg(mxbsecure)");
    if Path::new("src/mxbsecure.rs").exists() {
        println!("cargo::rustc-cfg=mxbsecure");
    }
    println!("cargo::rerun-if-changed=src/mxbsecure.rs");

    // Place the injected client DLL next to the built executable, so a dev build can find it
    // beside itself with nothing to copy by hand. The file is gitignored and put here by
    // `mxbapp-private/sync.sh`; absent in a public build, where this is a no-op.
    stage_secure_dll();

    shop_credentials();
    release_tag();

    tauri_build::build()
}

/// Stage `src/mxbsecure.dll` (if present) so both a dev build and the installer can find it.
///
/// Two destinations: beside the built exe (a dev build reads it there), and into `resources/`,
/// which `tauri.conf.json` globs into the packaged app — the beside-the-exe copy isn't in the
/// installer, so a released build needs this to ship the DLL at all. Both are no-ops in a
/// public build, where the gitignored file is absent.
fn stage_secure_dll() {
    println!("cargo::rerun-if-changed=src/mxbsecure.dll");
    let src = Path::new("src/mxbsecure.dll");
    if !src.exists() {
        return;
    }
    // (1) Beside the exe. OUT_DIR is `<target>/<profile>/build/<crate>-<hash>/out`; up three.
    let out_dir = std::env::var("OUT_DIR").unwrap_or_default();
    if let Some(target_dir) = Path::new(&out_dir).ancestors().nth(3) {
        if let Err(e) = std::fs::copy(src, target_dir.join("mxbsecure.dll")) {
            println!("cargo::warning=could not stage mxbsecure.dll beside the exe: {e}");
        }
    }
    // (2) Into resources/, bundled into the installer by the `resources` glob.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let res_dir = Path::new(&manifest_dir).join("resources");
    if std::fs::create_dir_all(&res_dir).is_ok() {
        if let Err(e) = std::fs::copy(src, res_dir.join("mxbsecure.dll")) {
            println!("cargo::warning=could not stage mxbsecure.dll into resources: {e}");
        }
    }
}

/// Bake in the git tag this build came from, when there is one.
///
/// `tauri.conf.json` carries a plain `x.y.z` and the release workflow never rewrites it, so
/// `package_info().version` can't tell a `v0.8.0-beta.1` build from the `v0.8.0` that follows
/// it. The tag is the only place that distinction exists — see `experimental_state`, which
/// prefers this over the packaged version so a beta names itself.
///
/// Absent is the normal case (every local build), and `option_env!` then yields `None`.
fn release_tag() {
    // Without this, `Swatinem/rust-cache` in CI would reuse an object file compiled against
    // the previous tag — same reason the shop credentials declare it below.
    println!("cargo::rerun-if-env-changed=MXB_RELEASE_TAG");
    if let Some(tag) = std::env::var("MXB_RELEASE_TAG")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        println!("cargo::rustc-env=MXB_RELEASE_TAG={}", tag.trim());
    }
}

/// Bake the shop-catalog API credential in, when the build has one.
///
/// The token can't be a Vite env var: those are inlined into the JS bundle, so shipping
/// it that way would hand it to anyone who unzips the app. It also can't be a runtime
/// setting, because it's one shared credential rather than a per-user login. So it comes
/// in here and lives only in Rust.
///
/// The process environment wins over `.env.local`, so a CI build using repo secrets can't
/// be quietly poisoned by a stale file in a checkout. Emitting nothing is a supported
/// outcome — `option_env!` then yields `None`, and the app hides its Shop tab.
fn shop_credentials() {
    // Without these, `Swatinem/rust-cache` in CI would happily reuse an object file
    // compiled against yesterday's token — or against no token at all.
    println!("cargo::rerun-if-changed=../.env.local");
    for key in SHOP_KEYS {
        println!("cargo::rerun-if-env-changed={key}");
    }

    let local = read_dotenv("../.env.local");
    for key in SHOP_KEYS {
        let value = std::env::var(key)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| local.get(key).cloned())
            .filter(|v| !v.trim().is_empty());
        if let Some(value) = value {
            println!("cargo::rustc-env={key}={value}");
        }
    }
}

/// The smallest `.env` reader that covers what we ask people to write: `KEY=value`,
/// `#` comments, blank lines, and optional surrounding quotes. A missing file is normal.
fn read_dotenv(path: &str) -> HashMap<String, String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| {
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            (key.trim().to_string(), value.to_string())
        })
        .collect()
}
