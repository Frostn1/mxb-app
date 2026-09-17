use std::path::Path;

/// The Windows side-by-side manifest every binary that links Tauri needs.
///
/// Tauri's window and dialog code imports from `comctl32.dll`, and the functions it wants
/// (`SetWindowSubclass` and friends) exist only in **version 6** — which lives in a
/// side-by-side assembly and is handed to a process only if its manifest asks for it. With no
/// manifest the loader binds the version 5 copy in System32, finds the exports missing, and
/// kills the process before `main` with `STATUS_ENTRYPOINT_NOT_FOUND`.
///
/// The two app crates get this from `tauri_build::build()`. This crate has no `tauri-build`
/// — it is a library, not an app — so its *test* binaries had nothing, and every one of this
/// crate's tests has been dying at startup on Windows since the day it was split out.
const COMMON_CONTROLS_MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>
"#;

/// The XOR pad the usage build key is obfuscated with before it is baked in. MUST match `PAD`
/// in `src/usage.rs`. Obfuscation, not secrecy — see the note there.
const USAGE_PAD: [u8; 32] = [
    0x50, 0x9e, 0x35, 0xb3, 0x30, 0xb8, 0x6e, 0x96, 0x77, 0x76, 0x19, 0x54,
    0xc7, 0x96, 0xad, 0x78, 0x19, 0xa3, 0x9e, 0xa6, 0x30, 0xa7, 0xb6, 0x4b,
    0x3a, 0x23, 0xa0, 0xcf, 0x71, 0x95, 0xa0, 0xf9,
];

/// Bake in the key usage reports are signed with, when this build was given one.
///
/// From the environment only — no `.env.local` fallback, unlike the shop credential. A local
/// build has no reason to sign: debug builds do not report at all, and the endpoint accepts
/// unsigned reports until a deployment says otherwise. CI is the only place this matters.
///
/// Emitting nothing is a supported outcome, and is what a fork produces: `option_env!` then
/// yields `None`, the app signs nothing, and the endpoint takes the report anyway while
/// `MXB_USAGE_REQUIRE_SIGNATURE` is off.
fn usage_build_key() {
    // Without this, Cargo would not know the compiled artifact depends on the variable at all —
    // `option_env!` is invisible to its fingerprint — and `Swatinem/rust-cache` in CI would
    // happily reuse an object file compiled against yesterday's key, or against no key.
    println!("cargo::rerun-if-env-changed=MXB_USAGE_KEY");

    let Some(key) = std::env::var("MXB_USAGE_KEY")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    else {
        return;
    };
    // Hex of the value XOR the cycled pad, so the key is not a `strings` hit in a shipped
    // binary. Anyone who reads the pad recovers it — that is true of all client-side
    // obfuscation, and `usage.rs` is plain about what the signature is therefore worth.
    let obf: String = key
        .as_bytes()
        .iter()
        .enumerate()
        .map(|(i, b)| format!("{:02x}", b ^ USAGE_PAD[i % USAGE_PAD.len()]))
        .collect();
    println!("cargo::rustc-env=MXB_USAGE_KEY_OBF={obf}");
}

fn main() {
    usage_build_key();

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

    // `rustc-link-arg` and not `-tests`: Cargo has no such key, whatever the shape of the
    // others suggests — it rejects the whole build script with "invalid instruction". The
    // plain form covers benchmarks, binaries, cdylibs, examples and tests, which for a crate
    // that is only ever an rlib means its test harness and nothing else. The apps link this
    // as a dependency and do not inherit it, so they keep the one manifest tauri-build gives
    // them rather than gaining a duplicate resource.
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows && msvc {
        let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
        let at = out.join("mxb-core-tests.manifest");
        std::fs::write(&at, COMMON_CONTROLS_MANIFEST).expect("write the test manifest");
        println!("cargo::rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo::rustc-link-arg=/MANIFESTINPUT:{}", at.display());
    }
}
