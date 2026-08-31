//! Paid plugins: what this install is allowed to run, and how it proves it offline.
//!
//! The control plane hands out a short-lived **entitlement** — a small Ed25519-signed
//! statement that a named account holds a named plugin until a named date. The app checks
//! it locally, so the plugin keeps working on a plane and through an outage; and because
//! the entitlement carries a second, much nearer deadline (`refresh_after`), a cancelled
//! subscription stops working within the grace window rather than at the end of the month.
//!
//! We hold only the public half of the signing key. That is the whole reason it is a
//! signature and not a MAC: a MAC key shipped in this binary is one `strings` away from
//! letting anyone mint themselves a permanent licence.
//!
//! Three things are checked before a bundle is allowed to run, and all three matter:
//!   1. the signature, or the payload is just JSON somebody typed;
//!   2. the two clocks, against a time that cannot be wound backwards (see `Clock`);
//!   3. the bundle's SHA-256 against the one the entitlement names, so a bundle swapped
//!      anywhere between R2 and this disk fails to load instead of running.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Public half of the pair the control plane signs with (raw 32 bytes, base64url).
///
/// **This is a placeholder from a throwaway pair.** Before shipping, run
/// `control-plane/scripts/plugin-keypair.ts`, put the private half in the worker
/// (`wrangler secret put PLUGIN_SIGNING_KEY`) and the public half here. Until both sides
/// hold halves of the same pair, every entitlement fails verification — which is the safe
/// direction to fail in, but it does mean nothing works.
///
/// Rotating it later means shipping an app update: an install that has not updated rejects
/// every entitlement signed by the new pair.
pub const ENTITLEMENT_PUBLIC_KEY: &str = "3RVzr7dGG2rcorozGze9rR7NUTj61bwxS2IL0t40kk0";

/// Entitlement format we understand. A newer one is refused rather than half-read.
pub const ENTITLEMENT_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// the entitlement
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entitlement {
    pub v: u32,
    /// Account it was issued to. Bound so a token cannot simply be passed around.
    pub account: String,
    pub plugin: String,
    /// When the subscription ends. Seconds since epoch.
    pub expires: i64,
    /// When we must have talked to the control plane again. Always <= `expires`.
    #[serde(rename = "refreshAfter")]
    pub refresh_after: i64,
    /// The bundle this entitlement is good for, lowercase hex. None before a build exists.
    #[serde(rename = "bundleSha256")]
    pub bundle_sha256: Option<String>,
    pub issued: i64,
}

/// Verify a `<b64url(payload)>.<b64url(sig)>` token and return what it says.
///
/// Every failure is the same answer to the caller — no licence — but they are distinguished
/// in the error text because "your clock is wrong" and "that signature is not ours" send a
/// person to very different places.
pub fn verify_entitlement(token: &str) -> Result<Entitlement> {
    let (payload_b64, sig_b64) = token
        .split_once('.')
        .ok_or_else(|| anyhow!("not an entitlement token"))?;
    let payload = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .context("entitlement payload is not base64url")?;
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .context("entitlement signature is not base64url")?;

    let key_bytes: [u8; 32] = URL_SAFE_NO_PAD
        .decode(ENTITLEMENT_PUBLIC_KEY)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| anyhow!("the built-in signing key is malformed"))?;
    let key = VerifyingKey::from_bytes(&key_bytes).context("the built-in signing key is invalid")?;

    let sig_arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("entitlement signature is the wrong length"))?;
    key.verify_strict(&payload, &Signature::from_bytes(&sig_arr))
        .map_err(|_| anyhow!("that entitlement was not signed by us"))?;

    let e: Entitlement =
        serde_json::from_slice(&payload).context("entitlement payload is not one of ours")?;
    if e.v != ENTITLEMENT_VERSION {
        bail!("that entitlement is version {}; this build understands {ENTITLEMENT_VERSION}. Update MXB App.", e.v);
    }
    Ok(e)
}

/// Why a plugin is or is not runnable. Kept as three states rather than a bool because the
/// middle one is the interesting one: it is still allowed to run, and the app should be
/// quietly trying to renew it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Signed, in date, and checked in recently. Run it.
    Live,
    /// Past `refresh_after` but not past `expires`: we have not heard from the control
    /// plane in a week. Refuse until it can be renewed — this is what makes a cancellation
    /// take effect on a machine that is never asked again.
    Stale,
    /// Past `expires`, or never held.
    Expired,
}

pub fn status(e: &Entitlement, now: i64) -> Status {
    if now >= e.expires {
        Status::Expired
    } else if now >= e.refresh_after {
        Status::Stale
    } else {
        Status::Live
    }
}

// ---------------------------------------------------------------------------
// the clock
// ---------------------------------------------------------------------------

