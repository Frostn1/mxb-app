//! Share codes that keep working after the track changes.
//!
//! A `MXBS1-` code from [`crate::fileshare`] is the whole share written down: the item list
//! and the catbox URLs are both inside the string. That is what lets it work with no server
//! at all, and it is also why a track author who recompiles has to send a new code round.
//! Testing a track meant a new link in Discord after every build.
//!
//! A live share splits the code from what it points at. catbox still holds the bytes — the
//! packing and the upload here are literally [`crate::fileshare::pack`], unchanged — and the
//! control plane holds a permanent `MXBL1-` code, a version number and the manifest that
//! version resolves to. Publish an update and every subscriber's next check finds it.
//!
//! **No account, no sign-in, nothing to enroll.** The server mints an update key at first
//! publish and this module writes it into the config next to the code, where the player
//! never sees it. Publishing an update is one button. That key is the only thing standing
//! between the author and anyone else who was given the public code, which is why
//! [`owner_code`] exists: it is the one way to carry a code to another machine, and losing
//! the config without it means the code can be shared for ever and never updated again.
//!
//! Installing is [`crate::fileshare::import`], also unchanged — a manifest *is* a
//! `FileShare`, so it is re-encoded into a `MXBS1-` string and handed to the importer that
//! already knows how to fetch, unzip and place it. Nothing about downloading is new here.

use crate::config::{self, AppConfig, LiveSubscription, PublishedShare};
use crate::fileshare::{self, FileShare};
use crate::paintsync::control_plane;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

/// Marks a live code, the way `MXBS1-` marks a static one.
pub const CODE_PREFIX: &str = "MXBL1-";

/// Splits a code from its update key in an owner code. Not a character any code contains.
const OWNER_SEP: char = '#';

/// How long a check may take before it is not worth waiting for. A subscription check runs
/// on a timer nobody asked for, so it must never be the reason a click feels slow.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// What the control plane says about a code.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShareBody {
    code: String,
    name: String,
    version: u32,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    updated_at: u64,
    manifest: FileShare,
}

/// The answer to a publish: the code to hand out, and the key that is never handed out.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishBody {
    code: String,
    #[serde(default)]
    update_key: String,
    name: String,
    version: u32,
}

/// One row of the Library's live list, as the UI renders it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveShareInfo {
    pub code: String,
    pub name: String,
    /// The version installed here.
    pub version: u32,
    /// The newest version the server had at the last check.
    pub latest: u32,
    pub checked_at: u64,
    /// Bytes the newest version weighs.
    pub size: u64,
    /// Unix seconds of the author's last publish, as the server reports it.
    pub published_at: u64,
    pub auto: bool,
    /// True when this machine published the code, which is what decides whether the row
    /// offers "Publish update" or "Update available".
    pub mine: bool,
    /// How many files the code carries, for a published row. Zero for a subscription.
    pub items: usize,
    /// The mods-relative paths an owned code carries, so "publish an update" can repack
    /// exactly what was shared last time without the player picking the files again.
    /// Empty for a subscription — a follower has no business republishing.
    pub rels: Vec<String>,
}

/// Normalise a code the way it might be typed rather than the way it was printed.
///
/// Deliberately the same folding the server does: any case, with or without the prefix,
/// with the dashes and spaces someone added to make it readable, and with the letters
/// Crockford treats as digits (`O` is zero, `I` and `L` are one). A player reading a code
/// out of a Discord message should not have to be careful.
pub fn normalise(text: &str) -> Option<String> {
    let mut s = text.trim().to_ascii_uppercase();
    if let Some(rest) = s.strip_prefix(CODE_PREFIX) {
        s = rest.to_string();
    }
    let s: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| match c {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        })
        .collect();
    if s.len() != 8 || !s.chars().all(|c| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c)) {
        return None;
    }
    Some(s)
}

/// True when this text looks like a live code, so the import box can route it.
pub fn is_live_code(text: &str) -> bool {
    let t = text.trim();
    t.to_ascii_uppercase().starts_with(CODE_PREFIX) && normalise(t).is_some()
}

/// The code plus its update key, for carrying a share to another machine.
///
/// This is the whole backup story for a live share. It is not shown in the UI beside the
/// code, and it is not what "copy" copies — anyone holding this can repoint the share for
/// everybody following it.
pub fn owner_code(share: &PublishedShare) -> String {
    format!("{}{}{}", share.code, OWNER_SEP, share.update_key)
}

