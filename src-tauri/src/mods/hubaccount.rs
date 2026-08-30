//! The signed-in half of MXB Hub — what this account owns, and how to let go of the session.
//!
//! WooCommerce keeps a customer's files on `/my-account/downloads/`, one row per purchased
//! file, each linking to a signed `?download_file=…&order=…&key=…` URL. That page is ordinary
//! HTML behind an ordinary login cookie: no Cloudflare, so [`crate::hub_session`]'s `reqwest`
//! client reads it directly and streams the file itself. Contrast [`super::mxbshop`], which
//! does the same job for mxbikes-shop.com through a parked WebView because every path there is
//! a managed challenge.
//!
//! The parsing is deliberately layered — the themed table, then the list form some themes use,
//! then any `download_file` link on the page. A storefront is free to restyle its account area,
//! and a purchases grid that empties itself the day it does is worse than one that finds the
//! links wherever they ended up. When all three find nothing, the page is dumped to the cache
//! directory so the selectors can be re-tuned against what the store actually served.

use crate::hub_session::HUB_BASE;
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::{AppHandle, Manager};

const DOWNLOADS_PATH: &str = "/my-account/downloads/";
const ACCOUNT_PATH: &str = "/my-account/";

/// One purchased file.
///
/// Mirrors [`super::mxbshop::ShopItem`] field for field. That is not an accident: the two
/// stores' purchase grids do the same job, the install command takes the same shape, and a
/// second set of near-identical names would make every shared UI decision a translation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubItem {
    pub id: u64,
    /// Identity for the install queue and the staging directory — the product's own URL slug
    /// where the page gave one, else a slug made from the title.
    pub slug: String,
    /// What the card is titled: the product, plus the file label when a product ships several.
    pub title: String,
    /// The product's own name, kept whole and separate rather than recovered by splitting
    /// `title` — a product may contain an em-dash of its own, and this is the string the
    /// artwork lookup and the card grouping key on.
    pub product: String,
    /// Which file of the product this row is. Empty when the product ships a single file.
    pub file_label: String,
    /// The product page, when the row linked to one.
    pub link: String,
    /// Kept for shape-compatibility with the shop's grid. WooCommerce's downloads table
    /// carries no purchase date, so this is empty rather than invented.
    pub date: String,
    /// Filled in later from the catalog, by slug — see [`match_products`].
    pub image: Option<String>,
    /// The signed WooCommerce download URL. Only ever an `https` URL on the store itself.
    pub download_url: String,
}

/// Fetch and parse the signed-in account's downloads.
pub async fn fetch_my_downloads(app: &AppHandle, client: &Client) -> anyhow::Result<Vec<HubItem>> {
    let resp = client
        .get(format!("{HUB_BASE}{DOWNLOADS_PATH}"))
        .send()
        .await?;
    let status = resp.status();
    let html = resp.text().await?;

    if !status.is_success() {
        anyhow::bail!("MXB Hub answered {status} for your downloads");
    }
    // WooCommerce serves the login form in place of the account page for a dead cookie — a 200,
    // not a redirect, so the status says nothing. Dropping the session here is what turns the
    // next visit into a "Sign in" button rather than a grid that fails the same way forever.
    if looks_like_login(&html) {
        crate::hub_session::forget(app);
        anyhow::bail!("Your MXB Hub session expired — please sign in again.");
    }

    let items = parse_downloads(&html);
    if items.is_empty() {
        // An account with no purchases is not a parse failure, and must not be reported as one.
        if has_empty_state(&html) {
            log::info!("MXB Hub reports no downloads on this account");
            return Ok(vec![]);
        }
        if let Ok(dir) = app.path().app_cache_dir() {
            let _ = std::fs::create_dir_all(&dir);
            let dump = dir.join("hub-downloads.html");
            let _ = std::fs::write(&dump, &html);
            log::warn!(
                "parsed 0 MXB Hub downloads; dumped the page to {}",
                dump.display()
            );
        }
    } else {
        log::info!("fetched {} MXB Hub downloads", items.len());
    }
    Ok(items)
}

