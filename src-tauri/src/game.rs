//! Which PiBoSo title the app is driving, and everything that differs between them.
//!
//! MX Bikes and GP Bikes are the same engine by the same developer: identical archive
//! formats (`.pkz`), paint format (`.pnt`), mesh format (`.edf`), `profile.ini` dialect
//! and mods-folder *mechanics*. What actually differs is a table of constants — where the
//! user folder lives, what the executable is called, which folders `mods/` contains, and
//! which catalog site serves them.
//!
//! So rather than fork the install/enable/scan machinery per game, every one of those
//! constants lives here and the machinery takes a `&GameProfile`. Adding a third PiBoSo
//! title (World Racing Series, Kart Racing Pro) is a new `const` in this file plus its
//! catalog session.

use crate::cookie_session::Site;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;

/// The title the app is currently driving.
///
/// Process-wide because that is what it is: one app process drives one game, and the
/// places that need to know — the process-list scan, the catalog client's cookie jar —
/// are reached from call paths that have no `AppConfig` to hand and would otherwise have
/// to grow one purely to pass it along.
///
/// Kept in step with the config by [`crate::config::load`] and [`crate::config::save`],
/// the two funnels every read and write of the active game goes through.
static ACTIVE: RwLock<Game> = RwLock::new(Game::Mxb);

/// Point the process-wide lookups at `game`. Idempotent.
pub fn set_active(game: Game) {
    if let Ok(mut slot) = ACTIVE.write() {
        *slot = game;
    }
}

/// The active title's profile. Falls back to MX Bikes if the lock was poisoned — the
/// title every install starts on, and a wrong answer here costs a mislabelled badge
/// rather than a wrong write.
pub fn active() -> &'static GameProfile {
    ACTIVE.read().map(|g| g.profile()).unwrap_or(&MXB)
}

/// The titles the app knows how to drive. Serialized as `"mxb"` / `"gpb"` — these strings
/// are the keys in `AppConfig::games` and travel to the frontend, so they are stable
/// identifiers and must not be renamed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Game {
    /// MX Bikes — the title the app shipped for.
    #[default]
    Mxb,
    Gpb,
}

impl Game {
    pub fn profile(self) -> &'static GameProfile {
        match self {
            Game::Mxb => &MXB,
            Game::Gpb => &GPB,
        }
    }

    pub fn id(self) -> &'static str {
        self.profile().id
    }

    /// Parse a stored/incoming id. Anything unrecognized falls back to MX Bikes rather
    /// than erroring: a config written by a *newer* build naming a game this one doesn't
    /// have should leave the app usable, not refuse to start.
    pub fn from_id(id: &str) -> Game {
        match id.trim().to_ascii_lowercase().as_str() {
            "gpb" => Game::Gpb,
            _ => Game::Mxb,
        }
    }

    pub const ALL: [Game; 2] = [Game::Mxb, Game::Gpb];
}

/// The folders MX Bikes' protection models live in under `mods/rider`, in the order they're
/// looked in.
///
/// `protections` is the game's own name for the slot: it is what `rider.pkz` calls the folder,
/// what the game's loader asks the mods tree for, and what mods on mxb-mods package themselves
/// as. Earlier versions of this app installed to the singular `protection`, so that is still
/// read — nothing is ever written to it.
pub const PROTECTION_AREAS: &[&str] = &["protections", "protection"];

/// The folder every title keeps its rider models in, under `mods/rider`.
///
/// MX Bikes calls what lives here a rider *profile* and hangs the kit, gloves and goggles off
/// it; GP Bikes calls the same thing a rider *model* and bakes the boots and gloves into it,
/// so its suits are `riders/<model>/paints`. Same folder, so it isn't a per-title constant —
/// but it *is* a destination both games install into, which is why it's named rather than
/// spelled out at each call site.
pub const RIDERS_DIR: &str = "riders";