/// Read an owner code back. Returns the public code and the update key.
pub fn parse_owner_code(text: &str) -> Option<(String, String)> {
    let (code, key) = text.trim().split_once(OWNER_SEP)?;
    let code = normalise(code)?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some((code, key.to_string()))
}

fn client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .context("couldn't build an HTTP client")
}

/// Publish, or republish, the given picks under a live code.
///
/// `code` names an existing share to update; `None` mints a new one. Either way the packing
/// and the upload are the same ones a plain share code uses — this only changes what is
/// written down at the end.
pub async fn publish(
    app: &AppHandle,
    cfg: &AppConfig,
    paths: &[String],
    name: &str,
    code: Option<&str>,
) -> anyhow::Result<LiveShareInfo> {
    // Resolved before anything is packed: republishing under a code this machine does not
    // hold the key for cannot work, and finding that out after an 80 MB upload is the wrong
    // order to find it out in.
    let existing = match code {
        Some(raw) => {
            let code = normalise(raw).context("that isn't a live share code")?;
            let found = cfg
                .published_shares
                .iter()
                .find(|s| normalise(&s.code).as_deref() == Some(code.as_str()))
                .cloned()
                .context(
                    "this machine doesn't hold the update key for that code — \
                     paste its owner code first, or publish a new one",
                )?;
            Some(found)
        }
        None => None,
    };

    let share = fileshare::pack(app, cfg, paths).await?;
    let rels: Vec<String> = share.items.iter().map(|i| i.rel.clone()).collect();
    let name = display_name(name, &share);

    let http = client()?;
    let base = control_plane();
    let body = serde_json::json!({ "name": name, "manifest": share });

    let published: PublishBody = match &existing {
        Some(prev) => {
            let plain = normalise(&prev.code).unwrap_or_else(|| prev.code.clone());
            let resp = http
                .put(format!("{base}/v1/share/{plain}"))
                .header("x-update-key", &prev.update_key)
                .json(&body)
                .send()
                .await
                .context("couldn't reach the control plane to publish the update")?;
            read_json(resp).await?
        }
        None => {
            let resp = http
                .post(format!("{base}/v1/share"))
                .json(&body)
                .send()
                .await
                .context("couldn't reach the control plane to publish the share")?;
            read_json(resp).await?
        }
    };

    // The update key comes back once, at mint time, and is never served again. Writing it
    // before anything else can fail is the difference between a share the author owns and a
    // code nobody can ever update.
    let update_key = if published.update_key.is_empty() {
        existing.as_ref().map(|p| p.update_key.clone()).unwrap_or_default()
    } else {
        published.update_key.clone()
    };
    if update_key.is_empty() {
        anyhow::bail!("the control plane returned no update key for this share");
    }

    let row = PublishedShare {
        code: published.code.clone(),
        update_key,
        name: published.name.clone(),
        version: published.version,
        rels,
        published_at: now_ms(),
    };
    let mut next = cfg.clone();
    next.published_shares
        .retain(|s| normalise(&s.code) != normalise(&row.code));
    next.published_shares.push(row.clone());
    config::save(app, &next)?;

    log::info!("live share: published {} as v{}", row.code, row.version);
    Ok(LiveShareInfo {
        code: row.code,
        name: row.name,
        version: row.version,
        latest: row.version,
        checked_at: row.published_at,
        size: share.total_size,
        published_at: row.published_at / 1000,
        auto: false,
        mine: true,
        items: share.items.len(),
        rels: row.rels.clone(),
    })
}

/// Fetch what a code points at, without installing it.
async fn fetch(code: &str) -> anyhow::Result<ShareBody> {
    let http = client()?;
    let resp = http
        .get(format!("{}/v1/share/{code}", control_plane()))
        .send()
        .await
        .context("couldn't reach the control plane to read that share code")?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        anyhow::bail!("no share with that code — check it was typed correctly");
    }
    read_json(resp).await
}

/// What a code carries, without installing it — the import dialog's preview.
///
/// Runs the manifest back through [`crate::fileshare::preview`], so the answer includes the
/// files this machine would have overwritten, on exactly the same terms as a pasted code.
pub async fn preview(cfg: &AppConfig, text: &str) -> anyhow::Result<fileshare::SharePreview> {
    let code = normalise(text).context("that isn't a live share code")?;
    let body = fetch(&code).await?;
    fileshare::preview(cfg, &fileshare::encode(&body.manifest))
}

