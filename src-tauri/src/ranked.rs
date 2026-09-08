//! MXB Ranked — a player's rank, season stats and last races.
//!
//! There is nothing to sign into. `mxb-ranked.com/Login` is Steam-only and exists so you can
//! *edit* your profile; reading one is public and server-rendered, so `GET /Rider/{guid}`
//! answers with the whole thing to anyone who asks.
//!
//! The GUID is not a third identity to collect either: every rider link on the site is `FF`
//! followed by the SteamID64 in hex (checked against all 35 rider ids on their Records,
//! Results and Season Ranks pages, 2026-09-08), and [`crate::steamid`] already reads the
//! signed-in SteamID64 off Steam's own `loginusers.vdf`, offline. So the player's own profile
//! costs one request and no setup at all.
//!
//! The exception is someone who bought MX Bikes direct rather than on Steam: they set a
//! stand-alone GUID on the site, and it is not derived from anything we can see. They pass it
//! in by hand, which is the same door used to look up a friend.
//!
//! Everything here is best-effort. The page is a Blazor app whose markup will move, so a
//! section that doesn't parse comes back empty rather than failing the request — one renamed
//! `<div>` must not blank the tab. The parts most likely to survive are what the parser leans
//! on: the table's `data-label` attributes, and label text like `Rank:` that the site prints
//! because a human reads it.

use scraper::{ElementRef, Html, Selector};
use serde::Serialize;

pub const BASE: &str = "https://mxb-ranked.com";

/// One discipline's standing in the selected season — the site shows Global, MX and SX side
/// by side, and they are the same shape.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RankCard {
    /// "Global", "MX", "SX".
    pub discipline: String,
    /// Position on the leaderboard.
    pub rank: String,
    /// The short badge the site prints — "S1" for Silver 1.
    pub badge: String,
    /// The badge spelled out, off the card's `title`.
    pub rank_name: String,
    /// The rank's own colour, so our card can look like theirs.
    pub color: String,
    pub mxp: String,
    pub races: String,
    pub avg_position: String,
    pub wins: String,
    pub podiums: String,
    pub wr_laps: String,
    pub pb_laps: String,
    pub holeshots: String,
}

/// One row of the last-50 table.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RaceRow {
    /// The lobby's full name — "MXB-Ranked.com | MX Freebies | 250 | G+ | NA | #99684".
    pub server: String,
    /// The track as ranked names it, which is the string the pack matcher will want.
    pub track: String,
    pub category: String,
    /// "2 / 3" — finishing position over starters.
    pub position: String,
    pub bike: String,
    pub mxp: String,
    pub global_mxp: String,
    /// "up" when the round gained MXP, "down" when it lost, empty when the page didn't say.
    pub mxp_dir: String,
    pub penalty_points: String,
    /// As the site writes it — "13 days ago".
    pub finished: String,
    /// The absolute timestamp behind that, from the cell's tooltip.
    pub finished_at: String,
    pub results_url: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RankedProfile {
    pub guid: String,
    pub name: String,
    /// Two-letter country from the flag sprite, lowercase; "un" when they haven't set one.
    pub country: String,
    pub exp: String,
    pub member_since: String,
    /// A–E, the site's own behaviour grade.
    pub rating: String,
    pub penalty_points: String,
    /// The global average the site prints beside it, which is the only thing that makes the
    /// number above mean anything.
    pub global_penalty_points: String,
    /// Which season the numbers below are for — "Season 7".
    pub season: String,
    pub cards: Vec<RankCard>,
    pub races: Vec<RaceRow>,
    /// The rider's own achievement banner on mxb-ranked, absolute. Empty when they have none.
    ///
    /// Earned artwork, and different for every rider — which is the one thing on the page that
    /// is *theirs* rather than a number about them, so it is worth carrying across.
    pub banner: String,
    /// The profile on their site, for the "open on mxb-ranked" link.
    pub url: String,
}

/// MX Bikes' GUID for a Steam account: `FF` + the SteamID64 as 16 uppercase hex digits.
pub fn guid_from_steam_id64(id: &str) -> Option<String> {
    let n: u64 = id.trim().parse().ok()?;
    // Below the base is not an account id at all, and formatting it would produce a GUID that
    // looks right and belongs to nobody.
    (n >= 76_561_197_960_265_728).then(|| format!("FF{n:016X}"))
}

/// The signed-in Steam account's GUID, or `None` when Steam can't be read.
pub fn local_guid() -> Option<String> {
    guid_from_steam_id64(&crate::steamid::current_steam_id64()?)
}