/// End the session on the server, so the copy of the cookie left in the WebView's own jar is
/// dead too. `Ok(false)` means the account page offered no logout link — already signed out.
pub async fn logout(client: &Client) -> anyhow::Result<bool> {
    let html = client
        .get(format!("{HUB_BASE}{ACCOUNT_PATH}"))
        .send()
        .await?
        .text()
        .await?;
    let Some(url) = logout_url(&html) else {
        return Ok(false);
    };
    // The URL carries WordPress's nonce, so it is only ever followed, never constructed.
    client.get(url).send().await?;
    Ok(true)
}

// ───────────────────────────────── parsing ─────────────────────────────────

/// True when this is the login form rather than the account area.
///
/// Both fields, not either: WooCommerce's account pages carry a search form of their own, and
/// matching on a lone `name="password"` condemned every signed-in page the moment the theme
/// added a newsletter box.
fn looks_like_login(html: &str) -> bool {
    (html.contains("woocommerce-form-login") || html.contains("class=\"login\""))
        && html.contains("name=\"username\"")
        && html.contains("name=\"password\"")
}

/// WooCommerce's own copy for an account that has bought nothing downloadable yet.
fn has_empty_state(html: &str) -> bool {
    html.contains("no-downloads")
        || html.contains("No downloads available yet")
        || html.contains("woocommerce-Message--info")
}

pub fn parse_downloads(html: &str) -> Vec<HubItem> {
    let doc = Html::parse_document(html);
    let rows = parse_table(&doc);
    if !rows.is_empty() {
        return finish(rows);
    }
    finish(parse_any_links(&doc))
}

/// One row before its title has been decided — the title depends on whether the product turns
/// out to ship more than one file, which is only knowable once every row is in.
struct Row {
    product: String,
    file_label: String,
    link: String,
    download_url: String,
}

/// The themed table (and the `<ul>` some themes render instead): a product cell and a file
/// cell, so the product name survives even when the link text is just "Download".
fn parse_table(doc: &Html) -> Vec<Row> {
    let row_sel = match Selector::parse("tr, li.woocommerce-MyAccount-downloads-file, li") {
        Ok(sel) => sel,
        Err(_) => return vec![],
    };
    let product_sel = Selector::parse(".download-product, .woocommerce-table__product-name").unwrap();
    let file_sel = Selector::parse("a[href*=\"download_file=\"]").unwrap();

    let mut rows = Vec::new();
    for row in doc.select(&row_sel) {
        let Some(anchor) = row.select(&file_sel).next() else {
            continue;
        };
        // A `<li>` nested inside a matched `<tr>` yields the same link twice. That is left to
        // the deduplication in `finish`, which keys on the download URL — and because
        // `select` walks in document order, the outer row (the one that still has its product
        // cell) is the copy that survives. An ancestor check here looked cheaper and was
        // wrong: every `<tr>` has a `<table>` above it that also contains the link, so it
        // rejected the entire table and quietly fell through to the text-only fallback.
        let Some(download_url) = safe_download_url(anchor.value().attr("href").unwrap_or("")) else {
            continue;
        };

        // The product cell, if the theme has one — never the cell holding the file link, whose
        // text is the file name.
        let product_cell = row
            .select(&product_sel)
            .find(|cell| cell.select(&file_sel).next().is_none());
        let product = product_cell.map(|c| text(&c)).unwrap_or_default();
        let link = product_cell
            .and_then(|c| Selector::parse("a[href]").ok().and_then(|s| c.select(&s).next()))
            .and_then(|a| safe_product_url(a.value().attr("href").unwrap_or("")))
            .unwrap_or_default();

        rows.push(Row {
            product,
            file_label: text(&anchor),
            link,
            download_url,
        });
    }
    rows
}

/// Last resort: every download link on the page, wherever it sits. Loses the product/file
/// split — the link text becomes the name — but a grid of correctly-named, installable files
/// beats an empty one.
fn parse_any_links(doc: &Html) -> Vec<Row> {
    let sel = Selector::parse("a[href*=\"download_file=\"]").unwrap();
    doc.select(&sel)
        .filter_map(|a| {
            let download_url = safe_download_url(a.value().attr("href").unwrap_or(""))?;
            Some(Row {
                product: String::new(),
                file_label: text(&a),
                link: String::new(),
                download_url,
            })
        })
        .collect()
}

