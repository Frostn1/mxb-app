//! Our own copy of the mxb-mods.com catalogue, tried before the site itself.
//!
//! The control plane (`api.mxbsecure.com/v1/assets/search`) keeps a mirror of the mods listing.
//! It answers faster than the site, never meets Cloudflare's bot check, and can't be refused to
//! a user on a flagged network. So listings and slug lookups ask it first and fall through to
//! the site (and from there to the WebView) when it can't be trusted for this request:
//!
//! - it is not mxb-mods.com being browsed (the mirror holds no gpb-mods.com),
//! - the request is one it can't answer the way the site does (popular ranking),
//! - it errored, timed out, or answered something that isn't the expected shape,
//! - its last finished sync is older than [`MAX_STALE_SECS`] (the fetcher feeding it stopped),
//! - it doesn't hold what was asked for (an empty page, an unknown category, a mod it hasn't
//!   seen yet).
//!
//! Anything that makes the mirror fail also parks it for [`COOLDOWN`], so a mirror that is down
//! costs one short request, not one per keystroke. The site's own backoff is untouched.

use super::{ModSort, ModSummary};
use serde_json::Value;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The mirror's walk finishes every ~10 minutes while its fetcher runs. Past this it is
/// behind the site by more than a person would expect, and the site is asked instead.
pub const MAX_STALE_SECS: i64 = 15 * 60;

/// How long a mirror that failed is left alone.
const COOLDOWN: Duration = Duration::from_secs(120);

/// Short on purpose: the mirror is the fast path, and a slow one is worse than the site.
const TIMEOUT: Duration = Duration::from_secs(6);

/// Unix seconds before which the mirror is not asked (0: no cooldown).
static PARKED_UNTIL: AtomicU64 = AtomicU64::new(0);

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn parked(now: i64) -> bool {
    PARKED_UNTIL.load(Ordering::Relaxed) as i64 > now
}

fn park(now: i64) {
    PARKED_UNTIL.store((now + COOLDOWN.as_secs() as i64) as u64, Ordering::Relaxed);
}

/// Why the mirror's answer is not used. Only logged: the caller just falls through.
#[derive(Debug, PartialEq, Eq)]
pub enum Skip {
    NotMxb,
    Parked,
    Unsupported,
    Failed(String),
    NotJson,
    Stale,
    Missing,
}

/// Is a sync time recent enough? `synced_at` is the control plane's ISO time; none means the
/// mirror has never finished a walk.
pub fn is_fresh(synced_at: Option<&str>, now: i64) -> bool {
    synced_at
        .and_then(super::shop_catalog::parse_date_str)
        .is_some_and(|at| now - at <= MAX_STALE_SECS)
}

/// The mirror's row as the app's own card. `None` for a row without the site's post id and
/// slug (an older control plane, or an upload): the app keys everything by them.
fn summary(row: &Value, category_id: u32) -> Option<ModSummary> {
    let id = row.get("wp_id")?.as_u64()?;
    let slug = row.get("wp_slug")?.as_str()?.to_string();
    let text = |k: &str| row.get(k).and_then(Value::as_str);
    Some(ModSummary {
        id,
        slug,
        title: text("title")?.to_string(),
        link: text("source_url").unwrap_or("").to_string(),
        // The site gives the post date site-local with no zone ("2026-10-12T16:40:41"). The
        // mirror's is UTC with a Z; same shape, no suffix.
        date: text("published").map(|d| d.trim_end_matches('Z').to_string()).unwrap_or_default(),
        image: text("thumb").map(str::to_string),
        category_id,
        author: text("author").map(str::to_string),
    })
}

/// A search response as cards. Err says why it can't be used: not the expected shape, stale,
/// or holding nothing for this request.
pub fn parse_page(body: &str, category_id: u32, now: i64) -> Result<Vec<ModSummary>, Skip> {
    let v: Value = serde_json::from_str(body).map_err(|_| Skip::NotJson)?;
    let rows = v.get("results").and_then(Value::as_array).ok_or(Skip::NotJson)?;
    if !is_fresh(v.get("synced_at").and_then(Value::as_str), now) {
        return Err(Skip::Stale);
    }
    let cards: Vec<ModSummary> = rows.iter().filter_map(|r| summary(r, category_id)).collect();
    // A row we couldn't read means the shape isn't what we expect; don't show half a page.
    if cards.len() != rows.len() || cards.is_empty() {
        return Err(Skip::Missing);
    }
    Ok(cards)
}

/// The one mod with this slug, from the mirror's by-slug answer.
pub fn parse_one(body: &str, category_id: u32, now: i64) -> Result<ModSummary, Skip> {
    let v: Value = serde_json::from_str(body).map_err(|_| Skip::NotJson)?;
    if !is_fresh(v.get("synced_at").and_then(Value::as_str), now) {
        return Err(Skip::Stale);
    }
    summary(&v, category_id).ok_or(Skip::Missing)
}

