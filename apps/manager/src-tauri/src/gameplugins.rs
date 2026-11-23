//! MX Bikes' native `.dlo` plugins.
//!
//! These are deliberately separate from MXB App plugins: the latter extend this app, while
//! these files are loaded by the game before its first frame. Every mutating path in here is
//! guarded and reversible because plugin folders commonly contain hand-edited settings.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;
use tauri::AppHandle;

pub const SUBPATH: &str = "plugins";
const STOCK: [&str; 3] = ["proxy64.dlo", "proxy_udp64.dlo", "xinput64.dli"];

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    ThirdParty,
    MxbApp,
}

#[derive(Clone, Copy)]
pub enum ConfigWhere {
    Plugins,
    Documents,
}

#[derive(Clone, Copy)]
pub struct ConfigFile {
    pub where_: ConfigWhere,
    pub path: &'static str,
    pub label: &'static str,
}

#[derive(Clone, Copy)]
pub enum Source {
    Github {
        repo: &'static str,
        asset_prefix: &'static str,
        asset_suffix: &'static str,
    },
    MxbMods,
    External {
        url: &'static str,
    },
}

#[derive(Clone, Copy)]
pub struct Known {
    pub id: &'static str,
    pub name: &'static str,
    pub author: &'static str,
    pub mxbmods_slug: Option<&'static str>,
    pub dlo: &'static [&'static str],
    pub stem_prefix: Option<&'static str>,
    pub owned: &'static [&'static str],
    /// Folders it puts files in that another plugin owns. Never moved or binned on this
    /// plugin's behalf; listed so the catalog says where its files live.
    #[allow(dead_code)]
    pub shared: &'static [&'static str],
    pub configs: &'static [ConfigFile],
    pub keys: &'static [(&'static str, &'static str)],
    pub key_from_ini: Option<(&'static str, &'static str, &'static str, &'static str)>,
    pub source: Source,
    pub made_for: Option<&'static str>,
    pub kind: Kind,
    pub note: Option<&'static str>,
    /// What racing mode does with it unless the player says otherwise.
    pub racing: Racing,
    /// Browse category: `hud`, `graphics`, `physics`, `fixes`, `timing` or `other`.
    pub category: &'static str,
    /// One plain line for the Browse card, written here rather than taken from the post so it
    /// says what the plugin does, not how it's marketed.
    pub summary: &'static str,
}

/// A plugin's place in racing mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Racing {
    /// Looks only (lighting, effects, camera, damage): off while racing by default.
    Off,
    /// Useful on track (HUDs, timing, status): stays on by default.
    Stay,
    /// A crash fix. Always stays on; the player can't put it on the racing list.
    AlwaysOn,
}

const NONE_CONFIG: &[ConfigFile] = &[];
const NONE_STR: &[&str] = &[];
const NONE_KEYS: &[(&str, &str)] = &[];
const GO_CONFIG: &[ConfigFile] = &[
    ConfigFile {
        where_: ConfigWhere::Plugins,
        path: "overhaul.ini",
        label: "Graphics settings",
    },
    ConfigFile {
        where_: ConfigWhere::Plugins,
        path: "keybinds.ini",
        label: "Keybinds",
    },
];
const DAMAGE_CONFIG: &[ConfigFile] = &[ConfigFile {
    where_: ConfigWhere::Plugins,
    path: "mxb_damage/settings.ini",
    label: "Damage settings",
}];
const MRP_CONFIG: &[ConfigFile] = &[ConfigFile {
    where_: ConfigWhere::Documents,
    path: "mxbmrp3/",
    label: "MXBMRP3 settings",
}];
const HUD_CONFIG: &[ConfigFile] = &[ConfigFile {
    where_: ConfigWhere::Documents,
    path: "Holeshot-HUD.ini",
    label: "HoleShot HUD settings",
}];

pub const KNOWN: &[Known] = &[
    Known {
        id: "graphics-overhaul",
        name: "Graphics Overhaul",
        author: "Finner24",
        mxbmods_slug: Some("graphics-overhaul-2"),
        dlo: &["overhaul64.dlo"],
        stem_prefix: None,
        owned: &["overhaul/", "presets/", "overhaul.ini", "keybinds.ini"],
        shared: NONE_STR,
        configs: GO_CONFIG,
        keys: &[
            ("F5", "Settings menu (change while riding)"),
            ("F9", "Effects on/off"),
            ("Ctrl+F9", "Reload settings"),
        ],
        key_from_ini: None,
        source: Source::MxbMods,
        made_for: None,
        kind: Kind::ThirdParty,
        note: Some("Replaces ReShade"),
        racing: Racing::Off,
        category: "graphics",
        summary: "Lighting, weather, rain and puddles, night skies and camera effects. Replaces ReShade.",
    },
    Known {
        id: "gamefx-plus",
        name: "GameFX+",
        author: "Finner24",
        mxbmods_slug: Some("gamefx-2"),
        dlo: &["gamefx64.dlo"],
        stem_prefix: None,
        owned: &["overhaul/shaders_fx.pak"],
        shared: &["overhaul/"],
        configs: NONE_CONFIG,
        keys: &[("F4", "Settings")],
        key_from_ini: None,
        source: Source::MxbMods,
        made_for: None,
        kind: Kind::ThirdParty,
        note: None,
        racing: Racing::Off,
        category: "graphics",
        summary: "New roost, dust, mud, water spray and exhaust effects for every rider.",
    },
    Known {
        id: "rider-pov-plus",
        name: "Rider POV+",
        author: "Finner24",
        mxbmods_slug: Some("rider-pov-2"),
        dlo: &["camera64.dlo"],
        stem_prefix: None,
        owned: NONE_STR,
        shared: NONE_STR,
        configs: NONE_CONFIG,
        keys: &[("F6", "Settings")],
        key_from_ini: None,
        source: Source::MxbMods,
        made_for: None,
        kind: Kind::ThirdParty,
        note: None,
        racing: Racing::Off,
        category: "graphics",
        summary: "A first-person camera that moves like your head: leans, jolts on landings, shakes over bumps.",
    },
    Known {
        id: "mxb-damage",
        name: "MXB Damage Plugin",
        author: "FroxyMXB",
        mxbmods_slug: Some("mxb-damage-plugin-2"),
        dlo: &["mxb_damage.dlo"],
        stem_prefix: None,
        owned: &["mxb_damage/"],
        shared: NONE_STR,
        configs: DAMAGE_CONFIG,
        keys: NONE_KEYS,
        key_from_ini: Some(("mxb_damage/settings.ini", "menu", "key", "Damage menu")),
        source: Source::Github {
            repo: "FroxyMXB/MXB-Damage-Plugin",
            asset_prefix: "mxb_damage_",
            asset_suffix: ".zip",
        },
        made_for: None,
        kind: Kind::ThirdParty,
        note: None,
        racing: Racing::Off,
        category: "physics",
        summary: "Visual damage and dirt: bars bend, levers snap, fenders crack. Looks only, no physics change.",
    },
    Known {
        id: "mxbmrp3",
        name: "MXBMRP3",
        author: "thomas4f",
        mxbmods_slug: Some("mxbmrp3"),
        dlo: &["mxbmrp3.dlo"],
        stem_prefix: None,
        owned: &["mxbmrp3_data/", "mxbmrp3_uninstall.exe"],
        shared: NONE_STR,
        configs: MRP_CONFIG,
        keys: NONE_KEYS,
        key_from_ini: None,
        source: Source::Github {
            repo: "thomas4f/mxbmrp3",
            asset_prefix: "mxbmrp3",
            asset_suffix: ".zip",
        },
        made_for: None,
        kind: Kind::ThirdParty,
        note: None,
        racing: Racing::Stay,
        category: "hud",
        summary: "Standings, timing, gap to your best, track map and radar. Open source.",
    },
    Known {
        id: "holeshot-hud",
        name: "HoleShot HUD",
        author: "ltroyer",
        mxbmods_slug: Some("holeshot-hud"),
        dlo: &["Holeshot-HUD.dlo"],
        stem_prefix: None,
        owned: NONE_STR,
        shared: NONE_STR,
        configs: HUD_CONFIG,
        keys: &[("F8", "Settings"), ("Ctrl + drag", "Move widgets")],
        key_from_ini: None,
        source: Source::External {
            url: "https://github.com/LeadingTrendTechnologies/HoleshotHUD/releases/latest",
        },
        made_for: None,
        kind: Kind::ThirdParty,
        note: Some("Requires its companion overlay program"),
        racing: Racing::Stay,
        category: "hud",
        summary: "Standings, relative, map, minimap and radar, drawn by its own overlay program.",
    },
    Known {
        id: "mxbfix",
        name: "MXBFix",
        author: "Amped",
        mxbmods_slug: Some("mxbfix-fixes-common-game-crashes-2"),
        dlo: &["mxbfix.dlo"],
        stem_prefix: None,
        owned: &["mxbfix.ini"],
        shared: NONE_STR,
        configs: NONE_CONFIG,
        keys: NONE_KEYS,
        key_from_ini: None,
        source: Source::Github {
            repo: "xAmped/mxbfix",
            asset_prefix: "mxbfix",
            asset_suffix: ".dlo",
        },
        made_for: None,
        kind: Kind::ThirdParty,
        note: None,
        racing: Racing::AlwaysOn,
        category: "fixes",
        summary: "Fixes common game crashes and reports where the game crashed.",
    },
    Known {
        id: "mxbtt",
        name: "MXBTT",
        author: "MXB A-Kit Studio",
        mxbmods_slug: Some("mxbtt-com-the-weekly-time-trial-prize-money-racing-platform"),
        dlo: &["mxbtt.dlo"],
        stem_prefix: None,
        owned: NONE_STR,
        shared: NONE_STR,
        configs: NONE_CONFIG,
        keys: NONE_KEYS,
        key_from_ini: None,
        source: Source::External {
            url: "https://mxbtt.com",
        },
        made_for: None,
        kind: Kind::ThirdParty,
        note: None,
        racing: Racing::Stay,
        category: "timing",
        summary: "Weekly time trials with prize money, with an in-game dashboard.",
    },
    Known {
        id: "akit-studio",
        name: "MXB A-Kit Studio",
        author: "MXB A-Kit Studio",
        mxbmods_slug: Some("mxb-a-kit-studio"),
        dlo: &["mx_replay_studio.dlo", "akit_studio.dlo"],
        stem_prefix: Some("akit"),
        owned: NONE_STR,
        shared: NONE_STR,
        configs: NONE_CONFIG,
        keys: NONE_KEYS,
        key_from_ini: None,
        source: Source::External {
            url: "https://mxbtt.com/akit-studio",
        },
        made_for: None,
        kind: Kind::ThirdParty,
        note: None,
        racing: Racing::Stay,
        category: "timing",
        summary: "Telemetry and setup engineering: live HUD, setup advisor and replay telemetry.",
    },
    Known {
        id: "mxpulse",
        name: "MXPulse",
        author: "KST Design",
        mxbmods_slug: Some("mxpulse"),
        dlo: NONE_STR,
        stem_prefix: Some("mxpulse"),
        owned: NONE_STR,
        shared: NONE_STR,
        configs: NONE_CONFIG,
        keys: &[("F9", "Settings")],
        key_from_ini: None,
        source: Source::External {
            url: "https://github.com/daf0707pro-lgtm/MXPulse/releases",
        },
        made_for: None,
        kind: Kind::ThirdParty,
        note: None,
        racing: Racing::Stay,
        category: "hud",
        summary: "A customizable HUD with live race information and modular widgets.",
    },
    Known {
        id: "discord-rpc",
        name: "Improved Discord Status",
        author: "stars",
        mxbmods_slug: Some("improved-discord-rich-presence-discord-rpc"),
        dlo: &["discord-rpc.dlo"],
        stem_prefix: None,
        owned: &["discord_game_sdk.dll"],
        shared: NONE_STR,
        configs: NONE_CONFIG,
        keys: NONE_KEYS,
        key_from_ini: None,
        source: Source::External {
            url: "https://github.com/starsssy/mxb-discord-rpc",
        },
        made_for: None,
        kind: Kind::ThirdParty,
        note: None,
        racing: Racing::Stay,
        category: "other",
        summary: "Shows what you're riding and where in your Discord status.",
    },
    Known {
        id: "ragdoll",
        name: "MX Bikes Ragdoll",
        author: "BoredDev",
        mxbmods_slug: Some("mx-bikes-ragdoll-v1-0-2"),
        dlo: &["mxb_ragdoll.dlo"],
        stem_prefix: None,
        owned: &["mxb_ragdoll.log"],
        shared: NONE_STR,
        configs: NONE_CONFIG,
        keys: NONE_KEYS,
        key_from_ini: None,
        source: Source::MxbMods,
        made_for: Some("beta21"),
        kind: Kind::ThirdParty,
        note: None,
        racing: Racing::Off,
        category: "physics",
        summary: "The rider falls as a ragdoll on crashes and climbs back on. Made for Beta 21.",
    },
    Known {
        id: "frostmod",
        name: "FrostMod",
        author: "MXB App",
        mxbmods_slug: None,
        dlo: &["frostmod.dlo", "frostmod_session.dlo"],
        stem_prefix: None,
        owned: &["frostmod.dir", "frostmod_session.log"],
        shared: NONE_STR,
        configs: NONE_CONFIG,
        keys: NONE_KEYS,
        key_from_ini: None,
        source: Source::External { url: "" },
        made_for: None,
        kind: Kind::MxbApp,
        note: Some("Managed in FrostMod"),
        racing: Racing::AlwaysOn,
        category: "fixes",
        summary: "Paint sync, model swap sync, refresh without restarting, crash fixes, Aussie mode.",
    },
    Known {
        id: "mxbsecure",
        name: "MXB Secure",
        author: "MXB App",
        mxbmods_slug: None,
        dlo: &["mxbsecure.dlo"],
        stem_prefix: None,
        owned: &["mxbsecure.dir", "mxbsecure.log"],
        shared: NONE_STR,
        configs: NONE_CONFIG,
        keys: NONE_KEYS,
        key_from_ini: None,
        source: Source::External { url: "" },
        made_for: None,
        kind: Kind::MxbApp,
        note: Some("Managed by Secure content"),
        racing: Racing::AlwaysOn,
        category: "other",
        summary: "Loads creator-protected content you own.",
    },
    Known {
        id: "mxbcoach",
        name: "MXB Coach",
        author: "MXB App",
        mxbmods_slug: None,
        dlo: &["mxbcoach.dlo"],
        stem_prefix: None,
        owned: &["mxbcoach_data/"],
        shared: NONE_STR,
        configs: NONE_CONFIG,
        keys: NONE_KEYS,
        key_from_ini: None,
        source: Source::External { url: "" },
        made_for: None,
        kind: Kind::MxbApp,
        note: Some("Managed by MXB Coach"),
        racing: Racing::AlwaysOn,
        category: "timing",
        summary: "MXB Coach's in-game overlay.",
    },
    Known {
        id: "frostfx",
        name: "FrostFX",
        author: "MXB App",
        mxbmods_slug: None,
        dlo: &["frostfx.dlo"],
        stem_prefix: None,
        owned: &["frostfx.ini", "frostfx.log"],
        shared: NONE_STR,
        configs: NONE_CONFIG,
        keys: NONE_KEYS,
        key_from_ini: None,
        source: Source::External { url: "" },
        made_for: None,
        kind: Kind::MxbApp,
        note: Some("Managed by MXB App"),
        racing: Racing::AlwaysOn,
        category: "graphics",
        summary: "FrostFX effects.",
    },
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigView {
    pub path: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyView {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub id: String,
    pub name: String,
    pub author: String,
    pub kind: Kind,
    pub enabled: bool,
    pub files: Vec<String>,
    pub configs: Vec<ConfigView>,
    pub size: u64,
    pub modified: Option<u64>,
    pub version: Option<String>,
    pub duplicate_disabled_copy: bool,
    pub keys: Vec<KeyView>,
    pub presets: Vec<String>,
    pub note: Option<String>,
    pub made_for: Option<String>,
    /// Racing mode turns it off (the player's choice, else the plugin's default).
    pub race_off: bool,
    /// A crash fix (or one of ours): always stays on, and the choice can't be changed.
    pub race_locked: bool,
    /// Another installed plugin already does this one's job (MXBFix under FrostMod).
    pub covered_by: Option<String>,
    pub category: String,
    pub summary: String,
    /// Racing mode's default for it: the catalog's, else its manifest's, else stays on.
    pub race_default: bool,
    /// The author's page, from the manifest.
    pub homepage: Option<String>,
    /// It ships an `.mxbplugin.json` the app read.
    pub has_manifest: bool,
    /// `owner/repo` whose GitHub releases say what's newest (manifest `updates.github`).
    #[serde(skip)]
    pub update_github: Option<String>,
    /// Conflicts the author declared: (the other plugin's id or `.dlo` name, why).
    #[serde(skip)]
    pub declared_conflicts: Vec<(String, String)>,
    /// `github`, `mxbmods` or `external`; `none` for a plugin the app can't fetch.
    pub source: String,
    /// The upload date recorded when the app installed it (mxb-mods has no versions).
    pub installed_updated: Option<String>,
}

fn eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}
fn stock(name: &str) -> bool {
    STOCK.iter().any(|s| eq(s, name))
}
fn known_by_id(id: &str) -> Option<&'static Known> {
    KNOWN.iter().find(|k| k.id == id)
}
fn matches(k: &Known, name: &str) -> bool {
    if k.dlo.iter().any(|n| eq(n, name)) {
        return true;
    }
    let stem = Path::new(name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    k.stem_prefix
        .is_some_and(|p| stem.to_lowercase().starts_with(&p.to_lowercase()))
}
fn find_case(dir: &Path, rel: &str) -> PathBuf {
    let mut out = dir.to_path_buf();
    for part in rel.trim_end_matches('/').split('/') {
        let candidate = fs::read_dir(&out)
            .ok()
            .and_then(|rd| {
                rd.flatten()
                    .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))
            })
            .map(|e| e.path());
        out = candidate.unwrap_or_else(|| out.join(part));
    }
    out
}
fn path_size(path: &Path) -> u64 {
    if path.is_file() {
        return path.metadata().map(|m| m.len()).unwrap_or(0);
    }
    fs::read_dir(path)
        .map(|rd| rd.flatten().map(|e| path_size(&e.path())).sum())
        .unwrap_or(0)
}
fn relative_slash(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
fn existing_paths(root: &Path, k: &Known, dlo_names: &[String]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = dlo_names
        .iter()
        .map(|n| find_case(root, n))
        .filter(|p| p.exists())
        .collect();
    out.extend(
        k.owned
            .iter()
            .map(|p| find_case(root, p))
            .filter(|p| p.exists()),
    );
    out.sort();
    out.dedup();
    out
}
fn read_ini_key(path: &Path, section: &str, key: &str) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let mut current = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            current = line[1..line.len() - 1].trim().to_string();
            continue;
        }
        if current.eq_ignore_ascii_case(section) {
            if let Some((a, b)) = line.split_once('=') {
                if a.trim().eq_ignore_ascii_case(key) {
                    return Some(b.trim().trim_matches('"').to_string());
                }
            }
        }
    }
    None
}

