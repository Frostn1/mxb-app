//! MXB Hub's public catalog — `shop.mxb-hub.com`, over the WooCommerce Store API.
//!
//! The third catalog in the app, and by some distance the easiest of the three.
//!
//!  - **mxb-mods.com** ([`super::mxb`]) is scraped and Cloudflare-fronted, and its REST API
//!    accepts `orderby` and then ignores it.
//!  - **mxbikes-shop.com** ([`super::shop_catalog`]) has no public API at all, so it is served
//!    from a credentialed JSON dump refreshed in the background.
//!  - **MXB Hub** publishes `/wp-json/wc/store/v1/`, which is WooCommerce's own read-only
//!    storefront API: unauthenticated, paged with `X-WP-Total`, and honouring `search`,
//!    `category`, `on_sale` and `orderby` for real. Measured 2026-08-30: no Cloudflare
//!    (`server: nginx`), so plain `reqwest` is enough and nothing here needs a WebView.
//!
//! So this queries live rather than caching a dump. There is no credential to hide, no
//! staleness to report, and a search is one request.
//!
//! Prices come back in **minor units** — `"400"` with `currency_minor_unit: 2` is $4.00 — and
//! are converted here, once, so nothing downstream has to remember that.

use super::shop_catalog::{parse_date_str, safe_image_url, sanitize_html, ShopPrice};
use super::Blocked;
use crate::cookie_session;
use crate::hub_session::{HUB_BASE, HUB_SITE};
use reqwest::cookie::Jar;
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Matches `HUB_PAGE_SIZE` in `src/api/hub.ts`.
pub const PER_PAGE: u32 = 24;

// ───────────────────────────────── what we hand out ─────────────────────────────────

