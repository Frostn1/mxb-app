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
    /// Always 0, and always from the catalog match where it matters: the downloads page files
    /// nothing under a category. Present because the purchases grid is the shop's, and this is
    /// the shape it reads — see [`super::mxbshop::ShopItem`].
    pub category_id: u32,
    /// Likewise `None` here and filled from the catalog, which is the only side that knows.
    pub author: Option<String>,
    /// Where the file comes from. Either a signed WooCommerce `?download_file=` URL on the
    /// store, or — see [`external`] — a link to a file host.
    pub download_url: String,
    /// True when the store hands the file off to somebody else.
    ///
    /// WooCommerce lets a downloadable product's "file" be any URL, and MXB Hub uses that: a
    /// number of the free mods are MediaFire links rather than uploads. It matters for two
    /// reasons, which is why it is recorded rather than sniffed at install time. The URL has
    /// to be *resolved* (a MediaFire folder is a web page, not a file), and it must be fetched
    /// **without** the user's session — a store-issued link is the only kind that has any
    /// business carrying their cookies.
    pub external: bool,
    /// The file host, for the resolver and for the download history. Empty when not external.
    pub host: String,
}

/// Fetch and parse the signed-in account's downloads.
pub async fn fetch_my_downloads(app: &AppHandle, client: &Client) -> anyhow::Result<Vec<HubItem>> {
    let resp = client
        .get(format!("{HUB_BASE}{DOWNLOADS_PATH}"))
        .send()
        .await?;
    // The account page is behind the same rate-based robot challenge as everything else, and
    // it arrives as a `202` of HTML — which parses to zero rows and would otherwise read as
    // "you own nothing". Reported as the challenge it is, so the command layer answers it.
    if super::hub::challenged(&resp) {
        return Err(super::Blocked::new(
            None,
            "MXB Hub is asking the app to prove it isn't a robot.",
        )
        .into());
    }
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
    external: bool,
    host: String,
}

/// The themed table (and the `<ul>` some themes render instead): a product cell and a file
/// cell, so the product name survives even when the link text is just "Download".
fn parse_table(doc: &Html) -> Vec<Row> {
    let row_sel = match Selector::parse("tr, li") {
        Ok(sel) => sel,
        Err(_) => return vec![],
    };
    let product_sel = Selector::parse(".download-product, .woocommerce-table__product-name").unwrap();
    // Deliberately wider than `download_file=`. A product whose file is hosted elsewhere links
    // straight out to it, so keying on that parameter alone made those rows invisible — the
    // mod simply wasn't in the grid, with nothing to say why.
    let file_sel = Selector::parse(
        "a.woocommerce-MyAccount-downloads-file[href], td.download-file a[href], \
         .download-file a[href], a[href*=\"download_file=\"]",
    )
    .unwrap();

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
        let Some(file) = download_link(anchor.value().attr("href").unwrap_or("")) else {
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
            download_url: file.url,
            external: file.external,
            host: file.host,
        });
    }
    rows
}

