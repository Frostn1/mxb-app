//! Tests for the shop catalog.
//!
//! Everything here runs against a checked-in fixture rather than the network, because the
//! interesting behaviour — what a price means, whether a sale is live, whether page 2 agrees
//! with page 1 — is decided by our code, not the store's availability. The one test that does
//! hit the network is `#[ignore]`d and exists to tell us what the real payload looks like.

use super::*;

const SAMPLE: &str = include_str!("../fixtures/frostmod-mods.sample.json");

/// The fixture's `generated_ts`: 2026-08-07T22:05:45Z. Every sale window in the fixture is
/// positioned relative to this, so "now" is a constant and the tests don't rot.
const NOW: i64 = 1_786_140_345;

fn sample() -> (Catalog, CategoryIndex) {
    build(parse_dump(SAMPLE, NOW).expect("fixture should parse"))
}

fn item<'a>(catalog: &'a Catalog, id: u64) -> &'a Item {
    catalog
        .items
        .iter()
        .find(|i| i.id == id)
        .unwrap_or_else(|| panic!("fixture should contain item {id}"))
}

fn ids(catalog: &Catalog, selected: &[usize]) -> Vec<u64> {
    selected.iter().map(|&i| catalog.items[i].id).collect()
}

// ───────────────────────────── parsing and tolerance ─────────────────────────────

#[test]
fn parses_the_sample_dump() {
    let (catalog, _) = sample();
    assert_eq!(catalog.currency, "USD");
    assert_eq!(catalog.generated_ts, Some(NOW));
    assert!(catalog.items.len() >= 9);
}

#[test]
fn tolerates_unknown_fields_at_both_levels() {
    let (catalog, _) = sample();
    // `some_future_top_level_field` and `bundle_contents`/`vendor_rating` are in the fixture
    // precisely so that adding a field on the store's side can't break us.
    let juliett = item(&catalog, 110);
    assert_eq!(juliett.title, "Juliett Unknown Fields");
}

#[test]
fn keeps_an_item_that_has_only_an_id_and_a_title() {
    let (catalog, _) = sample();
    let hotel = item(&catalog, 108);
    assert_eq!(hotel.base, None);
    assert!(hotel.image.is_none());
}

#[test]
fn skips_an_item_with_no_id() {
    let (catalog, _) = sample();
    assert!(
        !catalog.items.iter().any(|i| i.title == "India Has No Id"),
        "an item with no id can't be linked to, so it must not be listed"
    );
}