/// One gear area under `mods/rider` — a folder of models, each of which may carry paints.
pub struct RiderArea {
    /// Folder name under `mods/rider`.
    pub folder: &'static str,
    /// Library category for a model in this area.
    pub model_cat: &'static str,
    /// Library category for a paint in `<model>/paints`. `None` when the area holds
    /// models that can't be painted — GP Bikes' `animations` are riding styles, not gear.
    pub paint_cat: Option<&'static str>,
    /// The area's models can carry a `goggles/` folder alongside `paints/`. MX Bikes
    /// helmets do; GP Bikes' road helmets use visors and have no such folder.
    pub goggles: bool,
    /// New content of this kind may be installed here. False only for a folder that is
    /// read for what earlier versions put there — see MX Bikes' singular `protection`.
    /// Scans use every area; the pickers offer only these.
    pub installable: bool,
}

/// How `mods/rider` is laid out for a title.
pub struct RiderLayout {
    pub areas: &'static [RiderArea],
    /// `mods/rider/gloves` exists as a flat area (models directly inside, no per-model
    /// folder). MX Bikes only.
    pub gloves: bool,
    /// Subfolders inside `mods/rider/riders/<profile>/` beyond `paints`, as
    /// `(folder, library category)`.
    pub profile_extras: &'static [(&'static str, &'static str)],
    /// Rider models the game itself ships, so a paint for one has somewhere to go on a mods
    /// folder that is otherwise empty.
    ///
    /// Empty for GP Bikes, and that is the fact rather than an omission: nothing there
    /// paints the stock rider. Every GP suit is made for a downloaded model — Manu's
    /// "Modern Type 1", Bar's "(S) Suit 1 + Boots …" — and the catalog files them under
    /// that model's name. Offering an invented default would send a suit to a folder the
    /// game never reads.
    pub stock_profiles: &'static [&'static str],
}

/// Features that only exist for some titles. Gating on these rather than on `Game`
/// keeps "why is this hidden" answerable in one place, and means a future GP build of
/// FrostMod is a single `true`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Caps {
    /// FrostMod, the in-process mod loader. Attaches to either title as of FrostMod
    /// v0.10.0 — launched with `--game <id>`, see `frostmod_manage::start`.
    pub frostmod: bool,
    /// Re-run the game's profile loader in place after applying a preset. Implemented by
    /// calling a hardcoded `mxbikes.exe` function offset (`gameproc::LOADER_OFFSET`), which
    /// is that binary's alone — unlike FrostMod's reload, this has no GP equivalent yet.
    pub instant_refresh: bool,
    /// The 3D preview (Locker / Rider / bike preview). GP Bikes uses the same `.edf`
    /// meshes, but its bike part names and rider rig need their own bindings — not in
    /// this pass.
    pub viewer: bool,
    /// The authenticated paid-content shop. mxbikes-shop.com is MX Bikes only.
    pub shop: bool,
    /// The Servers tab and paint sync. Administers MX Bikes dedicated servers and keys
    /// riders by MX Bikes GUID, so it has no meaning for another title.
    pub servers: bool,
    /// Join a server by address, by launching the game with `-directconnect`. The argv
    /// parser it was found in and the default port it assumes are both MX Bikes'; GP
    /// Bikes has its own, so this stays off there until they're confirmed.
    pub join_by_address: bool,
    /// The Manage view — parking mods in `mxbapp_disabled` to trim what the game loads.
    /// Filesystem-only, so it *works* for any title; off for GP Bikes because it isn't
    /// wanted there, not because it can't.
    pub manage: bool,
}

