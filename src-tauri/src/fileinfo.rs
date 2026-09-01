//! What a file says about itself: who signed it, and who says they wrote it.
//!
//! Read for the modules loaded inside the running game — see [`crate::procmods`] — because a
//! file name and a hash only ever identify something we already know about. The first
//! sighting of anything is a name nobody recognises, and these two answers are what make it
//! readable without recognising it: an overlay that belongs in a game process is signed by a
//! company whose name you know, and a file that is neither is worth a look.
//!
//! **This module makes no judgements.** It reads two facts off a file and hands them over.
//! What either one means is decided by the control plane, the same as everything else here.
//!
//! Windows-only in substance: both answers come from Windows' own APIs, and there is nothing
//! equivalent to read under Wine (where the game's own libraries are the host's, and the
//! trust store is empty). Everywhere else these return "not checked", which is deliberately
//! not the same answer as "unsigned".

use serde::Serialize;
use std::path::Path;

/// The most any one string off a file is worth carrying. Long enough for the longest real
/// publisher name; short enough that a file which lies about itself can't fill a column.
const MAX_FIELD: usize = 96;

/// What Windows makes of a file's signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Trust {
    /// Not looked at: a system library, a platform with nothing to ask, or a file that was
    /// gone by the time we asked. Never folded into `unsigned` — "we didn't look" and "there
    /// is no signature" are different facts and only one of them is interesting.
    Unchecked,
    /// No signature at all.
    Unsigned,
    /// Signed, and the chain verifies.
    Signed,
    /// There is a signature and it does not verify — expired, revoked, self-signed, or the
    /// file has been modified since. Its own answer, because it is the rarest and the loudest.
    Untrusted,
}

/// What a file says about itself. Every string is bounded and may be empty.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Details {
    /// Who signed it, as the certificate's display name. Empty unless [`Trust::Signed`].
    #[serde(skip_serializing_if = "String::is_empty")]
    pub publisher: String,
    /// `CompanyName` from the version resource — a claim the file makes, not a checked one.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub company: String,
    /// `ProductName`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub product: String,
    /// `FileDescription`, which is the field Explorer shows and the one most often filled in.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// Everything read off one file in a single pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileFacts {
    pub trust: Trust,
    #[serde(flatten)]
    pub details: Details,
}

impl Default for FileFacts {
    fn default() -> Self {
        Self { trust: Trust::Unchecked, details: Details::default() }
    }
}

/// Read a file's signature and its version resource.
///
/// One entry point, so the caller does not have to know which half is which — and so the
/// platforms with no answer have one place to say so.
pub fn read(path: &Path) -> FileFacts {
    #[cfg(windows)]
    {
        let (trust, publisher) = windows::verify(path);
        let mut details = windows::version_strings(path);
        details.publisher = clamp(&publisher);
        FileFacts { trust, details }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        FileFacts::default()
    }
}

/// Trim a string off a file to something a column can hold, and drop anything with control
/// characters in it — a version resource is attacker-controlled text.
#[cfg_attr(not(windows), allow(dead_code))] // used on Windows and by the tests, which run anywhere
pub fn clamp(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_FIELD)
        .collect();
    cleaned.trim().to_string()
}

#[cfg(windows)]
mod windows {
    use super::{clamp, Details, Trust};
    use std::os::raw::c_void;
    use std::path::Path;

    type Handle = *mut c_void;

    #[repr(C)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    /// `WINTRUST_ACTION_GENERIC_VERIFY_V2` — the policy that answers "is this file signed by
    /// someone this machine trusts", which is the same question Explorer's Digital
    /// Signatures tab asks.
    const ACTION_GENERIC_VERIFY_V2: Guid = Guid {
        data1: 0x00AA_C56B,
        data2: 0xCD44,
        data3: 0x11D0,
        data4: [0x8C, 0xC2, 0x00, 0xC0, 0x4F, 0xC2, 0x95, 0xEE],
    };

    #[repr(C)]
    struct WintrustFileInfo {
        cb_struct: u32,
        file_path: *const u16,
        file_handle: Handle,
        known_subject: *const Guid,
    }

    /// Only the head of `WINTRUST_DATA` is ever written by us; the tail is Windows' own
    /// working space and is zeroed. Laid out in full because the API reads `cbStruct` and
    /// writes into `hWVTStateData`, and a short struct would have it writing past the end.
    #[repr(C)]
    struct WintrustData {
        cb_struct: u32,
        policy_callback_data: *mut c_void,
        sip_client_data: *mut c_void,
        ui_choice: u32,
        revocation_checks: u32,
        union_choice: u32,
        // The union: only the file-info arm is ever used.
        file_info: *mut WintrustFileInfo,
        state_action: u32,
        state_data: Handle,
        url_reference: *const u16,
        prov_flags: u32,
        ui_context: u32,
        signature_settings: *mut c_void,
    }