/// Scan both places a plugin can live. Stock shims never become candidates.
pub fn scan(game_dir: &Path, disabled_dir: &Path, docs_dir: &Path) -> Vec<Installed> {
    let active = game_dir.join("plugins");
    let mut names: Vec<(String, bool)> = Vec::new();
    for (root, enabled) in [(active.as_path(), true), (disabled_dir, false)] {
        if let Ok(rd) = fs::read_dir(root) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if e.path().is_file()
                    && e.path()
                        .extension()
                        .is_some_and(|x| x.eq_ignore_ascii_case("dlo"))
                    && !stock(&name)
                {
                    names.push((name, enabled));
                }
            }
        }
    }
    let mut ids: Vec<String> = Vec::new();
    for (name, _) in &names {
        let id = KNOWN
            .iter()
            .find(|k| matches(k, name))
            .map(|k| k.id.to_string())
            .unwrap_or_else(|| {
                format!(
                    "file:{}",
                    Path::new(name)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_lowercase()
                )
            });
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    let mut out = Vec::new();
    for id in ids {
        let known = known_by_id(&id);
        let matching: Vec<_> = names
            .iter()
            .filter(|(n, _)| {
                known.map(|k| matches(k, n)).unwrap_or_else(|| {
                    format!(
                        "file:{}",
                        Path::new(n)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_lowercase()
                    ) == id
                })
            })
            .collect();
        let enabled = matching.iter().any(|x| x.1);
        let duplicate_disabled_copy = enabled && matching.iter().any(|x| !x.1);
        let root = if enabled { active.as_path() } else { disabled_dir };
        let dlo_names: Vec<String> = matching
            .iter()
            .filter(|x| x.1 == enabled)
            .map(|x| x.0.clone())
            .collect();
        let stem = Path::new(dlo_names.first().map(String::as_str).unwrap_or(""))
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let unknown_owned = vec![
            format!("{stem}_data/"),
            format!("{stem}/"),
            format!("{stem}.ini"),
            format!("{stem}.log"),
        ];
        let paths = if let Some(k) = known {
            existing_paths(root, k, &dlo_names)
        } else {
            let mut p: Vec<_> = dlo_names.iter().map(|n| find_case(root, n)).collect();
            p.extend(
                unknown_owned
                    .iter()
                    .map(|n| find_case(root, n))
                    .filter(|p| p.exists()),
            );
            p
        };
        let configs = known
            .map(|k| {
                k.configs
                    .iter()
                    .filter_map(|c| {
                        let base = match c.where_ {
                            ConfigWhere::Plugins => root,
                            ConfigWhere::Documents => docs_dir,
                        };
                        let p = find_case(base, c.path);
                        p.exists().then(|| ConfigView {
                            path: p.to_string_lossy().into_owned(),
                            label: c.label.into(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let keys = known
            .map(|k| {
                let mut v: Vec<KeyView> = k
                    .keys
                    .iter()
                    .map(|(a, b)| KeyView {
                        key: (*a).into(),
                        label: (*b).into(),
                    })
                    .collect();
                if let Some((f, s, key, label)) = k.key_from_ini {
                    v.push(KeyView {
                        key: read_ini_key(&find_case(root, f), s, key)
                            .unwrap_or_else(|| "Q".into()),
                        label: label.into(),
                    });
                }
                v
            })
            .unwrap_or_default();
        let presets = if id == "graphics-overhaul" {
            let d = find_case(root, "presets");
            fs::read_dir(d)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| {
                            e.path()
                                .extension()
                                .is_some_and(|x| x.eq_ignore_ascii_case("ini"))
                        })
                        .map(|e| {
                            e.path()
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let dlo = paths
            .iter()
            .find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dlo")));
        out.push(Installed {
            id: id.clone(),
            name: known.map(|k| k.name.to_string()).unwrap_or(stem),
            author: known.map(|k| k.author.to_string()).unwrap_or_default(),
            kind: known.map(|k| k.kind).unwrap_or(Kind::ThirdParty),
            enabled,
            files: paths.iter().map(|p| relative_slash(root, p)).collect(),
            configs,
            size: paths.iter().map(|p| path_size(p)).sum(),
            modified: dlo.and_then(|p| {
                p.metadata()
                    .ok()?
                    .modified()
                    .ok()?
                    .duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs())
            }),
            version: None,
            duplicate_disabled_copy,
            keys,
            presets,
            note: known.and_then(|k| k.note.map(str::to_string)),
            made_for: known.and_then(|k| k.made_for.map(str::to_string)),
            race_off: racing_default(&id),
            race_locked: race_locked(&id),
            covered_by: None,
            category: known.map(|k| k.category).unwrap_or("other").into(),
            summary: known.map(|k| k.summary).unwrap_or_default().into(),
            source: known.map(|k| source_view(k).0).unwrap_or_else(|| "none".into()),
            installed_updated: None,
            race_default: racing_default(&id),
            homepage: None,
            has_manifest: false,
            update_github: None,
            declared_conflicts: Vec::new(),
        });
    }
    for p in &mut out {
        let root = if p.enabled { active.as_path() } else { disabled_dir };
        apply_manifest(p, root, docs_dir);
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

// ===========================================================================
// The plugin manifest: `<plugin>.mxbplugin.json`, beside the `.dlo`. Optional, and every
// field in it is. Documented for authors in docs/plugins.md; keep the two in step.
// ===========================================================================

/// The longest manifest the app reads. A real one is a few hundred bytes.
const MAX_MANIFEST: u64 = 64 * 1024;

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Manifest {
    pub name: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub homepage: Option<String>,
    pub game_version: Option<String>,
    pub files: Vec<String>,
    pub settings: Vec<ManifestSetting>,
    pub keys: Vec<ManifestKey>,
    pub racing: Option<String>,
    pub conflicts: Vec<ManifestConflict>,
    pub updates: Option<ManifestUpdates>,
}
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ManifestSetting {
    pub file: String,
    pub label: Option<String>,
    /// `plugins` (default) or `documents` (`Documents\PiBoSo\MX Bikes`).
    #[serde(rename = "where")]
    pub where_: Option<String>,
}
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ManifestKey {
    pub key: String,
    pub label: String,
}
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ManifestConflict {
    /// Another plugin: its `.dlo` file name, or a catalog id like `graphics-overhaul`.
    pub with: String,
    pub reason: String,
}
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ManifestUpdates {
    /// `owner/repo`: the newest GitHub release's tag is the latest version.
    pub github: Option<String>,
}

/// A short, single-line string from someone else's file, or nothing.
fn clean(s: Option<&str>, max: usize) -> Option<String> {
    let s: String = s?.chars().filter(|c| !c.is_control()).take(max).collect();
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}
fn clean_rel(s: &str) -> Option<String> {
    let s = s.trim().replace('\\', "/");
    let dir = s.ends_with('/');
    let rel = Path::new(s.trim_end_matches('/'));
    (!s.is_empty() && safe_rel(rel) && !stock(&rel.file_name()?.to_string_lossy()))
        .then(|| if dir { format!("{}/", rel.to_string_lossy().replace('\\', "/")) } else { rel.to_string_lossy().replace('\\', "/") })
}
fn valid_repo(s: &str) -> bool {
    let mut parts = s.split('/');
    let ok = |p: Option<&str>| {
        p.is_some_and(|p| !p.is_empty() && p.len() <= 100 && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)))
    };
    ok(parts.next()) && ok(parts.next()) && parts.next().is_none()
}

/// `<stem>.mxbplugin.json` for a plugin whose `.dlo` is `dlo_name`, under `root`.
pub fn manifest_name(dlo_name: &str) -> String {
    let stem = Path::new(dlo_name).file_stem().unwrap_or_default().to_string_lossy().into_owned();
    format!("{stem}.mxbplugin.json")
}
pub fn read_manifest(root: &Path, dlo_name: &str) -> Option<Manifest> {
    let path = find_case(root, &manifest_name(dlo_name));
    if fs::metadata(&path).ok()?.len() > MAX_MANIFEST {
        log::warn!("[plugins] {} is too large to read", path.display());
        return None;
    }
    match serde_json::from_slice(&fs::read(&path).ok()?) {
        Ok(m) => Some(m),
        Err(e) => {
            log::warn!("[plugins] couldn't read {}: {e}", path.display());
            None
        }
    }
}

/// Fill in what the author's manifest says. The catalog's own facts win for plugins the app
/// already knows; a manifest only adds what the catalog leaves out (version, more files).
fn apply_manifest(p: &mut Installed, root: &Path, docs_dir: &Path) {
    let Some(dlo) = p
        .files
        .iter()
        .find(|f| Path::new(f).extension().is_some_and(|x| x.eq_ignore_ascii_case("dlo")))
        .cloned()
    else {
        return;
    };
    let Some(m) = read_manifest(root, &dlo) else { return };
    p.has_manifest = true;
    let known = known_by_id(&p.id).is_some();
    if p.version.is_none() {
        p.version = clean(m.version.as_deref(), 32).map(|v| v.trim_start_matches(['v', 'V']).to_string());
    }
    let manifest_file = manifest_name(&dlo);
    if !p.files.iter().any(|f| f.eq_ignore_ascii_case(&manifest_file)) {
        p.files.push(manifest_file);
    }
    if known {
        return;
    }
    if let Some(n) = clean(m.name.as_deref(), 60) {
        p.name = n;
    }
    if let Some(a) = clean(m.author.as_deref(), 60) {
        p.author = a;
    }
    if let Some(d) = clean(m.description.as_deref(), 400) {
        p.summary = d;
    }
    if let Some(c) = m.category.as_deref().map(str::to_lowercase) {
        if ["hud", "graphics", "physics", "fixes", "timing", "other"].contains(&c.as_str()) {
            p.category = c;
        }
    }
    p.homepage = clean(m.homepage.as_deref(), 300).filter(|u| u.starts_with("https://"));
    p.made_for = clean(m.game_version.as_deref(), 16).map(|v| v.to_lowercase().replace(' ', ""));
    for f in m.files.iter().filter_map(|f| clean_rel(f)).take(64) {
        if find_case(root, &f).exists() && !p.files.contains(&f) {
            p.files.push(f);
        }
    }
    for s in m.settings.iter().take(16) {
        let Some(rel) = clean_rel(&s.file) else { continue };
        let base = if s.where_.as_deref().is_some_and(|w| w.eq_ignore_ascii_case("documents")) { docs_dir } else { root };
        let path = find_case(base, &rel);
        if path.exists() {
            p.configs.push(ConfigView {
                path: path.to_string_lossy().into_owned(),
                label: clean(s.label.as_deref(), 40).unwrap_or_else(|| rel.clone()),
            });
        }
    }
    for k in m.keys.iter().take(16) {
        if let (Some(key), Some(label)) = (clean(Some(&k.key), 24), clean(Some(&k.label), 80)) {
            p.keys.push(KeyView { key, label });
        }
    }
    match m.racing.as_deref().map(str::to_lowercase).as_deref() {
        Some("off") => p.race_default = true,
        Some("stay") => p.race_default = false,
        Some("always-on") => {
            p.race_default = false;
            p.race_locked = true;
        }
        _ => {}
    }
    p.race_off = p.race_default;
    p.declared_conflicts = m
        .conflicts
        .iter()
        .take(16)
        .filter_map(|c| Some((clean(Some(&c.with), 80)?, clean(Some(&c.reason), 200)?)))
        .collect();
    if let Some(repo) = m.updates.as_ref().and_then(|u| u.github.as_deref()).filter(|r| valid_repo(r)) {
        p.update_github = Some(repo.to_string());
        if p.homepage.is_none() {
            p.homepage = Some(format!("https://github.com/{repo}/releases"));
        }
        p.source = "external".into();
    }
}

fn safe_rel(rel: &Path) -> bool {
    !rel.is_absolute() && rel.components().all(|c| matches!(c, Component::Normal(_)))
}
fn disabled_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("mxbapp_disabled").join("plugins")
}

fn plugin_paths(root: &Path, id: &str) -> anyhow::Result<Vec<PathBuf>> {
    if let Some(k) = known_by_id(id) {
        if k.kind == Kind::MxbApp {
            anyhow::bail!("This plugin is managed elsewhere in MXB App.");
        }
        let names: Vec<String> = k
            .dlo
            .iter()
            .map(|s| s.to_string())
            .chain(
                fs::read_dir(root)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter_map(|e| {
                        let n = e.file_name().to_string_lossy().into_owned();
                        (matches(k, &n) && !k.dlo.iter().any(|x| eq(x, &n))).then_some(n)
                    }),
            )
            .collect();
        let mut paths = existing_paths(root, k, &names);
        // An author's manifest moves with the plugin it describes.
        for n in &names {
            let manifest = find_case(root, &manifest_name(n));
            if manifest.is_file() && !paths.contains(&manifest) {
                paths.push(manifest);
            }
        }
        return Ok(paths);
    }
    let stem = id
        .strip_prefix("file:")
        .ok_or_else(|| anyhow::anyhow!("Unknown plugin id"))?;
    if stock(&format!("{stem}.dlo")) {
        anyhow::bail!("Stock game files cannot be changed.");
    }
    let names = fs::read_dir(root)?
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            (e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("dlo"))
                && e.path()
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(stem))
            .then_some(n)
        })
        .collect::<Vec<_>>();
    let mut out: Vec<_> = names.iter().map(|n| find_case(root, n)).collect();
    if let Some(dlo) = names.first() {
        let manifest = find_case(root, &manifest_name(dlo));
        if manifest.is_file() {
            out.push(manifest);
        }
        if let Some(m) = read_manifest(root, dlo) {
            for f in m.files.iter().filter_map(|f| clean_rel(f)).take(64) {
                let path = find_case(root, &f);
                if path.exists() && !out.contains(&path) {
                    out.push(path);
                }
            }
        }
    }
    for n in [
        format!("{stem}_data"),
        stem.to_string(),
        format!("{stem}.ini"),
        format!("{stem}.log"),
    ] {
        let p = find_case(root, &n);
        if p.exists() {
            out.push(p)
        }
    }
    Ok(out)
}

/// Files that sit inside `id`'s folders but belong to another known plugin (GameFX+ keeps
/// `overhaul/shaders_fx.pak` inside Graphics Overhaul's `overhaul/`). Lowercase, `/`-separated.
fn others_files(id: &str) -> HashSet<String> {
    KNOWN
        .iter()
        .filter(|k| k.id != id)
        .flat_map(|k| k.owned.iter())
        .filter(|o| !o.ends_with('/'))
        .map(|o| o.to_lowercase())
        .collect()
}

/// Every file or folder that is `id`'s under `root`, minus anything another plugin owns.
/// A folder that holds another plugin's file is listed file by file instead.
fn plugin_items(root: &Path, id: &str) -> anyhow::Result<Vec<PathBuf>> {
    let protected = others_files(id);
    let mut out = Vec::new();
    for p in plugin_paths(root, id)? {
        let rel = p.strip_prefix(root)?;
        if !safe_rel(rel) {
            anyhow::bail!("Refusing an unsafe plugin path");
        }
        let rels = relative_slash(Path::new(""), rel).to_lowercase();
        if protected.contains(&rels) {
            continue;
        }
        if p.is_dir() && protected.iter().any(|x| x.starts_with(&(rels.clone() + "/"))) {
            for e in walk_files(&p) {
                let r = relative_slash(Path::new(""), e.strip_prefix(root)?).to_lowercase();
                if !protected.contains(&r) {
                    out.push(e);
                }
            }
        } else {
            out.push(p);
        }
    }
    Ok(out)
}

fn collect_moves(
    from_root: &Path,
    to_root: &Path,
    id: &str,
) -> anyhow::Result<Vec<(PathBuf, PathBuf)>> {
    let mut moves = Vec::new();
    for p in plugin_items(from_root, id)? {
        let rel = p.strip_prefix(from_root)?.to_path_buf();
        let to = to_root.join(&rel);
        // A folder whose twin already exists on the other side (GameFX+ created `overhaul/`
        // there) is merged file by file; `move_all` still refuses any single overwrite.
        if p.is_dir() && to.exists() {
            for e in walk_files(&p) {
                let r = e.strip_prefix(from_root)?.to_path_buf();
                moves.push((e, to_root.join(r)));
            }
        } else {
            moves.push((p, to));
        }
    }
    Ok(moves)
}

/// Remove folders left empty by a file-by-file move, deepest first. `remove_dir` only ever
/// removes an empty folder, so nothing of the player's can go with it.
fn prune_empty(root: &Path, moves: &[(PathBuf, PathBuf)]) {
    let mut dirs: Vec<PathBuf> = moves
        .iter()
        .flat_map(|(from, _)| from.ancestors().skip(1).map(Path::to_path_buf).collect::<Vec<_>>())
        .filter(|d| d.starts_with(root) && d != root)
        .collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    dirs.dedup();
    for d in dirs {
        let _ = fs::remove_dir(d);
    }
}
fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(root) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                out.extend(walk_files(&e.path()))
            } else {
                out.push(e.path())
            }
        }
    }
    out
}
fn move_all(moves: &[(PathBuf, PathBuf)]) -> anyhow::Result<()> {
    for (_, to) in moves {
        if to.exists() {
            anyhow::bail!("Refusing to overwrite {}", to.display())
        }
    }
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (from, to) in moves {
        if let Some(p) = to.parent() {
            fs::create_dir_all(p)?
        }
        if let Err(e) = fs::rename(from, to) {
            for (a, b) in done.iter().rev() {
                let _ = fs::rename(b, a);
            }
            return Err(e.into());
        }
        done.push((from.clone(), to.clone()));
    }
    Ok(())
}