/// The mirror's `sort=` for ours, or `None` when it can't order that way (popular).
pub fn sort_param(sort: ModSort) -> Option<&'static str> {
    match sort {
        ModSort::Newest => Some("newest"),
        ModSort::Oldest => Some("oldest"),
        _ => None,
    }
}

fn client() -> anyhow::Result<&'static reqwest::Client> {
    static CLIENT: std::sync::OnceLock<Result<reqwest::Client, String>> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(TIMEOUT)
                .user_agent(crate::mxb_session::ua())
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!("{e}"))
}

async fn get_body(url: &str, query: &[(&str, String)]) -> Result<(u16, String), Skip> {
    let resp = client()
        .map_err(|e| Skip::Failed(e.to_string()))?
        .get(url)
        .query(query)
        .send()
        .await
        .map_err(|e| Skip::Failed(e.to_string()))?;
    let status = resp.status().as_u16();
    let body = resp.text().await.map_err(|e| Skip::Failed(e.to_string()))?;
    Ok((status, body))
}

/// Run a mirror request through the gates that apply to every one: mxb-mods only, not parked.
/// A failure parks the mirror; a clean "it doesn't have that" does not.
async fn gated<T>(run: impl Future<Output = Result<T, Skip>>) -> Result<T, Skip> {
    if crate::mxb_session::base() != crate::mxb_session::MXB_SITE.base {
        return Err(Skip::NotMxb);
    }
    if parked(now_secs()) {
        return Err(Skip::Parked);
    }
    let out = run.await;
    if matches!(out, Err(Skip::Failed(_)) | Err(Skip::NotJson) | Err(Skip::Stale)) {
        park(now_secs());
    }
    out
}

/// One listing page from the mirror, in the app's own shape.
pub async fn search(query: &str, category_id: u32, page: u32, sort: ModSort) -> Result<Vec<ModSummary>, Skip> {
    let sort = sort_param(sort).ok_or(Skip::Unsupported)?;
    gated(async {
        let url = format!("{}/v1/assets/search", crate::paintsync::control_plane());
        let mut q = vec![
            ("source", "mirror".to_string()),
            ("category", category_id.to_string()),
            ("page", page.to_string()),
            ("sort", sort.to_string()),
        ];
        if !query.is_empty() {
            q.push(("q", query.to_string()));
        }
        let (status, body) = get_body(&url, &q).await?;
        match status {
            200 => parse_page(&body, category_id, now_secs()),
            // The mirror doesn't know this category: the site does.
            400 => Err(Skip::Missing),
            s => Err(Skip::Failed(format!("mirror answered {s}"))),
        }
    })
    .await
}

/// One mod by the site's slug, from the mirror.
pub async fn by_slug(slug: &str, category_id: u32) -> Result<ModSummary, Skip> {
    gated(async {
        let url = format!("{}/v1/assets/mirror/{}", crate::paintsync::control_plane(), slug);
        let (status, body) = get_body(&url, &[]).await?;
        match status {
            200 => parse_one(&body, category_id, now_secs()),
            404 => Err(Skip::Missing),
            s => Err(Skip::Failed(format!("mirror answered {s}"))),
        }
    })
    .await
}