/// Decide each row's title and identity, now that the whole page is in.
fn finish(rows: Vec<Row>) -> Vec<HubItem> {
    // Deduplicate first, then count. The other order says a product ships two files whenever
    // one file was listed twice — under a second order, or by a theme that renders the table
    // and a mobile list of the same rows — and every such title would grow a "— label" suffix
    // it hasn't earned.
    let mut seen: HashSet<String> = HashSet::new();
    let rows: Vec<&Row> = rows
        .iter()
        .filter(|row| seen.insert(row.download_url.clone()))
        .collect();

    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for row in &rows {
        if !row.product.is_empty() {
            *counts.entry(row.product.as_str()).or_default() += 1;
        }
    }

    let mut items = Vec::new();
    for row in rows {
        let multi = counts.get(row.product.as_str()).copied().unwrap_or(0) > 1;
        let product = match (row.product.trim(), row.file_label.trim()) {
            ("", "") => "Untitled".to_string(),
            ("", label) => label.to_string(),
            (product, _) => product.to_string(),
        };
        let title = match (row.product.trim().is_empty(), multi, row.file_label.trim()) {
            (false, true, label) if !label.is_empty() => format!("{product} — {label}"),
            _ => product.clone(),
        };

        items.push(HubItem {
            id: items.len() as u64 + 1,
            slug: slug_for(&row.link, &title),
            title,
            product,
            file_label: row.file_label.trim().to_string(),
            link: row.link.clone(),
            date: String::new(),
            image: None,
            download_url: row.download_url.clone(),
        });
    }
    items
}

/// The product's own URL slug where the row linked to its page, else one made from the title.
///
/// It has to be stable and unique per row: the install queue keys its cancel token, its staging
/// directory and its progress card on this, so two purchases sharing a slug would cancel each
/// other. A product with several files disambiguates on the file label, which is what the
/// title already carries.
fn slug_for(link: &str, title: &str) -> String {
    let from_link = reqwest::Url::parse(link)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .and_then(|s| s.filter(|p| !p.is_empty()).next_back())
                .map(str::to_string)
        })
        .filter(|s| !s.is_empty() && s != "product");

    let base = slugify(title);
    match from_link {
        // Both, when the title said more than the product page can: the slug names the product
        // and the title names the file within it.
        Some(slug) if slugify(title) != slug => format!("{slug}-{base}"),
        Some(slug) => slug,
        None if base.is_empty() => "hub-download".to_string(),
        None => base,
    }
}

fn slugify(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut dash = false;
    for ch in raw.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            dash = false;
        } else if !out.is_empty() && !dash {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').chars().take(72).collect()
}