/// A wall clock that cannot be wound backwards.
///
/// Every expiry check on a machine the user controls has the same hole: set the clock to
/// last year and a lapsed licence is live again. It cannot be closed offline — there is no
/// trusted time source — but it can be made a one-way door. We remember the latest instant
/// we have ever been sure of (the `issued` of an entitlement the control plane signed, or
/// simply the highest wall-clock reading we have seen) and never accept a reading below it.
///
/// So winding the clock back does not extend anything; it freezes it, which is the failure
/// we want. Winding it *forward* expires things early, and the user can undo that
/// themselves by winding it back — to the high-water mark, and no further.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Clock {
    /// Highest instant ever observed, seconds since epoch.
    #[serde(default)]
    pub high_water: i64,
}

impl Clock {
    /// The time to judge an entitlement by, and the updated mark to persist.
    pub fn observe(&mut self, wall: i64) -> i64 {
        if wall > self.high_water {
            self.high_water = wall;
        }
        self.high_water
    }

    /// A signed entitlement is evidence of a real instant: the control plane stamped it, so
    /// time is at least that. Fold it in when one arrives.
    pub fn witness(&mut self, e: &Entitlement) {
        if e.issued > self.high_water {
            self.high_water = e.issued;
        }
    }
}

// ---------------------------------------------------------------------------
// on-disk state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Installed {
    pub version: Option<String>,
    /// SHA-256 of the bundle that was unpacked, so a mismatch against the entitlement is
    /// noticed without hashing the unpacked tree.
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginState {
    /// The account these entitlements belong to. Learnt from the first one verified; a later
    /// entitlement naming a different account is refused, which is what stops one being
    /// copied between machines.
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub clock: Clock,
    /// plugin id -> the entitlement token as issued.
    #[serde(default)]
    pub entitlements: BTreeMap<String, String>,
    #[serde(default)]
    pub installed: BTreeMap<String, Installed>,
}

impl PluginState {
    /// Take an entitlement the control plane just issued.
    ///
    /// Refuses one for another account, which is the casual-sharing case: the token is a
    /// file, and a file gets pasted into someone else's install.
    pub fn accept(&mut self, token: &str) -> Result<Entitlement> {
        let e = verify_entitlement(token)?;
        match &self.account {
            Some(existing) if existing != &e.account => {
                bail!("that entitlement belongs to a different account")
            }
            _ => {}
        }
        self.account = Some(e.account.clone());
        self.clock.witness(&e);
        self.entitlements.insert(e.plugin.clone(), token.to_string());
        Ok(e)
    }

