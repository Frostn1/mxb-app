//! mxbikes-shop.com "My Downloads" source (paid, authenticated).
//!
//! Unlike the free mxb-mods.com catalog, this source lists the tracks the
//! signed-in user has *already purchased* on the shop's "All My Downloads" page
//! and hands back the authenticated file URL for each so it can be streamed
//! through the shared install pipeline. Requests use the authenticated client
//! from [`crate::shop_session`].

use crate::shop_session::SHOP_BASE;
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::{AppHandle, Manager};

/// One purchased download. Mirrors `ModSummary` (so the frontend can render it
/// with the same card) plus the authenticated `download_url` to stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopItem {
    pub id: u64,
    pub slug: String,
    pub title: String,
    pub link: String,
    pub date: String,
    pub image: Option<String>,
    pub category_id: u32,
    /// Authenticated Easy Digital Downloads file URL to stream.
    pub download_url: String,
}

/// Fetch and parse the signed-in user's "All My Downloads" page.
pub async fn fetch_my_downloads(app: &AppHandle, client: &Client) -> anyhow::Result<Vec<ShopItem>> {
    let url = format!("{SHOP_BASE}/all-my-downloads/");
    let resp = client.get(&url).send().await?;
    let final_url = resp.url().as_str().to_string();
    let status = resp.status();
    let html = resp.text().await?;

    // Cloudflare sits in front of the shop and binds its `cf_clearance` cookie to
    // the browser's IP + TLS fingerprint. When the replayed session no longer
    // satisfies it, Cloudflare serves a "Just a moment…" interstitial (usually
    // 403/503) rather than the real page. Treat it like an expired session so the
    // UI prompts a fresh sign-in (which mints a new clearance) — the wording must
    // contain "sign in" for `Shop.tsx` to drop back to the signed-out screen.
    let looks_like_challenge = html.contains("Just a moment")
        || html.contains("challenge-platform")
        || html.contains("cf-browser-verification");
    if looks_like_challenge {
        anyhow::bail!("MX Bikes Shop is blocking the app (Cloudflare). Please sign in again.");
    }

    // EDD bounces an unauthenticated user to the WordPress login form.
    let looks_like_login = final_url.contains("wp-login")
        || final_url.contains("/login")
        || (html.contains("name=\"log\"") && html.contains("name=\"pwd\""));
    if looks_like_login {
        anyhow::bail!("Your MX Bikes Shop session expired — please sign in again.");
    }
    if !status.is_success() {
        anyhow::bail!("MX Bikes Shop returned HTTP {}.", status.as_u16());
    }

    let items = parse_my_downloads(&html);
    if items.is_empty() {
        // No rows parsed: persist the raw page so the CSS selectors can be
        // verified/adjusted against the real logged-in markup during review.
        if let Ok(dir) = app.path().app_cache_dir() {
            let _ = std::fs::create_dir_all(&dir);
            let dump = dir.join("shop-downloads.html");
            let _ = std::fs::write(&dump, &html);
            log::warn!("parsed 0 shop downloads; dumped page to {}", dump.display());
        }
    } else {
        log::info!("fetched {} shop downloads", items.len());
    }
    Ok(items)
}

/// Extract purchased items from the "All My Downloads" HTML.
///
/// The shop is WordPress + Easy Digital Downloads, and the page renders with the
/// EDD "user downloads" block: a table of product rows, each holding the product
/// title in a `--product` column and one or more file-download links in a
/// `--files` column (e.g. PRO / AMS / low-quality variants). We parse that
/// structure directly. If a future theme change breaks it, we fall back to the
/// generic anchor scan; if *that* also yields nothing, `fetch_my_downloads` dumps
/// the page to the cache dir so selectors can be re-tuned.
pub fn parse_my_downloads(html: &str) -> Vec<ShopItem> {
    let doc = Html::parse_document(html);
    let items = parse_edd_blocks(&doc);
    if !items.is_empty() {
        return items;
    }
    parse_generic_anchors(&doc)
}

