//! Browsing the mxbikes-shop.com catalog.
//!
//! Unlike [`super::mxb`], which pages a WordPress REST API, the store hands us the whole
//! catalog as one JSON document (`frostmod-mods.json`, ~1300 items). So the shape here is
//! inverted: fetch once, keep it, and do the searching, filtering, sorting and paging
//! locally. That makes browsing instant and keeps working with the network down — and it
//! means the only thing that can go stale is *prices*, which is exactly what the refresh
//! policy below is built around.
//!
//! Deliberately not part of [`super::mxbshop`]. That module is a signed-in HTML scraper for
//! a user's purchased downloads; this one is a public catalog reader with a disk cache and a
//! price model. They share a domain name and nothing else.
//!
//! Scope: browsing only. Nothing here downloads, installs or buys — a purchase is a link
//! out to the store in the user's own browser.

use super::Blocked;
use crate::shop_catalog_session;
use crate::shop_credentials;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const SHOP_BASE: &str = "https://mxbikes-shop.com";
pub const CATALOG_URL: &str = "https://mxbikes-shop.com/frostmod-mods.json";

/// Matches [`super::super::Browse`]'s page size so "Load more" behaves identically on both
/// catalogs. Mirrors `SEARCH_PAGE_SIZE` in `src/api/mods.ts`.
const PER_PAGE: usize = 24;

/// How long a fetched dump is served without checking for a newer one.
///
/// Sales are time-boxed and the dump is the only place their prices live, so freshness is
/// correctness here, not just tidiness. Thirty minutes is affordable because a refresh is
/// conditional — an unchanged catalog costs a 304 and no body.
const CATALOG_TTL: Duration = Duration::from_secs(30 * 60);

/// Past this, a cached catalog stops being "recent enough to show quietly" and the UI says
/// so in a way the user can't dismiss. Prices this old are not worth trusting.
const MAX_STALE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Bumped when the on-disk shape changes, so old entries are dropped rather than misread.
const CACHE_DIR: &str = "shop-catalog-v1";

// ───────────────────────────────── client ─────────────────────────────────

/// One client for the whole session. It holds [`shop_catalog_session::jar`], so a
/// `cf_clearance` earned by the handshake WebView is picked up without rebuilding it.
fn client() -> anyhow::Result<&'static reqwest::Client> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| build_client().map_err(|e| format!("{e:#}")))
        .as_ref()
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn build_client() -> anyhow::Result<reqwest::Client> {
    use reqwest::header::{HeaderMap, HeaderValue};
    let mut headers = HeaderMap::new();
    for (k, v) in [
        ("accept", "application/json, text/plain, */*"),
        ("accept-language", "en-US,en;q=0.9"),
        ("sec-fetch-site", "same-origin"),
        ("sec-fetch-mode", "cors"),
        ("sec-fetch-dest", "empty"),
        ("referer", SHOP_BASE),
    ] {
        headers.insert(k, HeaderValue::from_static(v));
    }
    Ok(shop_catalog_session::client_builder()
        .default_headers(headers)
        .build()?)
}

// ───────────────────────────────── public wire types ─────────────────────────────────

/// What an item costs right now, and what it costs normally.
///
/// `on_sale` is computed when this is built, never stored — see [`is_on_sale`].
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShopPrice {
    /// The normal price. The low end of the range when `has_range`.
    pub base: Option<f64>,
    /// The high end of the normal range. `None` when the item has a single option.
    pub base_max: Option<f64>,
    /// The discounted price, when one is live. Low end of the range when `has_range`.
    pub sale: Option<f64>,
    pub sale_max: Option<f64>,
    /// A sale price exists *and* the clock is inside its window.
    pub on_sale: bool,
    /// The item has several options (a paint, or a paint plus the PSD), so a single figure
    /// would be a lie. Render `base`–`base_max`.
    pub has_range: bool,
    /// The store gives this away. Distinct from a price of 0: a pay-what-you-want item
    /// starts at 0 without being free, and "$0.00" reads as broken where "Free" doesn't.
    pub free: bool,
    /// Whole percent off, rounded down, from the low end of each range. `None` when it
    /// can't be computed honestly.
    pub discount_pct: Option<u32>,
    /// Unix seconds. Only ever `Some` when the dump actually carries a window — the UI must
    /// not invent an expiry.
    pub sale_ends: Option<i64>,
}

/// A catalog item as it appears in the grid.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopMod {
    pub id: u64,
    pub title: String,
    /// The product page. `None` when the dump gave us a URL we won't hand to the OS — see
    /// [`safe_shop_url`]. A `None` here disables the Buy button.
    pub url: Option<String>,
    pub image: Option<String>,
    pub author: Option<String>,
    pub author_url: Option<String>,
    pub category_ids: Vec<u64>,
    pub category_names: Vec<String>,
    /// Unix seconds, when the dump carried a usable date.
    pub updated: Option<i64>,
    pub price: ShopPrice,
}

/// Everything the grid shows, plus the parts only worth sending for one item at a time.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopModDetail {
    #[serde(flatten)]
    pub item: ShopMod,
    /// Sanitised in [`sanitize_html`] before it ever reaches the webview.
    pub description_html: Option<String>,
    pub images: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShopCategory {
    pub id: u64,
    pub name: String,
    pub slug: String,
    pub parent: Option<u64>,
    /// 0 for a top-level category. Lets the UI indent without walking the tree.
    pub depth: u8,
    /// Items in this category *and its descendants*, which is what the filter selects.
    pub count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopPage {
    pub items: Vec<ShopMod>,
    pub total: u32,
    pub has_more: bool,
    pub currency: String,
    pub generated_ts: Option<i64>,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopStatus {
    /// This build has a credential. False means the Shop tab shouldn't be shown at all.
    pub available: bool,
    /// Items currently loaded. 0 before the first successful fetch.
    pub count: u32,
    pub currency: String,
    /// When the store generated the dump we're serving.
    pub generated_ts: Option<i64>,
    /// When we last fetched it.
    pub fetched_at: Option<i64>,
    pub stale: bool,
    /// Past [`MAX_STALE`] — the UI should say so insistently rather than in passing.
    pub very_stale: bool,
    /// The last refresh failure, if the catalog we're serving came from disk.
    pub error: Option<String>,
}

/// How a listing is ordered.
///
/// Deliberately not [`super::ModSort`]. That enum's variants exist because mxb-mods.com
/// ignores `orderby` and its popular-posts plugin is the only real ordering on offer — a
/// constraint with nothing to do with sorting a list we hold in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShopSort {
    #[default]
    Newest,
    RecentlyUpdated,
    PriceAsc,
    PriceDesc,
    /// Discounted items first, deepest discount leading.
    OnSale,
    NameAsc,
}

// ───────────────────────────────── raw payload ─────────────────────────────────

/// A number the store may send as `12.5` or as `"12.50"`.
///
/// WooCommerce and Easy Digital Downloads both emit prices as strings in some
/// configurations, and which one we get isn't ours to decide. Untagged so serde simply
/// tries both rather than us guessing.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Num {
    F(f64),
    S(String),
}

