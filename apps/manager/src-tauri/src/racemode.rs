//! Race mode: before an app-launched join, set aside every mod the server doesn't need.
//!
//! MX Bikes mounts every `.pkz` under `mods` at startup, and it does it again on the way into
//! a server. A rider with three hundred tracks pays for all three hundred to race on one of
//! them. The server list already says which track a server runs and which bike classes it
//! lets in, so the app knows before the game starts what the session can possibly use.
//! Everything else can step aside for the length of the session — the same move Manage makes
//! (see [`crate::modstate`]), done automatically and undone when the game exits.
//!
//! This half is the *decision*, and nothing else: which of the installed mods to move. It
//! reads no disk and moves no file, so every rule below is a unit test rather than a folder
//! of fixtures. The caller builds the inventory (the library scan, each bike's `[data] cat`,
//! each track's inner folders) and does the moving.
//!
//! The rules lean hard towards keeping. A mod set aside that the session needed is a rider
//! who can't see the track, or a grid of riders on bikes the game can't draw; a mod kept that
//! the session didn't need costs a few hundred milliseconds of mounting. So:
//!
//! * Only **whole tracks** and **whole packed bikes** are candidates. Rider gear, paints,
//!   tyres, sounds and support packs are never moved, whatever the server runs.
//! * Anything the app couldn't read is kept — an archive it can't open, a bike with no
//!   category, a folder it can't place.
//! * An archive holding one needed thing and one unneeded thing is kept whole. Nothing here
//!   unpacks an archive to take half of it away.
//! * With no known server track, or a track this install doesn't have, it does nothing at
//!   all: the one mod that has to be there is the one it can't point at.

// Only the tests call this until the join path is wired to it.
#![allow(dead_code)]

use crate::bikeswap;
use mxb_core::tracksource;
use std::collections::BTreeSet;

/// What a server is running, as the server list reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerNeeds {
    /// The track id the server publishes — the folder name inside the track's archive, so
    /// `Farm14` has to find `Farm 14.pkz`. Compared through [`tracksource::key`].
    pub track: String,
    /// The layout of that track. Carried for the record and the log: a layout lives inside
    /// the track's own archive, so it never changes what has to stay.
    pub track_layout: String,
    /// The bike categories the server lets in (`MX1 OEM`, `MX2 OEM`, …). Empty means Open —
    /// any bike — which is also what the in-game browser shows as "Any".
    pub categories: Vec<String>,
    /// The track ships with the game, so there's no mod of it to find. A stock track is the
    /// one case where not finding the server's track installed is not a reason to stop.
    pub track_is_stock: bool,
}

/// One bike an archive carries, as its own config names it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BikeContent {
    /// `<name>.cfg` `ID` — what the profile's `bikeid` names.
    pub id: String,
    /// `<name>.ini` `[data] cat`. Blank when the bike declares none.
    pub class: String,
}

/// One installed mod, as the inventory pass saw it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    /// Path relative to the MX Bikes root, `mods/`-prefixed — [`crate::modstate::ModEntry`]'s
    /// `rel`, and the key the moving half addresses the mod by.
    pub rel: String,
    /// Library category: `track`, `bike`, `helmet`, `bikePaint`, `sound`, …
    pub category: String,
    /// An extracted folder rather than a single archive.
    pub is_dir: bool,
    /// The game can see it. A mod the player already parked isn't Race mode's to move, and
    /// it can't be what the session is waiting on either.
    pub enabled: bool,
    /// The contents were actually read — a track's marker files found, a bike's identity
    /// parsed. `false` is "couldn't tell", and couldn't tell is kept.
    pub known: bool,
    /// For a track: every track id it carries — its own stem and the folder of each track
    /// inside it. More than one for a pack.
    pub tracks: Vec<String>,
    /// For a bike: every bike it carries. More than one for a pack.
    pub bikes: Vec<BikeContent>,
    /// Holds a `.mxbsecure` or `.mxbkey` somewhere inside. Secured content has a key and a
    /// lease tied to where it sits, and is never moved.
    pub protected: bool,
}