#[test]
fn falls_back_to_a_placeholder_title_when_only_an_id_is_present() {
    let raw: RawMod = serde_json::from_str(r#"{"id": 77}"#).unwrap();
    let mapped = map_mod(raw, &CategoryIndex::default()).expect("an id alone is enough");
    assert_eq!(mapped.title, "Item 77");
}

// ───────────────────────────── prices ─────────────────────────────

#[test]
fn prices_parse_from_numbers_and_from_strings() {
    let (catalog, _) = sample();
    assert_eq!(item(&catalog, 101).base, Some(22.5));
    // "$24.99" / "19.99" — WooCommerce and EDD both emit prices as strings in some configs.
    let foxtrot = item(&catalog, 106);
    assert_eq!(foxtrot.base, Some(24.99));
    assert_eq!(foxtrot.sale, Some(19.99));
}

#[test]
fn strips_currency_symbols_and_separators() {
    assert_eq!(Num::S("$1,299.50".into()).as_f64(), Some(1299.50));
    assert_eq!(Num::S("  12.00 USD ".into()).as_f64(), Some(12.0));
    assert_eq!(Num::S("not a price".into()).as_f64(), None);
}

#[test]
fn a_live_sale_shows_both_prices_and_a_discount() {
    let (catalog, _) = sample();
    let price = price_of(item(&catalog, 101), NOW);
    assert!(price.on_sale);
    assert_eq!(price.base, Some(22.5));
    assert_eq!(price.sale, Some(11.25));
    assert_eq!(price.discount_pct, Some(50));
    assert_eq!(price.sale_ends, Some(1_790_000_000));
}

#[test]
fn has_range_only_when_price_max_exceeds_price() {
    let (catalog, _) = sample();
    assert!(price_of(item(&catalog, 104), NOW).has_range, "8.00–14.00 is a range");
    assert!(
        !price_of(item(&catalog, 101), NOW).has_range,
        "a null price_max is a single-option item"
    );

    // A price_max equal to price is a single-option item that reported both.
    let raw: RawMod =
        serde_json::from_str(r#"{"id":1,"title":"x","price":10,"price_max":10}"#).unwrap();
    let mapped = map_mod(raw, &CategoryIndex::default()).unwrap();
    assert!(!price_of(&mapped, NOW).has_range);
}

#[test]
fn a_range_that_is_also_on_sale_keeps_both_ends() {
    let (catalog, _) = sample();
    let price = price_of(item(&catalog, 105), NOW);
    assert!(price.on_sale && price.has_range);
    assert_eq!((price.base, price.base_max), (Some(10.0), Some(20.0)));
    assert_eq!((price.sale, price.sale_max), (Some(7.5), Some(15.0)));
    // From the low end of each range: (10 - 7.5) / 10.
    assert_eq!(price.discount_pct, Some(25));
}

#[test]
fn discount_is_rounded_down_rather_than_flattered() {
    let raw: RawMod =
        serde_json::from_str(r#"{"id":1,"title":"x","price":30,"sale_price":20}"#).unwrap();
    let mapped = map_mod(raw, &CategoryIndex::default()).unwrap();
    // 33.33% must not be shown as 34% off.
    assert_eq!(price_of(&mapped, NOW).discount_pct, Some(33));
}

// ───────────────────────────── sale windows ─────────────────────────────

#[test]
fn a_sale_with_no_window_is_live() {
    // The dump saying an item is discounted is the only signal we have; refusing to believe
    // it would hide every real sale.
    assert!(is_on_sale(Some(5.0), (None, None), NOW));
}

#[test]
fn a_sale_is_live_only_inside_its_window() {
    let window = (Some(1_785_000_000), Some(1_790_000_000));
    assert!(is_on_sale(Some(5.0), window, NOW));
    assert!(!is_on_sale(Some(5.0), window, 1_784_999_999), "before it starts");
    assert!(!is_on_sale(Some(5.0), window, 1_790_000_001), "after it ends");
}

#[test]
fn an_expired_window_hides_the_sale_even_though_sale_price_is_set() {
    // This is the whole reason windows are honoured — the dump still carries `sale_price`.
    let (catalog, _) = sample();
    let bravo = item(&catalog, 102);
    assert_eq!(bravo.sale, Some(5.0), "the raw sale price is still in the dump");

    let price = price_of(bravo, NOW);
    assert!(!price.on_sale);
    assert_eq!(price.sale, None, "a finished sale must not reach the UI");
    assert_eq!(price.discount_pct, None);
    assert_eq!(price.base, Some(20.0));
}

#[test]
fn a_sale_that_has_not_started_is_not_shown() {
    let (catalog, _) = sample();
    let price = price_of(item(&catalog, 103), NOW);
    assert!(!price.on_sale);
    assert_eq!(price.sale, None);
}

#[test]
fn sale_ends_is_absent_when_the_dump_carries_no_window() {
    let (catalog, _) = sample();
    // Foxtrot is on sale with no window at all — the UI must not invent an expiry for it.
    let price = price_of(item(&catalog, 106), NOW);
    assert!(price.on_sale);
    assert_eq!(price.sale_ends, None);
}

#[test]
fn effective_price_is_what_a_buyer_would_pay() {
    let (catalog, _) = sample();
    assert_eq!(effective_price(item(&catalog, 101), NOW), Some(11.25), "on sale");
    assert_eq!(effective_price(item(&catalog, 102), NOW), Some(20.0), "sale expired");
}

// ───────────────────────────── timestamps ─────────────────────────────

#[test]
fn parses_the_date_forms_a_wordpress_export_emits() {
    assert_eq!(parse_date_str("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(parse_date_str("2026-08-07T22:05:45+00:00"), Some(NOW));
    // The MySQL form WordPress stores natively, read as UTC.
    assert_eq!(parse_date_str("2026-08-07 22:05:45"), Some(NOW));
    assert_eq!(parse_date_str("2026-08-07"), Some(NOW - 79_545));
    assert_eq!(parse_date_str(""), None);
    assert_eq!(parse_date_str("tomorrow"), None);
}

#[test]
fn a_numeric_offset_is_subtracted_to_reach_utc() {
    let utc = parse_date_str("2026-08-07T22:05:45Z").unwrap();
    assert_eq!(parse_date_str("2026-08-08T00:05:45+02:00"), Some(utc));
    assert_eq!(parse_date_str("2026-08-07T20:05:45-0200"), Some(utc));
}

#[test]
fn milliseconds_are_recognised_and_scaled() {
    assert_eq!(parse_timestamp(&serde_json::json!(1_786_140_345_000i64)), Some(NOW));
    assert_eq!(parse_timestamp(&serde_json::json!(1786140345)), Some(NOW));
}

// ───────────────────────────── categories ─────────────────────────────

#[test]
fn flattens_a_nested_tree_with_depth() {
    let (_, index) = sample();
    let bikes = index.flat.iter().find(|c| c.id == 10).unwrap();
    let two_fifty = index.flat.iter().find(|c| c.id == 11).unwrap();
    assert_eq!(bikes.depth, 0);
    assert_eq!(two_fifty.depth, 1);
    assert_eq!(two_fifty.parent, Some(10));
}

#[test]
fn flattens_a_flat_parent_linked_tree_too() {
    let dump = r#"{"categories":[
        {"id":1,"name":"Bikes"},
        {"id":2,"name":"250cc","parent":1},
        {"id":3,"name":"Two Stroke","parent":2}
    ],"mods":[]}"#;
    let (_, index) = build(parse_dump(dump, NOW).unwrap());
    let depth = |id: u64| index.flat.iter().find(|c| c.id == id).unwrap().depth;
    assert_eq!((depth(1), depth(2), depth(3)), (0, 1, 2));
}

#[test]
fn survives_a_cyclic_parent_chain() {
    // Third-party data; a cycle must not spin the flattener or the descendant walk.
    let dump = r#"{"categories":[
        {"id":1,"name":"A","parent":2},
        {"id":2,"name":"B","parent":1}
    ],"mods":[]}"#;
    let (_, index) = build(parse_dump(dump, NOW).unwrap());
    assert_eq!(index.flat.len(), 2);
    assert!(index.descendants.contains_key(&1));
}

#[test]
fn filtering_by_a_parent_includes_its_children() {
    let (catalog, index) = sample();
    // 103 is in 250cc (11), 105 and 107 are in 450cc (12) — all beneath Bikes (10).
    let selected = select(&catalog, &index, "", Some(10), ShopSort::NameAsc, false, NOW);
    let found = ids(&catalog, &selected);
    assert!(found.contains(&103), "a 250cc item belongs under Bikes");
    assert!(found.contains(&107), "a 450cc item belongs under Bikes");
    assert!(!found.contains(&101), "a track does not");
}

#[test]
fn category_counts_include_descendants_and_never_double_count() {
    let (_, index) = sample();
    let bikes = index.flat.iter().find(|c| c.id == 10).unwrap();
    // 103 (250cc), 105 (450cc + Paints), 106 (250cc), 107 (450cc) — four distinct items,
    // and 105 must count once even though it sits in two branches.
    assert_eq!(bikes.count, 4);
}

#[test]
fn synthesises_categories_when_the_dump_has_none() {
    let dump = r#"{"mods":[{"id":1,"title":"x","category":[42]}]}"#;
    let (_, index) = build(parse_dump(dump, NOW).unwrap());
    assert_eq!(index.flat.len(), 1, "the filter row must never be empty");
    assert_eq!(index.flat[0].id, 42);
}

/// The live catalog identifies a mod's categories as `{"name", "slug"}` with **no id**, so
/// the slug is the only link back to the tree. The id-shaped forms are kept working so a
/// future export that switches to ids doesn't silently uncategorise everything.
#[test]
fn resolves_a_mods_categories_by_slug() {
    let (catalog, _) = sample();
    let alpha = item(&catalog, 101);
    assert_eq!(alpha.category_ids, vec![20], "a name/slug object — the real shape");
    assert_eq!(alpha.category_names, vec!["Tracks".to_string()]);

    assert_eq!(item(&catalog, 106).category_ids, vec![11], "a bare slug string");
    assert_eq!(item(&catalog, 107).category_ids, vec![12], "an object carrying an id");
}

/// Names come from the tree, not from the mod, so a card's label always matches the filter
/// pill that selects it.
#[test]
fn category_names_come_from_the_tree() {
    let (catalog, _) = sample();
    assert_eq!(
        item(&catalog, 105).category_names,
        vec!["Paints".to_string(), "450cc".to_string()]
    );
}

#[test]
fn an_unknown_slug_is_dropped_rather_than_guessed() {
    let dump = r#"{"categories":[{"id":1,"name":"Tracks","slug":"tracks"}],
                   "mods":[{"id":9,"name":"x","categories":[{"name":"Nope","slug":"nope"}]}]}"#;
    let (catalog, _) = build(parse_dump(dump, NOW).unwrap());
    assert!(catalog.items[0].category_ids.is_empty());
}