impl Num {
    fn as_f64(&self) -> Option<f64> {
        match self {
            Num::F(v) => v.is_finite().then_some(*v),
            Num::S(s) => {
                let cleaned: String = s
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                    .collect();
                cleaned.parse::<f64>().ok().filter(|v| v.is_finite())
            }
        }
    }

    fn as_u64(&self) -> Option<u64> {
        self.as_f64().filter(|v| *v >= 0.0).map(|v| v as u64)
    }
}

/// A field the store may send as a scalar or as a list — `category` vs `categories`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    fn into_vec(self) -> Vec<T> {
        match self {
            OneOrMany::One(v) => vec![v],
            OneOrMany::Many(v) => v,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawDump {
    generated_ts: Option<i64>,
    generated: Option<String>,
    currency: Option<String>,
    categories: Vec<RawCategory>,
    mods: Vec<RawMod>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawCategory {
    id: Option<Num>,
    #[serde(alias = "title", alias = "label")]
    name: Option<String>,
    slug: Option<String>,
    #[serde(alias = "parent_id")]
    parent: Option<Num>,
    /// The nested form. [`flatten_categories`] handles this and the flat `parent` form.
    children: Vec<RawCategory>,
}

/// One item, as permissively as serde will allow.
///
/// Only `id`, `price`, `price_max`, `sale_price`, `sale_price_max`, `updated`, `author` and
/// `author_url` are confirmed by the store's developer. Everything else is a guess, which is
/// why every field is optional, why the plausible spellings are covered by `alias`, and why
/// `extra` keeps whatever we didn't name.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawMod {
    // ── confirmed ──
    id: Option<Num>,
    price: Option<Num>,
    price_max: Option<Num>,
    sale_price: Option<Num>,
    sale_price_max: Option<Num>,
    updated: Option<Value>,
    author: Option<String>,
    author_url: Option<String>,
    free: Option<bool>,

    // ── guessed ──
    #[serde(alias = "name", alias = "post_title")]
    title: Option<String>,
    #[serde(alias = "permalink", alias = "link", alias = "product_url", alias = "href")]
    url: Option<String>,
    #[serde(
        alias = "thumbnail",
        alias = "thumb",
        alias = "featured_image",
        alias = "img"
    )]
    image: Option<String>,
    #[serde(alias = "gallery", alias = "screenshots")]
    images: Option<OneOrMany<String>>,
    /// Not `OneOrMany<Value>`: `Value` deserializes from anything, including an array, so
    /// the `One` variant would always win and swallow the list. Handled by
    /// [`category_ids_of`] instead.
    #[serde(alias = "categories", alias = "category_ids")]
    category: Option<Value>,
    #[serde(alias = "excerpt", alias = "content", alias = "short_description")]
    description: Option<String>,
    #[serde(alias = "created", alias = "published", alias = "date")]
    published: Option<Value>,

    // ── sale window: existence unknown, honoured if present ──
    #[serde(
        alias = "sale_from",
        alias = "sale_price_dates_from",
        alias = "discount_start"
    )]
    sale_start: Option<Value>,
    #[serde(
        alias = "sale_to",
        alias = "sale_price_dates_to",
        alias = "discount_end"
    )]
    sale_end: Option<Value>,

    /// Whatever the store sends that we didn't name. Never read at runtime — it exists so
    /// the ignored live test can print the union of unknown keys and tell us exactly what
    /// [`map_mod`] should be corrected to.
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

// ───────────────────────────────── internal model ─────────────────────────────────

/// One item, mapped. Holds the detail-only fields too, so detail needs no second lookup.
#[derive(Debug, Clone)]
struct Item {
    id: u64,
    title: String,
    url: Option<String>,
    image: Option<String>,
    images: Vec<String>,
    author: Option<String>,
    author_url: Option<String>,
    description_html: Option<String>,
    category_ids: Vec<u64>,
    category_names: Vec<String>,
    updated: Option<i64>,
    published: Option<i64>,
    free: bool,
    base: Option<f64>,
    base_max: Option<f64>,
    sale: Option<f64>,
    sale_max: Option<f64>,
    sale_window: (Option<i64>, Option<i64>),
    /// Lowercased title + author + categories, built once. Searching 1300 of these is a
    /// sub-millisecond scan, so there is no index beyond this.
    search_key: String,
}

#[derive(Debug, Default)]
struct CategoryIndex {
    flat: Vec<ShopCategory>,
    /// A category's own id plus every id beneath it, so filtering by a parent includes its
    /// children — which is what a person picking "Bikes" means.
    descendants: HashMap<u64, HashSet<u64>>,
    names: HashMap<u64, String>,
    /// Slug → id. The catalog identifies a mod's categories by `{name, slug}` with no id,
    /// so this is the only way to tie an item to the category tree.
    by_slug: HashMap<String, u64>,
}

#[derive(Debug)]
struct Catalog {
    items: Vec<Item>,
    categories: Vec<ShopCategory>,
    currency: String,
    generated_ts: Option<i64>,
    fetched_at: i64,
    etag: Option<String>,
    last_modified: Option<String>,
}

impl Catalog {
    fn age(&self) -> Duration {
        Duration::from_secs((now().saturating_sub(self.fetched_at)).max(0) as u64)
    }

    fn stale(&self) -> bool {
        self.age() > CATALOG_TTL
    }

    fn very_stale(&self) -> bool {
        self.age() > MAX_STALE
    }
}

// ───────────────────────────────── time ─────────────────────────────────

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Unix seconds from whatever the store used for a date.
///
/// Covers the forms a WordPress-derived export realistically emits: a unix integer (in
/// seconds or milliseconds), an ISO-8601 string with `Z` or a numeric offset, the MySQL
/// `YYYY-MM-DD HH:MM:SS` form WordPress stores natively, and a bare date.
fn parse_timestamp(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => {
            let raw = n.as_i64().or_else(|| n.as_f64().map(|f| f as i64))?;
            // Anything this large is milliseconds; a seconds value that big is year 33658.
            Some(if raw.abs() > 100_000_000_000 {
                raw / 1000
            } else {
                raw
            })
        }
        Value::String(s) => parse_date_str(s),
        _ => None,
    }
}