/// What the player is riding, from the active profile and preset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Player {
    /// The profile's `bikeid`. Blank when there's no profile to read.
    pub bike_id: String,
}

/// Why Race mode left everything where it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    /// The server list didn't say which track this server runs.
    NoTrack,
    /// It did, but no enabled mod here carries it and it isn't a stock track — either the
    /// player doesn't have it (the game will say so) or it's named in a way we can't match.
    /// Either way, moving tracks now could only move the one that matters.
    TrackNotInstalled(String),
}

/// Archives by these names are shared pieces other mods lean on, not mods of their own.
/// Matched against every path segment, so a `mods/tracks/common/…` folder is covered too.
const SUPPORT_WORDS: [&str; 4] = ["common", "misc", "support", "shared"];

/// Extensions of secured content. The blob and the key beside it move together or not at
/// all, and "not at all" is the only one of those that can't strand a key.
const PROTECTED_EXTS: [&str; 2] = [".mxbsecure", ".mxbkey"];

/// Normalize a rel for comparison, the way [`crate::modstate`] does: case and slash
/// direction both vary by where the path came from.
fn key(rel: &str) -> String {
    rel.split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/")
        .to_lowercase()
}

/// `pinned` is at `rel` or somewhere inside it.
fn holds(rel: &str, pinned: &BTreeSet<String>) -> bool {
    let k = key(rel);
    pinned.contains(&k) || pinned.iter().any(|p| p.starts_with(&format!("{k}/")))
}

fn is_protected(item: &Item) -> bool {
    let lower = item.rel.to_lowercase();
    item.protected || PROTECTED_EXTS.iter().any(|x| lower.ends_with(x))
}

fn is_support_pack(rel: &str) -> bool {
    key(rel)
        .split('/')
        .any(|seg| SUPPORT_WORDS.iter().any(|w| seg.contains(w)))
}

/// What kind of candidate an item is, or `None` when it isn't one at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Candidate {
    Track,
    Bike,
}

/// Whether Race mode is willing to move this mod, before asking whether the server needs it.
///
/// Deliberately a short allow-list rather than a list of exceptions: a new kind of content
/// the library learns to categorize next month stays put until someone decides otherwise.
fn candidate(item: &Item) -> Option<Candidate> {
    if !item.enabled || !item.known || is_protected(item) || is_support_pack(&item.rel) {
        return None;
    }
    match item.category.as_str() {
        "track" if !item.tracks.is_empty() => Some(Candidate::Track),
        // Packed only. A bike installed as a folder carries its liveries and model-swap sets
        // inside it, which other parts of the app track by path — and Manage's restore walk
        // can't tell a parked bike folder from a grouping folder.
        "bike" if !item.is_dir && !item.bikes.is_empty() => Some(Candidate::Bike),
        _ => None,
    }
}

/// Does any server category let this class in? Each entry may itself be a `/`-separated
/// list, the dedicated server's own `[event] category` shape.
fn class_allowed(class: &str, categories: &[String]) -> bool {
    categories.is_empty()
        || categories
            .iter()
            .any(|c| !c.trim().is_empty() && bikeswap::class_matches(class, c))
}

fn track_needed(item: &Item, want: &str) -> bool {
    item.tracks.iter().any(|t| tracksource::key(t) == want)
}

/// A bike archive stays when *anything* in it might be ridden: a bike in one of the server's
/// classes, a bike with no class to judge by, or the one the player has selected.
fn bike_needed(item: &Item, server: &ServerNeeds, player: &Player) -> bool {
    let mine = player.bike_id.trim();
    item.bikes.iter().any(|b| {
        b.class.trim().is_empty()
            || class_allowed(&b.class, &server.categories)
            || (!mine.is_empty() && b.id.trim().eq_ignore_ascii_case(mine))
    })
}