/// Parse the EDD "user downloads" block: one card per file link, titled by its
/// product. Products with multiple files disambiguate with the file's label.
fn parse_edd_blocks(doc: &Html) -> Vec<ShopItem> {
    let product_sel = Selector::parse("div.edd-order-item__product").unwrap();
    let title_sel = Selector::parse(".edd-blocks__row-column--product").unwrap();
    let file_sel = Selector::parse("a.edd-order-item__file-link[href]").unwrap();

    let mut items = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut id: u64 = 0;

    for row in doc.select(&product_sel) {
        let product = row
            .select(&title_sel)
            .next()
            .map(|t| clean(&t.text().collect::<String>()))
            .unwrap_or_default();

        let files: Vec<ElementRef> = row.select(&file_sel).collect();
        let multi = files.len() > 1;

        for a in files {
            let href = a.value().attr("href").unwrap_or("");
            let download_url = absolute(href);
            if !seen.insert(download_url.clone()) {
                continue;
            }

            let label = clean(&a.text().collect::<String>());
            let title = match (product.is_empty(), multi) {
                // Single file: the product name is the cleanest label.
                (false, false) => product.clone(),
                // Multiple files: keep the variant (PRO/AMS/…) distinct.
                (false, true) => format!("{product} — {label}"),
                // No product heading: fall back to the file's own label.
                (true, _) if !label.is_empty() => label,
                (true, _) => "Untitled".to_string(),
            };

            id += 1;
            items.push(ShopItem {
                id,
                slug: format!("shop-{id}"),
                title,
                link: download_url.clone(),
                date: String::new(),
                image: None,
                category_id: 0,
                download_url,
            });
        }
    }

    items
}

/// Theme-agnostic fallback: scan every anchor that looks like a file/EDD download
/// link and pull a title + thumbnail from its surrounding card/row.
fn parse_generic_anchors(doc: &Html) -> Vec<ShopItem> {
    let a_sel = Selector::parse("a[href]").unwrap();

    let mut items = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut id: u64 = 0;

    for a in doc.select(&a_sel) {
        let href = a.value().attr("href").unwrap_or("");
        if !is_download_link(href) {
            continue;
        }
        let download_url = absolute(href);
        if !seen.insert(download_url.clone()) {
            continue;
        }

        let (heading, image) = row_context(&a);
        let link_text = clean(&a.text().collect::<String>());
        let title = if !heading.is_empty() {
            heading
        } else if !link_text.is_empty() && !is_generic_label(&link_text) {
            link_text
        } else {
            "Untitled".to_string()
        };

        id += 1;
        items.push(ShopItem {
            id,
            slug: format!("shop-{id}"),
            title,
            link: download_url.clone(),
            date: String::new(),
            image,
            category_id: 0,
            download_url,
        });
    }

    items
}

/// Best-effort mod-type guess for a purchased item, used to choose the install
/// destination for content we can't otherwise classify — a non-plain `.pkz`,
/// which isn't a readable archive. Structured downloads (zip/rar/7z that carry a
/// `mods/` tree, top-level `bikes/tracks/rider/…`, or a `<Bike>/paints/` bundle)
/// still self-route by their folders in `install::place_mod`; this only sets the
/// fallback bucket for the rest. Defaults to `tracks`, the shop's dominant
/// content. Returns a `mods/` subfolder name (a `CATEGORY_DIRS` value).
pub fn guess_mod_type(title: &str) -> &'static str {
    let t = title.to_lowercase();

    // Rider gear/paints read off the title clearly.
    const RIDER: [&str; 10] = [
        "helmet", "boots", "glove", "goggle", "jersey", "gear set", "gearset",
        "neck brace", "rider paint", "rider kit",
    ];
    // Bike content: manufacturers + model families, plus explicit livery/bike words.
    // (`replica` is intentionally excluded — the shop tags tracks with it too.)
    const BIKE: [&str; 18] = [
        "ktm", "husqvarna", "husky", "gasgas", "gas gas", "yamaha", "honda",
        "kawasaki", "suzuki", "fantic", "sherco", "sx-f", "sxf", "crf", "livery",
        "oem bike", " bike ", "bike paint",
    ];

    if RIDER.iter().any(|k| t.contains(k)) {
        "rider"
    } else if BIKE.iter().any(|k| t.contains(k)) {
        "bikes"
    } else {
        "tracks"
    }
}