fn parse_date_str(raw: &str) -> Option<i64> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if s.chars().all(|c| c.is_ascii_digit() || c == '-') {
        if let Ok(v) = s.parse::<i64>() {
            return Some(if v.abs() > 100_000_000_000 { v / 1000 } else { v });
        }
    }

    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let mut secs = days_from_civil(year, month, day) * 86_400;

    // The time part, if there is one. `T` or a space separates it.
    let rest = &s[10..];
    let rest = rest.strip_prefix('T').or_else(|| rest.strip_prefix(' '));
    if let Some(rest) = rest {
        if rest.len() >= 5 {
            let hour: i64 = rest.get(0..2)?.parse().ok()?;
            let minute: i64 = rest.get(3..5)?.parse().ok()?;
            let second: i64 = rest
                .get(6..8)
                .filter(|v| v.chars().all(|c| c.is_ascii_digit()))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            secs += hour * 3600 + minute * 60 + second;
        }
        // A trailing offset means the fields above were local to it, so subtract it to get
        // UTC. No offset and no `Z` is treated as UTC, which is what the store's `generated`
        // field and WordPress's stored dates both mean in practice.
        if let Some(offset) = parse_offset(rest) {
            secs -= offset;
        }
    }
    Some(secs)
}

/// Seconds east of UTC from a trailing `+HH:MM`, `-HHMM` or `Z`.
fn parse_offset(rest: &str) -> Option<i64> {
    let tail = rest.trim_end();
    if tail.ends_with('Z') || tail.ends_with('z') {
        return Some(0);
    }
    // Look only at the tail, so the `-` inside a date can't be mistaken for a sign.
    let idx = tail.rfind(['+', '-'])?;
    // An offset is at least "+00", and must come after the time, not inside the date.
    if idx == 0 {
        return None;
    }
    let sign = if tail.as_bytes()[idx] == b'-' { -1 } else { 1 };
    let digits: String = tail[idx + 1..].chars().filter(char::is_ascii_digit).collect();
    let (hours, minutes) = match digits.len() {
        2 => (digits.parse::<i64>().ok()?, 0),
        4 => (
            digits.get(0..2)?.parse::<i64>().ok()?,
            digits.get(2..4)?.parse::<i64>().ok()?,
        ),
        _ => return None,
    };
    Some(sign * (hours * 3600 + minutes * 60))
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's `days_from_civil`).
/// Pulled in rather than adding a date crate for the two fields that need it.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

// ───────────────────────────────── sanitising ─────────────────────────────────

/// True for a URL we're willing to hand to the operating system's browser.
///
/// The Buy button feeds this straight into `shell:allow-open`, which launches whatever
/// handler the OS has registered for the scheme. The URL comes from a remote JSON document,
/// so it is not ours to trust: anything that isn't `https` on the store's own domain is
/// dropped, and the button goes away with it.
fn safe_shop_url(raw: &str) -> Option<String> {
    let url = reqwest::Url::parse(raw.trim()).ok()?;
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    (host == "mxbikes-shop.com" || host.ends_with(".mxbikes-shop.com")).then(|| url.to_string())
}

/// Same rule, relaxed to any https host, for images — thumbnails legitimately live on a CDN.
fn safe_image_url(raw: &str) -> Option<String> {
    let url = reqwest::Url::parse(raw.trim()).ok()?;
    (url.scheme() == "https").then(|| url.to_string())
}

/// Strip everything that can execute out of a description before it reaches the webview.
///
/// The description is rendered with `dangerouslySetInnerHTML`, and `tauri.conf.json` sets
/// `"csp": null`, so an injected `<script>` in this document would run with the app's own
/// privileges. This is the only place remote HTML enters the app, so it's the only place
/// that has to be careful — do not move the rendering somewhere that skips it.
fn sanitize_html(raw: &str) -> Option<String> {
    use scraper::{Html, Selector};

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Drop whole dangerous elements, contents and all.
    let mut html = trimmed.to_string();
    for tag in ["script", "iframe", "object", "embed", "style", "form", "link"] {
        html = strip_element(&html, tag);
    }

    // Then anything that can execute from an attribute.
    let doc = Html::parse_fragment(&html);
    let has_script_attr = Selector::parse("*")
        .ok()
        .map(|sel| {
            doc.select(&sel).any(|el| {
                el.value().attrs().any(|(name, value)| {
                    let name = name.to_ascii_lowercase();
                    name.starts_with("on")
                        || (matches!(name.as_str(), "href" | "src" | "srcset" | "action")
                            && value.trim_start().to_ascii_lowercase().starts_with("javascript:"))
                })
            })
        })
        .unwrap_or(false);

    // `scraper` can read a document but not rewrite one, so rather than half-clean the
    // markup we drop attributes wholesale when any of them is executable. A description
    // with an inline handler is not one worth rendering richly.
    if has_script_attr {
        html = strip_attributes(&html);
    }

    let cleaned = html.trim().to_string();
    (!cleaned.is_empty()).then_some(cleaned)
}

/// Remove `<tag …>…</tag>` and any self-closing or unterminated form of it.
fn strip_element(html: &str, tag: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0usize;
    while let Some(rel) = lower[cursor..].find(&open) {
        let start = cursor + rel;
        // `<scriptish>` is not `<script>`; require a delimiter after the name.
        let after = lower[start + open.len()..].chars().next();
        if !matches!(after, None | Some(' ') | Some('>') | Some('/') | Some('\n') | Some('\t') | Some('\r')) {
            out.push_str(&html[cursor..start + open.len()]);
            cursor = start + open.len();
            continue;
        }
        out.push_str(&html[cursor..start]);
        cursor = match lower[start..].find(&close) {
            Some(rel_end) => start + rel_end + close.len(),
            // Unterminated: skip to the end of the opening tag and keep the rest of the text.
            None => match lower[start..].find('>') {
                Some(rel_gt) => start + rel_gt + 1,
                None => html.len(),
            },
        };
    }
    out.push_str(&html[cursor.min(html.len())..]);
    out
}

/// Reduce every tag to its bare name, dropping all attributes.
fn strip_attributes(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let Some(gt) = rest[lt..].find('>') else {
            break;
        };
        let inner = &rest[lt + 1..lt + gt];
        let closing = inner.starts_with('/');
        let name: String = inner
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if !name.is_empty() {
            out.push('<');
            if closing {
                out.push('/');
            }
            out.push_str(&name);
            out.push('>');
        }
        rest = &rest[lt + gt + 1..];
    }
    out.push_str(rest);
    out
}

// ───────────────────────────────── mapping ─────────────────────────────────