/// Follow a code and install what it currently points at.
pub async fn subscribe(app: &AppHandle, cfg: &AppConfig, text: &str) -> anyhow::Result<FileShare> {
    let code = normalise(text).context("that isn't a live share code")?;
    let body = fetch(&code).await?;

    // Through the ordinary importer, by re-encoding the manifest into the code it already
    // reads. Every guard that protects a pasted share — the rel check, the bundle check,
    // the overwrite rules — therefore applies here without being restated.
    let installed = fileshare::import(app, cfg, &fileshare::encode(&body.manifest)).await?;

    let mut next = config::load(app).unwrap_or_else(|_| cfg.clone());
    remember(&mut next, &body, true);
    config::save(app, &next)?;

    log::info!("live share: subscribed to {} at v{}", body.code, body.version);
    Ok(installed)
}

/// Install whatever a followed code points at now.
pub async fn sync(app: &AppHandle, cfg: &AppConfig, text: &str) -> anyhow::Result<FileShare> {
    subscribe(app, cfg, text).await
}

/// Stop following a code. The files it installed stay where they are — unfollowing is about
/// updates, not about uninstalling a track someone is in the middle of riding.
pub fn forget(app: &AppHandle, cfg: &AppConfig, text: &str) -> anyhow::Result<()> {
    let code = normalise(text).context("that isn't a live share code")?;
    let mut next = cfg.clone();
    next.live_subscriptions
        .retain(|s| normalise(&s.code).as_deref() != Some(code.as_str()));
    next.published_shares
        .retain(|s| normalise(&s.code).as_deref() != Some(code.as_str()));
    config::save(app, &next)
}

/// Ask the server what version each followed code is on now.
///
/// Returns the full list either way, so the UI can render one answer rather than merging a
/// delta into what it already had. A code the server no longer serves keeps its row and its
/// installed version — a share that was withdrawn is not a track that vanished off the disk.
pub async fn check(app: &AppHandle, cfg: &AppConfig) -> anyhow::Result<Vec<LiveShareInfo>> {
    let mut next = config::load(app).unwrap_or_else(|_| cfg.clone());
    let mut pending: Vec<(String, u32)> = Vec::new();

    for sub in &next.live_subscriptions {
        if let Some(code) = normalise(&sub.code) {
            pending.push((code, sub.version));
        }
    }

    for (code, have) in pending {
        match fetch(&code).await {
            Ok(body) => {
                if let Some(sub) = next
                    .live_subscriptions
                    .iter_mut()
                    .find(|s| normalise(&s.code).as_deref() == Some(code.as_str()))
                {
                    sub.latest = body.version;
                    sub.name = body.name.clone();
                    sub.checked_at = now_ms();
                    sub.size = body.size;
                    sub.published_at = body.updated_at;
                }
                if body.version > have {
                    log::info!("live share: {} moved to v{} (have v{have})", body.code, body.version);
                }
            }
            // A check is a background courtesy. One code the server has forgotten, or a
            // laptop on a train, must not fail the whole sweep.
            Err(e) => log::warn!("live share: couldn't check {code}: {e:#}"),
        }
    }

    config::save(app, &next)?;

    // Auto-follow codes install here rather than inside the loop above, so the config that
    // records "we checked" is saved even if an install fails halfway.
    let auto: Vec<String> = next
        .live_subscriptions
        .iter()
        .filter(|s| s.auto && s.latest > s.version)
        .map(|s| s.code.clone())
        .collect();
    for code in auto {
        if let Err(e) = sync(app, &next, &code).await {
            log::warn!("live share: auto-update of {code} failed: {e:#}");
        }
    }

    list(app, cfg)
}