pub struct GameProfile {
    pub id: &'static str,
    /// Human name, used verbatim in the UI. Never translated — it's a product name.
    pub display: &'static str,
    /// Folder under `Documents/PiBoSo` the game writes its user data to.
    pub user_dir: &'static str,
    /// Steam AppID — names the Proton prefix under `steamapps/compatdata` and addresses
    /// the game in the `steam://rungameid/…` URL the Linux launcher uses.
    pub steam_appid: &'static str,
    /// Folder under `steamapps/common` the game installs to.
    pub steam_common: &'static str,
    pub exe: &'static str,
    /// A file whose presence proves a folder really is the game's *install* dir.
    pub install_marker: &'static str,
    /// Top-level folders under `<mods_path>/mods`. Drives install routing and which
    /// library tabs exist.
    pub mods_dirs: &'static [&'static str],
    /// The subset of `mods_dirs` that Manage enables and disables. Deliberately narrower:
    /// `tyres` and `misc` hold a handful of small files and aren't what a track load is
    /// waiting on, so parking them buys nothing and only risks breaking a setup.
    pub manage_dirs: &'static [&'static str],
    pub rider: RiderLayout,
    /// The mods catalog this title browses — mxb-mods.com or gpb-mods.com. Both are the
    /// same WordPress/WP-REST stack by the same author, so one client serves either; the
    /// `Site` carries the host and its own cookie jar identity.
    pub catalog: &'static Site,
    pub caps: Caps,
}

pub static MXB: GameProfile = GameProfile {
    id: "mxb",
    display: "MX Bikes",
    user_dir: "MX Bikes",
    steam_appid: "655500",
    steam_common: "MX Bikes",
    exe: "mxbikes.exe",
    // The core rider body archive — what the 3D preview needs, so it's also what proves
    // we found the install rather than just a folder named after it.
    install_marker: "rider.pkz",
    mods_dirs: &["bikes", "tracks", "rider", "tyres", "misc"],
    manage_dirs: &["tracks", "bikes", "rider"],
    rider: RiderLayout {
        areas: &[
            RiderArea {
                folder: "helmets",
                model_cat: "helmet",
                paint_cat: Some("helmetPaint"),
                goggles: true,
                installable: true,
            },
            RiderArea {
                folder: "boots",
                model_cat: "boots",
                paint_cat: Some("bootPaint"),
                goggles: false,
                installable: true,
            },
            RiderArea {
                folder: PROTECTION_AREAS[0],
                model_cat: "protection",
                paint_cat: Some("protectionPaint"),
                goggles: false,
                installable: true,
            },
            // The same slot under the name earlier versions installed to, so what they put
            // there still shows up in the library and the pickers.
            RiderArea {
                folder: PROTECTION_AREAS[1],
                model_cat: "protection",
                paint_cat: Some("protectionPaint"),
                goggles: false,
                installable: false,
            },
        ],
        gloves: true,
        profile_extras: &[("gloves", "gloves"), ("goggles", "goggles")],
        stock_profiles: &["default_mx", "default_sm"],
    },
    catalog: &crate::mxb_session::MXB_SITE,
    caps: Caps {
        frostmod: true,
        instant_refresh: true,
        viewer: true,
        shop: true,
        manage: true,
        join_by_address: true,
        servers: true,
    },
};

pub static GPB: GameProfile = GameProfile {
    id: "gpb",
    display: "GP Bikes",
    user_dir: "GP Bikes",
    steam_appid: "848050",
    steam_common: "GP Bikes",
    exe: "gpbikes.exe",
    // No `rider.pkz` to key on the way MX Bikes has, and the 3D preview doesn't run for
    // GP Bikes yet anyway — the executable is both the proof and the thing we launch.
    install_marker: "gpbikes.exe",
    mods_dirs: &["bikes", "tracks", "rider", "tyres", "misc"],
    manage_dirs: &["tracks", "bikes", "rider"],
    rider: RiderLayout {
        areas: &[
            RiderArea {
                folder: "helmets",
                model_cat: "helmet",
                paint_cat: Some("helmetPaint"),
                // Road helmets have visors, not goggles: no `goggles/` folder exists.
                goggles: false,
                installable: true,
            },
            // Riding-style animations. Models with nothing to paint, so no paint category
            // — a `.pnt` under here would be a stray.
            RiderArea {
                folder: "animations",
                model_cat: "animation",
                paint_cat: None,
                goggles: false,
                installable: true,
            },
        ],
        // GP Bikes bakes gloves and boots into the rider model — the `riders` folder is
        // full of entries like `(S) Suit 1 + Boots Alpinestars`. There is nothing to
        // pick separately, so there are no folders for them.
        gloves: false,
        profile_extras: &[],
        stock_profiles: &[],
    },
    catalog: &crate::mxb_session::GPB_SITE,
    caps: Caps {
        frostmod: true,
        instant_refresh: false,
        viewer: false,
        shop: false,
        manage: false,
        join_by_address: false,
        servers: false,
    },
};

