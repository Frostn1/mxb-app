use crate::shop_session::SHOP_BASE;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopItem {
    pub id: u64,
    pub slug: String,
    pub title: String,
    /// The product's own name, without the file label — what the store sells, and so the only
    /// string that can be matched against the public catalog or used to group a product's
    /// files under one card. Never empty: falls back to the file label, then "Untitled".
    pub product: String,
    /// Which file of the product this row is (`PRO`, `AMS`, …). Empty when the product ships
    /// a single file, which is the common case.
    pub file_label: String,
    pub link: String,
    pub date: String,
    pub image: Option<String>,
    pub category_id: u32,
    /// Authenticated Easy Digital Downloads file URL to stream.
    pub download_url: String,
}

/// Whether this HTML is Cloudflare's interstitial rather than the page we asked for.
///
/// Matched on markers the interstitial alone carries. `challenge-platform` was one of them and
/// had to go: Cloudflare injects a script from that path into *ordinary* pages too when its JS
/// detections are on, so now that the HTML comes from a real browser rather than from `reqwest`
/// it would condemn the very pages this exists to let through — every purchases list read as
/// "blocked", permanently.
fn looks_like_challenge(html: &str) -> bool {
    // Set by the interstitial's inline bootstrap, and by nothing else.
    html.contains("_cf_chl_opt")
        || html.contains("cf-browser-verification")
        || html.contains("<title>Just a moment...</title>")
}