/// Every live code this machine publishes or follows.
pub fn list(app: &AppHandle, cfg: &AppConfig) -> anyhow::Result<Vec<LiveShareInfo>> {
    let cfg = config::load(app).unwrap_or_else(|_| cfg.clone());
    let mut out: Vec<LiveShareInfo> = cfg
        .published_shares
        .iter()
        .map(|s| LiveShareInfo {
            code: s.code.clone(),
            name: s.name.clone(),
            version: s.version,
            latest: s.version,
            checked_at: s.published_at,
            size: 0,
            published_at: s.published_at / 1000,
            auto: false,
            mine: true,
            items: s.rels.len(),
            rels: s.rels.clone(),
        })
        .collect();
    out.extend(cfg.live_subscriptions.iter().map(|s| LiveShareInfo {
        code: s.code.clone(),
        name: s.name.clone(),
        version: s.version,
        latest: s.latest.max(s.version),
        checked_at: s.checked_at,
        size: s.size,
        published_at: s.published_at,
        auto: s.auto,
        mine: false,
        items: 0,
        rels: Vec::new(),
    }));
    Ok(out)
}

/// Install a new version as soon as one appears, or stop doing that.
pub fn set_auto(app: &AppHandle, cfg: &AppConfig, text: &str, auto: bool) -> anyhow::Result<()> {
    let code = normalise(text).context("that isn't a live share code")?;
    let mut next = cfg.clone();
    let sub = next
        .live_subscriptions
        .iter_mut()
        .find(|s| normalise(&s.code).as_deref() == Some(code.as_str()))
        .context("you're not following that code")?;
    sub.auto = auto;
    config::save(app, &next)
}

/// Adopt a code published on another machine, from its owner code.
pub async fn adopt(app: &AppHandle, cfg: &AppConfig, text: &str) -> anyhow::Result<LiveShareInfo> {
    let (code, key) = parse_owner_code(text).context("that isn't an owner code")?;
    let body = fetch(&code).await?;

    let row = PublishedShare {
        code: body.code.clone(),
        update_key: key,
        name: body.name.clone(),
        version: body.version,
        rels: body.manifest.items.iter().map(|i| i.rel.clone()).collect(),
        published_at: now_ms(),
    };
    let mut next = cfg.clone();
    next.published_shares
        .retain(|s| normalise(&s.code).as_deref() != Some(code.as_str()));
    next.published_shares.push(row.clone());
    config::save(app, &next)?;

    Ok(LiveShareInfo {
        code: row.code,
        name: row.name,
        version: row.version,
        latest: row.version,
        checked_at: row.published_at,
        size: body.size,
        published_at: body.updated_at,
        auto: false,
        mine: true,
        items: row.rels.len(),
        rels: row.rels.clone(),
    })
}

/// Record a subscription at the version just installed.
fn remember(cfg: &mut AppConfig, body: &ShareBody, installed: bool) {
    let code = normalise(&body.code).unwrap_or_else(|| body.code.clone());
    // A code this machine publishes is not also one it follows — the author already has the
    // files, and a row on both lists would offer them an update to their own track.
    if cfg
        .published_shares
        .iter()
        .any(|s| normalise(&s.code).as_deref() == Some(code.as_str()))
    {
        return;
    }
    let now = now_ms();
    if let Some(sub) = cfg
        .live_subscriptions
        .iter_mut()
        .find(|s| normalise(&s.code).as_deref() == Some(code.as_str()))
    {
        if installed {
            sub.version = body.version;
        }
        sub.latest = body.version;
        sub.name = body.name.clone();
        sub.checked_at = now;
        sub.size = body.size;
        sub.published_at = body.updated_at;
        return;
    }
    cfg.live_subscriptions.push(LiveSubscription {
        code: body.code.clone(),
        name: body.name.clone(),
        version: if installed { body.version } else { 0 },
        latest: body.version,
        checked_at: now,
        size: body.size,
        published_at: body.updated_at,
        auto: false,
    });
}

/// What to call a share when the player didn't say.
///
/// The first item's file name, without its extension — "RedBud.pkz" shared on its own is
/// "RedBud", which is what its author would have typed anyway.
fn display_name(given: &str, share: &FileShare) -> String {
    let given = given.trim();
    if !given.is_empty() {
        return given.chars().take(64).collect();
    }
    let first = share.items.first().map(|i| i.name.as_str()).unwrap_or("Share");
    let stem = first
        .rsplit_once('.')
        .map(|(s, _)| s)
        .filter(|s| !s.is_empty())
        .unwrap_or(first);
    let extra = share.items.len().saturating_sub(1);
    if extra > 0 {
        format!("{stem} +{extra}")
    } else {
        stem.to_string()
    }
}

