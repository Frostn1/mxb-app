//! Which machine this is, as a one-way hash — so a ban follows the PC and not only the account.
//!
//! The control plane already follows a ban through the GUID, the Steam login and every account
//! either one touches. A fresh token or a fresh Steam account on the same banned PC was the gap,
//! and this is the client half of closing it: every app in the lineup reports [`device_hash`] in
//! the `X-MXB-Device` header on the startup gate (`appgate.rs`) and when it mints a device account
//! (`account.rs`), and the server ties the account to it (`control-plane/src/devices.ts`).
//!
//! ## What leaves the machine
//!
//! Never the identifier. The OS's own machine id — `MachineGuid` on Windows, `IOPlatformUUID` on
//! macOS, `/etc/machine-id` on Linux — is hashed here with a domain tag, SHA-256 of
//! `"mxb-device-link/v1\0" ‖ id`, and only that digest is sent. The tag keeps the digest from
//! matching any other product's hash of the same id. The server does not store it either: it keys
//! it again with its own secret before writing it down, so what is kept is useless without that
//! secret. Account erasure deletes it.
//!
//! No machine id — an unreadable registry, a sandbox without `/etc/machine-id` — means no header,
//! never an error: the gate answers exactly as it did before this existed.

use std::sync::OnceLock;

use sha2::{Digest, Sha256};

/// The header the gate and the account mint read the hash from.
pub const DEVICE_HEADER: &str = "X-MXB-Device";

/// Separates this use of the machine id from every other hash of it. Versioned, so a change of
/// construction is a new tag rather than a silent mismatch.
const DOMAIN: &[u8] = b"mxb-device-link/v1\0";

/// SHA-256 of the domain tag and a machine id, lowercase hex. The id is trimmed and lower-cased
/// first, so the same machine hashes the same however the OS happened to spell it. `None` for an
/// empty id, which identifies nothing and must not collapse every such machine into one.
pub fn hash_machine_id(machine_id: &str) -> Option<String> {
    let id = machine_id.trim().to_ascii_lowercase();
    if id.is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN);
    hasher.update(id.as_bytes());
    Some(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// This machine's hash, read once per process. `None` when the OS will not say.
///
/// The first call reads the registry, a file, or (on macOS) asks `ioreg` once — a few
/// milliseconds, then cached, so it is cheap enough to call from the async paths that send it.
pub fn device_hash() -> Option<&'static str> {
    static CACHE: OnceLock<Option<String>> = OnceLock::new();
    CACHE.get_or_init(|| machine_id().as_deref().and_then(hash_machine_id)).as_deref()
}

/// A request with the device header added, when there is a hash to add.
pub fn with_device(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    match device_hash() {
        Some(hash) => request.header(DEVICE_HEADER, hash),
        None => request,
    }
}

/// The OS's machine identifier. Read here and hashed immediately; never logged, stored or sent.
#[cfg(windows)]
fn machine_id() -> Option<String> {
    windows_registry::LOCAL_MACHINE
        .open(r"SOFTWARE\Microsoft\Cryptography")
        .and_then(|key| key.get_string("MachineGuid"))
        .ok()
        .filter(|id| !id.trim().is_empty())
}

#[cfg(target_os = "macos")]
fn machine_id() -> Option<String> {
    let out = std::process::Command::new("/usr/sbin/ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()?;
    parse_ioreg(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn machine_id() -> Option<String> {
    ["/etc/machine-id", "/var/lib/dbus/machine-id"]
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|id| id.trim().to_string())
        .find(|id| !id.is_empty())
}

#[cfg(not(any(windows, unix)))]
fn machine_id() -> Option<String> {
    None
}

/// The `IOPlatformUUID` value out of `ioreg -rd1 -c IOPlatformExpertDevice`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_ioreg(out: &str) -> Option<String> {
    out.lines().find(|l| l.contains("\"IOPlatformUUID\"")).and_then(|line| {
        let value = line.split('=').nth(1)?.trim().trim_matches('"').trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic machine id: MachineGuid-shaped, identifies nothing.
    const MACHINE: &str = "00000000-0000-4000-8000-000000000001";

    #[test]
    fn matches_the_control_planes_vector() {
        // The same constant is asserted in `control-plane/test/devices.test.ts`.
        assert_eq!(
            hash_machine_id(MACHINE).as_deref(),
            Some("e0c8264c0b6227cb35e3a45b7e8388beda6076845e3194d9ca9bbe11a03ab9d5")
        );
    }

    #[test]
    fn is_deterministic_and_ignores_how_the_os_spelled_it() {
        let a = hash_machine_id(MACHINE).unwrap();
        assert_eq!(hash_machine_id(MACHINE).unwrap(), a);
        assert_eq!(hash_machine_id(&format!("  {}\n", MACHINE.to_ascii_uppercase())).unwrap(), a);
        assert_ne!(hash_machine_id("00000000-0000-4000-8000-000000000002").unwrap(), a);
    }

    #[test]
    fn is_domain_separated_and_never_the_raw_id() {
        let hash = hash_machine_id(MACHINE).unwrap();
        // Not the id, and not a bare SHA-256 of it: the tag is what keeps this from matching any
        // other product's hash of the same machine.
        assert!(!hash.contains(MACHINE));
        let bare: String = Sha256::digest(MACHINE.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
        assert_ne!(hash, bare);
        assert_eq!(hash.len(), 64);
        assert!(hash.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
    }

    #[test]
    fn an_empty_id_is_no_device() {
        assert_eq!(hash_machine_id(""), None);
        assert_eq!(hash_machine_id("   \n"), None);
    }

    #[test]
    fn reads_the_platform_uuid_out_of_ioreg() {
        let out = "+-o J314sAP  <class IOPlatformExpertDevice>\n    {\n      \"IOPlatformSerialNumber\" = \"X\"\n      \"IOPlatformUUID\" = \"00000000-0000-4000-8000-000000000001\"\n    }\n";
        assert_eq!(parse_ioreg(out).as_deref(), Some(MACHINE));
        assert_eq!(parse_ioreg("nothing here"), None);
    }
}