// ───────────────────────────── search ─────────────────────────────

#[test]
fn search_matches_title_and_author_case_insensitively() {
    let (catalog, index) = sample();
    let by_author = select(&catalog, &index, "MOUK", None, ShopSort::NameAsc, false, NOW);
    assert_eq!(ids(&catalog, &by_author), vec![103, 104, 105]);
}

#[test]
fn search_requires_every_term() {
    let (catalog, index) = sample();
    let both = select(&catalog, &index, "alpha supercross", None, ShopSort::NameAsc, false, NOW);
    assert_eq!(ids(&catalog, &both), vec![101]);

    let neither = select(&catalog, &index, "alpha bravo", None, ShopSort::NameAsc, false, NOW);
    assert!(neither.is_empty(), "terms narrow, they don't widen");
}

#[test]
fn punctuation_does_not_hide_a_match() {
    let (catalog, index) = sample();
    // The author is "Yamaha YZ-250 Fan"; searching "yz 250" should still find it.
    let found = select(&catalog, &index, "yz 250", None, ShopSort::NameAsc, false, NOW);
    assert_eq!(ids(&catalog, &found), vec![106]);
}

#[test]
fn on_sale_only_uses_the_window_not_just_the_field() {
    let (catalog, index) = sample();
    let found = ids(
        &catalog,
        &select(&catalog, &index, "", None, ShopSort::NameAsc, true, NOW),
    );
    assert!(found.contains(&101), "live sale");
    assert!(found.contains(&105), "live sale, ranged");
    assert!(found.contains(&106), "live sale, no window");
    assert!(!found.contains(&102), "expired");
    assert!(!found.contains(&103), "not started");
}