/// Tidy a GUID a person typed or pasted — from the site, a URL, or with spaces in it.
pub fn normalise_guid(raw: &str) -> Option<String> {
    let cut = raw.trim().trim_end_matches('/');
    let tail = cut.rsplit('/').next().unwrap_or(cut);
    let id: String = tail.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    (id.len() == 18).then(|| id.to_ascii_uppercase())
}

pub fn profile_url(guid: &str) -> String {
    format!("{BASE}/Rider/{guid}")
}

fn client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: std::sync::OnceLock<Result<reqwest::Client, String>> =
        std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .user_agent(crate::mxb_session::UA)
                .connect_timeout(std::time::Duration::from_secs(15))
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| format!("{e}"))
        })
        .as_ref()
        .map_err(|e| e.clone())
}

/// Fetch and parse one rider profile.
pub async fn fetch(guid: &str) -> Result<RankedProfile, String> {
    let Some(guid) = normalise_guid(guid) else {
        return Err(format!("{guid} isn't an MX Bikes GUID"));
    };
    let url = profile_url(&guid);
    let client = client()?;
    let res = client.get(&url).send().await.map_err(|e| format!("{e}"))?;
    if !res.status().is_success() {
        return Err(format!("mxb-ranked.com answered {}", res.status()));
    }
    let html = res.text().await.map_err(|e| format!("{e}"))?;
    Ok(parse(&html, &guid))
}

// ───────────────────────────────── parsing ─────────────────────────────────