/// Read a JSON body, turning the control plane's own error text into the message shown.
async fn read_json<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> anyhow::Result<T> {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        let msg = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or_else(|| format!("HTTP {status}"));
        anyhow::bail!("{msg}");
    }
    serde_json::from_str(&text).context("the control plane returned something unreadable")
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::BundleRef;

    fn share(items: &[(&str, &str)]) -> FileShare {
        FileShare {
            items: items
                .iter()
                .map(|(name, rel)| crate::fileshare::ShareItem {
                    name: (*name).to_string(),
                    rel: (*rel).to_string(),
                    size: 1,
                    is_dir: false,
                })
                .collect(),
            total_size: 1,
            bundle: BundleRef {
                url: "https://files.catbox.moe/a.zip".into(),
                host: "catbox".into(),
                size: 1,
                parts: vec![],
                part_sizes: vec![],
            },
        }
    }

    /// A code gets read out of a Discord message, or off a phone. Every one of these is the
    /// same share, and the app must not be the thing that says otherwise.
    #[test]
    fn a_code_reads_the_way_it_gets_typed() {
        for typed in [
            "MXBL1-K7QP4M2X",
            "mxbl1-k7qp4m2x",
            "K7QP4M2X",
            "  MXBL1-K7QP 4M2X  ",
            "K7QP-4M2X",
        ] {
            assert_eq!(normalise(typed).as_deref(), Some("K7QP4M2X"), "{typed}");
        }
    }

    /// The alphabet drops I, L, O and U so a code can be spoken. The first three still have
    /// to resolve when someone types what they heard.
    #[test]
    fn spoken_letters_fold_to_their_digits() {
        assert_eq!(normalise("O1IL2345").as_deref(), Some("01112345"));
        assert_eq!(normalise("K7QP4M2U"), None, "U is not in the alphabet");
    }

    #[test]
    fn a_wrong_length_is_not_a_code() {
        assert_eq!(normalise("K7QP4M2"), None);
        assert_eq!(normalise("K7QP4M2XY"), None);
        assert_eq!(normalise(""), None);
    }

    /// The import box has to tell a live code from a static one before it decides which
    /// path to take, and a bare eight characters is not enough to claim it.
    #[test]
    fn only_a_prefixed_code_claims_the_import_box() {
        assert!(is_live_code("MXBL1-K7QP4M2X"));
        assert!(is_live_code("mxbl1-k7qp4m2x"));
        assert!(!is_live_code("K7QP4M2X"), "no prefix, so it isn't claimed");
        assert!(!is_live_code("MXBS1-eyJpdGVtcyI6W119"), "that's a static share");
        assert!(!is_live_code("MXBL1-NOTACODE!"));
    }

    /// The update key must never travel with the public code. These two strings are the
    /// difference between sharing a track and handing over the ability to replace it.
    #[test]
    fn an_owner_code_carries_the_key_and_the_public_one_does_not() {
        let row = PublishedShare {
            code: "MXBL1-K7QP4M2X".into(),
            update_key: "s3cret-key".into(),
            ..Default::default()
        };
        let owner = owner_code(&row);
        assert!(owner.contains("s3cret-key"));
        assert!(!row.code.contains("s3cret-key"), "the public code is only the code");

        let (code, key) = parse_owner_code(&owner).unwrap();
        assert_eq!(code, "K7QP4M2X");
        assert_eq!(key, "s3cret-key");
        assert!(parse_owner_code("MXBL1-K7QP4M2X").is_none(), "no key, not an owner code");
        assert!(parse_owner_code("MXBL1-K7QP4M2X#").is_none(), "empty key");
    }

    #[test]
    fn a_share_names_itself_after_what_it_carries() {
        assert_eq!(display_name("", &share(&[("RedBud.pkz", "tracks/RedBud.pkz")])), "RedBud");
        assert_eq!(
            display_name("", &share(&[("RedBud.pkz", "tracks/RedBud.pkz"), ("a.pnt", "rider/a.pnt")])),
            "RedBud +1"
        );
        assert_eq!(display_name("  My Track  ", &share(&[("x.pkz", "tracks/x.pkz")])), "My Track");
    }

    /// The wire contract, against bytes the control plane actually returned.
    ///
    /// Two independent camelCase mappings have to agree for this feature to work at all —
    /// `updateKey` on the way out of a publish, and the whole `FileShare` nested under
    /// `manifest` on the way back — and a mismatch in either is a runtime parse failure
    /// with nothing at compile time to catch it. These are the real responses, copied from
    /// a local worker run.
    #[test]
    fn the_control_plane_answers_in_the_shape_this_module_reads() {
        let publish: PublishBody = serde_json::from_str(
            r#"{"code":"MXBL1-PJ753X5P","updateKey":"AqBVeX4vCus8p8uScHvgQIj4gxOcl6Yav-AWYEnaTKY",
                "name":"RedBud 2026","version":1,"size":68000000,"updatedAt":1788824813}"#,
        )
        .expect("a publish response parses");
        assert_eq!(publish.code, "MXBL1-PJ753X5P");
        assert_eq!(publish.version, 1);
        assert!(!publish.update_key.is_empty(), "the key is read, not dropped");

        let read: ShareBody = serde_json::from_str(
            r#"{"code":"MXBL1-PJ753X5P","name":"RedBud 2026","version":2,"size":71500000,
                "updatedAt":1788824813,
                "manifest":{"items":[{"name":"RedBud.pkz","rel":"tracks/EU/RedBud.pkz",
                "size":71500000,"isDir":false}],"totalSize":71500000,
                "bundle":{"url":"https://files.catbox.moe/def456.zip","host":"catbox",
                "size":71500000}}}"#,
        )
        .expect("a read response parses");
        assert_eq!(read.version, 2);
        assert_eq!(read.size, 71_500_000);
        assert_eq!(read.manifest.items[0].rel, "tracks/EU/RedBud.pkz");
        assert_eq!(read.manifest.bundle.url, "https://files.catbox.moe/def456.zip");
        assert!(read.manifest.bundle.parts.is_empty(), "a single-part bundle omits them");

        // And a publish with no key on it — an update rather than a mint — still parses, so
        // `publish` can fall back to the key it already holds.
        let update: PublishBody = serde_json::from_str(
            r#"{"code":"MXBL1-PJ753X5P","name":"RedBud 2026","version":2,"size":71500000,
                "updatedAt":1788824813}"#,
        )
        .expect("an update response parses");
        assert!(update.update_key.is_empty());
    }

    /// The author of a code holds the files already. A row on both lists would offer them
    /// an update to the track they just published.
    #[test]
    fn publishing_a_code_does_not_also_subscribe_to_it() {
        let mut cfg = AppConfig {
            published_shares: vec![PublishedShare {
                code: "MXBL1-K7QP4M2X".into(),
                update_key: "k".into(),
                version: 2,
                ..Default::default()
            }],
            ..Default::default()
        };
        let body = ShareBody {
            code: "MXBL1-K7QP4M2X".into(),
            name: "RedBud".into(),
            version: 2,
            size: 1,
            updated_at: 0,
            manifest: share(&[("RedBud.pkz", "tracks/RedBud.pkz")]),
        };
        remember(&mut cfg, &body, true);
        assert!(cfg.live_subscriptions.is_empty());
    }

    /// A check that finds a newer version must not claim it is installed — the gap between
    /// the two numbers is the whole "update available" state.
    #[test]
    fn a_check_moves_latest_and_leaves_the_installed_version_alone() {
        let mut cfg = AppConfig {
            live_subscriptions: vec![LiveSubscription {
                code: "MXBL1-K7QP4M2X".into(),
                name: "RedBud".into(),
                version: 1,
                latest: 1,
                ..Default::default()
            }],
            ..Default::default()
        };
        let body = ShareBody {
            code: "MXBL1-K7QP4M2X".into(),
            name: "RedBud 2026".into(),
            version: 4,
            size: 1,
            updated_at: 0,
            manifest: share(&[("RedBud.pkz", "tracks/RedBud.pkz")]),
        };

        remember(&mut cfg, &body, false);
        assert_eq!(cfg.live_subscriptions[0].version, 1, "nothing was installed");
        assert_eq!(cfg.live_subscriptions[0].latest, 4);
        assert_eq!(cfg.live_subscriptions[0].name, "RedBud 2026", "renames follow");

        remember(&mut cfg, &body, true);
        assert_eq!(cfg.live_subscriptions[0].version, 4, "now it is");
        assert_eq!(cfg.live_subscriptions.len(), 1, "and it is still one row");
    }
}