/// One product as the grid shows it.
///
/// Field-for-field the shape of [`super::shop_catalog::ShopMod`] where the two overlap, and
/// deliberately so: it lets the Hub grid reuse `PriceTag` and the money formatting rather than
/// growing a second, subtly different way to render a price.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubMod {
    pub id: u64,
    pub slug: String,
    pub title: String,
    /// The product page, or `None` when the store handed us a URL we won't pass to the OS —
    /// see [`safe_hub_url`]. `None` hides the Buy button.
    pub url: Option<String>,
    pub image: Option<String>,
    /// The creator, where the product sits under one. MXB Hub is organised by creator — a
    /// whole top-level category tree of them — so this is real attribution taken from the
    /// product's own categories, not a placeholder. See [`fill_authors`].
    pub author: Option<String>,
    pub author_url: Option<String>,
    pub category_ids: Vec<u64>,
    pub category_names: Vec<String>,
    /// Unix seconds. The Store API carries no date, so this is filled in from `wp/v2` — see
    /// [`fill_dates`] — and stays `None` when that lookup fails.
    pub updated: Option<i64>,
    pub price: ShopPrice,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubModDetail {
    #[serde(flatten)]
    pub item: HubMod,
    /// Sanitised in Rust — see [`sanitize_html`]. Rendered with `dangerouslySetInnerHTML`.
    pub description_html: Option<String>,
    pub images: Vec<String>,
    /// What the store says this product ships, verbatim from its short description. Useful
    /// because Hub listings say "in-game ready PKZ" or "PSD" there and nowhere else.
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubCategory {
    pub id: u64,
    pub name: String,
    pub slug: String,
    pub parent: Option<u64>,
    /// The category's page on the store. Doubles as the creator's page for the rows under
    /// [`CREATORS_SLUG`].
    pub link: Option<String>,
    /// 0 for top level, so the UI can indent without walking the tree.
    pub depth: u8,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubPage {
    pub items: Vec<HubMod>,
    pub total: u32,
    pub has_more: bool,
    pub currency: String,
}

/// The orders the Store API actually honours.
///
/// Checked against the live API rather than assumed: `date`, `price`, `popularity`, `title` and
/// `menu_order` all work, and `relevance` — which WooCommerce documents — is rejected with
/// `rest_invalid_param` on this install. Sorting a search by relevance is therefore not on
/// offer, and asking for it would 400 the whole page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HubSort {
    #[default]
    Newest,
    Popular,
    PriceAsc,
    PriceDesc,
    NameAsc,
}

impl HubSort {
    fn params(self) -> (&'static str, &'static str) {
        match self {
            HubSort::Newest => ("date", "desc"),
            HubSort::Popular => ("popularity", "desc"),
            HubSort::PriceAsc => ("price", "asc"),
            HubSort::PriceDesc => ("price", "desc"),
            HubSort::NameAsc => ("title", "asc"),
        }
    }
}

// ───────────────────────────────── what the API sends ─────────────────────────────────

#[derive(Debug, Deserialize)]
struct ApiProduct {
    id: u64,
    name: String,
    slug: String,
    permalink: String,
    #[serde(default)]
    short_description: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    prices: ApiPrices,
    #[serde(default)]
    images: Vec<ApiImage>,
    #[serde(default)]
    categories: Vec<ApiCategoryRef>,
    #[serde(default)]
    on_sale: bool,
}

/// Every figure is a string of *minor units* — `"1999"` is 19.99 at `currency_minor_unit: 2`.
/// `price_range` is `null` for a simple product and an object for a variable one; the Hub sells
/// only simple products today, but a bundle listed as variable would otherwise print its
/// cheapest option as if it were the price.
#[derive(Debug, Default, Deserialize)]
struct ApiPrices {
    #[serde(default)]
    price: String,
    #[serde(default)]
    regular_price: String,
    #[serde(default)]
    sale_price: String,
    #[serde(default)]
    price_range: Option<ApiPriceRange>,
    #[serde(default)]
    currency_code: String,
    #[serde(default = "two")]
    currency_minor_unit: u32,
}

fn two() -> u32 {
    2
}

#[derive(Debug, Deserialize)]
struct ApiPriceRange {
    #[serde(default)]
    min_amount: String,
    #[serde(default)]
    max_amount: String,
}

#[derive(Debug, Deserialize)]
struct ApiImage {
    #[serde(default)]
    src: String,
    #[serde(default)]
    thumbnail: String,
}

#[derive(Debug, Deserialize)]
struct ApiCategoryRef {
    id: u64,
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct ApiCategory {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    parent: u64,
    #[serde(default)]
    count: u32,
    #[serde(default)]
    permalink: String,
}

/// The `wp/v2` half of a listing: the only place a product's dates are published.
#[derive(Debug, Deserialize)]
struct ApiDates {
    id: u64,
    #[serde(default)]
    date_gmt: Option<String>,
    #[serde(default)]
    modified_gmt: Option<String>,
}

// ───────────────────────────────── the client ─────────────────────────────────

/// The clearance jar.
///
/// Holds whatever cookies [`crate::hub_clearance`] earned in a real browser, and nothing else —
/// this is emphatically *not* the signed-in jar. Keeping the two apart is what lets a catalog
/// request stay anonymous: browsing must work whether or not anyone has ever signed in, and a
/// public listing has no business carrying the user's session. The account half lives in
/// [`crate::hub_session`], which gets its own copy of the same clearance.
pub(crate) fn jar() -> &'static Arc<Jar> {
    static JAR: std::sync::OnceLock<Arc<Jar>> = std::sync::OnceLock::new();
    JAR.get_or_init(|| Arc::new(Jar::default()))
}

/// Take on a clearance earned in the WebView, so this client stops being challenged.
pub fn adopt_clearance(cookies: &[(String, String)]) -> anyhow::Result<()> {
    cookie_session::fill(jar(), &HUB_SITE, cookies)
}

/// One client for the session — connection reuse matters most for the search box, where a fast
/// typist would otherwise pay a TLS handshake per debounced keystroke.
///
/// Public because the image cache fetches this store's thumbnails with it: they are challenged
/// by exactly the same filter as the API, so serving them through any other client means a
/// grid of blank cards the moment the store decides we are a robot.
pub fn client() -> anyhow::Result<&'static Client> {
    static CLIENT: std::sync::OnceLock<Result<Client, String>> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            cookie_session::client_builder(&HUB_SITE, jar().clone())
                .build()
                .map_err(|e| format!("{e:#}"))
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Whether this response is SiteGround's robot challenge rather than what we asked for.
///
/// The store is on SiteGround, whose bot protection fires on **request rate** — which is why
/// it is invisible to a few hand probes and perfectly reproducible the moment a grid asks for
/// twenty-four thumbnails at once. What comes back is a `202` carrying an HTML "Robot
/// Challenge Screen" that computes a proof of work in a Web Worker, so no HTTP client can
/// answer it; only a real browser can. Detected here so it surfaces as something the command
/// layer can act on, rather than as "expected value at line 1 column 1".
///
/// Deliberately checked on the *response*, before the body is parsed: a challenge served in
/// place of an image is a 202 full of HTML, and caching that would poison the thumbnail cache
/// with a page.
pub fn challenged(resp: &Response) -> bool {
    if resp.headers().contains_key("sg-captcha") {
        return true;
    }
    // A 202 for a GET is already odd; a 202 of HTML where JSON or an image was asked for is
    // the challenge. Status alone is not enough — the header above is the reliable marker, and
    // this is the belt to its braces.
    let html = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/html"));
    resp.status().as_u16() == 202 && html
}

/// The challenge as an error the command layer knows to run the handshake for.
///
/// `None` for the status, which is what [`Blocked::clearable`] reads as "a browser could fix
/// this" — and here it genuinely can, because the challenge is solved by running its script.
fn blocked() -> anyhow::Error {
    Blocked::new(
        None,
        "MXB Hub is asking the app to prove it isn't a robot.",
    )
    .into()
}

fn store_api(path: &str) -> String {
    format!("{HUB_BASE}/wp-json/wc/store/v1/{path}")
}

// ───────────────────────────────── search ─────────────────────────────────

pub async fn search(
    query: &str,
    category_id: Option<u64>,
    page: u32,
    sort: HubSort,
    on_sale_only: bool,
) -> anyhow::Result<HubPage> {
    let page = page.max(1);
    let (orderby, order) = sort.params();
    let mut req = client()?
        .get(store_api("products"))
        .query(&[("per_page", PER_PAGE.to_string()), ("page", page.to_string())])
        .query(&[("orderby", orderby), ("order", order)]);

    let query = query.trim();
    if !query.is_empty() {
        req = req.query(&[("search", query)]);
    }
    if let Some(id) = category_id {
        req = req.query(&[("category", id.to_string())]);
    }
    if on_sale_only {
        req = req.query(&[("on_sale", "true")]);
    }

    let resp = req.send().await?;
    if challenged(&resp) {
        return Err(blocked());
    }
    let status = resp.status();
    // Past the last page WooCommerce answers 400 `rest_post_invalid_page_number` rather than an
    // empty list. That is the end of the grid, not an error to put on screen.
    if status.as_u16() == 400 {
        return Ok(HubPage {
            items: vec![],
            total: 0,
            has_more: false,
            currency: "USD".into(),
        });
    }
    if !status.is_success() {
        anyhow::bail!("MXB Hub answered {status} for the catalog");
    }

    let total = header_u32(&resp, "x-wp-total").unwrap_or(0);
    let total_pages = header_u32(&resp, "x-wp-totalpages").unwrap_or(0);
    let products: Vec<ApiProduct> = resp.json().await?;

    let currency = products
        .first()
        .map(|p| p.prices.currency_code.clone())
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| "USD".into());

    let mut items: Vec<HubMod> = products.iter().map(map_product).collect();
    // Deliberately no `fill_dates` here. A card shows a title, a price and a creator — the
    // date appears only on the detail page — so paying a second request per page of results
    // for a field nobody sees is a third of this view's traffic spent on nothing. It matters
    // more than it sounds: this store counts requests, and answers too many by refusing the
    // whole site. `detail` still fills it, for the one item that displays it.
    fill_authors(&mut items).await;

    Ok(HubPage {
        items,
        total,
        has_more: page < total_pages,
        currency,
    })
}

