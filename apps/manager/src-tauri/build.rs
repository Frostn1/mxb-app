use std::collections::HashMap;
use std::path::Path;

/// Keys read from `.env.local` (or the build environment) and baked into the binary.
/// See `src/shop_credentials.rs` for what they're for and why they live here.
const SHOP_KEYS: [&str; 2] = ["MXB_SHOP_API_HEADER", "MXB_SHOP_API_KEY"];

fn main() {
    // Optional local-only module. `sidecar.rs` itself lives in mxb-core now, so this
    // script cannot see it — mirror the decision core published instead of guessing from
    // our own tree. `sidecar_lock` is still ours and is gated on the same cfg, so the two
    // crates disagreeing would surface as a compile error inside it with nothing naming
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

    // The world-server browser client, gated the same way: present locally, absent from the
    // public tree. Holds the master-server protocol, so it never ships in the open source.
    println!("cargo::rustc-check-cfg=cfg(worldnet)");
    if Path::new("src/worldnet.rs").exists() {
        println!("cargo::rustc-cfg=worldnet");
    }
    println!("cargo::rerun-if-changed=src/worldnet.rs");

    // The secure-content packer lives in mxb-core now, for the same reason `sidecar` does:
    // both binaries use it. Mirror core's decision rather than looking for a file we no
    // longer hold.
    println!("cargo::rustc-check-cfg=cfg(mxbsecure)");
    if std::env::var("DEP_MXBCORE_MXBSECURE").as_deref() == Ok("1") {
        println!("cargo::rustc-cfg=mxbsecure");
    }

    // Place the injected client DLL next to the built executable, so a dev build can find it
    // beside itself with nothing to copy by hand. The file is gitignored and put here by
    // `mxbapp-private/sync.sh`; absent in a public build, where this is a no-op.
    stage_secure_binaries();

    shop_credentials();
    release_tag();

    tauri_build::build()
}

/// Stage the injected-client binaries (if present) so both a dev build and the installer can find
/// them: `src/mxbsecure.dll` on every platform, and `src/mxbsecure-inject.exe` — the attach
/// injector the Linux/Proton path launches inside the prefix (Windows injects in-process, so it
/// needs no exe; the file is simply absent on the Windows leg).
///
/// Two destinations each: beside the built exe (a dev build reads it there), and into `resources/`,
/// which `tauri.conf.json` globs into the packaged app — the beside-the-exe copy isn't in the
/// installer, so a released build needs this to ship the files at all. All no-ops in a public
/// build, where the gitignored files are absent.
fn stage_secure_binaries() {
    for name in ["mxbsecure.dll", "mxbsecure-inject.exe"] {
        println!("cargo::rerun-if-changed=src/{name}");
        let src = Path::new("src").join(name);
        if !src.exists() {
            continue;
        }
        // (1) Beside the exe. OUT_DIR is `<target>/<profile>/build/<crate>-<hash>/out`; up three.
        let out_dir = std::env::var("OUT_DIR").unwrap_or_default();
        if let Some(target_dir) = Path::new(&out_dir).ancestors().nth(3) {
            if let Err(e) = copy_if_changed(&src, &target_dir.join(name)) {
                println!("cargo::warning=could not stage {name} beside the exe: {e}");
            }
        }
        // (2) Into resources/, bundled into the installer by the `resources` glob.
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        let res_dir = Path::new(&manifest_dir).join("resources");
        if std::fs::create_dir_all(&res_dir).is_ok() {
            if let Err(e) = copy_if_changed(&src, &res_dir.join(name)) {
                println!("cargo::warning=could not stage {name} into resources: {e}");
            }
        }
    }
}

/// Copy only when the destination differs. Rewriting identical bytes still moves the file's
/// mtime, and `tauri dev` watches `src-tauri/`: the touch restarts the app, which runs this
/// again, which touches it again — the dev server rebuilds forever without ever launching.
fn copy_if_changed(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let (Ok(from), Ok(to)) = (std::fs::read(src), std::fs::read(dst)) {
        if from == to {
            return Ok(());
        }
    }
    std::fs::copy(src, dst).map(|_| ())
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
    println!("cargo::rerun-if-changed=../../../.env.local");
    for key in SHOP_KEYS {
        println!("cargo::rerun-if-env-changed={key}");
    }

    let local = read_dotenv("../../../.env.local");
    for key in SHOP_KEYS {
        let value = std::env::var(key)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| local.get(key).cloned())
            .filter(|v| !v.trim().is_empty());
        if let Some(value) = value {
            // Bake in an XOR-obfuscated form (hex of value XOR the cycled pad), not the
            // plaintext, so the credential isn't a `strings` hit in the shipped binary.
            // `shop_credentials.rs` holds the matching pad and decodes at runtime. Obfuscation,
            // not secrecy — see that module. The trim happens here so the runtime value is clean.
            let obf: String = value
                .trim()
                .as_bytes()
                .iter()
                .enumerate()
                .map(|(i, b)| format!("{:02x}", b ^ SHOP_PAD[i % SHOP_PAD.len()]))
                .collect();
            println!("cargo::rustc-env={key}_OBF={obf}");
        }
    }
}

/// The XOR pad the credential is obfuscated with. MUST match `PAD` in `src/shop_credentials.rs`.
const SHOP_PAD: [u8; 32] = [
    0x41, 0xcb, 0xdf, 0x60, 0x8f, 0xa1, 0xfa, 0x7b, 0xfd, 0xae, 0x81, 0x7c,
    0x3d, 0xd4, 0x15, 0xb4, 0x28, 0x11, 0xff, 0xa8, 0xa6, 0x73, 0x9c, 0x87,
    0x1f, 0x2a, 0x0d, 0xf9, 0x9f, 0x22, 0x8d, 0x8e,
];

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