/// The mods to set aside for this session, as `rel` paths, sorted.
///
/// `pinned` is every file that must stay visible whatever else happens: the player's own
/// paints and gear, and everything paint sync installed. A candidate holding one of them is
/// kept whole — the game reads a livery from inside its bike's folder, and moving the folder
/// would take the livery with it.
pub fn set_aside(
    server: &ServerNeeds,
    items: &[Item],
    player: &Player,
    pinned: &[String],
) -> Result<Vec<String>, Skip> {
    let want = tracksource::key(&server.track);
    if want.is_empty() {
        return Err(Skip::NoTrack);
    }
    // The server's track has to be *here* before anything else moves. Matched against every
    // enabled track, candidate or not — a secured or unreadable archive by the right name is
    // still the track, and it is kept either way.
    let installed = items.iter().any(|i| {
        i.enabled
            && i.category == "track"
            && (track_needed(i, &want)
                || tracksource::key(&stem(&i.rel)) == want)
    });
    if !installed && !server.track_is_stock {
        return Err(Skip::TrackNotInstalled(server.track.clone()));
    }

    let pinned: BTreeSet<String> = pinned.iter().map(|p| key(p)).filter(|p| !p.is_empty()).collect();
    let mut out: BTreeSet<String> = BTreeSet::new();
    for item in items {
        let needed = match candidate(item) {
            None => continue,
            Some(Candidate::Track) => track_needed(item, &want),
            Some(Candidate::Bike) => bike_needed(item, server, player),
        };
        if !needed && !holds(&item.rel, &pinned) {
            out.insert(item.rel.clone());
        }
    }
    Ok(out.into_iter().collect())
}

