//! Reading the buyer's Steam ID off the machine, offline.
//!
//! The secure-content key is sealed to a Steam ID at provision time and unsealed at play time
//! from the *live* identity — so this has to answer "who is logged into Steam right now" with
//! no network, and it must be the currently-signed-in account, not a cached one, or the
//! binding a copier can't defeat becomes one they can.
//!
//! Steam records this in `config/loginusers.vdf` in its install directory: a small text file
//! whose top-level keys are the SteamID64s that have signed in, one flagged `MostRecent 1`.
//! That is the value we bind to. No `steam_api.dll`, no running game — just the file Steam
//! keeps.

use std::path::PathBuf;

/// Override the `loginusers.vdf` location. For tests and for a non-standard install; a real
/// machine is found by [`steam_config_dir`].
const LOGINUSERS_ENV: &str = "MXB_STEAM_LOGINUSERS";

/// The SteamID64 of the account currently signed into Steam, or `None` if it can't be read.
///
/// `None` is not an error to hide: without it, secured content simply can't be provisioned or
/// opened, and the UI says so rather than sealing to nothing.
pub fn current_steam_id64() -> Option<String> {
    let path = if let Ok(p) = std::env::var(LOGINUSERS_ENV) {
        PathBuf::from(p)
    } else {
        steam_config_dir()?.join("loginusers.vdf")
    };
    let text = std::fs::read_to_string(&path).ok()?;
    most_recent_steam_id(&text)
}

/// The `config/` directory of the Steam install, per platform.
fn steam_config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(p) = windows_steam_path() {
            return Some(p.join("config"));
        }
        // Fall back to the usual install locations when the registry can't be read.
        for base in ["C:\\Program Files (x86)\\Steam", "C:\\Program Files\\Steam"] {
            let dir = PathBuf::from(base).join("config");
            if dir.join("loginusers.vdf").exists() {
                return Some(dir);
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").ok()?;
        Some(PathBuf::from(home).join("Library/Application Support/Steam/config"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let home = std::env::var("HOME").ok()?;
        for rel in ["/.local/share/Steam/config", "/.steam/steam/config"] {
            let dir = PathBuf::from(format!("{home}{rel}"));
            if dir.join("loginusers.vdf").exists() {
                return Some(dir);
            }
        }
        None
    }
}

/// Steam's install path from the registry (`HKCU\Software\Valve\Steam\SteamPath`).
#[cfg(windows)]
fn windows_steam_path() -> Option<PathBuf> {
    use std::os::raw::c_void;

    const HKEY_CURRENT_USER: isize = -2147483647; // 0x80000001
    const RRF_RT_REG_SZ: u32 = 0x0000_0002;

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegGetValueW(
            hkey: isize,
            subkey: *const u16,
            value: *const u16,
            flags: u32,
            typ: *mut u32,
            data: *mut c_void,
            data_len: *mut u32,
        ) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let subkey = wide("Software\\Valve\\Steam");
    let value = wide("SteamPath");
    let mut buf = [0u16; 512];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: a read-only registry query into a fixed stack buffer, length passed in bytes.
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut len,
        )
    };
    if rc != 0 {
        return None;
    }
    let chars = (len as usize / 2).saturating_sub(1); // drop the trailing NUL
    let s = String::from_utf16_lossy(&buf[..chars]);
    (!s.is_empty()).then(|| PathBuf::from(s))
}

/// The SteamID64 flagged `MostRecent 1` in a `loginusers.vdf`, or the first account if none is
/// flagged. Parsed by hand — the file is a tiny nested-quotes format and a VDF crate would be
/// far more than it is worth.
fn most_recent_steam_id(text: &str) -> Option<String> {
    let mut current: Option<String> = None;
    let mut first: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        // A top-level account block opens with its SteamID64 in quotes: `"7656119..."`.
        if let Some(id) = quoted_only(line).filter(|s| is_steam_id64(s)) {
            current = Some(id.to_string());
            if first.is_none() {
                first = Some(id.to_string());
            }
        } else if is_most_recent(line) {
            if let Some(id) = &current {
                return Some(id.clone());
            }
        }
    }
    first
}

/// A line that is exactly one quoted token (an account-block key), e.g. `"76561198000000001"`.
fn quoted_only(line: &str) -> Option<&str> {
    let inner = line.strip_prefix('"')?.strip_suffix('"')?;
    (!inner.contains('"')).then_some(inner)
}

/// `"MostRecent"  "1"` in any spacing.
fn is_most_recent(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.starts_with("\"mostrecent\"") && l.trim_end().ends_with("\"1\"")
}

/// 17 digits — a SteamID64.
fn is_steam_id64(s: &str) -> bool {
    s.len() == 17 && s.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
"users"
{
    "76561198000000001"
    {
        "AccountName"  "alice"
        "MostRecent"   "0"
    }
    "76561198000000002"
    {
        "AccountName"  "bob"
        "MostRecent"   "1"
    }
}
"#;

    #[test]
    fn picks_the_most_recent_account() {
        assert_eq!(most_recent_steam_id(SAMPLE).as_deref(), Some("76561198000000002"));
    }

    #[test]
    fn falls_back_to_the_first_when_none_is_flagged() {
        let none_flagged = SAMPLE.replace("\"MostRecent\"   \"1\"", "\"MostRecent\"   \"0\"");
        assert_eq!(most_recent_steam_id(&none_flagged).as_deref(), Some("76561198000000001"));
    }

    #[test]
    fn no_users_yields_none() {
        assert_eq!(most_recent_steam_id("\"users\"\n{\n}\n"), None);
    }

    #[test]
    fn a_non_id_key_is_not_taken_for_an_account() {
        // A stray quoted value that isn't a SteamID64 must not be read as an account id.
        assert_eq!(most_recent_steam_id("\"AccountName\" \"alice\"\n"), None);
    }
}