pub fn set_enabled(game_dir: &Path, id: &str, on: bool) -> anyhow::Result<()> {
    if crate::gameproc::is_game_running() {
        anyhow::bail!("Close MX Bikes first: the game holds its plugins open.");
    }
    let active = game_dir.join("plugins");
    let disabled = disabled_dir(game_dir);
    let (from, to) = if on {
        (&disabled, &active)
    } else {
        (&active, &disabled)
    };
    let moves = collect_moves(from, to, id)?;
    if moves.is_empty() {
        anyhow::bail!("Plugin files were not found.");
    }
    move_all(&moves)?;
    prune_empty(from, &moves);
    Ok(())
}

/// A lexical boundary used immediately before sending a plugin path to the Recycle Bin.
pub fn check_uninstall_target(game_dir: &Path, target: &Path) -> anyhow::Result<()> {
    if target
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        anyhow::bail!("Refusing a path containing '..'.");
    }
    let active = game_dir.join("plugins");
    let disabled = disabled_dir(game_dir);
    let under = (target.starts_with(&active) && target != active)
        || (target.starts_with(&disabled) && target != disabled);
    if !under || target == game_dir || target == disabled {
        anyhow::bail!("Refusing to remove a protected plugin folder.");
    }
    if target
        .file_name()
        .is_some_and(|n| stock(&n.to_string_lossy()))
    {
        anyhow::bail!("Stock game files cannot be removed.");
    }
    let rel = target
        .strip_prefix(&active)
        .or_else(|_| target.strip_prefix(&disabled))
        .ok()
        .map(|p| relative_slash(Path::new(""), p));
    if rel.is_some_and(|rel| {
        KNOWN
            .iter()
            .filter(|known| known.kind == Kind::MxbApp)
            .any(|known| {
                known.dlo.iter().chain(known.owned.iter()).any(|owned| {
                    rel.eq_ignore_ascii_case(owned.trim_end_matches('/'))
                        || rel
                            .to_lowercase()
                            .starts_with(&(owned.trim_end_matches('/').to_lowercase() + "/"))
                })
            })
    }) {
        anyhow::bail!("MXB App managed files cannot be removed here.");
    }
    Ok(())
}
pub fn uninstall(game_dir: &Path, id: &str) -> anyhow::Result<()> {
    if crate::gameproc::is_game_running() {
        anyhow::bail!("Close MX Bikes first: the game holds its plugins open.");
    }
    if known_by_id(id).is_some_and(|k| k.kind == Kind::MxbApp) {
        anyhow::bail!("This plugin is managed elsewhere in MXB App.");
    }
    for root in [game_dir.join("plugins"), disabled_dir(game_dir)] {
        for p in plugin_items(&root, id).unwrap_or_default() {
            check_uninstall_target(game_dir, &p)?;
            crate::trashbin::move_to_trash(&p)?;
        }
    }
    Ok(())
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Journal {
    ids: Vec<String>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PluginRecord {
    version: Option<String>,
    source: Option<String>,
    modified: Option<String>,
    installed_at: Option<String>,
    files: Vec<String>,
    dlo_size: Option<u64>,
    /// The player's choice for racing mode; `None` keeps the plugin's default.
    race_off: Option<bool>,
    /// The app turned this plugin off once because another one covers it. Never again
    /// after that: a player who turns it back on keeps it on.
    covered_off_done: bool,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Records {
    plugins: BTreeMap<String, PluginRecord>,
}
/// Our own state lives in the shared app-data root, like `config.json`, not in the bundle
/// identifier's folder: a dev profile (`MXB_DEV_PROFILE_ROOT`) then isolates it too.
fn state_path(app: &AppHandle, name: &str) -> anyhow::Result<PathBuf> {
    Ok(crate::config::data_dir(app)
        .ok_or_else(|| anyhow::anyhow!("no app data folder"))?
        .join(name))
}
fn records_path(app: &AppHandle) -> anyhow::Result<PathBuf> {
    state_path(app, "gameplugins.json")
}
fn load_records(app: &AppHandle) -> Records {
    records_path(app)
        .ok()
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
fn save_records(app: &AppHandle, r: &Records) -> anyhow::Result<()> {
    write_json_atomic(&records_path(app)?, r)
}
fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    if path.exists() {
        fs::remove_file(path)?
    }
    fs::rename(tmp, path)?;
    Ok(())
}
fn read_journal(path: &Path) -> Journal {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
/// What racing mode does with `id` when the player hasn't chosen: unknown plugins stay on,
/// since nothing says they are only looks.
fn racing_default(id: &str) -> bool {
    known_by_id(id).is_some_and(|k| k.racing == Racing::Off)
}
fn race_locked(id: &str) -> bool {
    known_by_id(id).is_some_and(|k| k.racing == Racing::AlwaysOn || k.kind == Kind::MxbApp)
}
/// The enabled plugins racing mode would turn off right now, in display order.
fn racing_set(installed: &[Installed], records: &Records) -> Vec<String> {
    installed
        .iter()
        .filter(|p| p.enabled && p.kind == Kind::ThirdParty && !p.race_locked)
        .filter(|p| {
            records
                .plugins
                .get(&p.id)
                .and_then(|r| r.race_off)
                .unwrap_or(p.race_default)
        })
        .map(|p| p.id.clone())
        .collect()
}

/// Turn off exactly `ids` and remember them in `journal`, added to whatever it already holds
/// (a join while racing mode is already on parks only what is new).
pub fn park_for_racing(game_dir: &Path, journal: &Path, ids: &[String]) -> anyhow::Result<usize> {
    if crate::gameproc::is_game_running() {
        anyhow::bail!("Close MX Bikes first: the game holds its plugins open.");
    }
    let mut j = read_journal(journal);
    let mut n = 0;
    for id in ids {
        if race_locked(id) {
            continue;
        }
        set_enabled(game_dir, id, false)?;
        if !j.ids.contains(id) {
            j.ids.push(id.clone());
        }
        n += 1;
        // Written after every move, so a crash mid-way still knows what to put back.
        write_json_atomic(journal, &j)?;
    }
    if j.ids.is_empty() {
        let _ = fs::remove_file(journal);
    }
    Ok(n)
}
pub fn restore_after_racing(game_dir: &Path, journal: &Path) -> anyhow::Result<usize> {
    if crate::gameproc::is_game_running() {
        anyhow::bail!("Close MX Bikes first: the game holds its plugins open.");
    }
    let j = read_journal(journal);
    let mut n = 0;
    for id in j.ids {
        if !plugin_paths(&disabled_dir(game_dir), &id)
            .unwrap_or_default()
            .is_empty()
            && !plugin_paths(&game_dir.join("plugins"), &id)
                .unwrap_or_default()
                .iter()
                .any(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dlo")))
        {
            if set_enabled(game_dir, &id, true).is_ok() {
                n += 1
            }
        }
    }
    let _ = fs::remove_file(journal);
    Ok(n)
}

/// Racing mode on an app join (Race mode on): turn off the racing set. Same journal as the
/// button, so the Plugins tab shows it and one click puts everything back; the game's exit
/// puts it back on its own through [`race_restore`] when the join turned it on.
pub fn race_park(app: &AppHandle, cfg: &crate::config::AppConfig) -> anyhow::Result<()> {
    if crate::gameproc::is_game_running() {
        return Ok(());
    }
    let journal = state_path(app, RACING_JOURNAL)?;
    let records = load_records(app);
    let game = PathBuf::from(cfg.install_dir());
    let installed = scan(&game, &disabled_dir(&game), Path::new(&cfg.mods_path));
    let ids = racing_set(&installed, &records);
    let by_hand = journal.exists();
    let n = park_for_racing(&game, &journal, &ids)?;
    // Only a join that switched racing mode on switches it off again when the game closes.
    // Racing mode the player turned on by hand stays on until they turn it off.
    if n > 0 && !by_hand {
        write_json_atomic(&state_path(app, RACING_BY_JOIN)?, &true)?;
    }
    if n > 0 {
        log::info!("[race] racing mode: turned off {n} plugin(s) for this session");
    }
    Ok(())
}
pub fn race_restore(app: &AppHandle) -> anyhow::Result<()> {
    if crate::gameproc::is_game_running() {
        return Ok(());
    }
    let marker = state_path(app, RACING_BY_JOIN)?;
    if !marker.exists() {
        return Ok(());
    }
    let cfg = crate::config::load(app).unwrap_or_default();
    restore_after_racing(Path::new(&cfg.install_dir()), &state_path(app, RACING_JOURNAL)?)?;
    let _ = fs::remove_file(marker);
    Ok(())
}
const RACING_JOURNAL: &str = "gameplugins-racing.json";
/// Present while racing mode is on because a join turned it on (not the button).
const RACING_BY_JOIN: &str = "gameplugins-racing-join.json";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub level: String,
    pub ids: Vec<String>,
    pub message: String,
}
/// (plugin, plugin that already does all of its job). From the crash coverage table: every
/// crash MXBFix fixes, FrostMod fixes too.
const COVERED_BY: &[(&str, &str)] = &[("mxbfix", "frostmod")];

fn covering(id: &str, installed: &[Installed]) -> Option<&'static str> {
    COVERED_BY
        .iter()
        .find(|(p, by)| *p == id && installed.iter().any(|i| i.id == *by && i.enabled))
        .map(|(_, by)| *by)
}

/// Turn off, once, a plugin another installed one already covers. Moved aside like any
/// switch-off, never deleted, and remembered so it isn't done twice. Returns the names.
fn turn_off_covered(game: &Path, installed: &[Installed], records: &mut Records) -> Vec<String> {
    let mut done = Vec::new();
    if crate::gameproc::is_game_running() {
        return done;
    }
    for p in installed.iter().filter(|p| p.enabled) {
        if covering(&p.id, installed).is_none() {
            continue;
        }
        let rec = records.plugins.entry(p.id.clone()).or_default();
        if rec.covered_off_done {
            continue;
        }
        match set_enabled(game, &p.id, false) {
            Ok(()) => {
                rec.covered_off_done = true;
                log::info!("[plugins] turned off {}: FrostMod already fixes the same crashes", p.name);
                done.push(p.name.clone());
            }
            Err(e) => log::warn!("[plugins] couldn't turn off {}: {e:#}", p.name),
        }
    }
    done
}

pub fn conflicts(
    installed: &[Installed],
    game_build: Option<&str>,
    reshade_active: bool,
) -> Vec<Conflict> {
    let on = |id: &str| installed.iter().any(|p| p.id == id && p.enabled);
    let mut out = Vec::new();
    if on("mxb-damage") && (on("gamefx-plus") || on("rider-pov-plus")) {
        out.push(Conflict{level:"error".into(),ids:vec!["mxb-damage".into(),if on("gamefx-plus"){"gamefx-plus"}else{"rider-pov-plus"}.into()],message:"MXB Damage Plugin doesn't work while GameFX+ or Rider POV+ is on (its author says so). Turn one of them off.".into()})
    }
    if on("graphics-overhaul") && reshade_active {
        out.push(Conflict {
            level: "warning".into(),
            ids: vec!["graphics-overhaul".into()],
            message: "Graphics Overhaul and ReShade both post-process the picture.".into(),
        })
    }
    for (plugin, by) in COVERED_BY {
        if on(plugin) && on(by) {
            out.push(Conflict {
                level: "warning".into(),
                ids: vec![(*plugin).into(), (*by).into()],
                message: "MXBFix and FrostMod patch the same game crashes, and two plugins hooking \
                          the same places can get in each other's way. FrostMod already fixes \
                          everything MXBFix does, so MXBFix can be turned off."
                    .into(),
            })
        }
    }
    for p in installed.iter().filter(|p| p.enabled) {
        for (with, reason) in &p.declared_conflicts {
            let other = installed.iter().find(|o| {
                o.enabled
                    && o.id != p.id
                    && (o.id.eq_ignore_ascii_case(with) || o.files.iter().any(|f| f.eq_ignore_ascii_case(with)))
            });
            if let Some(o) = other {
                out.push(Conflict {
                    level: "warning".into(),
                    ids: vec![p.id.clone(), o.id.clone()],
                    message: format!("{} and {}: {reason} (from {}'s author)", p.name, o.name, p.name),
                });
            }
        }
    }
    let hud: [&str; 3] = ["mxbmrp3", "holeshot-hud", "mxpulse"];
    let ids = hud
        .iter()
        .filter(|id| on(id))
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    if ids.len() > 1 {
        out.push(Conflict {
            level: "info".into(),
            ids,
            message: "Several HUD plugins draw standings or timing; their widgets may overlap."
                .into(),
        })
    }
    for p in installed.iter().filter(|p| p.enabled) {
        if p.files
            .iter()
            .filter(|file| {
                Path::new(file)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("dlo"))
            })
            .count()
            > 1
        {
            out.push(Conflict {
                level: "error".into(),
                ids: vec![p.id.clone()],
                message: format!("{} has more than one enabled .dlo copy.", p.name),
            });
        }
        if let (Some(made), Some(build)) = (p.made_for.as_deref(), game_build) {
            let major = |s: &str| {
                s.to_lowercase()
                    .strip_prefix("beta")
                    .unwrap_or(s)
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
            };
            if major(made) != major(build) {
                out.push(Conflict {
                    level: "warning".into(),
                    ids: vec![p.id.clone()],
                    message: format!(
                        "Made for Beta {}; this game is {}. It may not load",
                        major(made),
                        title_beta(build)
                    ),
                })
            }
        }
        if p.duplicate_disabled_copy {
            out.push(Conflict {
                level: "info".into(),
                ids: vec![p.id.clone()],
                message: format!("{} is enabled and also has a disabled copy.", p.name),
            })
        }
    }
    out
}
fn title_beta(s: &str) -> String {
    let s = s.trim();
    if s.to_lowercase().starts_with("beta") {
        format!("Beta {}", &s[4..])
    } else {
        s.into()
    }
}
pub fn version_older(installed: &str, latest: &str) -> bool {
    let nums = |s: &str| {
        s.trim_start_matches(['v', 'V'])
            .split('.')
            .map(|x| {
                x.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<u64>()
                    .unwrap_or(0)
            })
            .collect::<Vec<_>>()
    };
    let mut a = nums(installed);
    let mut b = nums(latest);
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a < b
}
pub fn game_build(install_dir: &Path) -> Option<String> {
    let bytes = fs::read(install_dir.join("mxbikes.exe")).ok()?;
    beta_in(&bytes)
}

/// The first `beta<digits>[letter]` in a byte buffer. The executable mentions "beta" in
/// other places too (menu text), so a bare word without a number never counts.
fn beta_in(bytes: &[u8]) -> Option<String> {
    let lower: Vec<u8> = bytes.iter().map(u8::to_ascii_lowercase).collect();
    let mut at = 0;
    while let Some(i) = lower[at..].windows(4).position(|w| w == b"beta") {
        let start = at + i + 4;
        let n = lower[start..].iter().take_while(|c| c.is_ascii_digit()).count();
        if n > 0 && n <= 3 {
            let digits = std::str::from_utf8(&lower[start..start + n]).ok()?;
            let suffix = lower
                .get(start + n)
                .filter(|c| c.is_ascii_lowercase())
                .filter(|_| lower.get(start + n + 1).is_none_or(|c| !c.is_ascii_alphanumeric()))
                .map(|c| (*c as char).to_string())
                .unwrap_or_default();
            return Some(format!("beta{digits}{suffix}"));
        }
        at = start;
    }
    None
}

pub fn is_plugins_subpath(s: &str) -> bool {
    s.trim_matches('/').eq_ignore_ascii_case(SUBPATH)
}
fn payload_root(extracted: &Path) -> anyhow::Result<PathBuf> {
    let direct = find_case(extracted, "plugins");
    if direct.is_dir()
        && walk_files(&direct)
            .iter()
            .any(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dlo")))
    {
        return Ok(direct);
    }
    if let Ok(rd) = fs::read_dir(extracted) {
        for e in rd.flatten().filter(|e| e.path().is_dir()) {
            let p = find_case(&e.path(), "plugins");
            if p.is_dir()
                && walk_files(&p)
                    .iter()
                    .any(|f| f.extension().is_some_and(|x| x.eq_ignore_ascii_case("dlo")))
            {
                return Ok(p);
            }
        }
    }
    let mut dlo_dirs = walk_files(extracted)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dlo")))
        .filter_map(|p| p.parent().map(Path::to_path_buf))
        .collect::<Vec<_>>();
    dlo_dirs.sort_by_key(|p| p.components().count());
    dlo_dirs.into_iter().next().ok_or_else(|| {
        anyhow::anyhow!(if walk_files(extracted).iter().any(|p| p
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("exe") || x.eq_ignore_ascii_case("msi")))
        {
            "This plugin installs with its own setup program. Use the Get it button."
        } else {
            "No .dlo plugin was found in this download."
        })
    })
}
/// A build of the plugin for another PiBoSo game. MXBMRP3 ships `mxbmrp3_gpb.dlo` (GP Bikes)
/// and `mxbmrp3_krp.dlo` (Kart Racing Pro) beside the MX Bikes one in a single zip; copied
/// into MX Bikes' plugins folder they would be loaded too.
fn other_game_build(rel: &Path) -> bool {
    let is_dlo = rel.extension().is_some_and(|x| x.eq_ignore_ascii_case("dlo"));
    let stem = rel.file_stem().unwrap_or_default().to_string_lossy().to_lowercase();
    is_dlo && ["_gpb", "_krp", "_wrs", "_kart"].iter().any(|s| stem.ends_with(s))
}

/// A readme or licence beside the plugin in its archive: for the person unpacking it, not
/// for the game, so it stays out of the plugins folder.
fn archive_doc(rel: &Path) -> bool {
    let name = rel.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
    ["readme", "license", "licence", "third_party", "changelog", "credits"]
        .iter()
        .any(|p| name.starts_with(p))
        || name.ends_with(".md")
}
fn setting_path(rel: &Path) -> bool {
    rel.components().any(|c| {
        c.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("presets")
    }) || rel.extension().is_some_and(|x| {
        ["ini", "cfg", "txt"]
            .iter()
            .any(|e| x.eq_ignore_ascii_case(e))
    })
}
pub fn install_extracted(
    _app: &AppHandle,
    extracted: &Path,
    game_path: &str,
    _slug: &str,
) -> anyhow::Result<Vec<String>> {
    if crate::gameproc::is_game_running() {
        anyhow::bail!("Close MX Bikes first: the game holds its plugins open.");
    }
    copy_payload(&payload_root(extracted)?, &Path::new(game_path).join("plugins"))
}

/// Copy a plugin payload into the game's plugins folder. Plugin binaries are replaced; the
/// player's settings (`.ini`/`.cfg`/`.txt`, anything under `presets`) never are, and readmes
/// and licences from the archive root stay out.
fn copy_payload(root: &Path, dest: &Path) -> anyhow::Result<Vec<String>> {
    let root = root.to_path_buf();
    let dest = dest.to_path_buf();
    let mut written = Vec::new();
    for src in walk_files(&root) {
        let rel = src.strip_prefix(&root)?;
        if !safe_rel(rel) {
            anyhow::bail!("Refusing an unsafe path in the plugin archive.");
        }
        if rel.file_name().is_some_and(|n| stock(&n.to_string_lossy())) {
            continue;
        }
        if rel.components().count() == 1 && (archive_doc(rel) || other_game_build(rel)) {
            continue;
        }
        let to = dest.join(rel);
        if to.exists() && setting_path(rel) {
            continue;
        }
        if let Some(p) = to.parent() {
            fs::create_dir_all(p)?
        }
        fs::copy(&src, &to)?;
        written.push(relative_slash(&dest, &to));
    }
    if !written.iter().any(|p| {
        Path::new(p)
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("dlo"))
    }) {
        anyhow::bail!("No safe .dlo plugin was installed.");
    }
    Ok(written)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Available {
    pub id: String,
    pub name: String,
    pub author: String,
    pub source: String,
    pub page: Option<String>,
    pub slug: Option<String>,
    pub note: Option<String>,
}
/// One card in Browse: every plugin the app knows, installed or not.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub author: String,
    pub category: String,
    pub summary: String,
    pub source: String,
    pub page: Option<String>,
    pub note: Option<String>,
    pub installed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginsView {
    pub installed: Vec<Installed>,
    pub available: Vec<Available>,
    pub catalog: Vec<CatalogEntry>,
    pub conflicts: Vec<Conflict>,
    pub game_build: Option<String>,
    pub game_running: bool,
    pub race_mode: bool,
    /// Names racing mode would turn off now (shown before the click).
    pub racing_planned: Vec<String>,
    /// Names racing mode has turned off (shown while it's on). Empty = racing mode is off.
    pub racing_off: Vec<String>,
    /// Racing mode came on by itself with a join; it goes off when the game closes.
    pub racing_by_join: bool,
    /// Plugins the app just turned off because another installed one covers them.
    pub auto_turned_off: Vec<String>,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Latest {
    pub id: String,
    pub version: Option<String>,
    pub updated: Option<String>,
    pub page: Option<String>,
    pub thumbnail: Option<String>,
    pub rating: Option<f64>,
    pub views: Option<u64>,
}

static LATEST_CACHE: std::sync::OnceLock<
    std::sync::Mutex<HashMap<String, (std::time::Instant, Latest)>>,
> = std::sync::OnceLock::new();

async fn latest_for(known: &'static Known) -> Latest {
    let cache = LATEST_CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(known.id)
        .filter(|(expires, _)| *expires > std::time::Instant::now())
        .map(|(_, latest)| latest.clone())
    {
        return cached;
    }
    let mut out = Latest {
        id: known.id.into(),
        ..Default::default()
    };
    match known.source {
        Source::Github {
            repo,
            asset_prefix,
            asset_suffix,
        } => {
            let client = reqwest::Client::builder()
                .user_agent("MXB-App")
                .timeout(std::time::Duration::from_secs(8))
                .build();
            if let Ok(client) = client {
                if let Ok(response) = client
                    .get(format!(
                        "https://api.github.com/repos/{repo}/releases/latest"
                    ))
                    .send()
                    .await
                {
                    if let Ok(response) = response.error_for_status() {
                        if let Ok(value) = response.json::<serde_json::Value>().await {
                            out.version = value["tag_name"]
                                .as_str()
                                .map(|v| v.trim_start_matches(['v', 'V']).to_string());
                            out.updated = value["published_at"].as_str().map(str::to_string);
                            let prefix = asset_prefix.to_lowercase();
                            let suffix = asset_suffix.to_lowercase();
                            out.page = value["assets"]
                                .as_array()
                                .and_then(|assets| {
                                    assets.iter().find(|asset| {
                                        let name =
                                            asset["name"].as_str().unwrap_or("").to_lowercase();
                                        name.starts_with(&prefix) && name.ends_with(&suffix)
                                    })
                                })
                                .and_then(|asset| asset["browser_download_url"].as_str())
                                .map(str::to_string)
                                .or_else(|| {
                                    Some(format!("https://github.com/{repo}/releases/latest"))
                                });
                        }
                    }
                }
            }
        }
        Source::MxbMods => {
            if let Some(slug) = known.mxbmods_slug {
                if let Ok(Some(value)) = crate::mods::mxbmods_api::api().by_slug(slug).await {
                    out.updated = value["modified"].as_str().map(str::to_string);
                    out.page = value["link"].as_str().map(str::to_string);
                    out.thumbnail = value["thumbnail"].as_str().map(str::to_string);
                    out.views = value["views"].as_u64();
                    out.rating = value["rating"]
                        .as_f64()
                        .or_else(|| value["rating"]["average"].as_f64());
                }
            }
        }
        Source::External { url } => {
            out.page = (!url.is_empty()).then(|| url.to_string());
        }
    }
    // Every listed plugin has an mxb-mods post; its picture and rating are worth showing even
    // when the files come from GitHub or the author's own site. Same control-plane client.
    if !matches!(known.source, Source::MxbMods) {
        if let Some(slug) = known.mxbmods_slug {
            if let Ok(Some(value)) = crate::mods::mxbmods_api::api().by_slug(slug).await {
                out.thumbnail = value["thumbnail"].as_str().map(str::to_string);
                out.rating = value["rating"]["average"].as_f64();
                out.views = value["views"].as_u64();
                if matches!(known.source, Source::External { .. }) {
                    out.updated = value["modified"].as_str().map(str::to_string);
                }
            }
        }
    }
    let succeeded = out.version.is_some()
        || out.updated.is_some()
        || matches!(known.source, Source::External { .. });
    let ttl = if succeeded {
        std::time::Duration::from_secs(60 * 60)
    } else {
        std::time::Duration::from_secs(10 * 60)
    };
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(
            known.id.into(),
            (std::time::Instant::now() + ttl, out.clone()),
        );
    out
}
fn source_view(k: &Known) -> (String, Option<String>) {
    match k.source {
        Source::Github { repo, .. } => (
            "github".into(),
            Some(format!("https://github.com/{repo}/releases/latest")),
        ),
        Source::MxbMods => (
            "mxbmods".into(),
            k.mxbmods_slug.map(|s| format!("https://mxb-mods.com/{s}")),
        ),
        Source::External { url } => ("external".into(), (!url.is_empty()).then(|| url.into())),
    }
}
fn local_view(app: &AppHandle) -> anyhow::Result<PluginsView> {
    let cfg = crate::config::load(app).unwrap_or_default();
    let game = PathBuf::from(cfg.install_dir());
    let disabled = disabled_dir(&game);
    let docs = PathBuf::from(&cfg.mods_path);
    let mut records = load_records(app);
    let mut installed = scan(&game, &disabled, &docs);
    let auto_off = turn_off_covered(&game, &installed, &mut records);
    if !auto_off.is_empty() {
        let _ = save_records(app, &records);
        installed = scan(&game, &disabled, &docs);
    }
    let covers: Vec<(String, Option<&str>)> = installed
        .iter()
        .map(|p| (p.id.clone(), covering(&p.id, &installed)))
        .collect();
    for (p, (_, by)) in installed.iter_mut().zip(covers) {
        p.covered_by = by.and_then(known_by_id).map(|k| k.name.to_string());
    }
    for p in &mut installed {
        if let Some(r) = records.plugins.get(&p.id) {
            if !race_locked(&p.id) {
                if let Some(off) = r.race_off {
                    p.race_off = off;
                }
            }
            p.installed_updated = r.modified.clone();
            let dlo = p.files.iter().find(|f| {
                Path::new(f)
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("dlo"))
            });
            let root = if p.enabled {
                game.join("plugins")
            } else {
                disabled.clone()
            };
            let size = dlo
                .and_then(|f| fs::metadata(root.join(f)).ok())
                .map(|m| m.len());
            if size == r.dlo_size {
                p.version = r.version.clone();
            }
        }
    }
    let ids: HashSet<_> = installed.iter().map(|p| p.id.as_str()).collect();
    let available = KNOWN
        .iter()
        .filter(|k| k.kind == Kind::ThirdParty && !ids.contains(k.id))
        // Nothing to offer that an installed plugin already does.
        .filter(|k| {
            !COVERED_BY.iter().any(|(p, by)| *p == k.id && installed.iter().any(|i| i.id == *by))
        })
        .map(|k| {
            let (s, p) = source_view(k);
            Available {
                id: k.id.into(),
                name: k.name.into(),
                author: k.author.into(),
                source: s,
                page: p,
                slug: k.mxbmods_slug.map(str::to_string),
                note: k.note.map(str::to_string),
            }
        })
        .collect();
    let catalog = KNOWN
        .iter()
        .filter(|k| k.kind == Kind::ThirdParty)
        .filter(|k| {
            ids.contains(k.id)
                || !COVERED_BY.iter().any(|(p, by)| *p == k.id && installed.iter().any(|i| i.id == *by))
        })
        .map(|k| {
            let (s, p) = source_view(k);
            CatalogEntry {
                id: k.id.into(),
                name: k.name.into(),
                author: k.author.into(),
                category: k.category.into(),
                summary: k.summary.into(),
                source: s,
                page: p,
                note: k.note.map(str::to_string),
                installed: ids.contains(k.id),
            }
        })
        .collect();
    let build = game_build(&game);
    let reshade = crate::reshade::status(&cfg.reshade_dir()).installed;
    let conflicts = conflicts(&installed, build.as_deref(), reshade);
    let journal = state_path(app, RACING_JOURNAL)?;
    let name_of = |id: &String| {
        installed
            .iter()
            .find(|p| &p.id == id)
            .map(|p| p.name.clone())
            .or_else(|| known_by_id(id).map(|k| k.name.to_string()))
            .unwrap_or_else(|| id.trim_start_matches("file:").to_string())
    };
    let racing_planned = racing_set(&installed, &records).iter().map(name_of).collect();
    let racing_off = read_journal(&journal).ids.iter().map(name_of).collect();
    let racing_by_join = state_path(app, RACING_BY_JOIN)?.exists();
    Ok(PluginsView {
        installed,
        available,
        catalog,
        conflicts,
        game_build: build,
        game_running: crate::gameproc::is_game_running(),
        race_mode: cfg.race_mode,
        racing_planned,
        racing_off,
        racing_by_join,
        auto_turned_off: auto_off,
    })
}

#[tauri::command]
pub async fn game_plugins_list(app: AppHandle) -> Result<PluginsView, String> {
    tauri::async_runtime::spawn_blocking(move || local_view(&app).map_err(|e| format!("{e:#}")))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn game_plugins_latest(app: AppHandle) -> Result<Vec<Latest>, String> {
    let futures = KNOWN
        .iter()
        .filter(|known| known.kind == Kind::ThirdParty)
        .map(latest_for);
    // Plugins outside the catalog whose manifest names a GitHub repo.
    let repos: Vec<(String, String)> = tauri::async_runtime::spawn_blocking(move || {
        let cfg = crate::config::load(&app).unwrap_or_default();
        let game = PathBuf::from(cfg.install_dir());
        scan(&game, &disabled_dir(&game), Path::new(&cfg.mods_path))
            .into_iter()
            .filter(|p| known_by_id(&p.id).is_none())
            .filter_map(|p| Some((p.id, p.update_github?)))
            .collect()
    })
    .await
    .map_err(|e| e.to_string())?;
    let manifest_futures = repos.into_iter().map(|(id, repo)| latest_from_repo(id, repo));
    let (mut known, extra) = tokio::time::timeout(
        std::time::Duration::from_secs(9),
        futures_util::future::join(
            futures_util::future::join_all(futures),
            futures_util::future::join_all(manifest_futures),
        ),
    )
    .await
    .map_err(|_| "Checking plugin updates timed out.".to_string())?;
    known.extend(extra);
    Ok(known)
}

/// The newest GitHub release of `repo`, for a plugin known only by its manifest. Same cache and
/// back-off as the catalog's checks; the version only, never a download.
async fn latest_from_repo(id: String, repo: String) -> Latest {
    let cache = LATEST_CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let key = format!("repo:{repo}");
    if let Some((_, hit)) = cache
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&key)
        .filter(|(expires, _)| *expires > std::time::Instant::now())
    {
        return Latest { id, ..hit.clone() };
    }
    let mut out = Latest { id: id.clone(), page: Some(format!("https://github.com/{repo}/releases/latest")), ..Default::default() };
    if let Ok(client) = reqwest::Client::builder().user_agent("MXB-App").timeout(std::time::Duration::from_secs(8)).build() {
        if let Ok(r) = client.get(format!("https://api.github.com/repos/{repo}/releases/latest")).send().await {
            if let Ok(v) = r.error_for_status() {
                if let Ok(v) = v.json::<serde_json::Value>().await {
                    out.version = v["tag_name"].as_str().map(|t| t.trim_start_matches(['v', 'V']).to_string());
                    out.updated = v["published_at"].as_str().map(str::to_string);
                }
            }
        }
    }
    let ttl = if out.version.is_some() { 60 * 60 } else { 10 * 60 };
    cache
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(key, (std::time::Instant::now() + std::time::Duration::from_secs(ttl), out.clone()));
    out
}
#[tauri::command]
pub async fn game_plugin_set_enabled(
    app: AppHandle,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = crate::config::load(&app).unwrap_or_default();
        set_enabled(Path::new(&cfg.install_dir()), &id, enabled).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn game_plugin_uninstall(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = crate::config::load(&app).unwrap_or_default();
        uninstall(Path::new(&cfg.install_dir()), &id).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn game_plugins_disable_all(app: AppHandle) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = crate::config::load(&app).unwrap_or_default();
        let j = state_path(&app, RACING_JOURNAL)?;
        let game = PathBuf::from(cfg.install_dir());
        let records = load_records(&app);
        let installed = scan(&game, &disabled_dir(&game), Path::new(""));
        park_for_racing(&game, &j, &racing_set(&installed, &records))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}
#[tauri::command]
pub async fn game_plugins_restore(app: AppHandle) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = crate::config::load(&app).unwrap_or_default();
        let j = state_path(&app, RACING_JOURNAL)?;
        let n = restore_after_racing(Path::new(&cfg.install_dir()), &j)?;
        let _ = fs::remove_file(state_path(&app, RACING_BY_JOIN)?);
        Ok::<usize, anyhow::Error>(n)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}
#[tauri::command]
pub async fn game_plugin_reveal(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = crate::config::load(&app).unwrap_or_default();
        let game = PathBuf::from(cfg.install_dir());
        let active = plugin_paths(&game.join("plugins"), &id).map_err(|e| format!("{e:#}"))?;
        let p = active
            .into_iter()
            .next()
            .or_else(|| {
                plugin_paths(&disabled_dir(&game), &id)
                    .ok()
                    .and_then(|paths| paths.into_iter().next())
            })
            .ok_or_else(|| "Plugin files were not found.".to_string())?;
        crate::library::reveal_in_explorer(&p.to_string_lossy()).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn game_plugin_set_race_off(app: AppHandle, id: String, off: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if race_locked(&id) {
            return Err("Crash fixes always stay on when racing.".into());
        }
        let mut records = load_records(&app);
        // Back to the default is stored as "no choice", so a better default later applies.
        let choice = (off != racing_default(&id)).then_some(off);
        records.plugins.entry(id).or_default().race_off = choice;
        save_records(&app, &records).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn game_plugin_install(app: AppHandle, id: String) -> Result<(), String> {
    let known = known_by_id(&id)
        .copied()
        .ok_or_else(|| "Unknown plugins have no download source.".to_string())?;
    if known.kind == Kind::MxbApp {
        return Err("This plugin is managed elsewhere in MXB App.".into());
    }
    let cfg = crate::config::load(&app).map_err(|e| format!("{e:#}"))?;
    let (url, host, version, modified, source) = match known.source {
        Source::Github {
            repo,
            asset_prefix,
            asset_suffix,
        } => {
            let client = reqwest::Client::builder()
                .user_agent("MXB-App")
                .timeout(std::time::Duration::from_secs(8))
                .build()
                .map_err(|e| e.to_string())?;
            let release: serde_json::Value = client
                .get(format!(
                    "https://api.github.com/repos/{repo}/releases/latest"
                ))
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|e| format!("Could not check the latest release: {e}"))?
                .json()
                .await
                .map_err(|e| format!("Could not read the latest release: {e}"))?;
            let prefix = asset_prefix.to_lowercase();
            let suffix = asset_suffix.to_lowercase();
            let asset = release["assets"]
                .as_array()
                .and_then(|assets| {
                    assets.iter().find(|asset| {
                        let name = asset["name"].as_str().unwrap_or("").to_lowercase();
                        name.starts_with(&prefix) && name.ends_with(&suffix)
                    })
                })
                .and_then(|asset| asset["browser_download_url"].as_str())
                .ok_or_else(|| "The latest release has no matching plugin download.".to_string())?;
            (
                asset.to_string(),
                "github.com".to_string(),
                release["tag_name"]
                    .as_str()
                    .map(|tag| tag.trim_start_matches(['v', 'V']).to_string()),
                release["published_at"].as_str().map(str::to_string),
                "github".to_string(),
            )
        }
        Source::MxbMods => {
            let slug = known
                .mxbmods_slug
                .ok_or_else(|| "Missing mxb-mods slug.".to_string())?;
            let detail = crate::mods::mxb::detail(slug)
                .await
                .map_err(|e| format!("Could not read the mod page: {e:#}"))?;
            let download = detail
                .downloads
                .iter()
                .filter(|d| {
                    !d.url
                        .to_lowercase()
                        .split(['?', '#'])
                        .next()
                        .unwrap_or("")
                        .ends_with(".exe")
                })
                .min_by_key(|d| !d.is_default)
                .ok_or_else(|| {
                    "This plugin has no in-app download. Use its Get it page.".to_string()
                })?;
            // The post's last edit, the same date `latest` compares against: the page's own
            // date is when it was first posted and never moves on an update.
            let modified = latest_for(known_by_id(&id).expect("checked above"))
                .await
                .updated
                .or(Some(detail.date));
            (
                download.url.clone(),
                download.host.clone(),
                None,
                modified,
                "mxbmods".to_string(),
            )
        }
        Source::External { .. } => {
            return Err("This plugin installs from its own website. Use the Get it button.".into())
        }
    };
    // Updating a plugin that is switched off: bring it back first, so the new files land on
    // top of the old ones rather than beside a disabled copy that would then shadow nothing
    // and confuse the next toggle.
    {
        let game = PathBuf::from(cfg.install_dir());
        let id = id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let disabled_only = scan(&game, &disabled_dir(&game), Path::new(""))
                .iter()
                .any(|p| p.id == id && !p.enabled);
            if disabled_only {
                set_enabled(&game, &id, true)
            } else {
                Ok(())
            }
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))?;
    }
    let slug = known.mxbmods_slug.unwrap_or(known.id);
    crate::install::add_to_library(&app, &cfg, slug, &url, &host, SUBPATH, "")
        .await
        .map_err(|e| format!("{e:#}"))?;
    let game = PathBuf::from(cfg.install_dir());
    if let Some(plugin) = scan(&game, &disabled_dir(&game), Path::new(&cfg.mods_path))
        .into_iter()
        .find(|p| p.id == id)
    {
        let root = if plugin.enabled {
            game.join("plugins")
        } else {
            disabled_dir(&game)
        };
        let dlo_size = plugin.files.iter().find_map(|file| {
            Path::new(file)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dlo"))
                .then(|| fs::metadata(root.join(file)).ok().map(|m| m.len()))
                .flatten()
        });
        let mut records = load_records(&app);
        let race_off = records.plugins.get(&id).and_then(|r| r.race_off);
        let covered_off_done = records.plugins.get(&id).is_some_and(|r| r.covered_off_done);
        records.plugins.insert(
            id,
            PluginRecord {
                version,
                source: Some(source),
                modified,
                installed_at: Some(crate::ledger::now_ms().to_string()),
                files: plugin.files,
                dlo_size,
                race_off,
                covered_off_done,
            },
        );
        save_records(&app, &records).map_err(|e| format!("{e:#}"))?;
    }
    Ok(())
}
#[tauri::command]
pub async fn game_plugin_open_config(
    app: AppHandle,
    id: String,
    path: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let v = local_view(&app).map_err(|e| format!("{e:#}"))?;
        let allowed = v.installed.iter().find(|p| p.id == id).is_some_and(|p| {
            p.configs
                .iter()
                .any(|c| Path::new(&c.path) == Path::new(&path))
        });
        if !allowed {
            return Err("That file is not a reported setting for this plugin.".into());
        }
        open_path(Path::new(&path))
    })
    .await
    .map_err(|e| e.to_string())?
}
fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tmp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("gameplugins-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(p.join("plugins")).unwrap();
        p
    }
    fn put(p: &Path, rel: &str) {
        let p = p.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap()
    }
    #[test]
    fn scans_known_unknown_and_never_stock() {
        let g = tmp("scan");
        put(&g, "plugins/proxy64.dlo");
        put(&g, "plugins/mxbmrp3.dlo");
        put(&g, "plugins/mxbmrp3_data/a");
        put(&g, "plugins/odd.dlo");
        let v = scan(&g, &disabled_dir(&g), Path::new(""));
        assert!(v.iter().any(|p| p.id == "mxbmrp3"));
        assert!(v.iter().any(|p| p.id == "file:odd"));
        assert!(!v.iter().any(|p| p.name.contains("proxy")));
    }
    #[test]
    fn shared_overhaul_file_stays_put() {
        let g = tmp("shared");
        for f in [
            "plugins/overhaul64.dlo",
            "plugins/gamefx64.dlo",
            "plugins/overhaul/a.pak",
            "plugins/overhaul/shaders_fx.pak",
        ] {
            put(&g, f)
        }
        let moves =
            collect_moves(&g.join("plugins"), &disabled_dir(&g), "graphics-overhaul").unwrap();
        move_all(&moves).unwrap();
        assert!(g.join("plugins/overhaul/shaders_fx.pak").is_file());
        assert!(disabled_dir(&g).join("overhaul/a.pak").is_file());
    }
    #[test]
    fn destination_collision_moves_nothing() {
        let g = tmp("collision");
        put(&g, "plugins/mxbfix.dlo");
        put(&g, "mxbapp_disabled/plugins/mxbfix.dlo");
        assert!(set_enabled(&g, "mxbfix", false).is_err());
        assert!(g.join("plugins/mxbfix.dlo").is_file());
    }
    #[test]
    fn guard_refuses_bad_shapes() {
        let g = Path::new("C:/MXB");
        for p in [
            g.to_path_buf(),
            g.join("plugins"),
            g.join("mxbapp_disabled/plugins"),
            g.join("plugins/../x"),
            PathBuf::from("C:/other/x"),
        ] {
            assert!(check_uninstall_target(g, &p).is_err(), "{}", p.display())
        }
        assert!(check_uninstall_target(g, &g.join("plugins/user.dlo")).is_ok());
        assert!(check_uninstall_target(g, &g.join("plugins/proxy64.dlo")).is_err());
    }
    #[test]
    fn payload_shapes_and_exe_only() {
        let a = tmp("payload-a");
        put(&a, "plugins/a.dlo");
        assert_eq!(payload_root(&a).unwrap(), a.join("plugins"));
        let b = tmp("payload-b");
        put(&b, "wrapper/plugins/a.dlo");
        assert_eq!(payload_root(&b).unwrap(), b.join("wrapper/plugins"));
        let c = tmp("payload-c");
        put(&c, "a.dlo");
        assert_eq!(payload_root(&c).unwrap(), c);
        let d = tmp("payload-d");
        put(&d, "setup.exe");
        assert!(payload_root(&d)
            .unwrap_err()
            .to_string()
            .contains("setup program"));
    }
    #[test]
    fn versions_compare_by_number() {
        assert!(version_older("1.9", "v1.10.0"));
        assert!(!version_older("1.10.1", "1.10"));
    }
    /// Uninstalling Graphics Overhaul must not take GameFX+'s shader pack out of the shared
    /// `overhaul/` folder with it.
    #[test]
    fn uninstall_items_leave_the_other_plugins_file() {
        let g = tmp("items");
        for f in [
            "plugins/overhaul64.dlo",
            "plugins/gamefx64.dlo",
            "plugins/overhaul/shaders.pak",
            "plugins/overhaul/shaders_fx.pak",
            "plugins/presets/Low-end.ini",
        ] {
            put(&g, f)
        }
        let items = plugin_items(&g.join("plugins"), "graphics-overhaul").unwrap();
        let rels: Vec<String> = items
            .iter()
            .map(|p| relative_slash(&g.join("plugins"), p).to_lowercase())
            .collect();
        assert!(rels.contains(&"overhaul/shaders.pak".to_string()), "{rels:?}");
        assert!(!rels.iter().any(|r| r == "overhaul" || r.ends_with("shaders_fx.pak")), "{rels:?}");
        assert!(rels.contains(&"overhaul64.dlo".to_string()));
        for p in &items {
            check_uninstall_target(&g, p).unwrap();
        }
    }

    /// Turning GO back on after GameFX+ made its own `overhaul/` in the plugins folder merges
    /// into it rather than refusing, and the emptied disabled folder is tidied away.
    #[test]
    fn enabling_merges_into_an_existing_folder() {
        let g = tmp("merge");
        put(&g, "mxbapp_disabled/plugins/overhaul64.dlo");
        put(&g, "mxbapp_disabled/plugins/overhaul/shaders.pak");
        put(&g, "plugins/gamefx64.dlo");
        put(&g, "plugins/overhaul/shaders_fx.pak");
        let moves = collect_moves(&disabled_dir(&g), &g.join("plugins"), "graphics-overhaul").unwrap();
        move_all(&moves).unwrap();
        prune_empty(&disabled_dir(&g), &moves);
        assert!(g.join("plugins/overhaul/shaders.pak").is_file());
        assert!(g.join("plugins/overhaul/shaders_fx.pak").is_file());
        assert!(g.join("plugins/overhaul64.dlo").is_file());
        assert!(!disabled_dir(&g).join("overhaul").exists());
    }

    #[test]
    fn install_skips_docs_and_keeps_player_settings() {
        let src = tmp("inst-src");
        put(&src, "mxbmrp3.dlo");
        put(&src, "LICENSE");
        put(&src, "README.txt");
        put(&src, "THIRD_PARTY_LICENSES.md");
        put(&src, "mxbmrp3_data/a.fnt");
        put(&src, "settings.ini");
        put(&src, "proxy64.dlo");
        put(&src, "mxbmrp3_gpb.dlo");
        put(&src, "mxbmrp3_krp.dlo");
        let dest = tmp("inst-dest");
        fs::write(dest.join("plugins/settings.ini"), b"mine").unwrap();
        let mut written = copy_payload(&payload_root(&src).unwrap(), &dest.join("plugins")).unwrap();
        written.sort();
        assert_eq!(written, vec!["mxbmrp3.dlo", "mxbmrp3_data/a.fnt"]);
        assert_eq!(fs::read(dest.join("plugins/settings.ini")).unwrap(), b"mine");
        assert!(!dest.join("plugins/proxy64.dlo").exists(), "a stock name is never written");
    }

    #[test]
    fn a_bare_beta_word_is_not_a_build() {
        assert_eq!(beta_in(b"Beta tester build ... beta21e\0").as_deref(), Some("beta21e"));
        assert_eq!(beta_in(b"betas beta20 x").as_deref(), Some("beta20"));
        assert_eq!(beta_in(b"no build here"), None);
    }

    /// With FrostMod in, MXBFix is turned off once (moved aside, not deleted) and flagged;
    /// turned back on by the player, it stays on.
    #[test]
    fn mxbfix_goes_off_once_under_frostmod() {
        let g = tmp("covered");
        put(&g, "plugins/mxbfix.dlo");
        put(&g, "plugins/mxbfix.ini");
        if crate::gameproc::is_game_running() {
            return;
        }
        let mut records = Records::default();
        let alone = scan(&g, &disabled_dir(&g), Path::new(""));
        assert!(turn_off_covered(&g, &alone, &mut records).is_empty(), "no FrostMod, nothing to do");
        put(&g, "plugins/frostmod.dlo");
        let both = scan(&g, &disabled_dir(&g), Path::new(""));
        assert!(conflicts(&both, None, false).iter().any(|c| c.ids.contains(&"mxbfix".to_string())));
        assert_eq!(turn_off_covered(&g, &both, &mut records), vec!["MXBFix".to_string()]);
        assert!(disabled_dir(&g).join("mxbfix.dlo").is_file());
        assert!(disabled_dir(&g).join("mxbfix.ini").is_file(), "its settings go with it");
        set_enabled(&g, "mxbfix", true).unwrap();
        let again = scan(&g, &disabled_dir(&g), Path::new(""));
        assert!(turn_off_covered(&g, &again, &mut records).is_empty(), "the player's choice stands");
        assert!(g.join("plugins/mxbfix.dlo").is_file());
    }

    /// An author's manifest names the plugin, adds its files and settings, sets its racing
    /// default and declares a conflict; bad paths in it are ignored, and its files move with it.
    #[test]
    fn a_manifest_describes_an_unknown_plugin() {
        let g = tmp("manifest");
        put(&g, "plugins/myplug.dlo");
        put(&g, "plugins/myplug_fonts/a.fnt");
        put(&g, "plugins/myplug.ini");
        put(&g, "plugins/overhaul64.dlo");
        fs::write(
            g.join("plugins/myplug.mxbplugin.json"),
            r#"{"name":"My Plug","version":"v1.4.0","author":"Me","category":"hud",
               "files":["myplug_fonts/","../escape.txt","proxy64.dlo"],
               "settings":[{"file":"myplug.ini","label":"Settings"},{"file":"../x.ini"}],
               "keys":[{"key":"F7","label":"Menu"}],"racing":"off",
               "conflicts":[{"with":"overhaul64.dlo","reason":"Both redraw the sky."}],
               "updates":{"github":"me/myplug"},"unknownField":1}"#,
        )
        .unwrap();
        let v = scan(&g, &disabled_dir(&g), Path::new(""));
        let p = v.iter().find(|p| p.id == "file:myplug").unwrap();
        assert_eq!(p.name, "My Plug");
        assert_eq!(p.version.as_deref(), Some("1.4.0"));
        assert_eq!(p.category, "hud");
        assert!(p.race_default && p.race_off);
        assert_eq!(p.configs.len(), 1, "the climbing settings path is ignored");
        assert_eq!(p.update_github.as_deref(), Some("me/myplug"));
        assert_eq!(p.source, "external");
        assert!(p.files.iter().any(|f| f == "myplug_fonts/"));
        assert!(!p.files.iter().any(|f| f.contains("escape") || f.contains("proxy")));
        let c = conflicts(&v, None, false);
        assert!(c.iter().any(|c| c.message.contains("Both redraw the sky.")), "{c:?}");
        if crate::gameproc::is_game_running() {
            return;
        }
        set_enabled(&g, "file:myplug", false).unwrap();
        for f in ["myplug.dlo", "myplug.mxbplugin.json", "myplug_fonts/a.fnt", "myplug.ini"] {
            assert!(disabled_dir(&g).join(f).exists(), "{f} moved");
        }
        assert!(g.join("plugins/overhaul64.dlo").is_file(), "the other plugin stays");
    }

    #[test]
    fn damage_conflicts_with_gamefx() {
        let g = tmp("conf");
        put(&g, "plugins/mxb_damage.dlo");
        put(&g, "plugins/gamefx64.dlo");
        put(&g, "plugins/overhaul64.dlo");
        put(&g, "plugins/mxbmrp3.dlo");
        put(&g, "plugins/Holeshot-HUD.dlo");
        let v = scan(&g, &disabled_dir(&g), Path::new(""));
        let c = conflicts(&v, Some("beta21e"), true);
        assert!(c.iter().any(|c| c.level == "error" && c.ids.contains(&"mxb-damage".to_string())));
        assert!(c.iter().any(|c| c.level == "warning" && c.ids == vec!["graphics-overhaul".to_string()]));
        assert!(c.iter().any(|c| c.level == "info" && c.ids.len() == 2));
    }

    #[test]
    fn racing_journal_round_trip() {
        let g = tmp("racing");
        put(&g, "plugins/mxbfix.dlo");
        put(&g, "plugins/overhaul64.dlo");
        put(&g, "plugins/mxbmrp3.dlo");
        put(&g, "plugins/mxb_damage.dlo");
        put(&g, "plugins/frostmod.dlo");
        let journal = g.join("j.json");
        if crate::gameproc::is_game_running() {
            return; // a real game holds plugins open; every move is refused then
        }
        // The player keeps the damage plugin on and wants MXBMRP3 off.
        let mut records = Records::default();
        records.plugins.entry("mxb-damage".into()).or_default().race_off = Some(false);
        records.plugins.entry("mxbmrp3".into()).or_default().race_off = Some(true);
        // Even an explicit choice can't put a crash fix on the list.
        records.plugins.entry("mxbfix".into()).or_default().race_off = Some(true);
        let installed = scan(&g, &disabled_dir(&g), Path::new(""));
        let mut set = racing_set(&installed, &records);
        set.sort();
        assert_eq!(set, vec!["graphics-overhaul", "mxbmrp3"]);
        assert_eq!(park_for_racing(&g, &journal, &set).unwrap(), 2);
        assert!(g.join("plugins/mxbfix.dlo").is_file(), "crash fixes stay on");
        assert!(g.join("plugins/frostmod.dlo").is_file(), "ours is never parked");
        assert!(g.join("plugins/mxb_damage.dlo").is_file(), "the player's choice wins");
        assert!(disabled_dir(&g).join("overhaul64.dlo").is_file());
        assert_eq!(restore_after_racing(&g, &journal).unwrap(), 2);
        assert!(g.join("plugins/overhaul64.dlo").is_file());
        assert!(g.join("plugins/mxbmrp3.dlo").is_file());
        assert!(!journal.exists());
    }

    #[test]
    fn reads_build_from_exe() {
        let g = tmp("build");
        fs::write(g.join("mxbikes.exe"), b"junk beta21e more").unwrap();
        assert_eq!(game_build(&g).as_deref(), Some("beta21e"));
    }
}