pub async fn detail(id: u64) -> anyhow::Result<HubModDetail> {
    let resp = client()?
        .get(store_api(&format!("products/{id}")))
        .send()
        .await?;
    if challenged(&resp) {
        return Err(blocked());
    }
    if !resp.status().is_success() {
        anyhow::bail!("MXB Hub answered {} for product {id}", resp.status());
    }
    let product: ApiProduct = resp.json().await?;

    let mut item = map_product(&product);
    fill_dates(std::slice::from_mut(&mut item)).await;
    fill_authors(std::slice::from_mut(&mut item)).await;

    // Every image the listing carries, largest form first, deduplicated — a Hub product often
    // repeats its hero shot as the gallery's first entry.
    let mut images: Vec<String> = Vec::new();
    for img in &product.images {
        let Some(url) = safe_image_url(&img.src).or_else(|| safe_image_url(&img.thumbnail)) else {
            continue;
        };
        if !images.contains(&url) {
            images.push(url);
        }
    }

    Ok(HubModDetail {
        item,
        description_html: sanitize_html(&product.description),
        images,
        summary: sanitize_html(&product.short_description),
    })
}

/// Catalog entries for a set of product slugs, in one request.
///
/// The Store API takes `slug` as a comma-separated list, so a whole purchases grid resolves in
/// a single round trip and on an exact match — no name-similarity fold like the shop's, which
/// exists only because that store's purchases page gives nothing else to key on. Slugs it does
/// not know are simply absent from the answer.
pub async fn by_slugs(slugs: &[String]) -> anyhow::Result<Vec<HubMod>> {
    if slugs.is_empty() {
        return Ok(vec![]);
    }
    let resp = client()?
        .get(store_api("products"))
        .query(&[
            ("slug", slugs.join(",")),
            ("per_page", "100".to_string()),
        ])
        .send()
        .await?;
    if challenged(&resp) {
        return Err(blocked());
    }
    if !resp.status().is_success() {
        anyhow::bail!("MXB Hub answered {} for a product lookup", resp.status());
    }
    let products: Vec<ApiProduct> = resp.json().await?;
    let mut items: Vec<HubMod> = products.iter().map(map_product).collect();
    fill_authors(&mut items).await;
    Ok(items)
}