/// Fetch and parse the signed-in user's "All My Downloads" page.
///
/// The HTML arrives from [`crate::shop_fetch`] — read out of a real browser sitting on the
/// page — rather than from `reqwest`. It has to: the page is behind a Cloudflare *managed*
/// challenge, which is cleared by running JavaScript, and a clearance earned in a WebView
/// cannot be replayed by an HTTP client. The parsing below is unchanged and unaware of this.
pub async fn fetch_my_downloads(app: &AppHandle, reload: bool) -> anyhow::Result<Vec<ShopItem>> {
    let html = crate::shop_fetch::downloads_html(reload).await?;

    if looks_like_challenge(&html) {
        anyhow::bail!("MX Bikes Shop is blocking the app (Cloudflare). Please sign in again.");
    }

    // EDD bounces an unauthenticated user to the WordPress login form.
    if html.contains("name=\"log\"") && html.contains("name=\"pwd\"") {
        anyhow::bail!("Your MX Bikes Shop session expired — please sign in again.");
    }

    let items = parse_my_downloads(&html);
    if items.is_empty() {
        // No rows parsed: dump the raw page so selectors can be re-tuned.
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

pub fn parse_my_downloads(html: &str) -> Vec<ShopItem> {
    let doc = Html::parse_document(html);
    let items = parse_edd_blocks(&doc);
    if !items.is_empty() {
        return items;
    }
    parse_generic_anchors(&doc)
}

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
                (false, false) => product.clone(),
                // Multiple files: keep the variant (PRO/AMS/…) distinct.
                (false, true) => format!("{product} — {label}"),
                (true, _) if !label.is_empty() => label.clone(),
                (true, _) => "Untitled".to_string(),
            };
            // The product name is kept whole and separate rather than recovered by splitting
            // `title` back on the em-dash: a product is free to have one in its own name, and
            // this is the string the catalog match and the card grouping both key on.
            let named = if product.is_empty() {
                if label.is_empty() {
                    "Untitled".to_string()
                } else {
                    label.clone()
                }
            } else {
                product.clone()
            };

            id += 1;
            items.push(ShopItem {
                id,
                slug: format!("shop-{id}"),
                title,
                product: named,
                // Only meaningful when the product ships more than one file; a lone file's
                // label is the file's own name and would just repeat the product on screen.
                file_label: if multi { label } else { String::new() },
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
            // This path finds one anchor at a time, so there is no product/file split to
            // make — the heading it climbed to *is* the product.
            product: title.clone(),
            title,
            file_label: String::new(),
            link: download_url.clone(),
            date: String::new(),
            image,
            category_id: 0,
            download_url,
        });
    }

    items
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

    /// The real interstitial has to be caught, or an empty grid gets reported as "you own
    /// nothing" when the truth is "Cloudflare refused us".
    #[test]
    fn the_cloudflare_interstitial_is_recognised() {
        // Trimmed from a live capture of the challenge on /all-my-downloads/.
        let interstitial = r#"<html><head><title>Just a moment...</title></head><body>
            <script>window._cf_chl_opt = {cType: 'managed', cRay: 'a28a310c1d2c760a'};</script>
            </body></html>"#;
        assert!(looks_like_challenge(interstitial));
    }

    /// ...and the *ordinary* page must not be, which is the sharper half. Cloudflare injects a
    /// `challenge-platform/scripts/jsd` script into pages that were served normally, so keying
    /// on that path — as this once did — condemns every good page the moment the HTML starts
    /// coming from a real browser instead of from `reqwest`.
    #[test]
    fn a_normal_page_carrying_cloudflares_injected_script_is_not_a_challenge() {
        let good = r#"<html><head><title>All My Downloads - MX Bikes Shop</title></head><body>
            <script src="/cdn-cgi/challenge-platform/scripts/jsd/main.js"></script>
            <div class="wp-block-edd-user-downloads">…</div>
            </body></html>"#;
        assert!(
            !looks_like_challenge(good),
            "a served page must not be mistaken for the interstitial"
        );
    }

    #[test]
    fn parses_edd_blocks_user_downloads() {
        // Mirrors the live "All My Downloads" markup (EDD user-downloads block).
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
        // Single-file product uses the product name verbatim (entities decoded), and carries
        // no file label — there is no variant to tell apart.
        assert_eq!(items[0].title, "2022 ARL MX RD1 – Fox Raceway");
        assert_eq!(items[0].product, "2022 ARL MX RD1 – Fox Raceway");
        assert_eq!(items[0].file_label, "");
        assert!(items[0].download_url.contains("eddfile="));
        assert!(items[0].image.is_none());
        // Multi-file product keeps each variant distinct.
        assert_eq!(items[1].title, "2024 ARLMX RD10 – MARYLAND — 2024_ARLMX_RD10_MARYLAND PRO");
        assert_eq!(items[2].title, "2024 ARLMX RD10 – MARYLAND — 2024_ARLMX_RD10_MARYLAND AMS");
        // …and both name the same product, which is what groups them onto one card.
        assert_eq!(items[1].product, "2024 ARLMX RD10 – MARYLAND");
        assert_eq!(items[2].product, "2024 ARLMX RD10 – MARYLAND");
        assert_eq!(items[1].file_label, "2024_ARLMX_RD10_MARYLAND PRO");
        assert_eq!(items[2].file_label, "2024_ARLMX_RD10_MARYLAND AMS");
    }

    #[test]
    fn product_survives_an_em_dash_in_its_own_name() {
        // The reason `product` is a field rather than `title.split(" — ")`: a product name may
        // contain the very separator the multi-file title is built with.
        let html = r#"
            <div class="edd-blocks__row edd-order-item__product">
              <div class="edd-blocks__row-column--product">Red Bull — Straight Rhythm</div>
              <div class="edd-blocks__row-column--files">
                <div class="edd-order-item__file">
                  <a href="/?eddfile=9&token=z" class="edd-order-item__file-link">RBSR PRO</a>
                </div>
                <div class="edd-order-item__file">
                  <a href="/?eddfile=8&token=y" class="edd-order-item__file-link">RBSR AMS</a>
                </div>
              </div>
            </div>
        "#;

        let items = parse_my_downloads(html);
        assert_eq!(items.len(), 2);
        for item in &items {
            assert_eq!(item.product, "Red Bull — Straight Rhythm");
        }
        assert_eq!(items[0].file_label, "RBSR PRO");
        assert_eq!(items[1].file_label, "RBSR AMS");
    }

    #[test]
    fn parses_edd_style_cards() {
        // Two card-style downloads plus a noise link that must be ignored.
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
        // One anchor per card, so the heading is the product and there is no variant.
        assert_eq!(items[0].product, "Sunset MX Park");
        assert_eq!(items[0].file_label, "");
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
}