    const WTD_UI_NONE: u32 = 2;
    const WTD_REVOKE_NONE: u32 = 0;
    const WTD_CHOICE_FILE: u32 = 1;
    const WTD_STATEACTION_VERIFY: u32 = 1;
    const WTD_STATEACTION_CLOSE: u32 = 2;
    /// Answer from the local store only. Without it a machine with no route out sits on a
    /// revocation lookup, and this runs beside a game.
    const WTD_CACHE_ONLY_URL_RETRIEVAL: u32 = 0x1000;
    const WTD_SAFER_FLAG: u32 = 0x100;

    /// `TRUST_E_NOSIGNATURE`, and the two errors a file with no signature can come back as
    /// when the subject interface package can't find one to parse.
    const TRUST_E_NOSIGNATURE: i32 = -2146762496; // 0x800B0100
    const TRUST_E_SUBJECT_FORM_UNKNOWN: i32 = -2146762477; // 0x800B0113
    const TRUST_E_PROVIDER_UNKNOWN: i32 = -2146762751; // 0x800B0001

    #[link(name = "wintrust")]
    unsafe extern "system" {
        fn WinVerifyTrust(hwnd: isize, action: *const Guid, data: *mut WintrustData) -> i32;
    }

    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptQueryObject(
            object_type: u32,
            object: *const c_void,
            expected_content: u32,
            expected_format: u32,
            flags: u32,
            msg_and_cert_encoding: *mut u32,
            content_type: *mut u32,
            format_type: *mut u32,
            cert_store: *mut Handle,
            msg: *mut Handle,
            context: *mut *const c_void,
        ) -> i32;
        fn CryptMsgGetParam(
            msg: Handle,
            param_type: u32,
            index: u32,
            data: *mut c_void,
            data_len: *mut u32,
        ) -> i32;
        fn CryptMsgClose(msg: Handle) -> i32;
        fn CertFindCertificateInStore(
            store: Handle,
            encoding: u32,
            find_flags: u32,
            find_type: u32,
            find_para: *const c_void,
            prev: *const c_void,
        ) -> *const c_void;
        fn CertGetNameStringW(
            cert: *const c_void,
            name_type: u32,
            flags: u32,
            type_para: *mut c_void,
            name: *mut u16,
            name_len: u32,
        ) -> u32;
        fn CertFreeCertificateContext(cert: *const c_void) -> i32;
        fn CertCloseStore(store: Handle, flags: u32) -> i32;
    }

    #[link(name = "version")]
    unsafe extern "system" {
        fn GetFileVersionInfoSizeW(file: *const u16, handle: *mut u32) -> u32;
        fn GetFileVersionInfoW(
            file: *const u16,
            handle: u32,
            len: u32,
            data: *mut c_void,
        ) -> i32;
        fn VerQueryValueW(
            block: *const c_void,
            sub_block: *const u16,
            buffer: *mut *mut c_void,
            len: *mut u32,
        ) -> i32;
    }

    const CERT_QUERY_OBJECT_FILE: u32 = 1;
    const CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED: u32 = 1 << 10;
    const CERT_QUERY_FORMAT_FLAG_BINARY: u32 = 1 << 1;
    const CMSG_SIGNER_CERT_INFO_PARAM: u32 = 7;
    const CERT_FIND_SUBJECT_CERT: u32 = 0x000B_0000;
    const CERT_NAME_SIMPLE_DISPLAY_TYPE: u32 = 4;
    const X509_ASN_ENCODING: u32 = 1;
    const PKCS_7_ASN_ENCODING: u32 = 0x0001_0000;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn wide_path(path: &Path) -> Vec<u16> {
        wide(&path.to_string_lossy())
    }

    /// Ask Windows whether the file is signed, and by whom.
    ///
    /// The publisher is only read when the chain verified: an unverified signature can carry
    /// any name at all, and reporting "signed by Microsoft" for a file that failed its own
    /// check would be worse than reporting nothing.
    pub fn verify(path: &Path) -> (Trust, String) {
        let wide = wide_path(path);
        let mut file = WintrustFileInfo {
            cb_struct: std::mem::size_of::<WintrustFileInfo>() as u32,
            file_path: wide.as_ptr(),
            file_handle: std::ptr::null_mut(),
            known_subject: std::ptr::null(),
        };
        let mut data = WintrustData {
            cb_struct: std::mem::size_of::<WintrustData>() as u32,
            policy_callback_data: std::ptr::null_mut(),
            sip_client_data: std::ptr::null_mut(),
            ui_choice: WTD_UI_NONE,
            revocation_checks: WTD_REVOKE_NONE,
            union_choice: WTD_CHOICE_FILE,
            file_info: &mut file,
            state_action: WTD_STATEACTION_VERIFY,
            state_data: std::ptr::null_mut(),
            url_reference: std::ptr::null(),
            prov_flags: WTD_CACHE_ONLY_URL_RETRIEVAL | WTD_SAFER_FLAG,
            ui_context: 0,
            signature_settings: std::ptr::null_mut(),
        };

        // SAFETY: both structs outlive the call, `cbStruct` is each one's real size, and the
        // path buffer is NUL-terminated and lives across it.
        let rc = unsafe { WinVerifyTrust(0, &ACTION_GENERIC_VERIFY_V2, &mut data) };

        // Every verify has to be closed, whatever it answered, or the state handle leaks —
        // and this runs on a timer for the life of a session.
        data.state_action = WTD_STATEACTION_CLOSE;
        // SAFETY: the same struct, handed back exactly as the API requires.
        unsafe { WinVerifyTrust(0, &ACTION_GENERIC_VERIFY_V2, &mut data) };

        match rc {
            0 => (Trust::Signed, publisher(path)),
            TRUST_E_NOSIGNATURE | TRUST_E_SUBJECT_FORM_UNKNOWN | TRUST_E_PROVIDER_UNKNOWN => {
                (Trust::Unsigned, String::new())
            }
            _ => (Trust::Untrusted, String::new()),
        }
    }

    /// The display name on the certificate that signed the file.
    ///
    /// Read through the message rather than through the verify call's state data: the signer
    /// certificate is fetched by its own issuer and serial, which needs no knowledge of
    /// `CRYPT_PROVIDER_DATA`'s layout — a struct that has grown between Windows versions and
    /// would be the one thing here that could be wrong on a machine we can't test on.
    fn publisher(path: &Path) -> String {
        let wide = wide_path(path);
        let mut store: Handle = std::ptr::null_mut();
        let mut msg: Handle = std::ptr::null_mut();
        let mut encoding: u32 = 0;
        let mut content: u32 = 0;
        let mut format: u32 = 0;

        // SAFETY: an out-parameter query against a NUL-terminated path; every handle it
        // hands back is closed below.
        let ok = unsafe {
            CryptQueryObject(
                CERT_QUERY_OBJECT_FILE,
                wide.as_ptr() as *const c_void,
                CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
                CERT_QUERY_FORMAT_FLAG_BINARY,
                0,
                &mut encoding,
                &mut content,
                &mut format,
                &mut store,
                &mut msg,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return String::new();
        }

        let name = signer_name(store, msg, encoding);

        // SAFETY: both handles came from the call above and are closed exactly once.
        unsafe {
            if !msg.is_null() {
                CryptMsgClose(msg);
            }
            if !store.is_null() {
                CertCloseStore(store, 0);
            }
        }
        name
    }

    fn signer_name(store: Handle, msg: Handle, encoding: u32) -> String {
        if msg.is_null() || store.is_null() {
            return String::new();
        }
        // The signer's issuer and serial, as an opaque blob: it is handed straight back to
        // `CertFindCertificateInStore`, which is why none of its fields need naming here.
        let mut len: u32 = 0;
        // SAFETY: a size query — no buffer is written when the data pointer is null.
        let sized = unsafe {
            CryptMsgGetParam(
                msg,
                CMSG_SIGNER_CERT_INFO_PARAM,
                0,
                std::ptr::null_mut(),
                &mut len,
            )
        };
        if sized == 0 || len == 0 || len > 64 * 1024 {
            return String::new();
        }
        let mut info = vec![0u8; len as usize];
        // SAFETY: the buffer is the size the call above asked for.
        let got = unsafe {
            CryptMsgGetParam(
                msg,
                CMSG_SIGNER_CERT_INFO_PARAM,
                0,
                info.as_mut_ptr() as *mut c_void,
                &mut len,
            )
        };
        if got == 0 {
            return String::new();
        }

        let encoding = if encoding == 0 { X509_ASN_ENCODING | PKCS_7_ASN_ENCODING } else { encoding };
        // SAFETY: `info` holds the `CERT_INFO` the message just wrote and outlives the call.
        let cert = unsafe {
            CertFindCertificateInStore(
                store,
                encoding,
                0,
                CERT_FIND_SUBJECT_CERT,
                info.as_ptr() as *const c_void,
                std::ptr::null(),
            )
        };
        if cert.is_null() {
            return String::new();
        }

        let mut buf = [0u16; 256];
        // SAFETY: a fixed buffer whose length is passed in characters, as the API expects.
        let written = unsafe {
            CertGetNameStringW(
                cert,
                CERT_NAME_SIMPLE_DISPLAY_TYPE,
                0,
                std::ptr::null_mut(),
                buf.as_mut_ptr(),
                buf.len() as u32,
            )
        };
        // SAFETY: the context came from `CertFindCertificateInStore` and is freed once.
        unsafe { CertFreeCertificateContext(cert) };

        if written <= 1 {
            return String::new();
        }
        clamp(&String::from_utf16_lossy(&buf[..(written as usize) - 1]))
    }

    /// `CompanyName`, `ProductName` and `FileDescription` off the version resource.
    ///
    /// All three are whatever the file was built saying — a claim, not a fact — which is why
    /// they sit beside the signature rather than instead of it.
    pub fn version_strings(path: &Path) -> Details {
        let wide = wide_path(path);
        let mut handle: u32 = 0;
        // SAFETY: a size query against a NUL-terminated path.
        let size = unsafe { GetFileVersionInfoSizeW(wide.as_ptr(), &mut handle) };
        // A file with no version resource answers zero, which is most of them.
        if size == 0 || size > 1024 * 1024 {
            return Details::default();
        }
        let mut block = vec![0u8; size as usize];
        // SAFETY: the buffer is the size the call above asked for.
        let ok = unsafe {
            GetFileVersionInfoW(wide.as_ptr(), 0, size, block.as_mut_ptr() as *mut c_void)
        };
        if ok == 0 {
            return Details::default();
        }

        // The resource is keyed by language and codepage, and a file only has to carry one.
        // Ask which, rather than guessing at `040904b0` — the guess is right for most
        // English-language builds and wrong for exactly the files that aren't.
        let Some((lang, code)) = translation(&block) else { return Details::default() };
        let prefix = format!("\\StringFileInfo\\{lang:04x}{code:04x}\\");
        Details {
            publisher: String::new(),
            company: query(&block, &format!("{prefix}CompanyName")),
            product: query(&block, &format!("{prefix}ProductName")),
            description: query(&block, &format!("{prefix}FileDescription")),
        }
    }

    /// The first language/codepage pair the resource declares.
    fn translation(block: &[u8]) -> Option<(u16, u16)> {
        let key = wide("\\VarFileInfo\\Translation");
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut len: u32 = 0;
        // SAFETY: `block` outlives the call; the pointer handed back points inside it.
        let ok = unsafe {
            VerQueryValueW(block.as_ptr() as *const c_void, key.as_ptr(), &mut ptr, &mut len)
        };
        if ok == 0 || ptr.is_null() || len < 4 {
            return None;
        }
        // SAFETY: four bytes inside `block`, which is still alive — two little-endian u16s.
        let pair = unsafe { std::slice::from_raw_parts(ptr as *const u16, 2) };
        Some((pair[0], pair[1]))
    }

    fn query(block: &[u8], sub_block: &str) -> String {
        let key = wide(sub_block);
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut len: u32 = 0;
        // SAFETY: as `translation` — the result points inside `block`.
        let ok = unsafe {
            VerQueryValueW(block.as_ptr() as *const c_void, key.as_ptr(), &mut ptr, &mut len)
        };
        if ok == 0 || ptr.is_null() || len == 0 || len > 4096 {
            return String::new();
        }
        // `len` counts characters and includes the trailing NUL when there is one.
        // SAFETY: `len` characters inside `block`, which is still alive.
        let chars = unsafe { std::slice::from_raw_parts(ptr as *const u16, len as usize) };
        let text: String = String::from_utf16_lossy(chars);
        clamp(text.trim_end_matches('\0'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_off_a_file_is_trimmed_and_bounded() {
        assert_eq!(clamp("  NVIDIA Corporation  "), "NVIDIA Corporation");
        assert_eq!(clamp(&"x".repeat(200)).len(), MAX_FIELD);
    }

    /// A version resource is text a file supplies about itself, so it reaches the admin page
    /// the same way any other untrusted string does — with the characters that would break a
    /// line or a log entry taken out first.
    #[test]
    fn control_characters_never_survive_a_field() {
        assert_eq!(clamp("Over\u{0}watch\nInc\r"), "OverwatchInc");
        assert_eq!(clamp("\u{7}"), "");
    }

    /// Nothing to ask on this platform, and "we did not look" is its own answer — folding it
    /// into `unsigned` would report every file on a Mac as unsigned.
    #[cfg(not(windows))]
    #[test]
    fn a_platform_with_nothing_to_ask_says_so() {
        let facts = read(Path::new("/usr/lib/libSystem.B.dylib"));
        assert_eq!(facts.trust, Trust::Unchecked);
        assert_eq!(facts.details, Details::default());
    }
}