/// The category tree, flattened depth-first with a `depth` on each row.
///
/// Cached for the session with a short TTL: the list is 99 rows that change when a creator is
/// added, and the filter row asks for it on every mount.
pub async fn categories() -> anyhow::Result<Vec<HubCategory>> {
    const TTL: Duration = Duration::from_secs(10 * 60);
    static CACHE: Mutex<Option<(Instant, Vec<HubCategory>)>> = Mutex::new(None);

    if let Some((at, cached)) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if at.elapsed() < TTL {
            return Ok(cached.clone());
        }
    }

    let resp = client()?
        .get(store_api("products/categories"))
        .query(&[("per_page", "100")])
        .send()
        .await?;
    if challenged(&resp) {
        return Err(blocked());
    }
    if !resp.status().is_success() {
        anyhow::bail!("MXB Hub answered {} for the categories", resp.status());
    }
    let raw: Vec<ApiCategory> = resp.json().await?;
    let tree = flatten(&raw);

    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), tree.clone()));
    Ok(tree)
}

// ───────────────────────────────── mapping ─────────────────────────────────

fn map_product(p: &ApiProduct) -> HubMod {
    HubMod {
        id: p.id,
        slug: p.slug.clone(),
        title: decode(&p.name),
        url: safe_hub_url(&p.permalink),
        image: p
            .images
            .first()
            .and_then(|i| safe_image_url(&i.thumbnail).or_else(|| safe_image_url(&i.src))),
        author: None,
        author_url: None,
        category_ids: p.categories.iter().map(|c| c.id).collect(),
        category_names: p.categories.iter().map(|c| decode(&c.name)).collect(),
        updated: None,
        price: map_price(&p.prices, p.on_sale),
    }
}

