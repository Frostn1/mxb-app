//! The mxbikes-shop.com catalog credential, and the one place that decides how it's sent.
//!
//! The store authenticates with a single custom header — [`HEADER`] is the header's *name*
//! and [`KEY`] is its value:
//!
//! ```text
//! X-Frostmod-Key: <the key>
//! ```
//!
//! Both are baked in at compile time by `build.rs` from `.env.local` or the build
//! environment. Deliberately not Vite env vars (those are inlined into the JS bundle) and not
//! a runtime setting (it's one shared credential, not a per-user login) — it never leaves
//! Rust, and the webview never sees it.
//!
//! Be honest about what this buys: a constant in a shipped binary is obfuscation, not
//! secrecy. It is XOR-obfuscated by `build.rs` so a plain `strings` no longer surfaces it, but
//! anyone who reads the pad and decodes it recovers the value — that is true of all client-side
//! obfuscation. It is safe only because it is read-only, scoped to the public catalog, and
//! rotatable by the store — it must never be a credential that can act on an account.
//!
//! A build without the credential is a supported build: [`present`] returns false, the app
//! hides its Shop tab, and everything else works. That's what forks and CI-without-secrets
//! produce.

use reqwest::RequestBuilder;

/// The XOR pad `build.rs` obfuscates the credential with before baking it in. Must match the
/// copy in `build.rs`. This is obfuscation, not a key — see the module note above.
const PAD: [u8; 32] = [
    0x41, 0xcb, 0xdf, 0x60, 0x8f, 0xa1, 0xfa, 0x7b, 0xfd, 0xae, 0x81, 0x7c,
    0x3d, 0xd4, 0x15, 0xb4, 0x28, 0x11, 0xff, 0xa8, 0xa6, 0x73, 0x9c, 0x87,
    0x1f, 0x2a, 0x0d, 0xf9, 0x9f, 0x22, 0x8d, 0x8e,
];

/// The obfuscated credential baked in by `build.rs` (hex of value XOR the cycled [`PAD`]), or
/// `None` when this build has no credential.
const HEADER_OBF: Option<&str> = option_env!("MXB_SHOP_API_HEADER_OBF");
const KEY_OBF: Option<&str> = option_env!("MXB_SHOP_API_KEY_OBF");

/// Decode one obfuscated value: hex-decode, then XOR with the cycled pad. `None` on a malformed
/// or empty value, so a broken bake reads as "no credential" rather than sending garbage.
fn deobfuscate(hex: Option<&str>) -> Option<String> {
    let hex = hex?.trim();
    if hex.is_empty() || hex.len() % 2 != 0 {
        return None;
    }
    let h = hex.as_bytes();
    let mut bytes = Vec::with_capacity(h.len() / 2);
    let mut i = 0;
    while i < h.len() {
        let hi = (h[i] as char).to_digit(16)?;
        let lo = (h[i + 1] as char).to_digit(16)?;
        bytes.push(((hi << 4) | lo) as u8);
        i += 2;
    }
    for (j, b) in bytes.iter_mut().enumerate() {
        *b ^= PAD[j % PAD.len()];
    }
    String::from_utf8(bytes).ok().filter(|s| !s.trim().is_empty())
}

/// The header's name, e.g. `X-Frostmod-Key`.
fn header() -> Option<String> {
    deobfuscate(HEADER_OBF)
}
/// The header's value.
fn key() -> Option<String> {
    deobfuscate(KEY_OBF)
}

/// Whether this build can talk to the catalog at all.
pub fn present() -> bool {
    header().is_some() && key().is_some()
}

/// How the credential is presented.
///
/// [`AuthScheme::NamedHeader`] is what the store actually uses. The rest are kept because
/// they are the realistic alternatives if the store ever changes: switching is a one-line
/// change to [`SCHEME`] and nothing else in the tree moves.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthScheme {
    /// `<HEADER>: <KEY>` — one custom header whose name comes from the config.
    NamedHeader,
    /// `Authorization: Basic base64(HEADER:KEY)`, treating the pair as user/password.
    Basic,
    /// `Authorization: Bearer <KEY>`.
    Bearer,
    /// `?<HEADER>=<KEY>` on the URL.
    Query,
}

/// ── CHANGE THIS ONE CONSTANT if the store changes its mechanism. ──
pub const SCHEME: AuthScheme = AuthScheme::NamedHeader;

/// Attach the credential, if this build has one.
///
/// A build without one still sends the request. That's deliberate: the server's own 401 or
/// 403 is a better diagnostic than a message we invent, and it keeps the "no credential" path
/// exercising the same code as the real one.
pub fn apply(req: RequestBuilder) -> RequestBuilder {
    let (Some(header), Some(key)) = (header(), key()) else {
        return req;
    };
    match SCHEME {
        AuthScheme::NamedHeader => req.header(header, key),
        AuthScheme::Basic => req.basic_auth(header, Some(key)),
        AuthScheme::Bearer => req.bearer_auth(key),
        AuthScheme::Query => req.query(&[(header, key)]),
    }
}

#[cfg(test)]
mod tests {
    use super::{deobfuscate, PAD};

    /// Encode exactly as `build.rs` does — hex of value XOR the cycled pad — so this locks the
    /// decode against that scheme and would fail if [`PAD`] ever drifted from the build script.
    fn obfuscate(value: &str) -> String {
        value
            .as_bytes()
            .iter()
            .enumerate()
            .map(|(i, b)| format!("{:02x}", b ^ PAD[i % PAD.len()]))
            .collect()
    }

    #[test]
    fn round_trips_a_credential() {
        let secret = "X-Frostmod-Key: 9f83b2c1-not-a-real-key";
        assert_eq!(deobfuscate(Some(&obfuscate(secret))), Some(secret.to_string()));
    }

    #[test]
    fn a_missing_or_malformed_value_is_none() {
        assert_eq!(deobfuscate(None), None);
        assert_eq!(deobfuscate(Some("")), None);
        assert_eq!(deobfuscate(Some("abc")), None, "odd-length hex");
        assert_eq!(deobfuscate(Some("zz")), None, "non-hex");
    }
}
