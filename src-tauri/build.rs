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

    shop_credentials();

    tauri_build::build()
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