/// Does this href point at a downloadable file (EDD action or archive)?
fn is_download_link(href: &str) -> bool {
    let h = href.to_lowercase();
    h.contains("eddfile=")
        || h.contains("edd_action=download")
        || h.contains("download_id=")
        || h.ends_with(".zip")
        || h.ends_with(".pkz")
        || h.ends_with(".rar")
        || h.ends_with(".7z")
}

/// Climb the anchor's ancestors to find the card/row's title heading and image.
fn row_context(a: &ElementRef) -> (String, Option<String>) {
    let img_sel = Selector::parse("img").unwrap();
    let heading_sel =
        Selector::parse("h1, h2, h3, h4, .edd_download_title, .download-title, .entry-title, strong")
            .unwrap();

    for node in a.ancestors().take(6) {
        let Some(el) = ElementRef::wrap(node) else {
            continue;
        };
        let image = el
            .select(&img_sel)
            .next()
            .and_then(|i| i.value().attr("src").or_else(|| i.value().attr("data-src")))
            .map(absolute);
        let heading = el
            .select(&heading_sel)
            .next()
            .map(|h| clean(&h.text().collect::<String>()))
            .filter(|s| !s.is_empty());
        if image.is_some() || heading.is_some() {
            return (heading.unwrap_or_default(), image);
        }
    }
    (String::new(), None)
}

/// Resolve a possibly-relative URL against the shop base.
fn absolute(href: &str) -> String {
    if href.starts_with("http") {
        href.to_string()
    } else if let Some(rest) = href.strip_prefix("//") {
        format!("https://{rest}")
    } else if href.starts_with('/') {
        format!("{SHOP_BASE}{href}")
    } else {
        format!("{SHOP_BASE}/{href}")
    }
}