// ════════════════════════════════════════════════════════════════════════════════════
//  FIELD MAPPING — THE ONLY PLACE THE STORE'S JSON SHAPE IS ASSUMED.
//
//  Confirmed by the store's developer: id, price, price_max, sale_price, sale_price_max,
//  updated, author, author_url.
//
//  EVERYTHING ELSE — title, url, image, images, category, description, published, and the
//  sale window — IS A GUESS. To correct it, run
//      cargo test live_shop_catalog_shape -- --ignored --nocapture
//  which prints the real payload's keys. Fix this function; nothing outside it assumes
//  anything about the store's field names.
// ════════════════════════════════════════════════════════════════════════════════════
fn map_mod(raw: RawMod, cats: &CategoryIndex) -> Option<Item> {
    let id = raw.id.as_ref().and_then(Num::as_u64);
    let title = raw
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string);

    // An item with no id can't be linked to, and one with no title can't be shown. Either
    // alone is survivable; neither is not.
    let (id, title) = match (id, title) {
        (Some(id), Some(title)) => (id, title),
        (Some(id), None) => (id, format!("Item {id}")),
        _ => return None,
    };

    let (category_ids, category_names) = resolve_categories(raw.category.as_ref(), cats);

    let image = raw.image.as_deref().and_then(safe_image_url);
    let images: Vec<String> = raw
        .images
        .map(OneOrMany::into_vec)
        .unwrap_or_default()
        .iter()
        .filter_map(|u| safe_image_url(u))
        .collect();

    let base = raw.price.as_ref().and_then(Num::as_f64);
    let base_max = raw.price_max.as_ref().and_then(Num::as_f64);
    let sale = raw.sale_price.as_ref().and_then(Num::as_f64);
    let sale_max = raw.sale_price_max.as_ref().and_then(Num::as_f64);

    let search_key = build_search_key(&title, raw.author.as_deref(), &category_names);

    Some(Item {
        id,
        title,
        url: raw.url.as_deref().and_then(safe_shop_url),
        image,
        images,
        author: raw
            .author
            .as_deref()
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .map(str::to_string),
        author_url: raw.author_url.as_deref().and_then(safe_shop_url),
        description_html: raw.description.as_deref().and_then(sanitize_html),
        free: raw.free.unwrap_or(false),
        category_ids,
        category_names,
        updated: raw.updated.as_ref().and_then(parse_timestamp),
        published: raw.published.as_ref().and_then(parse_timestamp),
        base,
        base_max,
        sale,
        sale_max,
        sale_window: (
            raw.sale_start.as_ref().and_then(parse_timestamp),
            raw.sale_end.as_ref().and_then(parse_timestamp),
        ),
        search_key,
    })
}