// ───────────────────────────── sorting and paging ─────────────────────────────

const ALL_SORTS: [ShopSort; 6] = [
    ShopSort::Newest,
    ShopSort::RecentlyUpdated,
    ShopSort::PriceAsc,
    ShopSort::PriceDesc,
    ShopSort::OnSale,
    ShopSort::NameAsc,
];

#[test]
fn every_sort_is_total() {
    let (catalog, _) = sample();
    for sort in ALL_SORTS {
        let mut a: Vec<usize> = (0..catalog.items.len()).collect();
        let mut b: Vec<usize> = a.clone();
        b.reverse();
        sort_items(&mut a, &catalog.items, sort, NOW);
        sort_items(&mut b, &catalog.items, sort, NOW);
        assert_eq!(
            ids(&catalog, &a),
            ids(&catalog, &b),
            "{sort:?} must not depend on the order it was given"
        );
    }
}

#[test]
fn page_two_never_repeats_page_one() {
    // Without a total tiebreak in the comparator this fails silently, as duplicated cards.
    let (catalog, index) = sample();
    for sort in ALL_SORTS {
        let selected = select(&catalog, &index, "", None, sort, false, NOW);
        let all = ids(&catalog, &selected);
        let mut seen = HashSet::new();
        for id in &all {
            assert!(seen.insert(*id), "{sort:?} listed item {id} twice");
        }
        assert_eq!(all.len(), catalog.items.len(), "{sort:?} dropped items");
    }
}