/// The fallback order, in one place so a test can pin it: the mirror first; the site (and its
/// own WebView fallback) only when the mirror gave nothing usable.
pub async fn mirror_then<T, E>(
    what: &str,
    mirror: impl Future<Output = Result<T, Skip>>,
    site: impl Future<Output = Result<T, E>>,
) -> Result<T, E> {
    match mirror.await {
        Ok(v) => Ok(v),
        Err(skip) => {
            match skip {
                Skip::NotMxb | Skip::Parked | Skip::Unsupported => {}
                other => log::info!("mirror skipped for {what}: {other:?}; asking the site"),
            }
            site.await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const NOW: i64 = 1_791_900_000;

    /// An ISO time `secs_ago` before NOW, without a date crate (civil_from_days).
    fn iso(secs_ago: i64) -> String {
        let t = NOW - secs_ago;
        let days = t.div_euclid(86_400);
        let rem = t.rem_euclid(86_400);
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
    }

    fn page(synced: Option<String>, rows: &[Value]) -> String {
        serde_json::json!({ "page": 1, "results": rows, "synced_at": synced }).to_string()
    }

    fn row(wp_id: u64) -> Value {
        serde_json::json!({
            "wp_id": wp_id, "wp_slug": format!("mod-{wp_id}"), "title": format!("Mod {wp_id}"),
            "source_url": format!("https://mxb-mods.com/mod-{wp_id}/"),
            "published": "2026-10-12T16:40:41Z", "thumb": "https://cdn.mxbsecure.com/t.webp",
            "author": "Someone"
        })
    }

    #[test]
    fn a_fresh_page_becomes_the_apps_cards() {
        let body = page(Some(iso(60)), &[row(7), row(8)]);
        let cards = parse_page(&body, 37, NOW).unwrap();
        assert_eq!(cards.len(), 2);
        assert_eq!((cards[0].id, cards[0].slug.as_str()), (7, "mod-7"));
        assert_eq!(cards[0].title, "Mod 7");
        assert_eq!(cards[0].link, "https://mxb-mods.com/mod-7/");
        // Same date shape the site's own listing has: no zone suffix.
        assert_eq!(cards[0].date, "2026-10-12T16:40:41");
        assert_eq!(cards[0].category_id, 37);
        assert_eq!(cards[0].author.as_deref(), Some("Someone"));
        assert_eq!(cards[0].image.as_deref(), Some("https://cdn.mxbsecure.com/t.webp"));
    }

    #[test]
    fn a_mirror_that_stopped_syncing_is_not_used() {
        assert_eq!(parse_page(&page(Some(iso(16 * 60)), &[row(1)]), 37, NOW).unwrap_err(), Skip::Stale);
        assert!(parse_page(&page(Some(iso(14 * 60)), &[row(1)]), 37, NOW).is_ok());
        // Never synced, or a control plane from before this change that reports no time.
        assert_eq!(parse_page(&page(None, &[row(1)]), 37, NOW).unwrap_err(), Skip::Stale);
    }

    #[test]
    fn a_page_the_mirror_cannot_fill_falls_through() {
        // Nothing for this request (a mod it has not seen yet).
        assert_eq!(parse_page(&page(Some(iso(1)), &[]), 37, NOW).unwrap_err(), Skip::Missing);
        // A row without the site's ids.
        let old = serde_json::json!({ "title": "x", "id": "uuid" });
        assert_eq!(parse_page(&page(Some(iso(1)), &[row(1), old]), 37, NOW).unwrap_err(), Skip::Missing);
        assert_eq!(parse_page("<html>challenge</html>", 37, NOW).unwrap_err(), Skip::NotJson);
    }

    #[test]
    fn one_mod_by_slug() {
        let mut one = row(9);
        one["synced_at"] = Value::String(iso(30));
        assert_eq!(parse_one(&one.to_string(), 22, NOW).unwrap().slug, "mod-9");
        one["synced_at"] = Value::String(iso(3600));
        assert_eq!(parse_one(&one.to_string(), 22, NOW).unwrap_err(), Skip::Stale);
    }

    #[test]
    fn only_newest_and_oldest_are_asked_of_the_mirror() {
        assert_eq!(sort_param(ModSort::Newest), Some("newest"));
        assert_eq!(sort_param(ModSort::Oldest), Some("oldest"));
        assert_eq!(sort_param(ModSort::PopularAll), None);
        assert_eq!(sort_param(ModSort::PopularWeek), None);
    }

    fn run<T>(f: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(f)
    }

    #[test]
    fn the_mirror_answers_first_and_the_site_is_never_asked() {
        let calls = RefCell::new(Vec::new());
        let out: Result<&str, ()> = run(mirror_then(
            "t",
            async {
                calls.borrow_mut().push("mirror");
                Ok("from mirror")
            },
            async {
                calls.borrow_mut().push("site");
                Ok("from site")
            },
        ));
        assert_eq!(out, Ok("from mirror"));
        assert_eq!(*calls.borrow(), ["mirror"]);
    }

    #[test]
    fn every_reason_to_skip_the_mirror_reaches_the_site_after_it() {
        for skip in [
            Skip::Stale,
            Skip::Missing,
            Skip::NotJson,
            Skip::Failed("down".into()),
            Skip::Parked,
            Skip::Unsupported,
            Skip::NotMxb,
        ] {
            let calls = RefCell::new(Vec::new());
            let out: Result<&str, ()> = run(mirror_then(
                "t",
                async {
                    calls.borrow_mut().push("mirror");
                    Err(skip)
                },
                async {
                    calls.borrow_mut().push("site");
                    Ok("from site")
                },
            ));
            assert_eq!(out, Ok("from site"));
            assert_eq!(*calls.borrow(), ["mirror", "site"]);
        }
    }

    #[test]
    fn the_sites_error_is_what_the_caller_sees() {
        // The site's own refusal (and from there the WebView) is untouched by the mirror.
        let out: Result<&str, &str> = run(mirror_then("t", async { Err(Skip::Stale) }, async { Err("403") }));
        assert_eq!(out, Err("403"));
    }

    #[test]
    fn a_failing_mirror_is_left_alone_for_a_while() {
        PARKED_UNTIL.store(0, Ordering::Relaxed);
        assert!(!parked(NOW));
        park(NOW);
        assert!(parked(NOW + 119));
        assert!(!parked(NOW + 121));
        PARKED_UNTIL.store(0, Ordering::Relaxed);
    }
}