/// A plugin's page: what mxb-mods says about it, through the control plane.
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDetail {
    pub id: String,
    pub description_html: Option<String>,
    pub images: Vec<String>,
    pub page: Option<String>,
    pub rating: Option<f64>,
    pub ratings: Option<u64>,
    pub views: Option<u64>,
    pub updated: Option<String>,
}

#[tauri::command]
pub async fn game_plugin_detail(id: String) -> Result<PluginDetail, String> {
    let known = known_by_id(&id).ok_or_else(|| "No page for an unknown plugin.".to_string())?;
    let mut out = PluginDetail { id: id.clone(), ..Default::default() };
    let Some(slug) = known.mxbmods_slug else { return Ok(out) };
    // Same control-plane client as Browse: its own back-off, its own cache.
    if let Ok(Some(v)) = crate::mods::mxbmods_api::api().by_slug(slug).await {
        // Remote HTML: through the same sanitiser as the shop's descriptions before the page
        // renders it as markup.
        // The post's pictures go to the gallery and come out of the text, the way a mod's
        // page treats them; the text then goes through the shop's sanitiser before it is
        // rendered as markup.
        let content = v["content_html"].as_str().unwrap_or("");
        out.images = v["images"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        out.images.extend(v["image"].as_str().map(str::to_string));
        out.images.extend(crate::mods::mxb::extract_images(content));
        let mut seen = HashSet::new();
        out.images.retain(|u| seen.insert(u.clone()));
        out.images.truncate(24);
        out.description_html =
            crate::mods::shop_catalog::sanitize_html(&crate::mods::mxb::strip_images(content));
        out.page = v["link"].as_str().map(str::to_string);
        out.rating = v["rating"]["average"].as_f64();
        out.ratings = v["rating"]["count"].as_u64();
        out.views = v["views"].as_u64();
        out.updated = v["modified"].as_str().map(str::to_string);
    }
    Ok(out)
}

/// The settings files the app will read or write for `id`: only those the scan reported.
fn allowed_config(app: &AppHandle, id: &str, path: &str) -> Result<PathBuf, String> {
    let v = local_view(app).map_err(|e| format!("{e:#}"))?;
    let ok = v
        .installed
        .iter()
        .find(|p| p.id == id)
        .is_some_and(|p| p.configs.iter().any(|c| Path::new(&c.path) == Path::new(path)));
    let p = PathBuf::from(path);
    if !ok || !p.is_file() {
        return Err("That file is not a settings file of this plugin.".into());
    }
    Ok(p)
}

/// The most of a settings file the editor will load. Plugin `.ini` files are a few KB.
const MAX_CONFIG: u64 = 512 * 1024;

#[tauri::command]
pub async fn game_plugin_read_config(app: AppHandle, id: String, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = allowed_config(&app, &id, &path)?;
        if fs::metadata(&p).map(|m| m.len()).unwrap_or(0) > MAX_CONFIG {
            return Err("This file is too large to edit here. Open it instead.".into());
        }
        let bytes = fs::read(&p).map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Save the player's edit of a settings file. The previous text is kept beside it as
/// `<name>.bak` (one copy, replaced each save), and the write goes through a temp file so a
/// failure leaves the old settings whole. Refused while the game runs: plugins write their
/// own settings back when the game closes, and would undo the edit.
#[tauri::command]
pub async fn game_plugin_write_config(
    app: AppHandle,
    id: String,
    path: String,
    text: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if crate::gameproc::is_game_running() {
            return Err("Close MX Bikes first: plugins save their own settings when the game closes.".into());
        }
        if text.len() as u64 > MAX_CONFIG {
            return Err("That's too much text for a settings file.".into());
        }
        let p = allowed_config(&app, &id, &path)?;
        let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let _ = fs::copy(&p, p.with_file_name(format!("{name}.bak")));
        let tmp = p.with_file_name(format!("{name}.mxbapp-tmp"));
        fs::write(&tmp, text.as_bytes()).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &p).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            e.to_string()
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