#[test]
fn price_sorting_uses_the_price_a_buyer_would_pay() {
    let (catalog, index) = sample();
    let selected = select(&catalog, &index, "", None, ShopSort::PriceAsc, false, NOW);
    let ordered = ids(&catalog, &selected);
    let pos = |id: u64| ordered.iter().position(|x| *x == id).unwrap();
    // 101 is 22.50 normally but 11.25 on sale, so it sorts below 102 at 20.00.
    assert!(pos(101) < pos(102));
    // The item with no price at all sorts last, not first.
    assert_eq!(*ordered.last().unwrap(), 108);
}

#[test]
fn on_sale_sorting_leads_with_the_deepest_discount() {
    let (catalog, index) = sample();
    let ordered = ids(
        &catalog,
        &select(&catalog, &index, "", None, ShopSort::OnSale, false, NOW),
    );
    // 112 is 100% off, 101 is 50%, 105 is 25% — everything not on sale comes after.
    assert_eq!(&ordered[..3], &[112, 101, 105]);
}

#[test]
fn newest_falls_back_to_updated_when_there_is_no_published_date() {
    let (catalog, _) = sample();
    let hotel = item(&catalog, 108);
    assert!(hotel.published.is_none() && hotel.updated.is_none());
    // It must still be sortable rather than panicking or vanishing.
    let mut indices: Vec<usize> = (0..catalog.items.len()).collect();
    sort_items(&mut indices, &catalog.items, ShopSort::Newest, NOW);
    assert_eq!(indices.len(), catalog.items.len());
}

// ───────────────────────────── URL and HTML safety ─────────────────────────────

#[test]
fn a_buy_url_must_be_https_on_the_store() {
    assert!(safe_shop_url("https://mxbikes-shop.com/product/x/").is_some());
    assert!(safe_shop_url("https://cdn.mxbikes-shop.com/product/x/").is_some());
    assert!(safe_shop_url("http://mxbikes-shop.com/product/x/").is_none(), "not https");
    assert!(safe_shop_url("https://evil.example.com/x").is_none(), "off origin");
    assert!(safe_shop_url("javascript:alert(1)").is_none());
    assert!(safe_shop_url("file:///etc/passwd").is_none());
    // A lookalike domain must not pass the suffix check.
    assert!(safe_shop_url("https://notmxbikes-shop.com/x").is_none());
}

#[test]
fn an_unsafe_url_disables_the_link_rather_than_the_item() {
    let (catalog, _) = sample();
    let golf = item(&catalog, 107);
    assert_eq!(golf.title, "Golf No Image", "the item is still listed");
    assert!(golf.url.is_none(), "its http:// url was dropped");
    assert!(golf.author_url.is_none(), "its off-origin author url was dropped");
}

#[test]
fn description_html_is_stripped_of_anything_executable() {
    let (catalog, _) = sample();
    let html = item(&catalog, 110)
        .description_html
        .as_deref()
        .expect("the fixture has a description");
    let lower = html.to_ascii_lowercase();
    assert!(!lower.contains("<script"), "script element must be gone");
    assert!(!lower.contains("alert("), "script contents must be gone");
    assert!(!lower.contains("onclick"), "event attributes must be gone");
    assert!(!lower.contains("javascript:"), "javascript: urls must be gone");
    assert!(lower.contains("great track"), "the actual text must survive");
}

#[test]
fn stripping_does_not_eat_a_tag_that_merely_starts_the_same_way() {
    let html = sanitize_html("<p>a</p><scriptish>b</scriptish>").unwrap();
    assert!(html.contains("scriptish"), "<scriptish> is not <script>");
}