fn text(el: &ElementRef) -> String {
    let raw: String = el.text().collect();
    html_escape::decode_html_entities(raw.trim())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A download URL we're willing to stream from: `https`, on the store, and carrying the
/// `download_file` parameter that makes it a file rather than a page.
///
/// This is the one string on the page that becomes a network request with the user's session
/// attached, so it is checked rather than trusted. A row pointing anywhere else is dropped.
fn safe_download_url(raw: &str) -> Option<String> {
    let url = on_the_store(raw)?;
    url.query_pairs()
        .any(|(k, _)| k == "download_file")
        .then(|| url.to_string())
}

fn safe_product_url(raw: &str) -> Option<String> {
    on_the_store(raw).map(|u| u.to_string())
}

fn on_the_store(raw: &str) -> Option<reqwest::Url> {
    let decoded = html_escape::decode_html_entities(raw.trim()).into_owned();
    let url = reqwest::Url::parse(&decoded).ok()?;
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    (host == "mxb-hub.com" || host.ends_with(".mxb-hub.com")).then_some(url)
}

/// WordPress's logout link, nonce and all, as printed in the account navigation.
fn logout_url(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse("a[href*=\"customer-logout\"], a[href*=\"action=logout\"]").ok()?;
    doc.select(&sel)
        .find_map(|a| safe_product_url(a.value().attr("href").unwrap_or("")))
}

// ───────────────────────────────── artwork ─────────────────────────────────

/// The catalog entry for each purchased row, positionally.
///
/// The downloads page gives a name and a link and nothing else, so this is what supplies the
/// artwork that makes the grid worth looking at. Looked up by *slug* rather than by name —
/// the row links to the product page, and the Store API takes a comma-separated `slug` list,
/// so the whole grid resolves in one request and an exact match. Rows whose product has since
/// been unlisted simply come back `None`.
pub async fn match_products(items: &[HubItem]) -> anyhow::Result<Vec<Option<super::hub::HubMod>>> {
    let slugs: Vec<String> = items
        .iter()
        .filter_map(|i| product_slug(&i.link))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if slugs.is_empty() {
        return Ok(vec![None; items.len()]);
    }

    let found = super::hub::by_slugs(&slugs).await?;
    Ok(items
        .iter()
        .map(|item| {
            let slug = product_slug(&item.link)?;
            found.iter().find(|m| m.slug == slug).cloned()
        })
        .collect())
}

/// The `…/product/<slug>/` segment of a product permalink.
fn product_slug(link: &str) -> Option<String> {
    let url = reqwest::Url::parse(link).ok()?;
    let mut segments: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    let last = segments.pop()?;
    (!last.is_empty() && last != "product").then(|| last.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WooCommerce's stock `myaccount/downloads.php`, as a theme renders it: a product cell
    /// and a file cell, and a product that ships two files.
    const TABLE: &str = r#"
    <div class="woocommerce-MyAccount-content">
    <table class="woocommerce-table woocommerce-table--order-downloads shop_table">
      <thead><tr><th class="download-product">Product</th><th class="download-file">Download</th></tr></thead>
      <tbody>
        <tr>
          <td class="download-product" data-title="Product">
            <a href="https://shop.mxb-hub.com/product/bell-moto-10-fox-vue-rolloff/">BELL MOTO 10 [FOX VUE ROLLOFF]</a>
          </td>
          <td class="download-file" data-title="Download">
            <a href="https://shop.mxb-hub.com/?download_file=16727&amp;order=wc_order_abc&amp;email=a%40b.c&amp;key=k1"
               class="woocommerce-MyAccount-downloads-file button alt">BellMoto10.pkz</a>
          </td>
        </tr>
        <tr>
          <td class="download-product" data-title="Product">
            <a href="https://shop.mxb-hub.com/product/tld-se5-psd/">TLD SE5 [PSD]</a>
          </td>
          <td class="download-file" data-title="Download">
            <a href="https://shop.mxb-hub.com/?download_file=16700&amp;order=wc_order_abc&amp;key=k2">SE5 PSD</a>
          </td>
        </tr>
        <tr>
          <td class="download-product" data-title="Product">
            <a href="https://shop.mxb-hub.com/product/tld-se5-psd/">TLD SE5 [PSD]</a>
          </td>
          <td class="download-file" data-title="Download">
            <a href="https://shop.mxb-hub.com/?download_file=16701&amp;order=wc_order_abc&amp;key=k3">SE5 PNT</a>
          </td>
        </tr>
      </tbody>
    </table></div>"#;

    /// The list form, with no product cell at all — the fallback path.
    const LIST: &str = r#"
    <ul class="woocommerce-MyAccount-downloads">
      <li class="woocommerce-MyAccount-downloads-file">
        <a href="https://shop.mxb-hub.com/?download_file=99&amp;order=wc_order_z&amp;key=k9">RED BUD KXF.pkz</a>
      </li>
    </ul>"#;

    #[test]
    fn the_stock_table_parses_into_installable_rows() {
        let items = parse_downloads(TABLE);
        assert_eq!(items.len(), 3);

        assert_eq!(items[0].product, "BELL MOTO 10 [FOX VUE ROLLOFF]");
        // One file: the title is just the product, with no dangling em-dash.
        assert_eq!(items[0].title, "BELL MOTO 10 [FOX VUE ROLLOFF]");
        assert_eq!(items[0].slug, "bell-moto-10-fox-vue-rolloff");
        assert!(items[0].download_url.contains("download_file=16727"));
        // `&amp;` in the markup is one parameter separator, not part of the value.
        assert!(items[0].download_url.contains("key=k1"), "{}", items[0].download_url);

        // Two files under one product: each keeps its own label, and its own identity.
        assert_eq!(items[1].title, "TLD SE5 [PSD] — SE5 PSD");
        assert_eq!(items[2].title, "TLD SE5 [PSD] — SE5 PNT");
        assert_ne!(items[1].slug, items[2].slug, "two rows must not share a slug");
    }

    #[test]
    fn the_list_form_falls_back_to_the_link_text() {
        let items = parse_downloads(LIST);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "RED BUD KXF.pkz");
        assert_eq!(items[0].product, "RED BUD KXF.pkz");
        assert_eq!(items[0].slug, "red-bud-kxf-pkz");
    }

    /// The same file under two orders is one thing to install.
    #[test]
    fn a_repeated_file_appears_once() {
        let html = format!("{LIST}{LIST}");
        assert_eq!(parse_downloads(&html).len(), 1);
    }

    /// The security boundary: these URLs are fetched with the user's session attached.
    #[test]
    fn only_signed_store_urls_are_downloadable() {
        assert!(safe_download_url("https://shop.mxb-hub.com/?download_file=1&key=k").is_some());
        for bad in [
            // Right shape, wrong host.
            "https://evil.com/?download_file=1&key=k",
            "https://shop.mxb-hub.com.evil.com/?download_file=1",
            // Right host, not a file.
            "https://shop.mxb-hub.com/my-account/",
            // Not https.
            "http://shop.mxb-hub.com/?download_file=1",
            "javascript:alert(1)",
            "",
        ] {
            assert!(safe_download_url(bad).is_none(), "{bad} must not be downloadable");
        }
    }

    #[test]
    fn a_row_pointing_off_the_store_is_dropped_not_rendered() {
        let html = r#"<table><tr>
            <td class="download-product">Trojan</td>
            <td class="download-file"><a href="https://evil.com/?download_file=1">Free stuff</a></td>
        </tr></table>"#;
        assert!(parse_downloads(html).is_empty());
    }

    #[test]
    fn the_login_form_is_recognised_but_an_account_page_is_not() {
        let login = r#"<form class="woocommerce-form woocommerce-form-login login">
            <input name="username"><input name="password" type="password"></form>"#;
        assert!(looks_like_login(login));
        // A signed-in account page with a search box must not read as signed out.
        assert!(!looks_like_login(
            r#"<div class="woocommerce-MyAccount-content"><input name="s"></div>"#
        ));
        assert!(!looks_like_login(TABLE));
    }

    #[test]
    fn an_empty_account_is_not_a_parse_failure() {
        assert!(has_empty_state(
            r#"<div class="woocommerce-Message woocommerce-Message--info woocommerce-info no-downloads">
               No downloads available yet.</div>"#
        ));
        assert!(!has_empty_state(TABLE));
    }

    #[test]
    fn the_logout_link_is_taken_from_the_page_never_built() {
        let html = r#"<nav><a href="https://shop.mxb-hub.com/my-account/customer-logout/?_wpnonce=abc123">Log out</a></nav>"#;
        assert_eq!(
            logout_url(html).as_deref(),
            Some("https://shop.mxb-hub.com/my-account/customer-logout/?_wpnonce=abc123")
        );
        assert!(logout_url("<nav><a href=\"https://evil.com/customer-logout/\">x</a></nav>").is_none());
        assert!(logout_url("<nav></nav>").is_none());
    }

    #[test]
    fn product_slugs_come_off_the_permalink() {
        assert_eq!(
            product_slug("https://shop.mxb-hub.com/product/mxb-hub-fc-paint/").as_deref(),
            Some("mxb-hub-fc-paint")
        );
        assert_eq!(product_slug("https://shop.mxb-hub.com/product/").as_deref(), None);
        assert_eq!(product_slug("").as_deref(), None);
    }

    #[test]
    fn slugify_makes_a_usable_staging_name() {
        assert_eq!(slugify("BELL MOTO 10 [FOX VUE ROLLOFF]"), "bell-moto-10-fox-vue-rolloff");
        assert_eq!(slugify("  ---  "), "");
        assert_eq!(slugify("2026 HRC Honda Black BG [PAINT]"), "2026-hrc-honda-black-bg-paint");
        assert!(slugify(&"x".repeat(200)).len() <= 72);
    }
}
