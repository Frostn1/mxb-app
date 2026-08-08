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
//! secrecy. `strings` recovers it in seconds. It is safe only because it is read-only, scoped
//! to the public catalog, and rotatable by the store — it must never be a credential that can
//! act on an account.
//!
//! A build without the credential is a supported build: [`present`] returns false, the app
//! hides its Shop tab, and everything else works. That's what forks and CI-without-secrets
//! produce.

use reqwest::RequestBuilder;

/// The header's name, e.g. `X-Frostmod-Key`.
pub const HEADER: Option<&str> = option_env!("MXB_SHOP_API_HEADER");
/// The header's value.
pub const KEY: Option<&str> = option_env!("MXB_SHOP_API_KEY");

/// Whether this build can talk to the catalog at all.
pub fn present() -> bool {
    matches!((HEADER, KEY), (Some(h), Some(k)) if !h.trim().is_empty() && !k.trim().is_empty())
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
    let (Some(header), Some(key)) = (HEADER, KEY) else {
        return req;
    };
    let (header, key) = (header.trim(), key.trim());
    if header.is_empty() || key.is_empty() {
        return req;
    }
    match SCHEME {
        AuthScheme::NamedHeader => req.header(header, key),
        AuthScheme::Basic => req.basic_auth(header, Some(key)),
        AuthScheme::Bearer => req.bearer_auth(key),
        AuthScheme::Query => req.query(&[(header, key)]),
    }
}