#[test]
fn an_unterminated_script_tag_still_gets_dropped() {
    let html = sanitize_html("<p>keep</p><script>never closed").unwrap();
    assert!(!html.to_ascii_lowercase().contains("<script"));
    assert!(html.contains("keep"));
}

#[test]
fn an_empty_description_becomes_none() {
    assert!(sanitize_html("   ").is_none());
    assert!(sanitize_html("").is_none());
}

// ───────────────────────────── freshness ─────────────────────────────

#[test]
fn staleness_follows_the_ttl() {
    let (mut catalog, _) = sample();
    catalog.fetched_at = now();
    assert!(!catalog.stale());

    catalog.fetched_at = now() - CATALOG_TTL.as_secs() as i64 - 1;
    assert!(catalog.stale());
    assert!(!catalog.very_stale());

    catalog.fetched_at = now() - MAX_STALE.as_secs() as i64 - 1;
    assert!(catalog.very_stale());
}

#[test]
fn a_cloudflare_block_page_is_recognised_in_both_shapes() {
    assert!(is_block_page("<title>Just a moment...</title>"), "the JS interstitial");
    assert!(
        is_block_page("<h1>Attention Required! | Cloudflare</h1>Sorry, you have been blocked"),
        "the WAF firewall page, which is what this endpoint actually returns"
    );
    assert!(
        !is_block_page(r#"{"schema":3,"mods":[]}"#),
        "an ordinary catalog must not look like a block"
    );
    assert!(
        !is_block_page("<p>challenge-platform script tag</p>"),
        "Cloudflare injects that into ordinary pages too — matching it would condemn them"
    );
}

#[test]
fn a_403_is_clearable_but_a_429_is_not() {
    let blocked = |code| {
        blocked_error(Some(code))
            .downcast::<Blocked>()
            .expect("must stay a Blocked so the command layer can act on it")
    };
    assert!(blocked(403).clearable());
    assert!(!blocked(429).clearable());
    assert!(
        blocked_error(None).downcast::<Blocked>().unwrap().clearable(),
        "an interstitial served as a 200"
    );
}

// ───────────────────────────── live ─────────────────────────────

/// Prints what the store actually sends, so [`map_mod`]'s guessed field names can be
/// corrected against reality rather than against a hunch.
///
/// Run with:
///     cargo test live_shop_catalog_shape -- --ignored --nocapture
#[tokio::test]
#[ignore = "hits the live mxbikes-shop.com catalog endpoint"]
async fn live_shop_catalog_shape() {
    if !shop_credentials::present() {
        eprintln!("no shop credential in this build — copy .env.local.example to .env.local");
        return;
    }

    let fetched = match fetch(None).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("fetch failed: {e}");
            return;
        }
    };
    let body = fetched.body.expect("an unconditional fetch returns a body");

    let raw: RawDump = serde_json::from_str(&body).expect("the catalog should be JSON");
    eprintln!("mods: {}", raw.mods.len());

    // The union of everything we did not name. This is the list to fix `map_mod` against.
    let mut unknown: Vec<String> = raw
        .mods
        .iter()
        .flat_map(|m| m.extra.keys().cloned())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    unknown.sort();
    eprintln!("unrecognised per-mod keys: {unknown:?}");

    let parsed = parse_dump(&body, now()).expect("the catalog should map");
    let now = now();
    let (catalog, index) = build(parsed);

    let free = catalog.items.iter().filter(|i| i.free).count();
    let on_sale = catalog
        .items
        .iter()
        .filter(|i| is_on_sale(i.sale, i.sale_window, now))
        .count();
    let ranged = catalog.items.iter().filter(|i| i.base_max.is_some()).count();
    let uncategorised = catalog.items.iter().filter(|i| i.category_ids.is_empty()).count();

    eprintln!(
        "mapped {} items, {} categories, currency {} — {free} free, {on_sale} on sale, \
         {ranged} ranged, {uncategorised} uncategorised",
        catalog.items.len(),
        catalog.categories.len(),
        catalog.currency,
    );
    for item in catalog.items.iter().take(3) {
        eprintln!("{item:#?}");
    }

    // These are the assertions that make this a canary rather than a printer: if the store
    // renames a field or changes a shape, one of them fails loudly instead of the app quietly
    // showing a catalog full of untitled, uncategorised, unpriced items.
    assert_eq!(raw.mods.len(), catalog.items.len(), "every item should map");
    assert!(
        unknown.is_empty(),
        "the store added fields we ignore ({unknown:?}) — check whether map_mod should read them"
    );
    assert!(!catalog.categories.is_empty(), "the category tree should resolve");
    assert!(
        uncategorised * 10 < catalog.items.len(),
        "{uncategorised} of {} items resolved to no category — the slug link is probably broken",
        catalog.items.len()
    );
    assert!(
        catalog.items.iter().all(|i| !i.title.starts_with("Item ")),
        "an item fell back to a placeholder title, so the name field moved"
    );
    assert!(
        catalog.items.iter().filter(|i| i.url.is_some()).count() * 10
            > catalog.items.len() * 9,
        "most items should have a usable product URL"
    );

    // A spot-check that the category filter actually selects things through the slug link.
    let tracks = index
        .flat
        .iter()
        .find(|c| c.slug == "tracks")
        .expect("the store has a 'tracks' category");
    let selected = select(&catalog, &index, "", Some(tracks.id), ShopSort::NameAsc, false, now);
    eprintln!("category '{}' selects {} items", tracks.name, selected.len());
    assert!(!selected.is_empty(), "filtering by 'tracks' should select something");
}