/// Every content folder name any known title puts directly under `mods/`.
///
/// The union across titles, not one game's list, because its job is *recognition*: when
/// an extracted archive already contains a `mods/<something>/` tree, this is what decides
/// that it does. Routing — where a file actually lands — is per-game and uses
/// [`GameProfile::mods_dirs`]. Being generous here costs nothing (an MX Bikes archive
/// simply never contains a GP-only folder) and being narrow would silently mis-place a
/// correctly-packed archive.
pub const ALL_MODS_DIRS: [&str; 5] = ["bikes", "tracks", "rider", "tyres", "misc"];

/// One gear area as the install pickers need it: where it is, and what may sit inside.
///
/// The library's own view of an area is [`RiderArea`], which also carries the categories a
/// scan tags entries with. Those mean nothing to a picker, and the legacy folders a scan
/// reads are not places to write — so what crosses to the frontend is this narrower thing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiderAreaInfo {
    pub folder: &'static str,
    /// Models here keep their liveries in `<model>/paints`.
    pub paints: bool,
    /// …and a `<model>/goggles` folder besides.
    pub goggles: bool,
}

/// What the frontend needs to render the game switcher and gate features.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInfo {
    pub id: &'static str,
    pub display: &'static str,
    /// The executable's file name. The UI names it when telling someone to point another
    /// tool at the game — ReShade's installer asks for exactly this file.
    pub exe: &'static str,
    pub mods_dirs: &'static [&'static str],
    /// Host of the catalog this title browses, e.g. `mxb-mods.com`. The UI names the site
    /// it is linking to, and naming the wrong one is worse than naming none.
    pub catalog_domain: &'static str,
    /// The gear areas under `mods/rider` new content installs into, in the order to offer
    /// them. Sent for the same reason `mods_dirs` is: the install picker has to name real
    /// folders, and which ones exist is the title's fact, not the UI's.
    pub rider_areas: Vec<RiderAreaInfo>,
    /// Rider models the title ships — `riders/<name>` that exist before anything is
    /// installed. Empty for GP Bikes; see [`RiderLayout::stock_profiles`].
    pub rider_stock_profiles: &'static [&'static str],
    /// Folders inside `riders/<model>/` beyond `paints` — MX Bikes' `gloves` and `goggles`.
    pub rider_profile_extras: Vec<&'static str>,
    pub caps: Caps,
}