/// Collapse whitespace in extracted text.
fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Generic link labels that aren't a real product title.
fn is_generic_label(s: &str) -> bool {
    let l = s.to_lowercase();
    matches!(l.as_str(), "download" | "download file" | "get file" | "file")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_edd_blocks_user_downloads() {
        // Mirrors the live "All My Downloads" markup (EDD user-downloads block):
        // product rows whose title lives in a sibling `--product` column, with one
        // or more file links per product and no thumbnails.
        let html = r#"
            <div class="wp-block-edd-user-downloads edd-blocks__user-downloads">
              <div class="edd-blocks__row edd-blocks__row-header edd-order-items__header">
                <div class="edd-blocks__row-label edd-blocks__row-label--product">Product</div>
                <div class="edd-blocks__row-label edd-blocks__row-label--files">Files</div>
              </div>
              <div class="edd-blocks__row edd-order-item__product edd-pro-search__product">
                <div class="edd-blocks__row-column edd-blocks__row-column--product edd-blocks__row-label">
                  2022 ARL MX RD1 &#8211; Fox Raceway
                </div>
                <div class="edd-blocks__row-column edd-blocks__row-column--files edd-order-item__files">
                  <div class="edd-order-item__file">
                    <a href="https://mxbikes-shop.com/index.php?eddfile=1257454%3A11356%3A0&#038;ttl=1&#038;token=aaa" class="edd-order-item__file-link">2022.ARL_.MX_.RD01-1.pkz</a>
                  </div>
                </div>
              </div>
              <div class="edd-blocks__row edd-order-item__product edd-pro-search__product">
                <div class="edd-blocks__row-column edd-blocks__row-column--product edd-blocks__row-label">
                  2024 ARLMX RD10 &#8211; MARYLAND
                </div>
                <div class="edd-blocks__row-column edd-blocks__row-column--files edd-order-item__files">
                  <div class="edd-order-item__file">
                    <a href="https://mxbikes-shop.com/index.php?eddfile=1&#038;token=b" class="edd-order-item__file-link">2024_ARLMX_RD10_MARYLAND PRO</a>
                  </div>
                  <div class="edd-order-item__file">
                    <a href="https://mxbikes-shop.com/index.php?eddfile=2&#038;token=c" class="edd-order-item__file-link">2024_ARLMX_RD10_MARYLAND AMS</a>
                  </div>
                </div>
              </div>
            </div>
        "#;

        let items = parse_my_downloads(html);
        // 1 file from the first product + 2 from the second.
        assert_eq!(items.len(), 3);
        // Single-file product uses the product name verbatim (entities decoded).
        assert_eq!(items[0].title, "2022 ARL MX RD1 – Fox Raceway");
        assert!(items[0].download_url.contains("eddfile="));
        assert!(items[0].image.is_none());
        // Multi-file product keeps each variant distinct.
        assert_eq!(items[1].title, "2024 ARLMX RD10 – MARYLAND — 2024_ARLMX_RD10_MARYLAND PRO");
        assert_eq!(items[2].title, "2024 ARLMX RD10 – MARYLAND — 2024_ARLMX_RD10_MARYLAND AMS");
    }

    #[test]
    fn parses_edd_style_cards() {
        // Two purchased downloads laid out as cards with a heading, thumbnail,
        // and an EDD download link; plus a noise link that must be ignored.
        let html = r#"
            <div class="edd_downloads_list">
              <div class="edd_download">
                <img src="/wp-content/uploads/track-a.jpg" />
                <h3 class="edd_download_title">Sunset MX Park</h3>
                <a href="/?edd_action=download&download_id=101&eddfile=abc">Download</a>
              </div>
              <div class="edd_download">
                <img src="https://cdn.example.com/track-b.jpg" />
                <h3 class="edd_download_title">Riverside National</h3>
                <a href="https://mxbikes-shop.com/files/riverside.pkz">Download</a>
              </div>
              <a href="/faqs/">FAQ</a>
            </div>
        "#;

        let items = parse_my_downloads(html);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Sunset MX Park");
        assert_eq!(
            items[0].image.as_deref(),
            Some("https://mxbikes-shop.com/wp-content/uploads/track-a.jpg")
        );
        assert!(items[0].download_url.contains("edd_action=download"));
        assert_eq!(items[1].title, "Riverside National");
        assert_eq!(items[1].download_url, "https://mxbikes-shop.com/files/riverside.pkz");
    }

    #[test]
    fn dedupes_repeated_links() {
        let html = r#"
            <a href="/files/x.zip">Download</a>
            <a href="/files/x.zip">Download again</a>
        "#;
        assert_eq!(parse_my_downloads(html).len(), 1);
    }

    #[test]
    fn guesses_mod_type_from_title() {
        // Tracks are the default (real product titles from the shop).
        assert_eq!(guess_mod_type("2022 ARL MX RD1 – Fox Raceway"), "tracks");
        assert_eq!(guess_mod_type("2024 ARLMX RD10 – MARYLAND"), "tracks");
        // Bike content by manufacturer/model or explicit livery wording.
        assert_eq!(guess_mod_type("2023 KTM 450 SX-F"), "bikes");
        assert_eq!(guess_mod_type("Factory Yamaha Livery Pack"), "bikes");
        // Rider gear/paints.
        assert_eq!(guess_mod_type("Fox V3 Helmet Paint"), "rider");
        assert_eq!(guess_mod_type("Alpinestars Boots + Gloves"), "rider");
    }
}