// ───────────────────────────── free, and 100% off ─────────────────────────────

/// The catalog carries 82 genuinely free items alongside pay-what-you-want ones that merely
/// *start* at 0, so the flag and the price are separate questions.
#[test]
fn a_free_item_is_flagged_rather_than_priced_at_zero() {
    let (catalog, _) = sample();
    let price = price_of(item(&catalog, 111), NOW);
    assert!(price.free, "the store says this one is free");
    assert_eq!(price.base, Some(0.0));
    assert!(!price.on_sale);
}

#[test]
fn a_priced_item_is_never_marked_free() {
    let (catalog, _) = sample();
    assert!(!price_of(item(&catalog, 101), NOW).free);

    // Starting at 0 with a range is pay-what-you-want, not free.
    let dump = r#"{"mods":[{"id":1,"name":"x","price":0,"price_max":2,"free":false}]}"#;
    let (catalog, _) = build(parse_dump(dump, NOW).unwrap());
    let price = price_of(&catalog.items[0], NOW);
    assert!(!price.free);
    assert!(price.has_range);
}

/// A sale price of exactly 0 is a real 100%-off promotion, and the catalog has them. Treating
/// 0 as "no sale" would show full price on the best deals in the store.
#[test]
fn a_hundred_percent_off_sale_is_live_not_ignored() {
    let (catalog, _) = sample();
    let price = price_of(item(&catalog, 112), NOW);
    assert!(price.on_sale);
    assert_eq!(price.base, Some(6.0));
    assert_eq!(price.sale, Some(0.0));
    assert_eq!(price.discount_pct, Some(100));
}

#[test]
fn a_negative_sale_price_is_refused() {
    assert!(!is_on_sale(Some(-1.0), (None, None), NOW));
}

/// The live catalog has no `published` field at all — only `updated` — so date ordering has
/// to fall back to it rather than dropping every item.
#[test]
fn ordering_survives_a_catalog_with_no_published_dates() {
    let dump = r#"{"mods":[
        {"id":1,"name":"a","updated":1786000000},
        {"id":2,"name":"b","updated":1786100000}
    ]}"#;
    let (catalog, index) = build(parse_dump(dump, NOW).unwrap());
    let ordered = ids(
        &catalog,
        &select(&catalog, &index, "", None, ShopSort::Newest, false, NOW),
    );
    assert_eq!(ordered, vec![2, 1], "newest falls back to `updated`");
}