/// Collapse an element's text the way a reader sees it.
fn text(el: ElementRef) -> String {
    el.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The value after `Label:` in a run of text, or empty.
fn after(s: &str, label: &str) -> String {
    s.split_once(label).map(|(_, v)| v.trim().to_string()).unwrap_or_default()
}

/// The first `<p>` under `root` whose text starts with `label`, minus the label.
fn labelled(root: ElementRef, label: &str) -> String {
    let p = Selector::parse("p").unwrap();
    root.select(&p)
        .map(text)
        .find(|t| t.starts_with(label))
        .map(|t| after(&t, label))
        .unwrap_or_default()
}

/// Pull `#RRGGBB` out of an inline style's `background`.
fn background(style: &str) -> String {
    style
        .split(';')
        .find_map(|d| {
            let (k, v) = d.split_once(':')?;
            k.trim().eq_ignore_ascii_case("background").then(|| v.trim().to_string())
        })
        .filter(|v| v.starts_with('#'))
        .unwrap_or_default()
}

pub fn parse(html: &str, guid: &str) -> RankedProfile {
    let doc = Html::parse_document(html);
    let mut out = RankedProfile { guid: guid.to_string(), url: profile_url(guid), ..Default::default() };

    // Name and EXP are both an <h6> in a chip, so they are told apart by what they say rather
    // than by where they sit — the chips move around with the achievement banner behind them.
    let chip_h6 = Selector::parse(".mud-chip-content h6").unwrap();
    for t in doc.select(&chip_h6).map(text) {
        match t.strip_suffix(" EXP") {
            Some(exp) => out.exp = exp.trim().to_string(),
            None if out.name.is_empty() => out.name = t,
            None => {}
        }
    }

    // The flag sprite carries the country as a class — `f-un` when unset.
    let flag = Selector::parse("span.flag").unwrap();
    out.country = doc
        .select(&flag)
        .next()
        .and_then(|el| {
            el.value().classes().find_map(|c| c.strip_prefix("f-").map(str::to_string))
        })
        .unwrap_or_default();

    let body = Selector::parse("body").unwrap();
    if let Some(body) = doc.select(&body).next() {
        // The GUID off the page wins over the one we asked with: a stand-alone rider looked up
        // by name would otherwise be labelled with the id we guessed.
        let printed = labelled(body, "GUID:");
        if !printed.is_empty() {
            out.guid = printed;
            out.url = profile_url(&out.guid);
        }
        out.member_since = labelled(body, "Member since:");
        // "1.731 (global avg = 2.697)" — two numbers in one sentence, because the site only
        // ever shows them together.
        let penalties = labelled(body, "Avg Penalty Points/Race:");
        let (mine, global) = penalties.split_once('(').unwrap_or((penalties.as_str(), ""));
        out.penalty_points = mine.trim().to_string();
        out.global_penalty_points =
            after(global, "=").trim_end_matches(')').trim().to_string();
    }

    // The grade is a class on its avatar, which is also what colours it.
    let rating = Selector::parse("div.mud-avatar").unwrap();
    out.rating = doc
        .select(&rating)
        .find_map(|el| {
            el.value().classes().find_map(|c| c.strip_prefix("playerRating-").map(str::to_string))
        })
        .unwrap_or_default();

    let season = Selector::parse(r#"[aria-label="Selected Season"]"#).unwrap();
    out.season = doc.select(&season).map(text).find(|t| !t.is_empty()).unwrap_or_default();

    // The banner is a CSS background rather than an <img>, so it is read off the style.
    let banner = Selector::parse(r#"[style*="Achievements"]"#).unwrap();
    out.banner = doc
        .select(&banner)
        .next()
        .and_then(|el| el.value().attr("style"))
        .and_then(|style| {
            let (_, rest) = style.split_once("url(")?;
            let (inner, _) = rest.split_once(')')?;
            Some(inner.trim().trim_matches(['\'', '"']).to_string())
        })
        .filter(|u| !u.is_empty())
        .map(|u| if u.starts_with("http") { u } else { format!("{BASE}/{}", u.trim_start_matches('/')) })
        .unwrap_or_default();

    out.cards = parse_cards(&doc);
    out.races = parse_races(&doc);
    out
}

fn parse_cards(doc: &Html) -> Vec<RankCard> {
    // A titled paper that prints "Rank:" is a standing; nothing else on the page is both.
    let paper = Selector::parse("div.mud-paper[title]").unwrap();
    let strong = Selector::parse("strong").unwrap();
    let badge = Selector::parse("div.mud-avatar strong").unwrap();
    let mxp = Selector::parse("h5 strong").unwrap();

    let mut out = Vec::new();
    for el in doc.select(&paper) {
        let whole = text(el);
        if !whole.contains("Rank:") {
            continue;
        }
        let rank_line = el.select(&strong).map(text).find(|t| t.starts_with("Rank:"));
        out.push(RankCard {
            discipline: el.select(&strong).map(text).next().unwrap_or_default(),
            rank: rank_line.map(|t| after(&t, "Rank:")).unwrap_or_default(),
            badge: el.select(&badge).next().map(text).unwrap_or_default(),
            rank_name: el.value().attr("title").unwrap_or_default().to_string(),
            color: background(el.value().attr("style").unwrap_or_default()),
            mxp: el.select(&mxp).next().map(text).unwrap_or_default(),
            races: labelled(el, "Races:"),
            avg_position: labelled(el, "Avg Position:"),
            wins: labelled(el, "Wins:"),
            podiums: labelled(el, "Podiums:"),
            wr_laps: labelled(el, "WR Laps:"),
            pb_laps: labelled(el, "PB Laps:"),
            holeshots: labelled(el, "Holeshots:"),
        });
    }
    out
}

fn parse_races(doc: &Html) -> Vec<RaceRow> {
    let row = Selector::parse("tbody tr").unwrap();
    let link = Selector::parse(r#"a[href*="/Results/"]"#).unwrap();
    // Every cell is tagged with the column it belongs to, which is the one thing on this page
    // that survives a re-style — so the columns are read by name and never by position.
    let cell = |r: ElementRef, label: &str| -> String {
        Selector::parse(&format!(r#"td[data-label="{label}"]"#))
            .ok()
            .and_then(|s| r.select(&s).next().map(text))
            .unwrap_or_default()
    };

    let mut out = Vec::new();
    for r in doc.select(&row) {
        let server = cell(r, "Server");
        let track = cell(r, "Track");
        if server.is_empty() && track.is_empty() {
            continue;
        }
        // The arrow on the MXP chip is the only thing that says which way the round went; the
        // number itself is printed unsigned.
        let dir = if r.html().contains("fa-arrow-up") {
            "up"
        } else if r.html().contains("fa-arrow-down") {
            "down"
        } else {
            ""
        };
        let finished_at = Selector::parse(r#"td[data-label="Finished"]"#)
            .ok()
            .and_then(|s| r.select(&s).next().and_then(|c| c.value().attr("title").map(str::to_string)))
            .unwrap_or_default();

        out.push(RaceRow {
            server,
            track,
            category: cell(r, "Category"),
            position: cell(r, "Position"),
            bike: cell(r, "Bike"),
            mxp: cell(r, "MXP"),
            global_mxp: cell(r, "Global MXP"),
            mxp_dir: dir.to_string(),
            penalty_points: cell(r, "Pen. Points"),
            finished: cell(r, "Finished"),
            finished_at,
            results_url: r
                .select(&link)
                .next()
                .and_then(|a| a.value().attr("href"))
                .map(|h| format!("{BASE}{h}"))
                .unwrap_or_default(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("fixtures/rider-profile.sample.html");

    #[test]
    fn guid_is_the_steam_id_in_hex() {
        // The rider whose page the fixture came from.
        assert_eq!(guid_from_steam_id64("76561197984950104").as_deref(), Some("FF011000010178A758"));
        // Not an account id, and not a number.
        assert_eq!(guid_from_steam_id64("0"), None);
        assert_eq!(guid_from_steam_id64("not a number"), None);
    }

    #[test]
    fn a_pasted_guid_is_tidied() {
        assert_eq!(normalise_guid(" ff011000010178a758 ").as_deref(), Some("FF011000010178A758"));
        assert_eq!(
            normalise_guid("https://mxb-ranked.com/Rider/FF011000010178A758").as_deref(),
            Some("FF011000010178A758")
        );
        assert_eq!(normalise_guid("FF0110"), None);
    }

    #[test]
    fn header_reads_off_the_page() {
        let p = parse(SAMPLE, "FF011000010178A758");
        assert_eq!(p.name, "Rand");
        assert_eq!(p.exp, "2087");
        assert_eq!(p.guid, "FF011000010178A758");
        assert_eq!(p.member_since, "3/26/2025");
        assert_eq!(p.rating, "A");
        assert_eq!(p.penalty_points, "1.731");
        assert_eq!(p.global_penalty_points, "2.697");
        assert_eq!(p.season, "Season 7");
        assert_eq!(p.url, "https://mxb-ranked.com/Rider/FF011000010178A758");
        assert_eq!(
            p.banner,
            "https://mxb-ranked.com/images/Achievements/2A73E25E-BA3C-429E-A57E-1F94C485F235.jpg"
        );
    }

    #[test]
    fn the_three_standings_are_read_apart() {
        let p = parse(SAMPLE, "FF011000010178A758");
        let names: Vec<&str> = p.cards.iter().map(|c| c.discipline.as_str()).collect();
        assert_eq!(names, ["Global", "MX", "SX"]);

        let global = &p.cards[0];
        assert_eq!(global.rank, "3110");
        assert_eq!(global.badge, "S1");
        assert_eq!(global.rank_name, "Silver 1");
        assert_eq!(global.color, "#C0C0C0");
        assert_eq!(global.mxp, "1240");
        assert_eq!(global.races, "8");
        assert_eq!(global.avg_position, "4.6");
        assert_eq!(global.wins, "4");
        assert_eq!(global.podiums, "4");
        assert_eq!(global.wr_laps, "0");
        assert_eq!(global.pb_laps, "6");
        assert_eq!(global.holeshots, "1");

        // SX is a different rank from the other two, so the badge really is per card.
        assert_eq!(p.cards[2].badge, "B1");
        assert_eq!(p.cards[2].races, "2");
    }

    #[test]
    fn races_are_read_by_column_name() {
        let p = parse(SAMPLE, "FF011000010178A758");
        assert_eq!(p.races.len(), 5);

        let first = &p.races[0];
        assert_eq!(first.server, "MXB-Ranked.com | 2026 ARL SX | OPEN | NA | #102801");
        assert_eq!(first.track, "2026 ARL SX ROUND 06 - SEATTLE");
        assert_eq!(first.category, "SX");
        assert_eq!(first.position, "2 / 3");
        assert_eq!(first.bike, "Yamaha YZ450F 2023");
        assert_eq!(first.mxp, "10");
        assert_eq!(first.global_mxp, "9");
        assert_eq!(first.mxp_dir, "up");
        assert_eq!(first.penalty_points, "1.500");
        assert_eq!(first.finished, "13 days ago");
        assert_eq!(first.finished_at, "8/25/2026 8:44:44 PM");
        assert!(first.results_url.starts_with("https://mxb-ranked.com/Results/"));

        // Every row carries a track, which is what the pack matcher will read.
        assert!(p.races.iter().all(|r| !r.track.is_empty()));
    }

    /// A rider who hasn't raced this season gets no standings at all — their page prints the
    /// header and the table and nothing between (observed on a real profile, 2026-09-08). The
    /// races still have to come back, so the tab isn't empty for them.
    #[test]
    fn a_profile_with_no_standings_still_has_its_races() {
        let no_cards = SAMPLE.replace("Rank:", "Nope:");
        let p = parse(&no_cards, "FF011000010178A758");
        assert!(p.cards.is_empty());
        assert_eq!(p.races.len(), 5);
        assert_eq!(p.name, "Rand");
    }

    /// The page is someone else's app and its markup will move. When it does, the parse has to
    /// come back thin rather than fail — the tab shows what it has and a refresh costs nothing.
    #[test]
    fn a_page_we_dont_recognise_parses_to_nothing() {
        let p = parse("<html><body><p>Nothing here</p></body></html>", "FF011000010178A758");
        assert_eq!(p.name, "");
        assert!(p.cards.is_empty());
        assert!(p.races.is_empty());
        // The identity we were asked about survives, so the view can still offer the link out.
        assert_eq!(p.guid, "FF011000010178A758");
    }

}