impl GameProfile {
    /// The gear areas new content may be installed into, in profile order.
    pub fn installable_areas(&'static self) -> impl Iterator<Item = &'static RiderArea> {
        self.rider.areas.iter().filter(|a| a.installable)
    }

    pub fn info(&'static self) -> GameInfo {
        GameInfo {
            id: self.id,
            display: self.display,
            exe: self.exe,
            mods_dirs: self.mods_dirs,
            catalog_domain: self.catalog.domain,
            rider_areas: self
                .installable_areas()
                .map(|a| RiderAreaInfo {
                    folder: a.folder,
                    paints: a.paint_cat.is_some(),
                    goggles: a.goggles,
                })
                .collect(),
            rider_stock_profiles: self.rider.stock_profiles,
            rider_profile_extras: self.rider.profile_extras.iter().map(|(f, _)| *f).collect(),
            caps: self.caps,
        }
    }
}

pub fn all_info() -> Vec<GameInfo> {
    Game::ALL.iter().map(|g| g.profile().info()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for g in Game::ALL {
            assert_eq!(Game::from_id(g.id()), g, "{} should round-trip", g.id());
        }
    }

    /// These ids are passed to FrostMod as `frostmod.exe --game <id>`, so they are a
    /// cross-repo contract: they must match the `id` field on FrostMod's `GameOffsets`
    /// (frostmod `src/offsets.h`). Renaming one here silently stops FrostMod attaching —
    /// it would fall back to its MX Bikes default and look "running" but do nothing.
    #[test]
    fn ids_match_what_frostmod_expects() {
        assert_eq!(Game::Mxb.id(), "mxb");
        assert_eq!(Game::Gpb.id(), "gpb");
    }

    /// A config naming a title this build doesn't have must still open — on MX Bikes,
    /// the title every existing install is already using.
    #[test]
    fn unknown_id_falls_back_to_mx_bikes() {
        assert_eq!(Game::from_id("wrs"), Game::Mxb);
        assert_eq!(Game::from_id(""), Game::Mxb);
    }

    /// The default must stay MX Bikes: every config written before this feature existed
    /// has no game recorded, and deserializing one has to land on the title it was for.
    #[test]
    fn default_is_mx_bikes() {
        assert_eq!(Game::default(), Game::Mxb);
    }

    /// `ALL_MODS_DIRS` only works as a recogniser if it really is the union — a game
    /// whose folders it doesn't cover would have correctly-packed archives mis-placed.
    #[test]
    fn all_mods_dirs_covers_every_game() {
        for g in Game::ALL {
            for dir in g.profile().mods_dirs {
                assert!(
                    ALL_MODS_DIRS.contains(dir),
                    "{} has a `{dir}` folder that ALL_MODS_DIRS doesn't list",
                    g.id(),
                );
            }
        }
    }

    /// The legacy singular `protection` is read so nothing already filed there vanishes,
    /// but it is not a place to write: the game's loader asks for `protections`.
    #[test]
    fn the_legacy_protection_folder_is_never_installed_to() {
        let mxb = Game::Mxb.profile();
        assert!(
            mxb.rider.areas.iter().any(|a| a.folder == PROTECTION_AREAS[1]),
            "the old spelling is still scanned",
        );
        assert!(
            !mxb.installable_areas().any(|a| a.folder == PROTECTION_AREAS[1]),
            "…but nothing is offered it as a destination",
        );
    }

    /// The install pickers are built from this list, so an MX-only folder appearing here
    /// would be offered on GP Bikes — where the loader never opens it, leaving the mod
    /// installed as far as the app is concerned and absent as far as the game is.
    #[test]
    fn gp_bikes_offers_only_the_folders_its_loader_reads() {
        let gpb = Game::Gpb.profile().info();
        let folders: Vec<&str> = gpb.rider_areas.iter().map(|a| a.folder).collect();
        assert_eq!(folders, ["helmets", "animations"]);
        for absent in ["boots", "protections", "protection", "gloves"] {
            assert!(!folders.contains(&absent), "GP Bikes has no `{absent}` folder");
        }
        assert!(
            gpb.rider_areas.iter().all(|a| !a.goggles),
            "road helmets have visors, not goggles",
        );
        assert!(
            !gpb.rider_areas.iter().any(|a| a.folder == "animations" && a.paints),
            "riding styles have nothing to paint",
        );
        assert!(gpb.rider_profile_extras.is_empty(), "gloves are part of the suit");
        assert!(
            gpb.rider_stock_profiles.is_empty(),
            "every GP suit is made for a downloaded rider model",
        );
    }

    #[test]
    fn gp_bikes_has_no_mx_only_features() {
        let caps = Game::Gpb.profile().caps;
        assert!(!caps.instant_refresh, "instant refresh calls an mxbikes.exe offset");
        assert!(!caps.shop, "mxbikes-shop.com is MX Bikes only");
        assert!(!caps.manage, "Manage is MX Bikes only for now");
        assert!(!caps.join_by_address, "GP's connect flag and port are unconfirmed");
    }
}