/// Last resort: every download link on the page, wherever it sits. Loses the product/file
/// split — the link text becomes the name — but a grid of correctly-named, installable files
/// beats an empty one.
fn parse_any_links(doc: &Html) -> Vec<Row> {
    let sel = Selector::parse(
        "a.woocommerce-MyAccount-downloads-file[href], a[href*=\"download_file=\"]",
    )
    .unwrap();
    doc.select(&sel)
        .filter_map(|a| {
            let file = download_link(a.value().attr("href").unwrap_or(""))?;
            Some(Row {
                product: String::new(),
                file_label: text(&a),
                link: String::new(),
                download_url: file.url,
                external: file.external,
                host: file.host,
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

    let mut items: Vec<HubItem> = Vec::new();
    let mut slugs: HashSet<String> = HashSet::new();
    for row in rows {
        let multi = counts.get(row.product.as_str()).copied().unwrap_or(0) > 1;
        let label = row.file_label.trim();
        let product = match (row.product.trim(), label) {
            ("", "") => "Untitled".to_string(),
            ("", label) => label.to_string(),
            (product, _) => product.to_string(),
        };
        let title = match (row.product.trim().is_empty(), multi, label) {
            (false, true, label) if !label.is_empty() => format!("{product} — {label}"),
            _ => product.clone(),
        };

        items.push(HubItem {
            id: items.len() as u64 + 1,
            slug: slug_for(&row.link, &product, label, multi, &mut slugs),
            title,
            product,
            file_label: row.file_label.trim().to_string(),
            link: row.link.clone(),
            date: String::new(),
            image: None,
            category_id: 0,
            author: None,
            external: row.external,
            host: row.host.clone(),
            download_url: row.download_url.clone(),
        });
    }
    items
}

/// The product's own URL slug where the row linked to its page, else one made from its name.
///
/// It has to be stable and unique per row: the install queue keys its cancel token, its staging
/// directory and its progress card on this, so two purchases sharing a slug would cancel each
/// other's download. Two things can collide — a product that ships several files, and (in the
/// text-only fallback) two rows whose link text happens to match — so the file label
/// disambiguates the first and `seen` guarantees the rest.
fn slug_for(
    link: &str,
    product: &str,
    file_label: &str,
    multi: bool,
    seen: &mut HashSet<String>,
) -> String {
    let base = product_slug(link)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| slugify(product));
    let base = if base.is_empty() {
        "hub-download".to_string()
    } else {
        base
    };

    let mut slug = match (multi, slugify(file_label)) {
        (true, label) if !label.is_empty() => format!("{base}-{label}"),
        _ => base,
    };
    // Whatever is left over. A numeric suffix is ugly and never normally reached; two installs
    // silently cancelling each other is worse.
    if seen.contains(&slug) {
        let stem = slug.clone();
        let mut n = 2;
        while seen.contains(&slug) {
            slug = format!("{stem}-{n}");
            n += 1;
        }
    }
    seen.insert(slug.clone());
    slug
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

/// One row's file, and whether the store is serving it or handing it off.
struct FileLink {
    url: String,
    external: bool,
    host: String,
}

/// What a download button on the account page points at.
///
/// Two shapes are legitimate, and they are *not* interchangeable:
///
///  - The store's own signed `?download_file=…&order=…&key=…` URL. Fetched with the user's
///    session, because that is what authorises it.
///  - A link to a file host, which WooCommerce allows for any downloadable product and which
///    MXB Hub uses for a number of its free mods. Fetched with the ordinary download client
///    and resolved first — a MediaFire *folder* is a web page listing files, not a file.
///
/// The distinction is the security-relevant part. Sending the user's store cookies to whatever
/// third-party URL happens to be on the page is exactly the mistake to avoid, so the answer
/// carries which kind it is rather than leaving the caller to guess from the host.
///
/// Anything that isn't `https` is refused outright, as is a store URL that is plainly a page
/// rather than a file.
fn download_link(raw: &str) -> Option<FileLink> {
    let decoded = html_escape::decode_html_entities(raw.trim()).into_owned();
    let url = reqwest::Url::parse(&decoded).ok()?;
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();

    if host == "mxb-hub.com" || host.ends_with(".mxb-hub.com") {
        // On the store, only a signed file link counts. Without this the "View" and product
        // links sitting in the same table would each be offered as a download.
        return url
            .query_pairs()
            .any(|(k, _)| k == "download_file")
            .then(|| FileLink {
                url: url.to_string(),
                external: false,
                host: String::new(),
            });
    }

    Some(FileLink {
        url: url.to_string(),
        external: true,
        host,
    })
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
        assert_eq!(items[1].slug, "tld-se5-psd-se5-psd");
        assert_eq!(items[2].slug, "tld-se5-psd-se5-pnt");
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

    /// Two rows that name themselves identically still get their own identity — the install
    /// queue keys its cancel token on this, so a collision means one download killing another.
    #[test]
    fn identical_rows_never_share_a_slug() {
        let html = r#"<ul>
          <li><a href="https://shop.mxb-hub.com/?download_file=1&amp;key=a">Paint.pkz</a></li>
          <li><a href="https://shop.mxb-hub.com/?download_file=2&amp;key=b">Paint.pkz</a></li>
        </ul>"#;
        let items = parse_downloads(html);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].slug, "paint-pkz");
        assert_eq!(items[1].slug, "paint-pkz-2");
    }

    /// The security boundary. A store link is fetched with the user's session attached; an
    /// off-store one must never be, and the flag is what keeps those two apart.
    #[test]
    fn store_links_and_handed_off_links_are_told_apart() {
        let store = download_link("https://shop.mxb-hub.com/?download_file=1&key=k").unwrap();
        assert!(!store.external);
        assert!(store.host.is_empty());

        // WooCommerce lets a product's file live anywhere, and MXB Hub uses that for some of
        // its free mods. Kept, but marked — never fetched with the store's cookies.
        let away = download_link("https://www.mediafire.com/folder/abc123/paint").unwrap();
        assert!(away.external);
        assert_eq!(away.host, "www.mediafire.com");

        for bad in [
            // On the store, but a page rather than a file.
            "https://shop.mxb-hub.com/my-account/",
            "https://shop.mxb-hub.com/product/a-paint/",
            // Not https.
            "http://www.mediafire.com/file/x",
            "javascript:alert(1)",
            "",
        ] {
            assert!(download_link(bad).is_none(), "{bad} must not be downloadable");
        }
    }

    /// A free mod whose file is a MediaFire folder has to reach the grid. Dropping it — which
    /// is what keying on `download_file=` alone did — loses the mod with nothing said.
    #[test]
    fn an_external_file_still_produces_a_row() {
        let html = r#"<table><tr>
            <td class="download-product"><a href="https://shop.mxb-hub.com/product/red-bud-kxf-paint/">RED BUD KXF [PAINT]</a></td>
            <td class="download-file"><a class="woocommerce-MyAccount-downloads-file"
               href="https://www.mediafire.com/folder/abc123/redbud">RED BUD KXF</a></td>
        </tr></table>"#;
        let items = parse_downloads(html);
        assert_eq!(items.len(), 1);
        assert!(items[0].external);
        assert_eq!(items[0].host, "www.mediafire.com");
        assert_eq!(items[0].slug, "red-bud-kxf-paint");
        assert_eq!(items[0].product, "RED BUD KXF [PAINT]");
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