/// `mods/tracks/EU/Farm 14.pkz` → `Farm 14`.
fn stem(rel: &str) -> String {
    let name = rel.rsplit(['/', '\\']).next().unwrap_or(rel);
    mxb_core::library::strip_ext(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(rel: &str, ids: &[&str]) -> Item {
        Item {
            rel: rel.into(),
            category: "track".into(),
            enabled: true,
            known: true,
            tracks: ids.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn bike(rel: &str, contents: &[(&str, &str)]) -> Item {
        Item {
            rel: rel.into(),
            category: "bike".into(),
            enabled: true,
            known: true,
            bikes: contents
                .iter()
                .map(|(id, class)| BikeContent { id: id.to_string(), class: class.to_string() })
                .collect(),
            ..Default::default()
        }
    }

    fn other(rel: &str, category: &str) -> Item {
        Item {
            rel: rel.into(),
            category: category.into(),
            enabled: true,
            known: true,
            ..Default::default()
        }
    }

    fn server(track: &str, categories: &[&str]) -> ServerNeeds {
        ServerNeeds {
            track: track.into(),
            categories: categories.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn run(s: &ServerNeeds, items: &[Item]) -> Vec<String> {
        set_aside(s, items, &Player::default(), &[]).expect("race mode runs")
    }

    fn library() -> Vec<Item> {
        vec![
            track("mods/tracks/Farm 14.pkz", &["Farm 14", "Farm14"]),
            track("mods/tracks/EU/RedBud.pkz", &["RedBud", "redbud_2024"]),
            track("mods/tracks/Milestone", &["Milestone"]),
            bike("mods/bikes/KTM 450.pkz", &[("ktm450", "MX1 OEM")]),
            bike("mods/bikes/YZ250F.pkz", &[("yz250f", "MX2 OEM")]),
            bike("mods/bikes/CR500.pkz", &[("cr500", "Classic MX1 OEM")]),
        ]
    }

    #[test]
    fn with_no_server_track_nothing_moves() {
        let err = set_aside(&server("  ", &["MX1 OEM"]), &library(), &Player::default(), &[]);
        assert_eq!(err, Err(Skip::NoTrack));
    }

    /// The one mod that has to be there is the one we can't point at, so nothing moves —
    /// not even the bikes, which we *could* judge.
    #[test]
    fn a_track_this_install_does_not_have_stops_everything() {
        let err = set_aside(&server("Nowhere", &["MX1 OEM"]), &library(), &Player::default(), &[]);
        assert_eq!(err, Err(Skip::TrackNotInstalled("Nowhere".into())));
    }

    /// A parked copy of the server's track isn't one the game can load, so it doesn't count.
    #[test]
    fn a_parked_copy_of_the_track_does_not_count_as_installed() {
        let mut items = library();
        items[0].enabled = false;
        let err = set_aside(&server("Farm14", &[]), &items, &Player::default(), &[]);
        assert!(matches!(err, Err(Skip::TrackNotInstalled(_))), "{err:?}");
    }

    /// A stock track has no mod of its own, and every modded track can step aside for it.
    #[test]
    fn a_stock_track_sets_every_modded_track_aside() {
        let mut s = server("forest", &[]);
        s.track_is_stock = true;
        let out = run(&s, &library());
        assert!(out.contains(&"mods/tracks/Farm 14.pkz".to_string()));
        assert!(out.contains(&"mods/tracks/EU/RedBud.pkz".to_string()));
        assert!(out.contains(&"mods/tracks/Milestone".to_string()));
    }

    #[test]
    fn keeps_the_server_track_and_sets_the_others_aside() {
        let out = run(&server("Farm14", &[]), &library());
        assert!(!out.contains(&"mods/tracks/Farm 14.pkz".to_string()), "{out:?}");
        assert!(out.contains(&"mods/tracks/EU/RedBud.pkz".to_string()), "{out:?}");
        assert!(out.contains(&"mods/tracks/Milestone".to_string()), "extracted tracks too");
    }

    /// The server reports the folder inside the archive, which needn't be the file name.
    #[test]
    fn the_server_track_is_found_by_its_inner_folder() {
        let out = run(&server("redbud_2024", &[]), &library());
        assert!(!out.contains(&"mods/tracks/EU/RedBud.pkz".to_string()), "{out:?}");
        assert!(out.contains(&"mods/tracks/Farm 14.pkz".to_string()));
    }

    /// A layout lives inside its track's archive, so it never changes what stays.
    #[test]
    fn the_layout_does_not_change_what_stays() {
        let plain = run(&server("Farm14", &["MX1 OEM"]), &library());
        let mut s = server("Farm14", &["MX1 OEM"]);
        s.track_layout = "Short".into();
        assert_eq!(run(&s, &library()), plain);
    }

    #[test]
    fn keeps_bikes_in_the_server_classes_and_sets_the_rest_aside() {
        let out = run(&server("Farm14", &["mx1 oem"]), &library());
        assert!(!out.contains(&"mods/bikes/KTM 450.pkz".to_string()), "case-insensitive: {out:?}");
        assert!(out.contains(&"mods/bikes/YZ250F.pkz".to_string()));
        assert!(out.contains(&"mods/bikes/CR500.pkz".to_string()));
    }

    /// The dedicated server's own shape: several classes in one `/`-separated entry.
    #[test]
    fn a_slash_separated_category_lets_each_class_in() {
        let out = run(&server("Farm14", &["MX1 OEM/MX2 OEM"]), &library());
        assert!(!out.contains(&"mods/bikes/KTM 450.pkz".to_string()));
        assert!(!out.contains(&"mods/bikes/YZ250F.pkz".to_string()));
        assert!(out.contains(&"mods/bikes/CR500.pkz".to_string()));
    }

    /// Open class — no categories at all — lets every bike in, so every bike stays.
    #[test]
    fn an_open_server_keeps_every_bike() {
        let out = run(&server("Farm14", &[]), &library());
        assert!(out.iter().all(|r| !r.starts_with("mods/bikes/")), "{out:?}");
    }

    /// A bike with no `[data] cat` can't be judged, and couldn't-tell is kept.
    #[test]
    fn a_bike_with_no_category_is_kept() {
        let mut items = library();
        items.push(bike("mods/bikes/Mystery.pkz", &[("mystery", "  ")]));
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        assert!(!out.contains(&"mods/bikes/Mystery.pkz".to_string()), "{out:?}");
    }

    /// An archive whose identity couldn't be read — a pack, a protected zip — is kept, as is
    /// a track archive whose markers weren't found.
    #[test]
    fn an_unreadable_archive_is_kept() {
        let mut items = library();
        items.push(Item { known: false, ..bike("mods/bikes/OEM Bikes.pkz", &[]) });
        items.push(Item { known: false, ..track("mods/tracks/Odd.pkz", &["Odd"]) });
        items.push(bike("mods/bikes/NoIdentity.pkz", &[]));
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        for kept in ["mods/bikes/OEM Bikes.pkz", "mods/tracks/Odd.pkz", "mods/bikes/NoIdentity.pkz"] {
            assert!(!out.contains(&kept.to_string()), "{kept}: {out:?}");
        }
    }

    /// The player's own bike stays even when the server's class wouldn't keep it: the join
    /// selects a compatible bike first, but a join the app couldn't steer mustn't leave the
    /// profile naming a bike that isn't there.
    #[test]
    fn the_players_selected_bike_is_kept() {
        let player = Player { bike_id: "CR500".into() };
        let out = set_aside(&server("Farm14", &["MX1 OEM"]), &library(), &player, &[]).unwrap();
        assert!(!out.contains(&"mods/bikes/CR500.pkz".to_string()), "{out:?}");
        assert!(out.contains(&"mods/bikes/YZ250F.pkz".to_string()));
    }

    /// The player's paints and gear, and anything paint sync installed, are pinned: a
    /// candidate that holds one is kept whole rather than taking it along.
    #[test]
    fn a_candidate_holding_a_pinned_file_is_kept() {
        let mut items = library();
        items.push(track("mods/tracks/Paintable", &["Paintable"]));
        let pinned = vec![
            "mods/tracks/Paintable/paints/mine.pnt".to_string(),
            // A different case and slash direction still names the same file.
            "MODS\\bikes\\YZ250F.pkz".to_string(),
        ];
        let out = set_aside(&server("Farm14", &["MX1 OEM"]), &items, &Player::default(), &pinned).unwrap();
        assert!(!out.contains(&"mods/tracks/Paintable".to_string()), "{out:?}");
        assert!(!out.contains(&"mods/bikes/YZ250F.pkz".to_string()), "{out:?}");
        assert!(out.contains(&"mods/bikes/CR500.pkz".to_string()));
    }

    /// Rider models, gear, paints, tyres and sounds are never candidates, whatever the server
    /// runs.
    #[test]
    fn rider_gear_paints_tyres_and_sounds_always_stay() {
        let mut items = library();
        let always = [
            ("mods/rider/riders/default_mx", "rider"),
            ("mods/rider/helmets/AGV", "helmet"),
            ("mods/rider/helmets/AGV/paints/Red.pnt", "helmetPaint"),
            ("mods/rider/helmets/AGV/goggles/Tint.pnt", "goggles"),
            ("mods/rider/boots/Alpinestars", "boots"),
            ("mods/rider/boots/Alpinestars/paints/White.pnt", "bootPaint"),
            ("mods/rider/gloves/Fox.pnt", "gloves"),
            ("mods/rider/riders/default_mx/paints/Kit.pnt", "outfit"),
            ("mods/rider/protections/Leatt", "protection"),
            ("mods/tyres/Dunlop.pkz", "tyre"),
            ("mods/bikes/KTM450/sounds", "sound"),
            ("mods/bikes/CR500/paints/Red.pnt", "bikePaint"),
            ("mods/bikes/CR500/FrostMod Models/2024.pkz", "bikeModelSwap"),
        ];
        for (rel, cat) in always {
            items.push(other(rel, cat));
        }
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        for (rel, _) in always {
            assert!(!out.contains(&rel.to_string()), "{rel} moved: {out:?}");
        }
    }

    /// Shared packs other mods lean on stay, whichever content folder they sit in.
    #[test]
    fn support_packs_always_stay() {
        let mut items = library();
        items.push(track("mods/tracks/Common Objects.pkz", &["common objects"]));
        items.push(track("mods/tracks/misc/Banners.pkz", &["banners"]));
        items.push(bike("mods/bikes/Support Parts.pkz", &[("parts", "Parts")]));
        items.push(bike("mods/bikes/Shared Rims.pkz", &[("rims", "Parts")]));
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        for kept in [
            "mods/tracks/Common Objects.pkz",
            "mods/tracks/misc/Banners.pkz",
            "mods/bikes/Support Parts.pkz",
            "mods/bikes/Shared Rims.pkz",
        ] {
            assert!(!out.contains(&kept.to_string()), "{kept}: {out:?}");
        }
    }

    /// Secured content — the blob, its key, or a folder holding either — never moves.
    #[test]
    fn secured_content_always_stays() {
        let mut items = library();
        items.push(track("mods/tracks/Locked.pkz.mxbsecure", &["Locked"]));
        items.push(bike("mods/bikes/Locked Bike.mxbsecure", &[("lb", "MX2 OEM")]));
        items.push(track("mods/tracks/Keyed.mxbkey", &["Keyed"]));
        items.push(Item { protected: true, ..track("mods/tracks/Inside", &["Inside"]) });
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        for kept in [
            "mods/tracks/Locked.pkz.mxbsecure",
            "mods/bikes/Locked Bike.mxbsecure",
            "mods/tracks/Keyed.mxbkey",
            "mods/tracks/Inside",
        ] {
            assert!(!out.contains(&kept.to_string()), "{kept}: {out:?}");
        }
    }

    /// A pack with one needed track or bike in it is kept whole.
    #[test]
    fn a_pack_with_anything_needed_is_kept_whole() {
        let mut items = library();
        items.push(track("mods/tracks/Pack.pkz", &["Pack", "Farm14", "Other"]));
        items.push(bike("mods/bikes/Mixed.pkz", &[("a", "MX2 OEM"), ("b", "MX1 OEM")]));
        items.push(bike("mods/bikes/AllMX2.pkz", &[("c", "MX2 OEM"), ("d", "MX2 OEM")]));
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        assert!(!out.contains(&"mods/tracks/Pack.pkz".to_string()), "{out:?}");
        assert!(!out.contains(&"mods/bikes/Mixed.pkz".to_string()), "{out:?}");
        assert!(out.contains(&"mods/bikes/AllMX2.pkz".to_string()), "nothing needed in it");
    }

    /// Whole tracks and whole *packed* bikes only. A bike installed as a folder carries its
    /// liveries and swap sets, which the app tracks by path elsewhere.
    #[test]
    fn only_whole_tracks_and_packed_bikes_are_candidates() {
        let mut items = library();
        items.push(Item { is_dir: true, ..bike("mods/bikes/RM250", &[("rm250", "MX2 OEM")]) });
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        assert!(!out.contains(&"mods/bikes/RM250".to_string()), "{out:?}");
        assert!(out.contains(&"mods/bikes/YZ250F.pkz".to_string()));
    }

    /// A mod the player parked themselves isn't Race mode's to list, or it would end up in
    /// the journal and come back at session end.
    #[test]
    fn a_mod_already_parked_is_not_listed() {
        let mut items = library();
        items[1].enabled = false;
        let out = run(&server("Farm14", &[]), &items);
        assert!(!out.contains(&"mods/tracks/EU/RedBud.pkz".to_string()), "{out:?}");
    }

    #[test]
    fn the_list_is_sorted_and_free_of_duplicates() {
        let mut items = library();
        items.push(items[1].clone());
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        let mut sorted = out.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(out, sorted);
    }
}