/// Tie a mod to the category tree, returning `(ids, names)`.
///
/// The catalog lists a mod's categories as `[{"name": "Tracks", "slug": "tracks"}, …]` —
/// **with no id** — so the slug is the only link back to the tree, which is where the ids,
/// parents and counts live. Matching on slug rather than name matters: names are display
/// text and can repeat or be re-worded, slugs are the stable key.
///
/// The id-shaped forms are still handled, so a future export that switches to ids doesn't
/// silently produce uncategorised items.
fn resolve_categories(value: Option<&Value>, cats: &CategoryIndex) -> (Vec<u64>, Vec<String>) {
    let mut ids: Vec<u64> = Vec::new();
    let mut names: Vec<String> = Vec::new();

    fn walk(
        value: &Value,
        cats: &CategoryIndex,
        ids: &mut Vec<u64>,
        names: &mut Vec<String>,
    ) {
        match value {
            Value::Array(items) => {
                for item in items {
                    walk(item, cats, ids, names);
                }
            }
            Value::Object(map) => {
                let id = map
                    .get("slug")
                    .and_then(Value::as_str)
                    .and_then(|s| cats.by_slug.get(&s.trim().to_ascii_lowercase()).copied())
                    .or_else(|| {
                        map.get("id")
                            .or_else(|| map.get("term_id"))
                            .and_then(scalar_category_id)
                    });
                // The name is taken from the tree when we matched it, so the label a mod
                // shows is the same one its filter pill shows.
                let name = id
                    .and_then(|id| cats.names.get(&id).cloned())
                    .or_else(|| {
                        map.get("name")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|n| !n.is_empty())
                            .map(str::to_string)
                    });
                if let Some(id) = id {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
                if let Some(name) = name {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
            other => {
                if let Some(id) = scalar_category_id(other) {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                    if let Some(name) = cats.names.get(&id) {
                        if !names.contains(name) {
                            names.push(name.clone());
                        }
                    }
                } else if let Value::String(s) = other {
                    // A bare slug, rather than a bare id.
                    let key = s.trim().to_ascii_lowercase();
                    if let Some(&id) = cats.by_slug.get(&key) {
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                        if let Some(name) = cats.names.get(&id) {
                            if !names.contains(name) {
                                names.push(name.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(value) = value {
        walk(value, cats, &mut ids, &mut names);
    }
    (ids, names)
}

fn scalar_category_id(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().parse::<u64>().ok(),
        _ => None,
    }
}

fn build_search_key(title: &str, author: Option<&str>, categories: &[String]) -> String {
    let mut key = String::with_capacity(title.len() + 32);
    key.push_str(title);
    if let Some(author) = author {
        key.push(' ');
        key.push_str(author);
    }
    for c in categories {
        key.push(' ');
        key.push_str(c);
    }
    normalize(&key)
}

/// Lowercase, and reduce anything that isn't alphanumeric to a space, so "Yamaha YZ-250"
/// matches a search for "yz 250".
fn normalize(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ───────────────────────────────── categories ─────────────────────────────────

/// Flatten whichever tree shape we were handed.
///
/// Handles the nested `children` form and the flat `parent`-linked form, because the dump's
/// shape isn't confirmed and both are ordinary ways to serialise a WordPress term tree. A
/// `parent` chain from third-party data can also be circular, so depth is bounded.
fn flatten_categories(raw: Vec<RawCategory>) -> Vec<ShopCategory> {
    let mut flat: Vec<ShopCategory> = Vec::new();
    let mut seen: HashSet<u64> = HashSet::new();

    // Nested form: recurse, carrying depth and parent down.
    fn walk(
        nodes: Vec<RawCategory>,
        parent: Option<u64>,
        depth: u8,
        out: &mut Vec<ShopCategory>,
        seen: &mut HashSet<u64>,
    ) {
        if depth > 8 {
            return;
        }
        for node in nodes {
            let Some(id) = node.id.as_ref().and_then(Num::as_u64) else {
                continue;
            };
            let explicit_parent = node.parent.as_ref().and_then(Num::as_u64).filter(|p| *p != 0);
            if seen.insert(id) {
                out.push(ShopCategory {
                    id,
                    name: node
                        .name
                        .as_deref()
                        .map(str::trim)
                        .filter(|n| !n.is_empty())
                        .unwrap_or("Uncategorised")
                        .to_string(),
                    slug: node.slug.clone().unwrap_or_default(),
                    parent: parent.or(explicit_parent),
                    depth,
                    count: 0,
                });
            }
            walk(node.children, Some(id), depth + 1, out, seen);
        }
    }
    walk(raw, None, 0, &mut flat, &mut seen);

    // Flat form: the walk above recorded `parent` but everything landed at depth 0. Recompute
    // depth by climbing, with a visited set so a cycle can't spin.
    let parents: HashMap<u64, Option<u64>> = flat.iter().map(|c| (c.id, c.parent)).collect();
    for cat in &mut flat {
        let mut depth = 0u8;
        let mut cursor = cat.parent;
        let mut visited: HashSet<u64> = HashSet::from([cat.id]);
        while let Some(p) = cursor {
            if !visited.insert(p) || depth >= 8 {
                break;
            }
            depth += 1;
            cursor = parents.get(&p).copied().flatten();
        }
        cat.depth = depth;
    }

    flat.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
    flat
}

fn build_index(flat: Vec<ShopCategory>) -> CategoryIndex {
    let names = flat.iter().map(|c| (c.id, c.name.clone())).collect();
    let by_slug: HashMap<String, u64> = flat
        .iter()
        .filter(|c| !c.slug.is_empty())
        .map(|c| (c.slug.to_ascii_lowercase(), c.id))
        .collect();
    let parents: HashMap<u64, Option<u64>> = flat.iter().map(|c| (c.id, c.parent)).collect();

    // A category selects itself plus everything under it. Built by walking each node's
    // ancestors — cheaper and simpler than materialising the tree, and cycle-safe.
    let mut descendants: HashMap<u64, HashSet<u64>> = HashMap::new();
    for cat in &flat {
        descendants.entry(cat.id).or_default().insert(cat.id);
        let mut cursor = cat.parent;
        let mut visited: HashSet<u64> = HashSet::from([cat.id]);
        while let Some(p) = cursor {
            if !visited.insert(p) {
                break;
            }
            descendants.entry(p).or_default().insert(cat.id);
            cursor = parents.get(&p).copied().flatten();
        }
    }

    CategoryIndex {
        flat,
        descendants,
        names,
        by_slug,
    }
}

/// The filter row must never be empty. When the dump has no usable tree, synthesise one
/// from the ids the items actually reference.
fn categories_from_items(items: &[Item]) -> Vec<ShopCategory> {
    let mut ids: Vec<u64> = items
        .iter()
        .flat_map(|i| i.category_ids.iter().copied())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    ids.sort_unstable();
    ids.into_iter()
        .map(|id| ShopCategory {
            id,
            name: format!("Category {id}"),
            slug: String::new(),
            parent: None,
            depth: 0,
            count: 0,
        })
        .collect()
}

// ───────────────────────────────── prices ─────────────────────────────────

/// Whether a discount is live *right now*.
///
/// `now` is a parameter rather than a call to the clock so the window logic is testable —
/// the interesting cases are all "what does this look like at time T".
///
/// A sale price with no window is taken at face value: the dump saying an item is discounted
/// is the only signal we have, and refusing to believe it would hide every real sale. That
/// is precisely why the refresh policy matters — with no window fields, freshness is the
/// only thing keeping a finished sale from being shown forever.
fn is_on_sale(sale: Option<f64>, window: (Option<i64>, Option<i64>), now: i64) -> bool {
    let Some(sale) = sale else {
        return false;
    };
    // `0.0` is a real sale price — the catalog carries 100%-off promotions, and rejecting
    // them here would quietly show the full price on exactly the best deals in the store.
    // Only a nonsensical negative is refused.
    if sale < 0.0 {
        return false;
    }
    match window {
        (Some(start), _) if now < start => false,
        (_, Some(end)) if now > end => false,
        _ => true,
    }
}

fn price_of(item: &Item, now: i64) -> ShopPrice {
    let on_sale = is_on_sale(item.sale, item.sale_window, now);
    // A `price_max` equal to `price` is a single-option item that happened to report both.
    let has_range = item
        .base_max
        .zip(item.base)
        .map(|(max, base)| max > base)
        .unwrap_or(false);

    let discount_pct = match (item.base, item.sale) {
        (Some(base), Some(sale)) if on_sale && base > 0.0 && sale < base => {
            Some(((base - sale) / base * 100.0).floor() as u32)
        }
        _ => None,
    };

    ShopPrice {
        free: item.free,
        base: item.base,
        base_max: has_range.then_some(item.base_max).flatten(),
        sale: on_sale.then_some(item.sale).flatten(),
        sale_max: (on_sale && has_range).then_some(item.sale_max).flatten(),
        on_sale,
        has_range,
        discount_pct,
        sale_ends: on_sale.then_some(item.sale_window.1).flatten(),
    }
}

/// What the item costs today — the sale price when one is live, otherwise the base. Used
/// for price sorting, so that sorting by price sorts by what a buyer would actually pay.
fn effective_price(item: &Item, now: i64) -> Option<f64> {
    if is_on_sale(item.sale, item.sale_window, now) {
        item.sale.or(item.base)
    } else {
        item.base
    }
}

fn summary_of(item: &Item, now: i64) -> ShopMod {
    ShopMod {
        id: item.id,
        title: item.title.clone(),
        url: item.url.clone(),
        image: item.image.clone(),
        author: item.author.clone(),
        author_url: item.author_url.clone(),
        category_ids: item.category_ids.clone(),
        category_names: item.category_names.clone(),
        updated: item.updated,
        price: price_of(item, now),
    }
}

// ───────────────────────────────── filter / sort ─────────────────────────────────

/// Ordering by ascending "missing sorts last", with a total tiebreak.
///
/// The tiebreak is not decoration. Without it, two items with equal keys can swap places
/// between one `search` call and the next, and page 2 then repeats or skips whatever moved
/// across the boundary — a bug that shows up as duplicated cards and no error anywhere.
fn cmp_opt<T: PartialOrd>(a: Option<T>, b: Option<T>) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn sort_items(indices: &mut [usize], items: &[Item], sort: ShopSort, now: i64) {
    use std::cmp::Ordering;
    indices.sort_by(|&a, &b| {
        let (x, y) = (&items[a], &items[b]);
        let primary = match sort {
            // Newest first, so the comparison is reversed.
            ShopSort::Newest => cmp_opt(y.published.or(y.updated), x.published.or(x.updated)),
            ShopSort::RecentlyUpdated => cmp_opt(y.updated.or(y.published), x.updated.or(x.published)),
            ShopSort::PriceAsc => cmp_opt(effective_price(x, now), effective_price(y, now)),
            ShopSort::PriceDesc => cmp_opt(effective_price(y, now), effective_price(x, now)),
            ShopSort::OnSale => {
                let (sx, sy) = (price_of(x, now), price_of(y, now));
                sy.on_sale
                    .cmp(&sx.on_sale)
                    .then_with(|| cmp_opt(sy.discount_pct, sx.discount_pct))
            }
            ShopSort::NameAsc => x.title.to_lowercase().cmp(&y.title.to_lowercase()),
        };
        // Every ordering ends here. See `cmp_opt`.
        primary.then(x.id.cmp(&y.id)).then(Ordering::Equal)
    });
}

/// The last filtered-and-sorted result, so "Load more" doesn't redo the whole scan.
///
/// Keyed on `generated_ts` as well as the query, so a background refresh invalidates it
/// without anyone having to remember to clear it.
type QueryKey = (String, Option<u64>, ShopSort, bool, Option<i64>);

fn memo() -> &'static Mutex<Option<(QueryKey, Arc<Vec<usize>>)>> {
    static MEMO: OnceLock<Mutex<Option<(QueryKey, Arc<Vec<usize>>)>>> = OnceLock::new();
    MEMO.get_or_init(|| Mutex::new(None))
}

/// Poison-tolerant, like [`super::mxb`]'s — a panic in one caller shouldn't disable browsing.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn select(
    catalog: &Catalog,
    index: &CategoryIndex,
    query: &str,
    category_id: Option<u64>,
    sort: ShopSort,
    on_sale_only: bool,
    now: i64,
) -> Arc<Vec<usize>> {
    let normalized = normalize(query);
    let key: QueryKey = (
        normalized.clone(),
        category_id,
        sort,
        on_sale_only,
        catalog.generated_ts,
    );
    if let Some((cached_key, cached)) = lock(memo()).as_ref() {
        if *cached_key == key {
            return cached.clone();
        }
    }

    let wanted: Option<&HashSet<u64>> = category_id.and_then(|id| index.descendants.get(&id));
    let terms: Vec<&str> = normalized.split_whitespace().collect();

    let mut indices: Vec<usize> = catalog
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            // Every term must match, so "yamaha 250" narrows rather than widens.
            terms.iter().all(|t| item.search_key.contains(t))
                && wanted
                    .map(|set| item.category_ids.iter().any(|c| set.contains(c)))
                    .unwrap_or(true)
                && (!on_sale_only || is_on_sale(item.sale, item.sale_window, now))
        })
        .map(|(i, _)| i)
        .collect();

    sort_items(&mut indices, &catalog.items, sort, now);

    let result = Arc::new(indices);
    *lock(memo()) = Some((key, result.clone()));
    result
}

// ───────────────────────────────── state ─────────────────────────────────

fn state() -> &'static Mutex<Option<Arc<(Catalog, CategoryIndex)>>> {
    static STATE: OnceLock<Mutex<Option<Arc<(Catalog, CategoryIndex)>>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

/// The last refresh failure, kept so a served-from-disk catalog can say why it's old.
fn last_error() -> &'static Mutex<Option<String>> {
    static ERR: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    ERR.get_or_init(|| Mutex::new(None))
}

fn loaded() -> Option<Arc<(Catalog, CategoryIndex)>> {
    lock(state()).clone()
}

fn install(catalog: Catalog) -> Arc<(Catalog, CategoryIndex)> {
    let arc = Arc::new(build(catalog));
    *lock(state()) = Some(arc.clone());
    *lock(memo()) = None;
    arc
}

/// Index a parsed catalog: resolve the category tree, then count each category's items
/// including its descendants — because that's what picking the category selects, and a
/// count that disagreed with the filter would just look broken.
///
/// Separate from [`install`] so it can be exercised without touching global state.
fn build(catalog: Catalog) -> (Catalog, CategoryIndex) {
    let index = if catalog.categories.is_empty() {
        build_index(categories_from_items(&catalog.items))
    } else {
        build_index(catalog.categories.clone())
    };

    let mut counts: HashMap<u64, u32> = HashMap::new();
    for item in &catalog.items {
        // An item in both "250cc" and "Bikes" must count once towards "Bikes", not twice.
        let mut counted: HashSet<u64> = HashSet::new();
        for cid in &item.category_ids {
            for (ancestor, set) in &index.descendants {
                if set.contains(cid) && counted.insert(*ancestor) {
                    *counts.entry(*ancestor).or_default() += 1;
                }
            }
        }
    }

    let mut index = index;
    for cat in &mut index.flat {
        cat.count = counts.get(&cat.id).copied().unwrap_or(0);
    }

    let mut catalog = catalog;
    catalog.categories = index.flat.clone();
    (catalog, index)
}

// ───────────────────────────────── disk cache ─────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct CacheMeta {
    fetched_at: i64,
    generated_ts: Option<i64>,
    etag: Option<String>,
    last_modified: Option<String>,
}

fn cache_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;
    let root = app.path().app_cache_dir().ok()?;
    Some(root.join(CACHE_DIR))
}

fn read_cache(app: &tauri::AppHandle) -> Option<(String, CacheMeta)> {
    let dir = cache_dir(app)?;
    let body = std::fs::read_to_string(dir.join("dump.json")).ok()?;
    let meta: CacheMeta = std::fs::read_to_string(dir.join("meta.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    Some((body, meta))
}

/// The raw bytes are kept verbatim, not the parsed form. A future correction to [`map_mod`]
/// can then be applied to the catalog already on disk instead of needing the network.
fn write_cache(app: &tauri::AppHandle, body: &str, meta: &CacheMeta) {
    let Some(dir) = cache_dir(app) else { return };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("could not create the shop catalog cache dir: {e}");
        return;
    }
    if let Err(e) = std::fs::write(dir.join("dump.json"), body) {
        log::warn!("could not cache the shop catalog: {e}");
        return;
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(meta) {
        let _ = std::fs::write(dir.join("meta.json"), bytes);
    }
}

// ───────────────────────────────── parsing ─────────────────────────────────

fn parse_dump(body: &str, fetched_at: i64) -> anyhow::Result<Catalog> {
    let raw: RawDump = serde_json::from_str(body)
        .map_err(|e| anyhow::anyhow!("the shop catalog wasn't readable JSON: {e}"))?;

    let index = build_index(flatten_categories(raw.categories));
    let total = raw.mods.len();
    let items: Vec<Item> = raw
        .mods
        .into_iter()
        .filter_map(|m| map_mod(m, &index))
        .collect();

    // One line, not one per item — a malformed catalog shouldn't flood the log.
    if items.len() < total {
        log::warn!(
            "shop catalog: skipped {} of {total} items with neither an id nor a title",
            total - items.len()
        );
    }

    let generated_ts = raw
        .generated_ts
        .or_else(|| raw.generated.as_deref().and_then(parse_date_str));

    Ok(Catalog {
        items,
        categories: index.flat,
        currency: raw
            .currency
            .filter(|c| !c.trim().is_empty())
            .unwrap_or_else(|| "USD".to_string()),
        generated_ts,
        fetched_at,
        etag: None,
        last_modified: None,
    })
}

// ───────────────────────────────── fetching ─────────────────────────────────

/// The two shapes a Cloudflare refusal takes on this site.
///
/// The endpoint currently returns the *firewall* page ("Sorry, you have been blocked"),
/// which is a WAF rule rather than a challenge — a `cf_clearance` may not clear it at all.
/// The JS interstitial is the other shape, and is the one the handshake exists for.
///
/// Deliberately not keyed on `challenge-platform`: Cloudflare injects that script into
/// ordinary pages too, so matching it would condemn perfectly good responses. Same reasoning
/// as [`super::mxb::is_challenge`].
fn is_block_page(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("cf-browser-verification")
        || lower.contains("cf_chl_opt")
        || lower.contains("<title>just a moment")
        || lower.contains("attention required!")
        || lower.contains("sorry, you have been blocked")
}

fn blocked_error(status: Option<u16>) -> anyhow::Error {
    let message = match status {
        Some(401) | Some(403) => {
            "mxbikes-shop.com refused the request. If this keeps happening the store may need \
             to allow this app through, or the app's access may need renewing — please report it."
        }
        Some(429) => "mxbikes-shop.com is rate-limiting us. Give it a minute, then hit Retry.",
        Some(503) => "mxbikes-shop.com is unavailable right now. Try again shortly.",
        _ => {
            "mxbikes-shop.com served a Cloudflare check instead of the catalog, and its check \
             window didn't clear it. Open mxbikes-shop.com in your normal browser, then hit Retry."
        }
    };
    anyhow::Error::new(Blocked::new(status, message))
}

struct Fetched {
    body: Option<String>,
    etag: Option<String>,
    last_modified: Option<String>,
}

/// One conditional GET. `body: None` means a 304 — what we hold is already current.
async fn fetch(conditional: Option<(&Option<String>, &Option<String>)>) -> anyhow::Result<Fetched> {
    let mut req = client()?.get(CATALOG_URL);
    if let Some((etag, last_modified)) = conditional {
        if let Some(etag) = etag {
            req = req.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        if let Some(lm) = last_modified {
            req = req.header(reqwest::header::IF_MODIFIED_SINCE, lm);
        }
    }
    let resp = shop_credentials::apply(req).send().await?;

    let status = resp.status();
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(Fetched {
            body: None,
            etag: header(&resp, reqwest::header::ETAG),
            last_modified: header(&resp, reqwest::header::LAST_MODIFIED),
        });
    }

    let etag = header(&resp, reqwest::header::ETAG);
    let last_modified = header(&resp, reqwest::header::LAST_MODIFIED);

    if !status.is_success() {
        // The body tells us whether Cloudflare or the store said no; either way this is a
        // `Blocked` so the command layer can decide whether a handshake is worth trying.
        let body = resp.text().await.unwrap_or_default();
        if is_block_page(&body) || matches!(status.as_u16(), 401 | 403 | 429 | 503) {
            return Err(blocked_error(Some(status.as_u16())));
        }
        return Err(anyhow::anyhow!("mxbikes-shop.com returned {status}"));
    }

    let body = resp.text().await?;
    // A challenge served with a 200 would otherwise parse as "not JSON", which is a much
    // worse thing to tell a user than "we were blocked".
    if is_block_page(&body) {
        return Err(blocked_error(None));
    }
    Ok(Fetched {
        body: Some(body),
        etag,
        last_modified,
    })
}

fn header(resp: &reqwest::Response, name: reqwest::header::HeaderName) -> Option<String> {
    resp.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// Only one refresh runs at a time — several commands arriving together must not each pull
/// a multi-megabyte document.
fn refresh_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Returns the catalog now in force, and whether it differs from what we held. Callers use
/// the flag to decide whether the UI has anything worth hearing about.
async fn refresh(
    app: &tauri::AppHandle,
    force: bool,
) -> anyhow::Result<(Arc<(Catalog, CategoryIndex)>, bool)> {
    let _guard = refresh_lock().lock().await;

    // Another caller may have refreshed while we waited on the lock.
    if !force {
        if let Some(current) = loaded() {
            if !current.0.stale() {
                return Ok((current, false));
            }
        }
    }

    let current = loaded();
    // A forced refresh sends no conditional headers: "Refresh" must mean refresh, not
    // "ask politely and accept a 304".
    let conditional = match (force, current.as_ref()) {
        (false, Some(c)) => Some((&c.0.etag, &c.0.last_modified)),
        _ => None,
    };

    let fetched = match fetch(conditional).await {
        Ok(f) => f,
        Err(e) => {
            *lock(last_error()) = Some(format!("{e}"));
            return Err(e);
        }
    };

    let Some(body) = fetched.body else {
        // 304: what we hold is current, so restamp it rather than refetching.
        *lock(last_error()) = None;
        if let Some(current) = current {
            let mut catalog = clone_catalog(&current.0);
            catalog.fetched_at = now();
            catalog.etag = fetched.etag.or(catalog.etag);
            catalog.last_modified = fetched.last_modified.or(catalog.last_modified);
            write_cache(
                app,
                "",
                &CacheMeta {
                    fetched_at: catalog.fetched_at,
                    generated_ts: catalog.generated_ts,
                    etag: catalog.etag.clone(),
                    last_modified: catalog.last_modified.clone(),
                },
            );
            // A 304 restamps freshness without changing a single price.
            return Ok((install(catalog), false));
        }
        return Err(anyhow::anyhow!(
            "mxbikes-shop.com said the catalog was unchanged, but we don't have a copy of it"
        ));
    };

    let mut catalog = parse_dump(&body, now())?;
    catalog.etag = fetched.etag;
    catalog.last_modified = fetched.last_modified;

    // A CDN edge can serve a copy older than one we already have. Adopting it would roll
    // prices backwards, so keep what we've got. Equal timestamps are still adopted — that
    // costs nothing and covers a same-second regeneration.
    if let (Some(current), Some(new_ts)) = (current.as_ref(), catalog.generated_ts) {
        if let Some(old_ts) = current.0.generated_ts {
            if new_ts < old_ts {
                log::warn!(
                    "shop catalog: ignoring a copy generated at {new_ts}, older than the {old_ts} we hold"
                );
                *lock(last_error()) = None;
                return Ok((current.clone(), false));
            }
        }
    }

    write_cache(
        app,
        &body,
        &CacheMeta {
            fetched_at: catalog.fetched_at,
            generated_ts: catalog.generated_ts,
            etag: catalog.etag.clone(),
            last_modified: catalog.last_modified.clone(),
        },
    );
    let changed = current
        .as_ref()
        .map(|c| c.0.generated_ts != catalog.generated_ts)
        .unwrap_or(true);
    *lock(last_error()) = None;
    Ok((install(catalog), changed))
}

/// `Catalog` isn't `Clone` (it holds the parsed items), and only the 304 path needs a copy.
fn clone_catalog(c: &Catalog) -> Catalog {
    Catalog {
        items: c.items.clone(),
        categories: c.categories.clone(),
        currency: c.currency.clone(),
        generated_ts: c.generated_ts,
        fetched_at: c.fetched_at,
        etag: c.etag.clone(),
        last_modified: c.last_modified.clone(),
    }
}

/// Memory, then disk, then the network — and once something is loaded, serve it immediately
/// and refresh behind the user's back.
///
/// The grid should never wait on a network round-trip it doesn't need. A stale-but-present
/// catalog paints instantly and quietly corrects itself; only a completely cold start with
/// no cache is allowed to block.
async fn ensure_loaded(app: &tauri::AppHandle) -> anyhow::Result<Arc<(Catalog, CategoryIndex)>> {
    if let Some(current) = loaded() {
        if current.0.stale() {
            spawn_background_refresh(app);
        }
        return Ok(current);
    }

    if let Some((body, meta)) = read_cache(app) {
        if !body.is_empty() {
            match parse_dump(&body, meta.fetched_at) {
                Ok(mut catalog) => {
                    catalog.etag = meta.etag;
                    catalog.last_modified = meta.last_modified;
                    let arc = install(catalog);
                    if arc.0.stale() {
                        spawn_background_refresh(app);
                    }
                    return Ok(arc);
                }
                Err(e) => log::warn!("shop catalog: cached copy was unusable ({e}); refetching"),
            }
        }
    }

    Ok(refresh(app, false).await?.0)
}

/// Don't retry a failing background refresh more often than this.
///
/// Without it, a user who is offline with a stale cache would fire a fetch on every
/// debounced keystroke: `ensure_loaded` runs per search, the catalog stays stale because the
/// refresh keeps failing, so every single search tries again.
const BACKGROUND_RETRY: Duration = Duration::from_secs(60);

fn last_attempt() -> &'static Mutex<Option<std::time::Instant>> {
    static AT: OnceLock<Mutex<Option<std::time::Instant>>> = OnceLock::new();
    AT.get_or_init(|| Mutex::new(None))
}

fn spawn_background_refresh(app: &tauri::AppHandle) {
    use tauri::Emitter;
    {
        let mut at = lock(last_attempt());
        if at.map(|t| t.elapsed() < BACKGROUND_RETRY).unwrap_or(false) {
            return;
        }
        *at = Some(std::time::Instant::now());
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match refresh(&app, false).await {
            // Only announce a catalog that actually changed. Emitting on a no-op makes the
            // grid re-run its search and re-render for nothing.
            Ok((_, true)) => {
                // The grid re-runs its search on this, so prices correct themselves without
                // the user doing anything.
                let _ = app.emit("shop-catalog-updated", status(&app));
            }
            Ok((_, false)) => {}
            Err(e) => log::warn!("shop catalog: background refresh failed: {e}"),
        }
    });
}

// ───────────────────────────────── public API ─────────────────────────────────

pub fn available() -> bool {
    shop_credentials::present()
}

pub fn status(app: &tauri::AppHandle) -> ShopStatus {
    let _ = app;
    let current = loaded();
    match current {
        Some(c) => ShopStatus {
            available: available(),
            count: c.0.items.len() as u32,
            currency: c.0.currency.clone(),
            generated_ts: c.0.generated_ts,
            fetched_at: Some(c.0.fetched_at),
            stale: c.0.stale(),
            very_stale: c.0.very_stale(),
            error: lock(last_error()).clone(),
        },
        None => ShopStatus {
            available: available(),
            count: 0,
            currency: "USD".to_string(),
            generated_ts: None,
            fetched_at: None,
            stale: false,
            very_stale: false,
            error: lock(last_error()).clone(),
        },
    }
}

pub async fn categories(app: &tauri::AppHandle) -> anyhow::Result<Vec<ShopCategory>> {
    Ok(ensure_loaded(app).await?.0.categories.clone())
}

pub async fn search(
    app: &tauri::AppHandle,
    query: &str,
    category_id: Option<u64>,
    page: u32,
    sort: ShopSort,
    on_sale_only: bool,
) -> anyhow::Result<ShopPage> {
    let current = ensure_loaded(app).await?;
    let (catalog, index) = (&current.0, &current.1);
    let now = now();

    let selected = select(catalog, index, query, category_id, sort, on_sale_only, now);
    let start = (page.max(1) as usize - 1) * PER_PAGE;
    let items: Vec<ShopMod> = selected
        .iter()
        .skip(start)
        .take(PER_PAGE)
        .map(|&i| summary_of(&catalog.items[i], now))
        .collect();

    Ok(ShopPage {
        has_more: start + items.len() < selected.len(),
        total: selected.len() as u32,
        items,
        currency: catalog.currency.clone(),
        generated_ts: catalog.generated_ts,
        stale: catalog.stale(),
    })
}

/// Served from the same in-memory list as the grid, so opening an item costs nothing and
/// works with the network down.
pub async fn detail(app: &tauri::AppHandle, id: u64) -> anyhow::Result<ShopModDetail> {
    let current = ensure_loaded(app).await?;
    let now = now();
    let item = current
        .0
        .items
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| anyhow::anyhow!("that item is no longer in the shop catalog"))?;

    Ok(ShopModDetail {
        item: summary_of(item, now),
        description_html: item.description_html.clone(),
        images: if item.images.is_empty() {
            item.image.clone().into_iter().collect()
        } else {
            item.images.clone()
        },
    })
}

pub async fn force_refresh(app: &tauri::AppHandle) -> anyhow::Result<ShopStatus> {
    refresh(app, true).await?;
    Ok(status(app))
}

#[cfg(test)]
mod tests;