/// Minor units to a real number, e.g. `("1999", 2)` → `19.99`.
fn amount(raw: &str, minor_unit: u32) -> Option<f64> {
    let value: f64 = raw.trim().parse().ok()?;
    Some(value / 10f64.powi(minor_unit as i32))
}

fn map_price(prices: &ApiPrices, on_sale: bool) -> ShopPrice {
    let unit = prices.currency_minor_unit;
    let regular = amount(&prices.regular_price, unit);
    let sale = amount(&prices.sale_price, unit);
    let live = amount(&prices.price, unit);

    // A variable product prices as a span. Both ends have to travel together or the struck-out
    // figure stops corresponding to the live one — see `PriceTag`.
    let (base_max, has_range) = match prices.price_range.as_ref() {
        Some(range) => (amount(&range.max_amount, unit), true),
        None => (None, false),
    };
    let base = match prices.price_range.as_ref() {
        Some(range) => amount(&range.min_amount, unit).or(regular),
        None => regular.or(live),
    };

    // `on_sale` is the store's own flag; a sale price equal to the regular one is not a sale.
    let on_sale = on_sale && sale.is_some() && sale != regular;
    let discount_pct = match (on_sale, base, sale) {
        (true, Some(b), Some(s)) if b > 0.0 && s < b => Some((((b - s) / b) * 100.0) as u32),
        _ => None,
    };

    ShopPrice {
        base,
        base_max,
        sale: on_sale.then_some(sale).flatten(),
        sale_max: None,
        on_sale,
        has_range,
        // WooCommerce has no pay-what-you-want price, so free here really is a price of zero —
        // which is what the store's own Free Mods category is made of.
        free: live == Some(0.0) && !on_sale,
        discount_pct,
        // The Store API publishes no sale window, and the UI must never invent one.
        sale_ends: None,
    }
}