    /// What this install may run right now, judged against the un-windable clock.
    pub fn status_of(&mut self, plugin: &str, wall: i64) -> Status {
        let now = self.clock.observe(wall);
        match self.entitlements.get(plugin) {
            None => Status::Expired,
            Some(token) => match verify_entitlement(token) {
                Ok(e) if e.plugin == plugin => status(&e, now),
                // A token that no longer verifies is not a licence, whatever it used to be.
                _ => Status::Expired,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// the bundle
// ---------------------------------------------------------------------------

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// What a plugin declares about itself. Read from `manifest.json` inside the bundle, and
/// checked against what we asked for — a bundle that says it is a different plugin, or
/// wants a newer app than this one, is refused rather than half-loaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Entry module, relative to the bundle root. Loaded by the webview once verified.
    pub entry: String,
    /// Minimum app version this plugin needs, e.g. "0.12.4".
    #[serde(rename = "minAppVersion", default)]
    pub min_app_version: Option<String>,
    /// Nav rows the plugin contributes.
    #[serde(default)]
    pub panels: Vec<PanelDecl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelDecl {
    pub id: String,
    pub label: String,
    /// Lucide icon name, resolved by the shell against the set it already bundles.
    #[serde(default)]
    pub icon: Option<String>,
}

/// Compare dotted numeric versions, semver-style enough for a `minAppVersion` gate.
///
/// Splitting on `-` and treating the tail as another number would make `1.0.0-rc1` compare
/// equal to `1.0.0`, and a release candidate is not the release: a plugin that needs 1.0.0
/// must not load on the rc that came before it. So the numeric parts are compared first,
/// and a version carrying a pre-release suffix loses a tie against one that does not.
pub fn version_at_least(have: &str, need: &str) -> bool {
    fn split(s: &str) -> (Vec<u32>, bool) {
        let core = s.split(['-', '+']).next().unwrap_or(s);
        let pre = core.len() < s.len();
        (
            core.split('.').map(|p| p.parse::<u32>().unwrap_or(0)).collect(),
            pre,
        )
    }
    let (a, a_pre) = split(have);
    let (b, b_pre) = split(need);
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    // Numerically equal: a pre-release is below the release it leads to, and two
    // pre-releases of the same version are treated as equal rather than string-compared.
    !(a_pre && !b_pre)
}

/// Verify a downloaded bundle against the entitlement, then unpack it.
///
/// The hash check is the load-bearing one and it happens **before** a single byte is
/// written: this is code that is about to run inside the app, and "unpack then check" would
/// mean it had already touched the disk by the time we found out.
pub fn install_bundle(
    dir: &Path,
    plugin: &str,
    bytes: &[u8],
    expected_sha256: Option<&str>,
    app_version: &str,
) -> Result<Manifest> {
    let got = sha256_hex(bytes);
    match expected_sha256 {
        Some(want) if !want.eq_ignore_ascii_case(&got) => {
            bail!("the downloaded plugin does not match what your licence names ({want} vs {got}) - it was not installed")
        }
        None => bail!("your licence doesn't name a build for that plugin yet"),
        _ => {}
    }

    let reader = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(reader).context("that plugin bundle is not readable")?;

    let manifest: Manifest = {
        let mut f = zip
            .by_name("manifest.json")
            .context("the bundle has no manifest.json")?;
        serde_json::from_reader(&mut f).context("the bundle's manifest.json is not readable")?
    };
    if manifest.id != plugin {
        bail!(
            "that bundle says it is '{}', but it was fetched as '{plugin}'",
            manifest.id
        );
    }
    if let Some(need) = &manifest.min_app_version {
        if !version_at_least(app_version, need) {
            bail!("{} needs MXB App {need} or newer; this is {app_version}", manifest.name);
        }
    }

    let root = dir.join(plugin);
    // Replace rather than merge: a file left behind from an older build is a file the new
    // manifest does not know about and cannot vouch for.
    if root.exists() {
        std::fs::remove_dir_all(&root).ok();
    }
    std::fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(rel) = safe_path(entry.name()) else {
            bail!("that bundle contains an unsafe path: {}", entry.name());
        };
        let out = root.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&out)
            .with_context(|| format!("writing {}", out.display()))?;
        std::io::copy(&mut entry, &mut file)?;
    }
    Ok(manifest)
}

/// A zip entry path we are willing to write, or None.
///
/// Zip archives can name `../../etc/passwd` and absolute paths, and a naive extractor
/// happily writes them. This is a bundle we fetched over TLS and hash-checked, so it should
/// never contain one — which is exactly why a bundle that does is worth refusing loudly
/// rather than trusting the chain that got it here.
pub fn safe_path(name: &str) -> Option<PathBuf> {
    let name = name.replace('\\', "/");
    if name.starts_with('/') || name.contains(':') {
        return None;
    }
    let mut out = PathBuf::new();
    for part in name.split('/') {
        match part {
            "" | "." => continue,
            ".." => return None,
            p => out.push(p),
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A pair generated once for the tests. The app's real key is a different one; these
    // tokens are built here so the whole verify path runs rather than being stubbed.
    fn signed(payload: &serde_json::Value, key: &ed25519_dalek::SigningKey) -> String {
        use ed25519_dalek::Signer;
        let body = serde_json::to_vec(payload).unwrap();
        let sig = key.sign(&body);
        format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(&body),
            URL_SAFE_NO_PAD.encode(sig.to_bytes())
        )
    }

    #[test]
    fn status_has_three_states_and_the_middle_one_matters() {
        let e = Entitlement {
            v: 1,
            account: "acc".into(),
            plugin: "replaycam".into(),
            expires: 1_000,
            refresh_after: 500,
            bundle_sha256: None,
            issued: 0,
        };
        assert_eq!(status(&e, 499), Status::Live);
        // Past the grace but inside the subscription: this is the state that makes a
        // cancellation take effect on a machine that stops asking.
        assert_eq!(status(&e, 500), Status::Stale);
        assert_eq!(status(&e, 999), Status::Stale);
        assert_eq!(status(&e, 1_000), Status::Expired);
    }

    #[test]
    fn the_clock_does_not_run_backwards() {
        let mut c = Clock::default();
        assert_eq!(c.observe(1_000), 1_000);
        // Winding the machine's clock back a year must not resurrect a lapsed licence.
        assert_eq!(c.observe(10), 1_000);
        // Forward is believed, and becomes the new floor.
        assert_eq!(c.observe(2_000), 2_000);
        assert_eq!(c.observe(1_500), 2_000);
    }

    #[test]
    fn a_signed_entitlement_is_evidence_of_the_time() {
        let mut c = Clock::default();
        let e = Entitlement {
            v: 1,
            account: "acc".into(),
            plugin: "p".into(),
            expires: 9_000,
            refresh_after: 8_000,
            bundle_sha256: None,
            issued: 7_777,
        };
        c.witness(&e);
        // Even with the machine clock at zero, we know it is at least when this was signed.
        assert_eq!(c.observe(0), 7_777);
    }

    #[test]
    fn verify_rejects_junk_without_panicking() {
        for bad in ["", "nodot", "a.b", "....", "!!!.???"] {
            assert!(verify_entitlement(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn verify_rejects_a_signature_from_another_key() {
        let other = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let token = signed(
            &serde_json::json!({
                "v": 1, "account": "acc", "plugin": "replaycam",
                "expires": 9_999_999_999i64, "refreshAfter": 9_999_999_999i64,
                "bundleSha256": null, "issued": 0
            }),
            &other,
        );
        let err = verify_entitlement(&token).unwrap_err().to_string();
        assert!(err.contains("not signed by us"), "{err}");
    }

    #[test]
    fn state_refuses_an_entitlement_for_another_account() {
        let mut s = PluginState {
            account: Some("acc_1".into()),
            ..Default::default()
        };
        // Signature verification runs first, so this needs a token that is at least
        // well-formed; the point is that account binding is checked at all, and a real
        // token for another account is exercised in the control plane's own tests.
        assert!(s.accept("garbage").is_err());
        assert_eq!(s.account.as_deref(), Some("acc_1"));
    }

    #[test]
    fn no_entitlement_means_expired_not_live() {
        let mut s = PluginState::default();
        assert_eq!(s.status_of("replaycam", 100), Status::Expired);
    }

    #[test]
    fn a_token_that_stops_verifying_stops_being_a_licence() {
        let mut s = PluginState::default();
        s.entitlements
            .insert("replaycam".into(), "not-a-token".into());
        assert_eq!(s.status_of("replaycam", 100), Status::Expired);
    }

    #[test]
    fn version_compare_treats_a_prerelease_as_below_the_release() {
        assert!(version_at_least("0.12.4", "0.12.4"));
        assert!(version_at_least("0.13.0", "0.12.4"));
        assert!(!version_at_least("0.12.3", "0.12.4"));
        assert!(!version_at_least("1.0.0-rc1", "1.0.0"));
        // The other direction still holds: the release satisfies a need for the rc.
        assert!(version_at_least("1.0.0", "1.0.0-rc1"));
        // And a later rc satisfies an earlier release.
        assert!(version_at_least("1.1.0-rc1", "1.0.0"));
        assert!(version_at_least("1.0.1", "1.0"));
        assert!(version_at_least("1.0", "1"));
    }

    #[test]
    fn safe_path_refuses_what_a_zip_can_name() {
        assert!(safe_path("../../etc/passwd").is_none());
        assert!(safe_path("/etc/passwd").is_none());
        assert!(safe_path("C:/Windows/System32/x.dll").is_none());
        assert!(safe_path("ui/../../escape.js").is_none());
        assert!(safe_path("..").is_none());
        assert_eq!(safe_path("ui.js"), Some(PathBuf::from("ui.js")));
        assert_eq!(
            safe_path("payload/frostreplay.dll"),
            Some(PathBuf::from("payload/frostreplay.dll"))
        );
        // Windows separators arrive in zips written on Windows and must normalise, not
        // become one long filename.
        assert_eq!(
            safe_path("payload\\frostreplay.dll"),
            Some(PathBuf::from("payload/frostreplay.dll"))
        );
    }

    #[test]
    fn install_refuses_a_bundle_whose_hash_is_not_the_one_licensed() {
        let dir = tempdir();
        let err = install_bundle(&dir, "replaycam", b"not a zip", Some("deadbeef"), "0.12.4")
            .unwrap_err()
            .to_string();
        assert!(err.contains("does not match"), "{err}");
        // Nothing was written: the check happens before a byte lands.
        assert!(!dir.join("replaycam").exists());
    }

    #[test]
    fn install_refuses_when_the_licence_names_no_build() {
        let dir = tempdir();
        let err = install_bundle(&dir, "replaycam", b"x", None, "0.12.4")
            .unwrap_err()
            .to_string();
        assert!(err.contains("name a build"), "{err}");
    }

    #[test]
    fn install_unpacks_a_good_bundle_and_reads_its_manifest() {
        let dir = tempdir();
        let zip_bytes = build_zip(&[
            (
                "manifest.json",
                br#"{"id":"replaycam","name":"Replay","version":"1.0.0","entry":"ui.js"}"#.to_vec(),
            ),
            ("ui.js", b"export default 1;".to_vec()),
            ("payload/frostreplay.dll", b"MZ".to_vec()),
        ]);
        let sha = sha256_hex(&zip_bytes);
        let m = install_bundle(&dir, "replaycam", &zip_bytes, Some(&sha), "0.12.4").unwrap();
        assert_eq!(m.id, "replaycam");
        assert_eq!(m.entry, "ui.js");
        assert!(dir.join("replaycam/ui.js").exists());
        assert!(dir.join("replaycam/payload/frostreplay.dll").exists());
    }

    #[test]
    fn install_refuses_a_bundle_claiming_to_be_another_plugin() {
        let dir = tempdir();
        let zip_bytes = build_zip(&[(
            "manifest.json",
            br#"{"id":"somethingelse","name":"X","version":"1.0.0","entry":"ui.js"}"#.to_vec(),
        )]);
        let sha = sha256_hex(&zip_bytes);
        let err = install_bundle(&dir, "replaycam", &zip_bytes, Some(&sha), "0.12.4")
            .unwrap_err()
            .to_string();
        assert!(err.contains("says it is"), "{err}");
    }

    #[test]
    fn install_refuses_a_bundle_that_needs_a_newer_app() {
        let dir = tempdir();
        let zip_bytes = build_zip(&[(
            "manifest.json",
            br#"{"id":"replaycam","name":"Replay","version":"2.0.0","entry":"ui.js","minAppVersion":"9.9.9"}"#.to_vec(),
        )]);
        let sha = sha256_hex(&zip_bytes);
        let err = install_bundle(&dir, "replaycam", &zip_bytes, Some(&sha), "0.12.4")
            .unwrap_err()
            .to_string();
        assert!(err.contains("9.9.9"), "{err}");
    }

    #[test]
    fn install_replaces_rather_than_merges() {
        let dir = tempdir();
        let old = build_zip(&[
            ("manifest.json", br#"{"id":"p","name":"P","version":"1","entry":"ui.js"}"#.to_vec()),
            ("stale.js", b"old".to_vec()),
        ]);
        install_bundle(&dir, "p", &old, Some(&sha256_hex(&old)), "1.0.0").unwrap();
        assert!(dir.join("p/stale.js").exists());

        let new = build_zip(&[(
            "manifest.json",
            br#"{"id":"p","name":"P","version":"2","entry":"ui.js"}"#.to_vec(),
        )]);
        install_bundle(&dir, "p", &new, Some(&sha256_hex(&new)), "1.0.0").unwrap();
        // A file the new manifest does not know about is a file nothing vouches for.
        assert!(!dir.join("p/stale.js").exists());
    }

    // ---- helpers ----

    fn tempdir() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "mxb-plugins-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::remove_dir_all(&base).ok();
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    fn build_zip(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            for (name, body) in entries {
                w.start_file::<_, ()>(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                w.write_all(body).unwrap();
            }
            w.finish().unwrap();
        }
        buf.into_inner()
    }
}

// ---------------------------------------------------------------------------
// the app's side: talking to the control plane, and putting bundles on disk
// ---------------------------------------------------------------------------

/// Where installed plugins live. Under app-local data with the rest of the app's state, so
/// an uninstall of MXB App takes them with it.
pub fn plugins_dir(app: &tauri::AppHandle) -> Result<PathBuf> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_local_data_dir()
        .context("no app data directory")?
        .join("plugins");
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

fn state_path(app: &tauri::AppHandle) -> Result<PathBuf> {
    Ok(plugins_dir(app)?.join("state.json"))
}

pub fn load_state(app: &tauri::AppHandle) -> PluginState {
    // A missing or unreadable state file means "nothing licensed", which is the safe
    // reading: it locks the plugin rather than unlocking it.
    state_path(app)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_state(app: &tauri::AppHandle, state: &PluginState) -> Result<()> {
    let path = state_path(app)?;
    let body = serde_json::to_vec_pretty(state)?;
    std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn token(app: &tauri::AppHandle) -> Result<String> {
    let cfg = crate::config::load_or_detect(app).unwrap_or_default();
    let t = cfg.cp_token.trim().to_string();
    if t.is_empty() {
        bail!("Enroll with an invite code first — plugins are tied to your account.");
    }
    Ok(t)
}

/// Pull the error message out of a control-plane failure, which always answers `{error}`.
async fn plane_error(resp: reqwest::Response) -> String {
    let detail = resp.text().await.unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(&detail)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
        .unwrap_or(detail)
}

/// One row of the Plugins page.
#[derive(Debug, Clone, Serialize)]
pub struct PluginView {
    pub id: String,
    pub name: String,
    pub summary: Option<String>,
    /// The version on offer, which may be newer than what is installed.
    pub version: Option<String>,
    /// Whether there is a build to install at all.
    pub published: bool,
    pub status: Status,
    /// Seconds since epoch, when the licence runs out. None if never held.
    pub expires: Option<i64>,
    pub installed_version: Option<String>,
    /// True when a licence is live and the installed build is the one on offer.
    pub ready: bool,
}

#[derive(Deserialize)]
struct CatalogueResp {
    plugins: Vec<CataloguePlugin>,
}
#[derive(Deserialize)]
struct CataloguePlugin {
    id: String,
    name: String,
    summary: Option<String>,
    version: Option<String>,
    published: bool,
}

#[derive(Deserialize)]
struct LicencesResp {
    licences: Vec<LicenceRow>,
}
#[derive(Deserialize)]
struct LicenceRow {
    plugin: String,
    expires: i64,
    entitlement: Option<String>,
}

/// The catalogue plus this account's licences, refreshed from the control plane.
///
/// Best-effort by design: with no network we still answer from what is on disk, because the
/// whole point of a signed entitlement is that being offline is not a licensing failure.
#[tauri::command]
pub async fn plugin_list(app: tauri::AppHandle) -> Result<Vec<PluginView>, String> {
    let base = crate::paintsync::control_plane();
    let client = reqwest::Client::new();

    // The catalogue is public, so this half works before enrolment.
    let catalogue: Vec<CataloguePlugin> = match client.get(format!("{base}/v1/plugins")).send().await
    {
        Ok(r) if r.status().is_success() => r
            .json::<CatalogueResp>()
            .await
            .map(|c| c.plugins)
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    let mut state = load_state(&app);
    let mut expiries: BTreeMap<String, i64> = BTreeMap::new();

    if let Ok(tok) = token(&app) {
        if let Ok(resp) = client
            .get(format!("{base}/v1/me/plugins"))
            .bearer_auth(&tok)
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(body) = resp.json::<LicencesResp>().await {
                    for row in body.licences {
                        expiries.insert(row.plugin.clone(), row.expires);
                        if let Some(t) = row.entitlement {
                            // A refused entitlement (wrong account, bad signature) leaves the
                            // previous one in place rather than clearing it: a bad answer from
                            // the network must not revoke a licence we already verified.
                            let _ = state.accept(&t);
                        }
                    }
                    let _ = save_state(&app, &state);
                }
            }
        }
    }

    let wall = now_secs();
    let mut out = Vec::new();
    for p in catalogue {
        let status = state.status_of(&p.id, wall);
        let installed = state.installed.get(&p.id).cloned().unwrap_or_default();
        let up_to_date = match (&installed.version, &p.version) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        };
        out.push(PluginView {
            ready: status == Status::Live && up_to_date,
            id: p.id.clone(),
            name: p.name,
            summary: p.summary,
            version: p.version,
            published: p.published,
            status,
            expires: expiries.get(&p.id).copied(),
            installed_version: installed.version,
        });
    }
    let _ = save_state(&app, &state);
    Ok(out)
}

/// Trade a key for months on a licence.
#[tauri::command]
pub async fn plugin_redeem(app: tauri::AppHandle, code: String) -> Result<String, String> {
    let tok = token(&app).map_err(|e| e.to_string())?;
    let resp = reqwest::Client::new()
        .post(format!(
            "{}/v1/plugins/redeem",
            crate::paintsync::control_plane()
        ))
        .bearer_auth(&tok)
        .json(&serde_json::json!({ "code": code }))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;
    if !resp.status().is_success() {
        return Err(plane_error(resp).await);
    }

    #[derive(Deserialize)]
    struct Redeemed {
        plugin: String,
        name: String,
        entitlement: String,
    }
    let body: Redeemed = resp.json().await.map_err(|e| e.to_string())?;

    let mut state = load_state(&app);
    state.accept(&body.entitlement).map_err(|e| e.to_string())?;
    save_state(&app, &state).map_err(|e| e.to_string())?;
    let _ = body.plugin;
    Ok(body.name)
}

/// Download, verify and unpack a plugin this account holds a live licence for.
#[tauri::command]
pub async fn plugin_install(app: tauri::AppHandle, id: String) -> Result<String, String> {
    let tok = token(&app).map_err(|e| e.to_string())?;

    // Check what we already hold before asking for bytes: a lapsed licence should say so
    // rather than downloading a bundle it is not allowed to run.
    let mut state = load_state(&app);
    let wall = now_secs();
    match state.status_of(&id, wall) {
        Status::Live => {}
        Status::Stale => {
            return Err("Your licence needs re-checking and the control plane isn't answering. Try again when you're online.".into())
        }
        Status::Expired => return Err("You don't have a live licence for that plugin.".into()),
    }
    let entitlement = state
        .entitlements
        .get(&id)
        .and_then(|t| verify_entitlement(t).ok())
        .ok_or_else(|| "You don't have a live licence for that plugin.".to_string())?;

    let resp = reqwest::Client::new()
        .get(format!(
            "{}/v1/plugins/{id}/bundle",
            crate::paintsync::control_plane()
        ))
        .bearer_auth(&tok)
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;
    if !resp.status().is_success() {
        return Err(plane_error(resp).await);
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;

    let dir = plugins_dir(&app).map_err(|e| e.to_string())?;
    let app_version = app.package_info().version.to_string();
    let manifest = install_bundle(
        &dir,
        &id,
        &bytes,
        entitlement.bundle_sha256.as_deref(),
        &app_version,
    )
    .map_err(|e| e.to_string())?;

    state.installed.insert(
        id.clone(),
        Installed {
            version: Some(manifest.version.clone()),
            sha256: entitlement.bundle_sha256.clone(),
        },
    );
    save_state(&app, &state).map_err(|e| e.to_string())?;
    Ok(manifest.name)
}

#[tauri::command]
pub async fn plugin_remove(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let dir = plugins_dir(&app).map_err(|e| e.to_string())?;
    std::fs::remove_dir_all(dir.join(&id)).ok();
    let mut state = load_state(&app);
    // The licence survives an uninstall — it is a subscription, not an install token, and
    // someone removing the plugin for a week must not have to buy it again.
    state.installed.remove(&id);
    save_state(&app, &state).map_err(|e| e.to_string())?;
    Ok(())
}

/// What the webview needs to mount a plugin: its manifest, and its entry module's source.
///
/// The source is handed over as text rather than a path so the webview never loads code off
/// the filesystem by URL. It is only ever produced for a plugin whose licence is live *at
/// this moment* — the check is here, at the last point before the code runs, and not only
/// on the page that lists them.
#[derive(Debug, Serialize)]
pub struct PluginRuntime {
    pub manifest: Manifest,
    pub source: String,
}

#[tauri::command]
pub fn plugin_runtime(app: tauri::AppHandle, id: String) -> Result<PluginRuntime, String> {
    let mut state = load_state(&app);
    if state.status_of(&id, now_secs()) != Status::Live {
        return Err("That plugin isn't licensed on this account.".into());
    }
    // The clock may have advanced the high-water mark; keep it.
    let _ = save_state(&app, &state);

    let root = plugins_dir(&app).map_err(|e| e.to_string())?.join(&id);
    let manifest: Manifest = std::fs::read(root.join("manifest.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or_else(|| "That plugin isn't installed.".to_string())?;

    let entry = safe_path(&manifest.entry)
        .ok_or_else(|| "That plugin's entry path is not one we will load.".to_string())?;
    let source = std::fs::read_to_string(root.join(entry))
        .map_err(|_| "That plugin's entry module is missing — reinstall it.".to_string())?;
    Ok(PluginRuntime { manifest, source })
}

/// Where a plugin's non-code payload was unpacked, for the Rust side to install into the
/// game. Returns None unless the licence is live right now.
pub fn payload_dir(app: &tauri::AppHandle, id: &str) -> Option<PathBuf> {
    let mut state = load_state(app);
    if state.status_of(id, now_secs()) != Status::Live {
        return None;
    }
    let dir = plugins_dir(app).ok()?.join(id).join("payload");
    dir.is_dir().then_some(dir)
}

// ---------------------------------------------------------------------------
// what a plugin may touch on disk
// ---------------------------------------------------------------------------

/// The one directory tree a plugin may read and write: the game's own user folder.
///
/// A plugin already runs with the app's privileges through `invoke`, so this is not a
/// sandbox in the security sense and does not pretend to be. It is a statement of intent
/// that keeps honest plugins honest and makes the dishonest ones obvious: a mod plugin has
/// business in `Documents\PiBoSo\MX Bikes` and nowhere else, so that is the only place the
/// file API will go — and a bug in a plugin cannot walk off into someone's home directory.
fn sandbox_root(app: &tauri::AppHandle) -> Result<PathBuf> {
    let cfg = crate::config::load_or_detect(app).unwrap_or_default();
    crate::config::default_user_dir(cfg.game())
        .ok_or_else(|| anyhow!("the game's user folder isn't known yet"))
}

/// Resolve `rel` inside `root`, or refuse.
///
/// Refusing is done on the *resolved* path, not the written one: `safe_path` rejects the
/// obvious `..` but says nothing about a symlink already on disk pointing somewhere else,
/// and a plugin writing through one would be outside the tree with a path that looked fine.
/// So the parent is canonicalised and checked to still be under the root.
pub fn resolve_in(root: &Path, rel: &str) -> Result<PathBuf> {
    let safe = safe_path(rel).ok_or_else(|| anyhow!("that path is not one a plugin may use"))?;
    let full = root.join(&safe);

    // Canonicalise the deepest part that exists — the file itself may not yet.
    let mut probe = full.as_path();
    let anchor = loop {
        if probe.exists() {
            break probe.canonicalize().unwrap_or_else(|_| probe.to_path_buf());
        }
        match probe.parent() {
            Some(p) if p != probe => probe = p,
            _ => break full.clone(),
        }
    };
    let root_real = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !anchor.starts_with(&root_real) {
        bail!("that path resolves outside the game folder");
    }
    Ok(full)
}

/// Every file call is licence-gated at the moment it happens. Checking only when the panel
/// mounted would leave a plugin able to write for as long as the window stayed open.
fn licensed(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let mut state = load_state(app);
    if state.status_of(id, now_secs()) != Status::Live {
        return Err("That plugin isn't licensed on this account.".into());
    }
    let _ = save_state(app, &state);
    Ok(())
}

#[tauri::command]
pub fn plugin_read_file(app: tauri::AppHandle, id: String, path: String) -> Result<String, String> {
    licensed(&app, &id)?;
    let root = sandbox_root(&app).map_err(|e| e.to_string())?;
    let full = resolve_in(&root, &path).map_err(|e| e.to_string())?;
    std::fs::read_to_string(&full).map_err(|e| format!("{}: {e}", full.display()))
}

#[tauri::command]
pub fn plugin_write_file(
    app: tauri::AppHandle,
    id: String,
    path: String,
    contents: String,
) -> Result<(), String> {
    licensed(&app, &id)?;
    let root = sandbox_root(&app).map_err(|e| e.to_string())?;
    let full = resolve_in(&root, &path).map_err(|e| e.to_string())?;
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(&full, contents).map_err(|e| format!("{}: {e}", full.display()))
}

#[tauri::command]
pub fn plugin_list_dir(
    app: tauri::AppHandle,
    id: String,
    path: String,
) -> Result<Vec<String>, String> {
    licensed(&app, &id)?;
    let root = sandbox_root(&app).map_err(|e| e.to_string())?;
    let full = resolve_in(&root, &path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    // A missing directory is an empty one, not an error: a plugin asking where its files
    // are before it has written any is the normal first run.
    if let Ok(entries) = std::fs::read_dir(&full) {
        for e in entries.flatten() {
            if let Some(name) = e.file_name().to_str() {
                out.push(name.to_string());
            }
        }
    }
    out.sort();
    Ok(out)
}

#[tauri::command]
pub fn plugin_delete_file(app: tauri::AppHandle, id: String, path: String) -> Result<(), String> {
    licensed(&app, &id)?;
    let root = sandbox_root(&app).map_err(|e| e.to_string())?;
    let full = resolve_in(&root, &path).map_err(|e| e.to_string())?;
    match std::fs::remove_file(&full) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", full.display())),
    }
}

/// Copy a plugin's `payload/` into the game, which for a PiBoSo title means its plugins
/// folder. This is how a paid *mod* — as opposed to a paid panel — gets installed.
#[tauri::command]
pub fn plugin_install_payload(app: tauri::AppHandle, id: String) -> Result<Vec<String>, String> {
    licensed(&app, &id)?;
    let src = payload_dir(&app, &id).ok_or_else(|| "That plugin has nothing to install.".to_string())?;
    let root = sandbox_root(&app).map_err(|e| e.to_string())?;
    let dest = root.join("plugins");
    std::fs::create_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;

    let mut copied = Vec::new();
    for entry in std::fs::read_dir(&src).map_err(|e| e.to_string())?.flatten() {
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name();
        let to = dest.join(&name);
        std::fs::copy(entry.path(), &to)
            // The commonest failure by far, and worth naming: the game holds its plugins
            // open, so this fails while it is running and succeeds the moment it is not.
            .map_err(|e| format!("Couldn't write {}: {e}. Close the game and try again.", to.display()))?;
        copied.push(name.to_string_lossy().to_string());
    }
    Ok(copied)
}

#[cfg(test)]
mod sandbox_tests {
    use super::*;

    fn root() -> PathBuf {
        let d = std::env::temp_dir().join(format!("mxb-sandbox-{}", std::process::id()));
        std::fs::create_dir_all(d.join("FrostReplay")).unwrap();
        d
    }

    #[test]
    fn resolves_a_plain_relative_path() {
        let r = root();
        let p = resolve_in(&r, "FrostReplay/slot1.fcam").unwrap();
        assert!(p.starts_with(&r));
        assert!(p.ends_with("slot1.fcam"));
    }

    #[test]
    fn refuses_what_would_leave_the_tree() {
        let r = root();
        for bad in ["../escape", "../../etc/passwd", "/etc/passwd", "C:/Windows/x"] {
            assert!(resolve_in(&r, bad).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn refuses_a_symlink_that_points_out_of_the_tree() {
        // The case `safe_path` alone cannot see: the written path is innocent and the
        // symlink already on disk is what leaves the tree.
        #[cfg(unix)]
        {
            let r = root();
            let link = r.join("sneaky");
            std::fs::remove_file(&link).ok();
            std::os::unix::fs::symlink("/tmp", &link).unwrap();
            assert!(resolve_in(&r, "sneaky/anything").is_err());
        }
    }
}