/// Fill in `updated` from `wp/v2/product`, which is the only endpoint that publishes dates.
///
/// One extra request per page of 24, asking for three fields — not per item. Best-effort by
/// design: a failure leaves every `updated` at `None`, the cards simply show no date, and the
/// grid still renders. Never worth failing a search over.
async fn fill_dates(items: &mut [HubMod]) {
    if items.is_empty() {
        return;
    }
    let ids = items
        .iter()
        .map(|i| i.id.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let fetched = async {
        let resp = client()
            .ok()?
            .get(format!("{HUB_BASE}/wp-json/wp/v2/product"))
            .query(&[
                ("include", ids.as_str()),
                ("_fields", "id,date_gmt,modified_gmt"),
                ("per_page", "100"),
            ])
            .send()
            .await
            .ok()?;
        if challenged(&resp) {
            return None;
        }
        resp.json::<Vec<ApiDates>>().await.ok()
    }
    .await;

    let Some(dates) = fetched else {
        log::debug!("MXB Hub dates unavailable for this page; cards will show none");
        return;
    };
    for row in dates {
        let stamp = row
            .modified_gmt
            .as_deref()
            .or(row.date_gmt.as_deref())
            .and_then(parse_wp_date);
        if let Some(item) = items.iter_mut().find(|i| i.id == row.id) {
            item.updated = stamp;
        }
    }
}

/// The top-level category every creator's own category hangs off.
const CREATORS_SLUG: &str = "creators";

/// Attribute each item to its creator, from the category tree.
///
/// MXB Hub files a product under both what it is ("Helmet") and who made it ("CSTAR"), the
/// latter as a child of a `creators` root. That makes the creator recoverable from data the
/// product already carries, which is the only place it is published — the Store API has no
/// author field, and the storefront prints the name nowhere else a listing can reach.
///
/// Reads the tree through [`categories`], so after the first call this costs nothing. Silent
/// on failure: an unattributed card is a card, and no product is worth failing a search over.
async fn fill_authors(items: &mut [HubMod]) {
    if items.is_empty() {
        return;
    }
    let Ok(tree) = categories().await else {
        return;
    };
    let Some(root) = tree.iter().find(|c| c.slug == CREATORS_SLUG) else {
        return;
    };

    for item in items {
        let creator = item
            .category_ids
            .iter()
            .filter_map(|id| tree.iter().find(|c| c.id == *id))
            .find(|c| c.parent == Some(root.id));
        if let Some(creator) = creator {
            item.author = Some(creator.name.clone());
            item.author_url = creator.link.clone();
        }
    }
}

/// WordPress prints `2026-08-30T16:20:55` with no zone on a `_gmt` field — it *is* UTC, and
/// the marker is simply missing. [`parse_date_str`] already reads that form as UTC, which is
/// why this delegates rather than growing a second date parser next to it.
fn parse_wp_date(raw: &str) -> Option<i64> {
    parse_date_str(raw)
}

/// A paging header as a number. Absent or unparseable means "the store didn't say", which the
/// callers treat as zero rather than as an error.
fn header_u32(resp: &reqwest::Response, name: &str) -> Option<u32> {
    resp.headers().get(name)?.to_str().ok()?.trim().parse().ok()
}

/// Depth-first, parents before their children, so the filter row reads as the tree it is.
fn flatten(raw: &[ApiCategory]) -> Vec<HubCategory> {
    fn walk(raw: &[ApiCategory], parent: u64, depth: u8, out: &mut Vec<HubCategory>) {
        // More than three levels would mean a cycle or a tree deeper than any storefront needs;
        // stopping is better than recursing forever on malformed input.
        if depth > 3 {
            return;
        }
        let mut children: Vec<&ApiCategory> = raw.iter().filter(|c| c.parent == parent).collect();
        children.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
        for child in children {
            out.push(HubCategory {
                id: child.id,
                name: decode(&child.name),
                slug: child.slug.clone(),
                parent: (child.parent != 0).then_some(child.parent),
                link: safe_hub_url(&child.permalink),
                depth,
                count: child.count,
            });
            walk(raw, child.id, depth + 1, out);
        }
    }

    let mut out = Vec::with_capacity(raw.len());
    walk(raw, 0, 0, &mut out);
    out
}

/// True for a URL we're willing to hand to the operating system's browser.
///
/// Same rule as the shop's, against this store's host. The Buy button feeds it straight into
/// `shell:allow-open`, which launches whatever handler the OS has registered — so a permalink
/// that isn't https on MXB Hub is dropped and the button goes with it.
fn safe_hub_url(raw: &str) -> Option<String> {
    let url = reqwest::Url::parse(raw.trim()).ok()?;
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    (host == "mxb-hub.com" || host.ends_with(".mxb-hub.com")).then(|| url.to_string())
}

/// WordPress publishes titles pre-escaped — `Gear PSD&#8217;s`, `Yamaha &amp; Honda`.
fn decode(raw: &str) -> String {
    html_escape::decode_html_entities(raw.trim()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRODUCTS: &str = include_str!("fixtures/hub-products.sample.json");
    const CATEGORIES: &str = include_str!("fixtures/hub-categories.sample.json");

    fn products() -> Vec<ApiProduct> {
        serde_json::from_str(PRODUCTS).expect("the products fixture must still parse")
    }

    /// The fixtures are real responses, captured 2026-08-30. If the store changes shape, this
    /// is what says so.
    #[test]
    fn the_live_shape_maps_to_a_card() {
        let items: Vec<HubMod> = products().iter().map(map_product).collect();
        assert_eq!(items.len(), 3);

        let first = &items[0];
        assert!(!first.title.is_empty());
        assert!(!first.slug.is_empty());
        assert!(first.url.as_deref().unwrap().starts_with("https://shop.mxb-hub.com/"));
        assert!(first.image.as_deref().unwrap().starts_with("https://"));
        assert!(!first.category_names.is_empty());
    }

    /// The one conversion everything else rides on: the API's `"400"` is $4.00, not $400.
    #[test]
    fn prices_are_minor_units() {
        assert_eq!(amount("400", 2), Some(4.0));
        assert_eq!(amount("1999", 2), Some(19.99));
        assert_eq!(amount("0", 2), Some(0.0));
        // A currency without minor units (JPY) must not be divided.
        assert_eq!(amount("400", 0), Some(400.0));
        assert_eq!(amount("", 2), None);
        assert_eq!(amount("free", 2), None);

        let priced = products()
            .iter()
            .map(|p| map_price(&p.prices, p.on_sale))
            .collect::<Vec<_>>();
        assert!(
            priced.iter().all(|p| p.base.unwrap_or(0.0) < 1000.0),
            "a four-figure price means the minor-unit divide was skipped: {priced:#?}"
        );
    }

    #[test]
    fn a_zero_price_reads_as_free() {
        let prices = ApiPrices {
            price: "0".into(),
            regular_price: "0".into(),
            sale_price: "0".into(),
            currency_minor_unit: 2,
            currency_code: "USD".into(),
            price_range: None,
        };
        let price = map_price(&prices, false);
        assert!(price.free);
        assert!(!price.on_sale);
        assert_eq!(price.base, Some(0.0));
    }

    #[test]
    fn a_sale_carries_both_figures_and_a_percentage() {
        let prices = ApiPrices {
            price: "800".into(),
            regular_price: "1000".into(),
            sale_price: "800".into(),
            currency_minor_unit: 2,
            currency_code: "USD".into(),
            price_range: None,
        };
        let price = map_price(&prices, true);
        assert!(price.on_sale);
        assert_eq!(price.base, Some(10.0));
        assert_eq!(price.sale, Some(8.0));
        assert_eq!(price.discount_pct, Some(20));
        assert!(!price.free);
    }

    /// The store's flag is what decides, not the numbers: `on_sale: false` with a sale price
    /// equal to the regular one is an ordinary product, and must not print a −0% badge.
    #[test]
    fn a_sale_price_equal_to_the_regular_one_is_not_a_sale() {
        let prices = ApiPrices {
            price: "400".into(),
            regular_price: "400".into(),
            sale_price: "400".into(),
            currency_minor_unit: 2,
            currency_code: "USD".into(),
            price_range: None,
        };
        let price = map_price(&prices, true);
        assert!(!price.on_sale);
        assert_eq!(price.discount_pct, None);
    }

    #[test]
    fn categories_flatten_parents_before_children() {
        let raw: Vec<ApiCategory> =
            serde_json::from_str(CATEGORIES).expect("the categories fixture must still parse");
        let tree = flatten(&raw);

        assert_eq!(tree.len(), raw.len(), "every category must appear exactly once");
        assert!(tree.first().unwrap().depth == 0);

        // A child never precedes its parent, and its depth is one deeper.
        let mut seen: Vec<u64> = Vec::new();
        for row in &tree {
            if let Some(parent) = row.parent {
                let at = seen.iter().position(|id| *id == parent);
                assert!(at.is_some(), "{} came before its parent {parent}", row.name);
                let parent_depth = tree.iter().find(|c| c.id == parent).unwrap().depth;
                assert_eq!(row.depth, parent_depth + 1, "{}", row.name);
            } else {
                assert_eq!(row.depth, 0, "{}", row.name);
            }
            seen.push(row.id);
        }

        // The store escapes its own names; the UI must not print "Gear PSD&#8217;s".
        assert!(
            tree.iter().all(|c| !c.name.contains("&#")),
            "an HTML entity survived into a category name"
        );
    }

    #[test]
    fn only_the_stores_own_https_urls_are_openable() {
        assert!(safe_hub_url("https://shop.mxb-hub.com/product/x/").is_some());
        assert!(safe_hub_url("https://mxb-hub.com/product/x/").is_some());
        for bad in [
            "http://shop.mxb-hub.com/product/x/",
            "https://evil.com/product/x/",
            "https://mxb-hub.com.evil.com/x",
            "javascript:alert(1)",
            "",
        ] {
            assert!(safe_hub_url(bad).is_none(), "{bad} must not be openable");
        }
    }

    #[test]
    fn wordpress_gmt_dates_are_read_as_utc() {
        // 2026-08-30T16:20:55Z
        assert_eq!(parse_wp_date("2026-08-30T16:20:55"), Some(1_788_106_855));
        assert_eq!(
            parse_wp_date("2026-08-30T16:20:55Z"),
            parse_wp_date("2026-08-30T16:20:55")
        );
        assert_eq!(parse_wp_date("not a date"), None);
    }

    /// The whole read path against the real store. Ignored by default — it needs the network,
    /// and a CI run must not fail because a shop is down. Run it when the store changes shape:
    /// `cargo test --bin mxb-app -- --ignored --nocapture hub_live`
    #[tokio::test]
    #[ignore = "hits shop.mxb-hub.com"]
    async fn hub_live_catalog_answers() {
        let page = search("", None, 1, HubSort::Newest, false).await.unwrap();
        assert_eq!(page.items.len(), PER_PAGE as usize);
        assert!(page.total > 100, "total was {}", page.total);
        assert!(page.has_more);
        assert_eq!(page.currency, "USD");

        let cats = categories().await.unwrap();
        assert!(cats.len() > 50, "only {} categories", cats.len());
        assert!(cats.iter().any(|c| c.slug == "free-mods"));

        // Search, category filter and paging all narrow what comes back.
        let free = search("", Some(163), 1, HubSort::PriceAsc, false).await.unwrap();
        assert!(free.total > 0 && free.total < page.total);
        assert!(free.items.iter().all(|i| i.price.free), "{:#?}", free.items);

        let searched = search("honda", None, 1, HubSort::Newest, false).await.unwrap();
        assert!(searched.total > 0 && searched.total < page.total);

        // Detail, on whatever the newest item happens to be.
        let detail = detail(page.items[0].id).await.unwrap();
        assert_eq!(detail.item.id, page.items[0].id);
        assert!(!detail.images.is_empty());

        // …and the by-slug lookup the purchases grid uses for artwork.
        let slugs: Vec<String> = page.items.iter().take(3).map(|i| i.slug.clone()).collect();
        let found = by_slugs(&slugs).await.unwrap();
        assert_eq!(found.len(), 3);

        // Creators are recovered from the category tree; most of the store is filed under one.
        assert!(
            page.items.iter().filter(|i| i.author.is_some()).count() >= 10,
            "creator attribution stopped working"
        );

        // Dates come from `wp/v2`, and are filled for the detail page only — the listing
        // deliberately doesn't pay for them.
        assert!(detail.item.updated.is_some(), "wp/v2 date enrichment stopped working");
        assert!(
            page.items.iter().all(|i| i.updated.is_none()),
            "the listing should not be spending a request on dates it never shows"
        );
    }

    #[test]
    fn every_sort_maps_to_a_parameter_the_api_accepts() {
        // `relevance` is rejected by this install, so it must never appear here.
        for sort in [
            HubSort::Newest,
            HubSort::Popular,
            HubSort::PriceAsc,
            HubSort::PriceDesc,
            HubSort::NameAsc,
        ] {
            let (orderby, order) = sort.params();
            assert!(
                ["date", "popularity", "price", "title", "menu_order"].contains(&orderby),
                "{orderby} is not an order the Store API accepts"
            );
            assert!(["asc", "desc"].contains(&order));
        }
    }
}
