//! Universal drop handling: work out what the user dropped, show it to them, then install.
//!
//! Everything here is read-only until [`commit`] runs. A drop is staged to a temp directory,
//! classified into [`DropItem`]s, and handed to the UI; nothing under `mods/` is touched until
//! the user confirms. The routing decision itself is **not** made here — it comes from
//! [`crate::install::plan_placement`], the same function [`crate::install::place_mod`] uses, so
//! the destination shown in the review sheet is by construction the destination written to disk.
//!
//! What this module adds on top of the placer is the *category*: `place_mod` has always been
//! told whether it is installing a track or a bike. A dropped file has nobody to tell it, so
//! [`classify_typed`] works it out from the content — and where the content genuinely cannot
//! say (which bike does this loose paint belong to?), the item is marked as needing a choice
//! rather than guessed at.

use crate::install::{self, RouteRule};
use crate::library;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The category passed to `plan_placement` while probing. Any value works for the
/// self-describing rules (they ignore it); `misc` is chosen because it is the one category
/// that never triggers a special case — `tracks` would set `wrap_loose`, `rider` would
/// suppress the paints-bundle rule.
const PROBE_TYPE: &str = "misc";

/// How deep to look for installable units inside a dropped archive. One level past the root
/// covers the realistic "pack of three liveries" shape without turning an unpacked track's
/// asset tree into fifty rows.
const MAX_SPLIT_DEPTH: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentKind {
    /// A whole `mods/` tree, or a set of top-level category folders.
    ModsTree,
    Track,
    Bike,
    BikePaint,
    SoundSet,
    RiderGear,
    /// Recognised as game content but we can't say what kind — the user must place it.
    Unknown,
}

impl ContentKind {
    /// The `mods/<x>` subpath this kind installs into, or `None` when only the user knows.
    fn subpath(self) -> Option<&'static str> {
        match self {
            ContentKind::ModsTree => Some("mods"),
            ContentKind::Track => Some("mods/tracks"),
            ContentKind::Bike | ContentKind::BikePaint | ContentKind::SoundSet => {
                Some("mods/bikes")
            }
            ContentKind::RiderGear => Some("mods/rider"),
            ContentKind::Unknown => None,
        }
    }
}

/// Why the classifier decided what it did, as a stable key the UI translates.
///
/// This is deliberately an enum rather than a prose string: the reason shown to the user has
/// to survive translation into six languages, and a free-text "found engine.scl + sfx.cfg"
/// would not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DetectReason {
    /// The archive carries a `mods/` folder.
    ModsTree,
    /// The archive root holds `bikes/`, `tracks/`, …
    CategoryDirs,
    /// A `<Bike>/paints/` bundle.
    PaintsBundle,
    /// `engine.scl` + `sfx.cfg`.
    SoundMarkers,
    /// Track data files (`.map`, `.trh`, …).
    TrackMarkers,
    /// A `.pkz` whose `[info]` block describes a track.
    TrackPackage,
    /// A bike `.ini` + `.cfg` naming the bike.
    BikeConfig,
    /// `PNT` paint files with no folder saying which model they belong to.
    LoosePaint,
    /// Rider gear folders (`helmets/`, `boots/`, …).
    GearFolders,
    /// The paint's textures name the rider body — an outfit, not a bike livery.
    RiderTexture,
    /// The paint's textures name a piece of gear (helmet, boots, goggles…).
    GearTexture,
    /// Nothing identified it.
    Unrecognised,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestChoice {
    /// Written straight into `destFolder`, e.g. `MX1OEM_2023_KTM_450/paints`.
    pub value: String,
    /// A real folder or bike name — shown verbatim, never translated.
    pub label: String,
    /// The category this destination lives under. Carried with the choice so the frontend
    /// never has to infer it from the shape of the path.
    pub subpath: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DropItem {
    pub id: String,
    /// Display name — the folder or file the unit came from.
    pub name: String,
    pub kind: ContentKind,
    pub reason: DetectReason,
    /// `mods/<x>`. Empty when the kind is `Unknown` and the user has not chosen yet.
    pub subpath: String,
    /// Sub-folder under `subpath` — the default, organisational folder included.
    pub dest_folder: String,
    /// The part of `dest_folder` that is structural rather than a user preference. A bike
    /// must stay in `mods/bikes/<Bike>/` wherever it is filed, so choosing the folder "MX2"
    /// has to mean `MX2/<Bike>` rather than replacing the bike's own folder.
    pub keep_folder: String,
    /// The item cannot be installed until the user picks a destination. Set when the
    /// content genuinely does not say where it belongs.
    pub needs_choice: bool,
    /// Offered destinations when `needs_choice` — installed bikes, gear models, profiles.
    pub choices: Vec<DestChoice>,
    /// Existing files this item would replace, relative to the mods folder.
    pub collisions: Vec<String>,
    pub file_count: usize,
    pub bytes: u64,
    /// Extra detail worth showing: the bike's real name and class, a track's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedFile {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DropPlan {
    pub id: String,
    pub items: Vec<DropItem>,
    pub skipped: Vec<SkippedFile>,
    pub total_bytes: u64,
}

/// One row as the user left it after reviewing. Rows they unchecked simply aren't sent.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitItem {
    pub id: String,
    pub subpath: String,
    pub dest_folder: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledItem {
    pub id: String,
    pub name: String,
    pub files: usize,
    /// Where it landed, relative to the mods folder — for the receipt.
    pub dest: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedItem {
    pub id: String,
    pub name: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitOutcome {
    pub installed: Vec<InstalledItem>,
    pub failed: Vec<FailedItem>,
}

/// A staged unit, held between `plan` and `commit`.
struct StagedItem {
    /// Where the unit's content lives right now — inside the staging dir for an archive or a
    /// loose file, or the user's own folder when they dropped one (we never copy those twice).
    path: PathBuf,
    name: String,
    /// Slug handed to `place_mod`, which uses it to name a wrapped track folder.
    slug: String,
}

struct StagedPlan {
    /// Deleted on commit or cancel. `None` when every source was a folder we referenced
    /// in place and there is nothing of ours to clean up.
    work: Option<PathBuf>,
    items: HashMap<String, StagedItem>,
}

static PLANS: Mutex<Option<HashMap<String, StagedPlan>>> = Mutex::new(None);

fn with_plans<T>(f: impl FnOnce(&mut HashMap<String, StagedPlan>) -> T) -> T {
    let mut guard = PLANS.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

fn next_id(prefix: &str) -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{stamp:x}-{n}")
}

// ---------------------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------------------

/// What the classifier concluded about one piece of content.
#[derive(Debug, Clone)]
struct Verdict {
    kind: ContentKind,
    reason: DetectReason,
    detail: Option<String>,
    /// Where it should go by default, under the kind's category. A suggestion the user can
    /// override — empty means "the category root".
    dest_folder: String,
    /// The part of `dest_folder` that is structural rather than a preference.
    ///
    /// A bike folder is the case that needs this: `place_mod` copies a folder's *contents*
    /// into the category, which is right for a `.pkz` but would scatter `RM250.ini`/`.cfg`
    /// loose into `mods/bikes`. Naming the folder keeps the bike in `mods/bikes/RM250/`, and
    /// keeping it separate means filing that bike under "MX2" yields `MX2/RM250` rather than
    /// dropping the bike's own folder on the floor.
    keep_folder: String,
    /// The content named its category but not its destination — the user must finish it.
    needs_choice: bool,
}

impl Verdict {
    fn new(kind: ContentKind, reason: DetectReason) -> Self {
        Verdict {
            kind,
            reason,
            detail: None,
            dest_folder: String::new(),
            keep_folder: String::new(),
            needs_choice: false,
        }
    }
    fn detail(mut self, d: Option<String>) -> Self {
        self.detail = d;
        self
    }
    fn dest(mut self, d: impl Into<String>) -> Self {
        self.dest_folder = d.into();
        self
    }
    fn keep(mut self, d: impl Into<String>) -> Self {
        self.keep_folder = d.into();
        self.dest_folder = self.keep_folder.clone();
        self
    }
    fn ask(mut self) -> Self {
        self.needs_choice = true;
        self
    }
}

/// A unit found inside a staged root, before destinations are resolved.
struct Unit {
    path: PathBuf,
    verdict: Verdict,
}

/// Does this directory describe itself well enough to route without asking?
///
/// Mirrors `plan_placement`'s self-describing rules so the two agree on what counts as
/// "obvious"; anything it declines falls through to [`classify_typed`].
fn from_route_rule(rule: RouteRule) -> Option<Verdict> {
    match rule {
        RouteRule::ModsTree => Some(Verdict::new(ContentKind::ModsTree, DetectReason::ModsTree)),
        RouteRule::CategoryDirs => Some(Verdict::new(ContentKind::ModsTree, DetectReason::CategoryDirs)),
        RouteRule::PaintsBundle => Some(Verdict::new(ContentKind::BikePaint, DetectReason::PaintsBundle)),
        RouteRule::SoundBundle | RouteRule::LooseSound => {
            Some(Verdict::new(ContentKind::SoundSet, DetectReason::SoundMarkers))
        }
        RouteRule::Typed => None,
    }
}

/// What classifying and placing a drop are relative to: the mods folder to look in, and the
/// title whose folder layout applies.
///
/// The title is carried rather than read from [`crate::game::active`] at each use so that the
/// whole module is a function of its inputs — one drop is classified against one title, and a
/// test can put either one in without touching process-wide state.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    mods_path: &'a str,
    game: &'static crate::game::GameProfile,
}

impl<'a> Ctx<'a> {
    fn new(mods_path: &'a str) -> Self {
        Ctx {
            mods_path,
            game: crate::game::active(),
        }
    }

    /// Does `dir` hold the rider folders this title reads — `helmets/`, `riders/`, and
    /// whatever else it keeps gear in?
    ///
    /// Per-title rather than a fixed list because the two barely overlap: MX Bikes has
    /// `boots` and `protections`, GP Bikes bakes both into the rider model and has
    /// `animations` instead.
    fn has_gear_dirs(&self, dir: &Path) -> bool {
        std::iter::once(crate::game::RIDERS_DIR)
            .chain(self.game.rider.areas.iter().map(|a| a.folder))
            .any(|g| install::child_dir(dir, g).is_some())
    }

    /// Whether `area` is a folder this title actually keeps content in. `goggles` and
    /// `gloves` are folders *inside* another one rather than areas of their own, so they're
    /// answered by the layout flags instead.
    fn has_area(&self, area: &str) -> bool {
        let rider = &self.game.rider;
        match area {
            "gloves" => rider.gloves || rider.profile_extras.iter().any(|(f, _)| *f == area),
            "goggles" => rider.areas.iter().any(|a| a.goggles),
            _ => self.game.installable_areas().any(|a| a.folder == area),
        }
    }
}

// Link-following: what's being classified here is a folder the player dragged in from
// their own disk, and a junction in it is theirs — see `crate::linkwalk`.
fn files_in(dir: &Path) -> Vec<PathBuf> {
    crate::linkwalk::files(dir)
}

fn dirs_in(dir: &Path) -> Vec<PathBuf> {
    crate::linkwalk::subdirs(dir)
}

/// Identify a directory that `plan_placement` had no opinion about.
///
/// Returns `None` when the directory looks like a container of other things rather than a
/// unit in its own right, which is the signal to look one level deeper.
fn classify_typed(dir: &Path, ctx: Ctx) -> Option<Verdict> {
    // An extracted track: `.map`/`.trh`/… sitting next to each other.
    if library::dir_has_track_markers(dir) {
        return Some(Verdict::new(ContentKind::Track, DetectReason::TrackMarkers));
    }

    // A bike folder names itself in `<stem>.ini` + `<stem>.cfg`.
    if let Some(id) = crate::bikeswap::read_identity(dir) {
        let detail = (!id.class.is_empty()).then(|| format!("{} · {}", id.name, id.class));
        let folder = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Some(
            Verdict::new(ContentKind::Bike, DetectReason::BikeConfig)
                .detail(detail.or(Some(id.name)))
                .keep(folder),
        );
    }

    // Rider gear ships as `helmets/`, `riders/`, … — one level below the `rider` category
    // folder, so `plan_placement`'s CATEGORY_DIRS rule never sees it.
    if ctx.has_gear_dirs(dir) {
        return Some(Verdict::new(ContentKind::RiderGear, DetectReason::GearFolders));
    }

    let files = files_in(dir);
    let subdirs = dirs_in(dir);

    // A `.pkz` is a complete package — ask it what it is, from its file list alone.
    let pkzs: Vec<&PathBuf> = files.iter().filter(|p| install::has_ext(p, "pkz")).collect();
    if let Some(pkz) = pkzs.first() {
        return Some(classify_pkz(pkz));
    }

    // Loose paints. The header carries the paint's own display name rather than the model
    // it targets, but the *textures* are named after the mesh they cover — a rider outfit
    // paints `rider`, a bike paints `framecompletemap`/`wheels` — so that is what says
    // where it belongs.
    if subdirs.is_empty() {
        if let Some(pnt) = files.iter().find(|p| install::has_ext(p, "pnt")) {
            return Some(classify_paint(pnt, ctx));
        }
    }

    None
}

/// Track markers, as `library` knows them — the files that make a folder a track.
const TRACK_EXTS: [&str; 5] = ["map", "trh", "tsc", "rdf", "ssc"];

/// Identify a `.pkz` from its entry names.
///
/// Deliberately does **not** call `pkz::read_meta`: that parses the `[info]` block *and*
/// decodes and rescales the preview image, which on a large track is seconds of work for a
/// question the file list already answers.
///
/// Order matters. `bikeswap::read_identity` returns `Some` when *either* `<stem>.ini` or
/// `<stem>.cfg` is present — and a track `.pkz` carries an `.ini` too, which is exactly how a
/// track came back labelled "Bike". The bike verdict therefore requires the `.cfg`, and the
/// track markers are checked first regardless.
fn classify_pkz(pkz: &Path) -> Verdict {
    let stem = pkz
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let names = crate::pkz::entry_names(pkz).unwrap_or_default();
    let base_of = |n: &str| n.rsplit('/').next().unwrap_or(n).to_ascii_lowercase();
    let ext_of = |n: &str| {
        base_of(n)
            .rsplit_once('.')
            .map(|(_, e)| e.to_string())
            .unwrap_or_default()
    };

    if names.iter().any(|n| TRACK_EXTS.contains(&ext_of(n).as_str())) {
        return Verdict::new(ContentKind::Track, DetectReason::TrackPackage);
    }

    let cfg = format!("{}.cfg", stem.to_ascii_lowercase());
    if names.iter().any(|n| base_of(n) == cfg) {
        // Now that it really is a bike, the small `.ini` is worth reading for its real name.
        let detail = crate::bikeswap::read_identity(pkz).map(|id| {
            if id.class.is_empty() {
                id.name
            } else {
                format!("{} · {}", id.name, id.class)
            }
        });
        return Verdict::new(ContentKind::Bike, DetectReason::BikeConfig).detail(detail);
    }

    // A locked `.pkz` on a build without the reader lists nothing; say so rather than guess.
    Verdict::new(ContentKind::Unknown, DetectReason::Unrecognised).ask()
}

/// Texture names that mean "this paints the rider's body", so it belongs on a rider model
/// rather than on a piece of gear.
///
/// `rider` is MX Bikes' name for the sheet. GP Bikes' rider models don't use it: Manu's calls
/// the suit `Suit.tga`, the casual model calls the whole thing `Outfit.tga`, and both carry
/// an `Arms.tga`. Checked before the gear hints below, which matters most on GP — its suits
/// carry the boots' textures too (they're part of the model), so a suit tested for `boot`
/// first would be filed as boots, in a folder GP Bikes has no loader for.
const RIDER_BODY_TEXTURES: [&str; 4] = ["rider", "suit", "outfit", "arms"];

/// Gear areas a paint's texture names can name, as `(texture hint, area folder)`. Hints for
/// areas the active title lacks are skipped — see [`classify_paint`].
const GEAR_TEXTURE_HINTS: [(&str, &str); 6] = [
    ("goggle", "goggles"),
    ("lens", "goggles"),
    ("helmet", "helmets"),
    ("boot", "boots"),
    ("glove", "gloves"),
    ("chest", crate::game::PROTECTION_AREAS[0]),
];

/// Identify a loose `.pnt` from the textures inside it.
///
/// Verified against real paints: a rider outfit carries a texture called `rider`, while a
/// bike paint carries bike parts (`framecompletemap`, `wheels`). `_n`/`_r` are the normal and
/// roughness maps of the same texture, so only the base names matter.
///
/// An outfit gets a real default rather than an empty picker: MX Bikes always ships
/// `default_mx`, so `riders/default_mx/paints` is somewhere it will actually load from, and
/// it is the same guess the install dialog has always made. Gear defaults to the only model
/// of its kind when there is exactly one — with two, nothing in the file picks between them.
///
/// Falls back to a bike paint, the commonest case by far.
fn classify_paint(pnt: &Path, ctx: Ctx) -> Verdict {
    let bike = || Verdict::new(ContentKind::BikePaint, DetectReason::LoosePaint).ask();
    let Ok(bytes) = std::fs::read(pnt) else {
        return bike();
    };
    let Ok(textures) = crate::paint::decode_any(&bytes) else {
        return bike();
    };

    let names: Vec<String> = textures
        .iter()
        .map(|t| {
            let n = t.name.to_ascii_lowercase();
            n.strip_suffix("_n")
                .or_else(|| n.strip_suffix("_r"))
                .unwrap_or(&n)
                .to_string()
        })
        .collect();

    if names.iter().any(|n| RIDER_BODY_TEXTURES.contains(&n.as_str())) {
        let v = Verdict::new(ContentKind::RiderGear, DetectReason::RiderTexture);
        // The model the game ships if it ships one, else the only one installed. GP Bikes
        // ships none, so there its suits genuinely have to be asked about until a rider
        // model exists to wear them.
        return match (
            ctx.game.rider.stock_profiles.first(),
            library::scan_rider_targets(ctx.mods_path).profiles.as_slice(),
        ) {
            (Some(stock), _) => v.dest(profile_dest(stock, "paints")),
            (None, [only]) => v.dest(profile_dest(only, "paints")),
            _ => v.ask(),
        };
    }

    for (hint, area) in GEAR_TEXTURE_HINTS {
        if !names.iter().any(|n| n.contains(hint)) {
            continue;
        }
        // A hint for a folder this title doesn't have is not a match — it's a texture the
        // rider model carries because the part is built into it. Skipping keeps looking
        // rather than filing the paint somewhere the game will never read.
        if !ctx.has_area(area) {
            continue;
        }
        let v = Verdict::new(ContentKind::RiderGear, DetectReason::GearTexture);
        // Gloves hang off a rider profile; the rest hang off a gear model.
        if area == "gloves" {
            return match ctx.game.rider.stock_profiles.first() {
                Some(stock) => v.dest(profile_dest(stock, "gloves")),
                None => v.ask(),
            };
        }
        let t = library::scan_rider_targets(ctx.mods_path);
        let models = match area {
            "helmets" | "goggles" => &t.helmets,
            "boots" => &t.boots,
            _ => &t.protection,
        };
        return match models.as_slice() {
            [only] if area == "goggles" => v.dest(format!("helmets/{only}/goggles")),
            [only] => v.dest(format!("{area}/{only}/paints")),
            // Nothing installed, or more than one candidate — the file can't choose.
            _ => v.ask(),
        };
    }

    bike()
}

/// `riders/<model>/<sub>` — where everything worn on the rider itself lives.
fn profile_dest(profile: &str, sub: &str) -> String {
    format!("{}/{profile}/{sub}", crate::game::RIDERS_DIR)
}

/// Split a staged root into the units the user will see as rows.
fn units_in(
    dir: &Path,
    mods_dir: &Path,
    ctx: Ctx,
    slug: &str,
    depth: usize,
) -> Vec<Unit> {
    let route = install::plan_placement(dir, mods_dir, PROBE_TYPE, "", slug);
    if let Some(verdict) = from_route_rule(route.rule) {
        return vec![Unit {
            path: dir.to_path_buf(),
            verdict,
        }];
    }

    // `plan_placement` unwraps single-child wrapper folders; classify what it settled on,
    // not the wrapper, or a `Mod v2/` folder would hide the bike inside it.
    let base = install::unwrap_wrapper(dir);

    if let Some(verdict) = classify_typed(&base, ctx) {
        return vec![Unit {
            path: base,
            verdict,
        }];
    }

    if depth < MAX_SPLIT_DEPTH {
        let children = dirs_in(&base);
        if !children.is_empty() {
            let found: Vec<Unit> = children
                .iter()
                .flat_map(|c| {
                    let child_slug = c
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| slug.to_string());
                    units_in(c, mods_dir, ctx, &child_slug, depth + 1)
                })
                .collect();
            // Only accept the split if it actually identified something; otherwise a folder
            // of loose assets would explode into a list of `Unknown` rows.
            if found.iter().any(|u| u.verdict.kind != ContentKind::Unknown) {
                return found;
            }
        }
    }

    vec![Unit {
        path: base,
        verdict: Verdict::new(ContentKind::Unknown, DetectReason::Unrecognised).ask(),
    }]
}

// ---------------------------------------------------------------------------------------
// Destinations
// ---------------------------------------------------------------------------------------

fn bike_choices(mods_path: &str, paints: bool) -> Vec<DestChoice> {
    crate::bikeswap::scan_installed_bikes(mods_path)
        .into_iter()
        .map(|b| {
            let folder = Path::new(&b.path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| b.id.clone());
            DestChoice {
                value: if paints {
                    format!("{folder}/paints")
                } else {
                    folder.clone()
                },
                label: b.name,
                subpath: "mods/bikes".into(),
            }
        })
        .collect()
}

/// Gear destinations: every installed model, plus the fixed places new gear and outfits go.
///
/// Built from the active title's own layout, because the two games' rider folders barely
/// overlap — offering GP Bikes a `boots` folder would file a mod where its loader never
/// looks, and offering it `riders/default_mx` would invent a rider model that doesn't exist.
///
/// This must never come back empty. An earlier version listed only what was installed, so on
/// a fresh mods folder the picker opened onto "Nothing installed to put this on yet" and the
/// row could not be corrected at all — a dead end for the one case that most needs a choice.
fn gear_choices(ctx: Ctx) -> Vec<DestChoice> {
    let game = ctx.game;
    let t = library::scan_rider_targets(ctx.mods_path);
    let mut out = Vec::new();
    let mut choice = |value: String, label: String| {
        out.push(DestChoice {
            value,
            label,
            subpath: "mods/rider".into(),
        })
    };

    for area in game.installable_areas() {
        let folder = area.folder;
        let models = match folder {
            "helmets" => &t.helmets,
            "boots" => &t.boots,
            "animations" => &t.animations,
            _ => &t.protection,
        };
        for m in models {
            if area.paint_cat.is_some() {
                choice(format!("{folder}/{m}/paints"), format!("{m} — {folder} paints"));
            }
            if area.goggles {
                choice(format!("{folder}/{m}/goggles"), format!("{m} — goggles"));
            }
        }
    }

    let mut profiles: Vec<String> = t.profiles.clone();
    for p in game.rider.stock_profiles {
        if !profiles.iter().any(|x| x.eq_ignore_ascii_case(p)) {
            profiles.push(p.to_string());
        }
    }
    // MX Bikes wears a kit over its rider; GP Bikes' rider *is* the suit.
    let worn = if game.rider.stock_profiles.is_empty() { "suit paints" } else { "outfit" };
    for p in &profiles {
        choice(profile_dest(p, "paints"), format!("{p} — {worn}"));
        for (extra, _) in game.rider.profile_extras {
            choice(profile_dest(p, extra), format!("{p} — {extra}"));
        }
    }

    // Somewhere to put a brand new model, which by definition isn't installed yet.
    choice(
        crate::game::RIDERS_DIR.to_string(),
        format!("{} — new rider model", crate::game::RIDERS_DIR),
    );
    for area in game.installable_areas() {
        choice(area.folder.to_string(), format!("{} — new model", area.folder));
    }
    out
}

/// The folders a category's content already lives in, plus the category root.
///
/// Identifying content correctly is not the same as knowing where the user wants it filed,
/// so every row gets this list — the classifier's destination is a default to override, not
/// a decision. Mirrors the install dialog: the distinct folders installed mods sit in.
fn folder_choices(mods_path: &str, subpath: &str) -> Vec<DestChoice> {
    let leaf = subpath.rsplit('/').next().unwrap_or(subpath).to_string();
    let mut out = vec![DestChoice {
        value: String::new(),
        label: format!("{leaf} — folder root"),
        subpath: subpath.to_string(),
    }];
    if let Ok(mods) = library::scan_mods(mods_path, subpath) {
        let mut seen: Vec<String> = Vec::new();
        for m in mods {
            if m.folder.is_empty() || seen.iter().any(|f| f.eq_ignore_ascii_case(&m.folder)) {
                continue;
            }
            seen.push(m.folder.clone());
            out.push(DestChoice {
                value: m.folder.clone(),
                label: m.folder,
                subpath: subpath.to_string(),
            });
        }
    }
    out
}

/// Everywhere a dropped item could plausibly go, for the rows we can't place ourselves.
fn all_choices(ctx: Ctx, paints: bool) -> Vec<DestChoice> {
    let mut c = bike_choices(ctx.mods_path, paints);
    c.extend(gear_choices(ctx));
    // Unidentified content still has to be placeable — offer the bare category roots so a
    // dropped file the classifier couldn't name can at least be filed by hand.
    for (label, subpath) in [
        ("tracks", "mods/tracks"),
        ("bikes", "mods/bikes"),
        ("rider", "mods/rider"),
        ("misc", "mods/misc"),
    ] {
        c.push(DestChoice {
            value: String::new(),
            label: format!("{label} — folder root"),
            subpath: subpath.into(),
        });
    }
    c
}

/// Resolve a unit into the row the user reviews.
fn to_item(
    unit: Unit,
    ctx: Ctx,
    mods_dir: &Path,
    slug: &str,
    source_name: &str,
) -> (DropItem, StagedItem) {
    let Unit { path, verdict } = unit;

    // The staged root of an archive or a loose file is a numbered scratch directory, whose
    // name ("0") means nothing to anyone. Fall back to what the user actually dropped.
    let own = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = if own.is_empty() || own.chars().all(|c| c.is_ascii_digit()) {
        source_name.to_string()
    } else {
        own
    };

    let needs_choice = verdict.needs_choice;

    // Every row gets a picker: identifying content correctly is not the same as knowing
    // which folder the user files it under.
    let choices = if matches!(verdict.kind, ContentKind::ModsTree) {
        // A mods tree lands by its own internal layout; there is no folder to choose.
        Vec::new()
    } else if matches!(
        verdict.reason,
        DetectReason::RiderTexture | DetectReason::GearTexture
    ) {
        // Textures already said it's worn, not ridden — lead with gear and profiles.
        let mut c = gear_choices(ctx);
        c.extend(bike_choices(ctx.mods_path, true));
        c
    } else if needs_choice {
        all_choices(ctx, matches!(verdict.kind, ContentKind::BikePaint))
    } else if matches!(
        verdict.kind,
        ContentKind::BikePaint | ContentKind::SoundSet | ContentKind::RiderGear
    ) {
        let mut c = bike_choices(ctx.mods_path, matches!(verdict.kind, ContentKind::BikePaint));
        c.extend(gear_choices(ctx));
        c
    } else {
        folder_choices(ctx.mods_path, verdict.kind.subpath().unwrap_or("mods/misc"))
    };

    let subpath = if needs_choice {
        String::new()
    } else {
        verdict.kind.subpath().unwrap_or("mods/misc").to_string()
    };
    let dest_folder = if needs_choice {
        String::new()
    } else {
        verdict.dest_folder.clone()
    };

    let staged = StagedItem {
        path: path.clone(),
        name: name.clone(),
        slug: slug.to_string(),
    };

    let mut item = DropItem {
        id: next_id("item"),
        name,
        kind: verdict.kind,
        reason: verdict.reason,
        subpath: subpath.clone(),
        dest_folder: dest_folder.clone(),
        keep_folder: verdict.keep_folder.clone(),
        needs_choice,
        choices,
        collisions: Vec::new(),
        file_count: 0,
        bytes: 0,
        detail: verdict.detail,
    };

    // A row with no destination yet has nothing to preview; the frontend re-previews once
    // the user picks one.
    if !subpath.is_empty() {
        let (files, bytes, collisions) =
            preview(&staged.path, mods_dir, &subpath, &dest_folder, &staged.slug);
        item.file_count = files;
        item.bytes = bytes;
        item.collisions = collisions;
    }

    (item, staged)
}

/// What placing `src` would write: file count, total bytes, and the existing files it replaces.
fn preview(
    src: &Path,
    mods_dir: &Path,
    subpath: &str,
    dest_folder: &str,
    slug: &str,
) -> (usize, u64, Vec<String>) {
    let type_folder = type_folder_of(subpath);
    let route = install::plan_placement(src, mods_dir, type_folder, dest_folder, slug);
    let writes = install::writes_for(&route.placement);
    let bytes = writes
        .iter()
        .filter_map(|(s, _)| std::fs::metadata(s).ok().map(|m| m.len()))
        .sum();
    let collisions = writes
        .iter()
        .filter(|(_, dst)| dst.exists())
        .map(|(_, dst)| {
            dst.strip_prefix(mods_dir)
                .unwrap_or(dst)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    (writes.len(), bytes, collisions)
}

/// `mods/bikes` → `bikes`. A bare `mods` (the whole-tree case) has no type folder of its own;
/// `place_mod`'s ModsTree rule ignores the value anyway.
fn type_folder_of(subpath: &str) -> &str {
    let last = subpath.rsplit(['/', '\\']).next().unwrap_or("misc");
    if last.eq_ignore_ascii_case("mods") {
        "misc"
    } else {
        last
    }
}

// ---------------------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------------------

/// Stage and classify a drop. Nothing under `mods/` is touched.
pub fn plan(mods_path: &str, paths: &[String]) -> anyhow::Result<DropPlan> {
    if mods_path.trim().is_empty() {
        anyhow::bail!("no MX Bikes folder configured");
    }
    let mods_dir = library::mods_subdir(mods_path, "mods");
    let ctx = Ctx::new(mods_path);

    let plan_id = next_id("drop");
    let work = install::staging_dir("drop");
    let mut used_work = false;

    let mut items = Vec::new();
    let mut staged = HashMap::new();
    let mut skipped = Vec::new();

    for (idx, raw) in paths.iter().enumerate() {
        let src = Path::new(raw);
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| raw.clone());
        let slug = src
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| name.clone());

        // Each source gets its own staging sub-directory so two archives holding a folder of
        // the same name can't merge into each other before the user has seen either.
        let dest = work.join(format!("{idx}"));
        let root = match install::stage_input(src, &dest) {
            Ok(install::StagedKind::Directory) => src.to_path_buf(),
            Ok(_) => {
                used_work = true;
                dest
            }
            Err(e) => {
                skipped.push(SkippedFile {
                    name,
                    reason: format!("{e:#}"),
                });
                continue;
            }
        };

        for unit in units_in(&root, &mods_dir, ctx, &slug, 0) {
            let (item, st) = to_item(unit, ctx, &mods_dir, &slug, &name);
            staged.insert(item.id.clone(), st);
            items.push(item);
        }
    }

    let total_bytes = items.iter().map(|i| i.bytes).sum();

    with_plans(|m| {
        m.insert(
            plan_id.clone(),
            StagedPlan {
                work: used_work.then(|| work.clone()),
                items: staged,
            },
        )
    });

    Ok(DropPlan {
        id: plan_id,
        items,
        skipped,
        total_bytes,
    })
}

/// Re-price one row after the user changed its destination.
pub fn repreview(
    mods_path: &str,
    plan_id: &str,
    item_id: &str,
    subpath: &str,
    dest_folder: &str,
) -> anyhow::Result<(usize, u64, Vec<String>)> {
    let mods_dir = library::mods_subdir(mods_path, "mods");
    with_plans(|m| {
        let plan = m
            .get(plan_id)
            .ok_or_else(|| anyhow::anyhow!("this drop has expired — drop the files again"))?;
        let item = plan
            .items
            .get(item_id)
            .ok_or_else(|| anyhow::anyhow!("unknown item"))?;
        Ok(preview(
            &item.path,
            &mods_dir,
            subpath,
            dest_folder,
            &item.slug,
        ))
    })
}

/// Install the rows the user kept. Rows they dropped from the list are simply absent.
pub fn commit(
    mods_path: &str,
    plan_id: &str,
    choices: &[CommitItem],
) -> anyhow::Result<CommitOutcome> {
    let mods_dir = library::mods_subdir(mods_path, "mods");

    // Copy out what we need so the registry isn't held locked across the file I/O.
    let jobs: Vec<(String, String, PathBuf, String, String, String)> = with_plans(|m| {
        let plan = m
            .get(plan_id)
            .ok_or_else(|| anyhow::anyhow!("this drop has expired — drop the files again"))?;
        Ok::<_, anyhow::Error>(
            choices
                .iter()
                .filter_map(|c| {
                    let it = plan.items.get(&c.id)?;
                    Some((
                        c.id.clone(),
                        it.name.clone(),
                        it.path.clone(),
                        it.slug.clone(),
                        c.subpath.clone(),
                        c.dest_folder.clone(),
                    ))
                })
                .collect(),
        )
    })?;

    let mut installed = Vec::new();
    let mut failed = Vec::new();

    for (id, name, path, slug, subpath, dest_folder) in jobs {
        match install_one(&mods_dir, mods_path, &path, &subpath, &dest_folder, &slug) {
            Ok(files) => installed.push(InstalledItem {
                id,
                name,
                files,
                dest: display_dest(&subpath, &dest_folder),
            }),
            Err(e) => failed.push(FailedItem {
                id,
                name,
                error: format!("{e:#}"),
            }),
        }
    }

    Ok(CommitOutcome { installed, failed })
}

fn display_dest(subpath: &str, dest_folder: &str) -> String {
    if dest_folder.is_empty() {
        subpath.to_string()
    } else {
        format!("{subpath}/{}", dest_folder.replace('\\', "/"))
    }
}

fn install_one(
    mods_dir: &Path,
    mods_path: &str,
    src: &Path,
    subpath: &str,
    dest_folder: &str,
    slug: &str,
) -> anyhow::Result<usize> {
    if subpath.trim().is_empty() {
        anyhow::bail!("no destination chosen");
    }
    guard_segments(dest_folder)?;
    guard_segments(subpath)?;

    let type_folder = type_folder_of(subpath);
    let route = install::plan_placement(src, mods_dir, type_folder, dest_folder, slug);

    // Belt and braces: the destination the router produced must still sit inside the mods
    // folder even if a segment slipped past `guard_segments`.
    for (_, dst) in install::writes_for(&route.placement) {
        if !dst.starts_with(mods_dir) {
            anyhow::bail!("refusing to write outside the MX Bikes folder");
        }
        let _ = mods_path;
    }

    install::place_mod(src, mods_dir, type_folder, dest_folder, slug)
}

/// `sanitize` maps the separator characters but not `..`, and `place_mod` pushes each
/// segment onto the destination — harmless while every value came from the app's own
/// pickers, not once the review sheet lets a user type one.
fn guard_segments(value: &str) -> anyhow::Result<()> {
    for seg in value.split(['/', '\\']).filter(|s| !s.is_empty()) {
        if seg == ".." {
            anyhow::bail!("refusing a destination that climbs out of the MX Bikes folder");
        }
    }
    Ok(())
}

/// Drop a plan and delete anything we staged for it.
pub fn cancel(plan_id: &str) {
    let taken = with_plans(|m| m.remove(plan_id));
    if let Some(plan) = taken {
        if let Some(work) = plan.work {
            let _ = std::fs::remove_dir_all(work);
        }
    }
}

/// Which bikes gained a sound set, so the Library can tell them from stock.
pub fn sound_bikes(plan_id: &str, ids: &[String]) -> Vec<String> {
    with_plans(|m| {
        let Some(plan) = m.get(plan_id) else {
            return Vec::new();
        };
        let mut out: Vec<String> = Vec::new();
        for id in ids {
            let Some(it) = plan.items.get(id) else {
                continue;
            };
            for bike in install::sound_bikes_in(&it.path) {
                if !out.iter().any(|n: &String| n.eq_ignore_ascii_case(&bike)) {
                    out.push(bike);
                }
            }
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Game;
    use std::fs;

    /// The title a test classifies against. Passed in rather than set process-wide, so a GP
    /// Bikes case and an MX Bikes case can run side by side — which, since `cargo test` runs
    /// them in parallel threads, they do.
    fn ctx(game: Game, mods_path: &str) -> Ctx<'_> {
        Ctx {
            mods_path,
            game: game.profile(),
        }
    }

    fn mx(mods_path: &str) -> Ctx<'_> {
        ctx(Game::Mxb, mods_path)
    }

    fn gpb(mods_path: &str) -> Ctx<'_> {
        ctx(Game::Gpb, mods_path)
    }

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("frost-dz-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// A structurally valid `.pnt` carrying named 1x1 textures, so the classifier has real
    /// bytes to read rather than a stub.
    fn write_pnt(path: &Path, textures: &[&str]) {
        use std::io::Write as _;
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut buf = Vec::new();
        buf.extend_from_slice(b"PNT\x00");
        let mut name = [0u8; 100];
        name[..4].copy_from_slice(b"test");
        buf.extend_from_slice(&name);
        buf.extend_from_slice(&(textures.len() as u32).to_le_bytes());
        for t in textures {
            let mut n = [0u8; 100];
            let b = t.as_bytes();
            n[..b.len()].copy_from_slice(b);
            buf.extend_from_slice(&n);
            buf.extend_from_slice(&1u32.to_le_bytes()); // width
            buf.extend_from_slice(&1u32.to_le_bytes()); // height
            buf.extend_from_slice(&[0u8; 16]); // md5
            let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
            enc.write_all(&[0u8, 0, 0, 255]).unwrap();
            let payload = enc.finish().unwrap();
            buf.extend_from_slice(&((payload.len() + 8) as u32).to_le_bytes());
            buf.extend_from_slice(&[0u8; 8]); // padding
            buf.extend_from_slice(&payload);
        }
        fs::write(path, buf).unwrap();
    }

    fn write(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    /// The property the whole design rests on: what the review sheet promises is what the
    /// commit writes. Same directory, same category, same destination — same file list.
    #[test]
    fn preview_matches_what_commit_writes() {
        let root = tmp("roundtrip");
        let src = root.join("src");
        let mods = root.join("mods");
        write(&src.join("Hangtown/Hangtown.map"), "m");
        write(&src.join("Hangtown/Hangtown.trh"), "t");
        fs::create_dir_all(&mods).unwrap();

        let (files, _bytes, collisions) = preview(&src, &mods, "mods/tracks", "", "Hangtown");
        assert!(collisions.is_empty(), "nothing installed yet");

        let written = install::place_mod(&src, &mods, "tracks", "", "Hangtown").unwrap();
        assert_eq!(files, written, "preview count must equal what was written");

        // And a second identical placement must now report every file as a collision.
        let (_, _, again) = preview(&src, &mods, "mods/tracks", "", "Hangtown");
        assert_eq!(again.len(), written, "re-drop must flag every file");
    }

    #[test]
    fn extracted_track_is_recognised() {
        let root = tmp("track");
        write(&root.join("Hangtown.map"), "m");
        write(&root.join("Hangtown.trh"), "t");
        let v = classify_typed(&root, mx("")).expect("classified");
        let (kind, reason) = (v.kind, v.reason);
        assert_eq!(kind, ContentKind::Track);
        assert_eq!(reason, DetectReason::TrackMarkers);
    }

    #[test]
    fn bike_folder_is_recognised_by_its_configs() {
        let root = tmp("bike");
        let bike = root.join("MX1OEM_2023");
        write(&bike.join("MX1OEM_2023.ini"), "[info]\nname = Test 450\n[data]\ncat = MX1\n");
        write(&bike.join("MX1OEM_2023.cfg"), "ID = MX1OEM_2023\n");
        let v = classify_typed(&bike, mx("")).expect("classified");
        let (kind, reason, detail) = (v.kind, v.reason, v.detail.clone());
        assert_eq!(kind, ContentKind::Bike);
        assert_eq!(reason, DetectReason::BikeConfig);
        assert_eq!(detail.as_deref(), Some("Test 450 · MX1"));
    }

    #[test]
    fn loose_paints_demand_a_bike() {
        let root = tmp("paints");
        write(&root.join("Blue.pnt"), "PNT\0");
        let v = classify_typed(&root, mx("")).expect("classified");
        let (kind, reason) = (v.kind, v.reason);
        assert_eq!(kind, ContentKind::BikePaint);
        assert_eq!(reason, DetectReason::LoosePaint);

        let unit = Unit {
            path: root.clone(),
            verdict: v,
        };
        let (item, _) = to_item(unit, mx(""), &root.join("mods"), "Blue", "Blue.pnt");
        assert!(item.needs_choice, "nothing says which bike these paint");
        assert!(item.subpath.is_empty(), "must not guess a category");
    }

    #[test]
    fn gear_folders_route_to_rider() {
        let root = tmp("gear");
        write(&root.join("helmets/Airoh/paints/Red.pnt"), "PNT\0");
        let v = classify_typed(&root, mx("")).expect("classified");
        let (kind, reason) = (v.kind, v.reason);
        assert_eq!(kind, ContentKind::RiderGear);
        assert_eq!(reason, DetectReason::GearFolders);
    }

    #[test]
    fn a_mixed_archive_splits_into_one_row_per_mod() {
        let root = tmp("mixed");
        let mods = root.join("mods");
        write(&root.join("drop/Hangtown/Hangtown.map"), "m");
        write(
            &root.join("drop/MX1OEM_2023/MX1OEM_2023.ini"),
            "[info]\nname = Test 450\n",
        );
        write(&root.join("drop/MX1OEM_2023/MX1OEM_2023.cfg"), "ID = X\n");

        let units = units_in(&root.join("drop"), &mods, mx(""), "drop", 0);
        let kinds: Vec<ContentKind> = units.iter().map(|u| u.verdict.kind).collect();
        assert!(kinds.contains(&ContentKind::Track), "{kinds:?}");
        assert!(kinds.contains(&ContentKind::Bike), "{kinds:?}");
        assert_eq!(units.len(), 2, "one row each, nothing extra");
    }

    #[test]
    fn a_wrapper_folder_does_not_hide_the_mod() {
        let root = tmp("wrapper");
        let mods = root.join("mods");
        write(&root.join("drop/Hangtown v2/Hangtown.map"), "m");
        write(&root.join("drop/Hangtown v2/readme.txt"), "hi");

        let units = units_in(&root.join("drop"), &mods, mx(""), "drop", 0);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].verdict.kind, ContentKind::Track);
    }

    #[test]
    fn a_mods_tree_is_one_row_and_keeps_its_own_layout() {
        let root = tmp("modstree");
        let mods = root.join("mods");
        write(&root.join("drop/mods/tracks/Hangtown/Hangtown.map"), "m");

        let units = units_in(&root.join("drop"), &mods, mx(""), "drop", 0);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].verdict.kind, ContentKind::ModsTree);
        assert_eq!(units[0].verdict.reason, DetectReason::ModsTree);
    }

    #[test]
    fn unrecognised_content_asks_rather_than_guesses() {
        let root = tmp("unknown");
        write(&root.join("drop/frame.edf"), "\0\0\0");

        let mods = root.join("mods");
        let units = units_in(&root.join("drop"), &mods, mx(""), "drop", 0);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].verdict.kind, ContentKind::Unknown);

        let (item, _) = to_item(
            Unit {
                path: units[0].path.clone(),
                verdict: units[0].verdict.clone(),
            },
            mx(""),
            &mods,
            "frame",
            "frame.edf",
        );
        assert!(item.needs_choice);
        assert!(item.subpath.is_empty());
    }

    /// Build a `.zip` on disk from `(name, body)` pairs.
    fn make_zip(path: &Path, entries: &[(&str, &str)]) {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, body) in entries {
            w.start_file::<_, ()>(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    /// The full journey a user takes: drop a track archive, a bike archive and a loose paint
    /// in one gesture; review; choose a bike for the paint; install. Then re-drop the track
    /// and confirm the collisions are reported.
    #[test]
    fn end_to_end_drop_review_and_install() {
        let root = tmp("e2e");
        let game = root.join("MX Bikes");
        let mods = game.join("mods");
        let mods_path = game.to_string_lossy().into_owned();

        // An installed bike, so the loose paint has somewhere to be offered.
        write(
            &mods.join("bikes/MX1OEM_2023/MX1OEM_2023.ini"),
            "[info]\nname = Test 450\n[data]\ncat = MX1\n",
        );
        write(
            &mods.join("bikes/MX1OEM_2023/MX1OEM_2023.cfg"),
            "ID = MX1OEM_2023\n",
        );

        let drops = root.join("drops");
        fs::create_dir_all(&drops).unwrap();
        let track_zip = drops.join("Hangtown.zip");
        make_zip(
            &track_zip,
            &[
                ("Hangtown/Hangtown.map", "map"),
                ("Hangtown/Hangtown.trh", "trh"),
                ("Hangtown/readme.txt", "hi"),
            ],
        );
        let bike_zip = drops.join("NewBike.zip");
        make_zip(
            &bike_zip,
            &[
                ("RM250/RM250.ini", "[info]\nname = RM 250\n"),
                ("RM250/RM250.cfg", "ID = RM250\n"),
            ],
        );
        let paint = drops.join("Blue.pnt");
        write(&paint, "PNT\0whatever");

        let staged = plan(
            &mods_path,
            &[
                track_zip.to_string_lossy().into_owned(),
                bike_zip.to_string_lossy().into_owned(),
                paint.to_string_lossy().into_owned(),
            ],
        )
        .expect("planned");

        assert_eq!(staged.items.len(), 3, "one row per dropped thing");

        let track = staged
            .items
            .iter()
            .find(|i| i.kind == ContentKind::Track)
            .expect("track row");
        assert_eq!(track.subpath, "mods/tracks");
        assert!(!track.needs_choice);
        // `readme.txt` is junk to the placer but still travels inside the track folder.
        assert!(track.file_count >= 2, "{}", track.file_count);
        assert!(track.collisions.is_empty(), "clean install");

        let bike = staged
            .items
            .iter()
            .find(|i| i.kind == ContentKind::Bike)
            .expect("bike row");
        assert_eq!(bike.subpath, "mods/bikes");
        assert_eq!(bike.detail.as_deref(), Some("RM 250"));
        assert_eq!(
            bike.dest_folder, "RM250",
            "a bike folder keeps its own folder rather than scattering its configs"
        );

        let pnt = staged
            .items
            .iter()
            .find(|i| i.kind == ContentKind::BikePaint)
            .expect("paint row");
        assert!(pnt.needs_choice, "nothing says which bike it paints");
        assert!(
            pnt.choices.iter().any(|c| c.label == "Test 450"),
            "installed bike offered: {:?}",
            pnt.choices.iter().map(|c| &c.label).collect::<Vec<_>>()
        );

        // The user picks a bike for the paint, then installs everything.
        let chosen = pnt
            .choices
            .iter()
            .find(|c| c.label == "Test 450")
            .unwrap()
            .value
            .clone();
        let outcome = commit(
            &mods_path,
            &staged.id,
            &[
                // Exactly what the review sheet sends: each row's own resolved destination.
                CommitItem {
                    id: track.id.clone(),
                    subpath: track.subpath.clone(),
                    dest_folder: track.dest_folder.clone(),
                },
                CommitItem {
                    id: bike.id.clone(),
                    subpath: bike.subpath.clone(),
                    dest_folder: bike.dest_folder.clone(),
                },
                CommitItem {
                    id: pnt.id.clone(),
                    subpath: "mods/bikes".into(),
                    dest_folder: chosen,
                },
            ],
        )
        .expect("committed");

        assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
        assert_eq!(outcome.installed.len(), 3);

        assert!(mods.join("tracks/Hangtown/Hangtown.map").is_file());
        assert!(mods.join("bikes/RM250/RM250.cfg").is_file());
        assert!(
            mods.join("bikes/MX1OEM_2023/paints/Blue.pnt").is_file(),
            "paint landed in the chosen bike's paints folder"
        );

        // Re-dropping the same track must now report every file as a replacement.
        let again = plan(&mods_path, &[track_zip.to_string_lossy().into_owned()])
            .expect("re-planned");
        let row = &again.items[0];
        assert_eq!(row.kind, ContentKind::Track);
        assert!(
            !row.collisions.is_empty(),
            "a re-drop must warn before overwriting"
        );
        assert!(
            row.collisions.iter().any(|c| c.contains("Hangtown.map")),
            "{:?}",
            row.collisions
        );

        cancel(&again.id);
        let _ = fs::remove_dir_all(&root);
    }

    /// A folder dropped straight from the file manager is staged by reference — we must not
    /// copy a multi-gigabyte track twice — and still installs correctly.
    #[test]
    fn a_dropped_folder_installs_without_being_copied_first() {
        let root = tmp("folder");
        let game = root.join("MX Bikes");
        let mods_path = game.to_string_lossy().into_owned();
        let src = root.join("Hangtown");
        write(&src.join("Hangtown.map"), "map");

        let staged = plan(&mods_path, &[src.to_string_lossy().into_owned()]).expect("planned");
        assert_eq!(staged.items.len(), 1);
        assert_eq!(staged.items[0].kind, ContentKind::Track);

        let outcome = commit(
            &mods_path,
            &staged.id,
            &[CommitItem {
                id: staged.items[0].id.clone(),
                subpath: "mods/tracks".into(),
                dest_folder: String::new(),
            }],
        )
        .expect("committed");
        assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
        assert!(game.join("mods/tracks/Hangtown/Hangtown.map").is_file());
        assert!(src.join("Hangtown.map").is_file(), "source left alone");
        let _ = fs::remove_dir_all(&root);
    }

    /// The regression that shipped a track labelled "Bike": `bikeswap::read_identity` says
    /// `Some` when *either* `<stem>.ini` or `<stem>.cfg` is present, and a track `.pkz`
    /// carries an `.ini` too. The track markers must win, and a bike must show its `.cfg`.
    #[test]
    fn a_track_pkz_is_not_mistaken_for_a_bike() {
        let root = tmp("trackpkz");
        let pkz = root.join("2026_ARLMX_RD02_RANCHO_CO_Pro.pkz");
        make_zip(
            &pkz,
            &[
                ("2026_ARLMX_RD02_RANCHO_CO_Pro.ini", "[info]\nname = Rancho\n"),
                ("2026_ARLMX_RD02_RANCHO_CO_Pro.map", "mapdata"),
            ],
        );
        let v = classify_pkz(&pkz);
        let (kind, reason) = (v.kind, v.reason);
        assert_eq!(kind, ContentKind::Track, "an .ini alone must not mean bike");
        assert_eq!(reason, DetectReason::TrackPackage);

        // And the same shape reached through the normal classifier.
        let k = classify_typed(&root, mx("")).expect("classified").kind;
        assert_eq!(k, ContentKind::Track);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_bike_pkz_is_recognised_by_its_cfg() {
        let root = tmp("bikepkz");
        let pkz = root.join("RM250.pkz");
        make_zip(
            &pkz,
            &[
                ("RM250.ini", "[info]\nname = RM 250\n[data]\ncat = MX2\n"),
                ("RM250.cfg", "ID = RM250\n"),
            ],
        );
        let v = classify_pkz(&pkz);
        let (kind, reason, detail) = (v.kind, v.reason, v.detail.clone());
        assert_eq!(kind, ContentKind::Bike);
        assert_eq!(reason, DetectReason::BikeConfig);
        assert_eq!(detail.as_deref(), Some("RM 250 · MX2"));
        let _ = fs::remove_dir_all(&root);
    }

    /// Destinations must never come back empty, even with nothing installed — that was the
    /// dead end where a loose paint could not be placed at all.
    #[test]
    fn destinations_are_offered_even_on_an_empty_mods_folder() {
        let root = tmp("emptymods");
        let mods_path = root.to_string_lossy().into_owned();
        let choices = all_choices(mx(&mods_path), true);
        assert!(!choices.is_empty(), "a picker with no options is a dead end");
        assert!(
            choices.iter().any(|c| c.value == "riders/default_mx/paints"),
            "the stock rider profile is always a valid home: {:?}",
            choices.iter().map(|c| &c.value).collect::<Vec<_>>()
        );
        assert!(choices.iter().any(|c| c.value == "helmets"), "new-model slots offered");
        let _ = fs::remove_dir_all(&root);
    }

    /// An outfit shouldn't make the user hunt for a destination: MX Bikes always ships
    /// `default_mx`, so that is a real place the paint will load from.
    #[test]
    fn a_rider_outfit_defaults_to_the_stock_profile() {
        let root = tmp("outfit");
        let mods = root.join("mods");
        let pnt = root.join("Astars.pnt");
        write_pnt(&pnt, &["rider", "rider_n"]);

        let v = classify_paint(&pnt, mx(""));
        assert_eq!(v.kind, ContentKind::RiderGear);
        assert_eq!(v.reason, DetectReason::RiderTexture);
        assert!(!v.needs_choice, "an outfit has a real default");
        assert_eq!(v.dest_folder, "riders/default_mx/paints");

        let (item, _) = to_item(
            Unit {
                path: root.clone(),
                verdict: v,
            },
            mx(""),
            &mods,
            "Astars",
            "Astars.pnt",
        );
        assert_eq!(item.subpath, "mods/rider");
        assert_eq!(item.dest_folder, "riders/default_mx/paints");
        assert!(!item.choices.is_empty(), "still overridable");
        let _ = fs::remove_dir_all(&root);
    }

    /// GP Bikes' suits carry the boots' textures, because the boots are part of the rider
    /// model — `(S) Suit 1 + Boots Alpinestars` is one mesh. Tested for `boot` first, a suit
    /// was filed as boots: a folder GP Bikes has no loader for, so the paint vanished from
    /// the game while the app reported it installed.
    #[test]
    fn a_gp_suit_is_not_mistaken_for_boots() {
        let root = tmp("gpsuit");
        let mods_path = root.to_string_lossy().into_owned();
        // Manu's Modern Rider, the model nearly every GP suit is made for.
        fs::create_dir_all(root.join("mods/rider/riders/Modern Type 1/paints")).unwrap();
        let pnt = root.join("Alpinestars.pnt");
        write_pnt(&pnt, &["Suit", "Suit_n", "Boots", "Arms"]);

        let v = classify_paint(&pnt, gpb(&mods_path));
        assert_eq!(v.kind, ContentKind::RiderGear);
        assert_eq!(v.reason, DetectReason::RiderTexture, "the suit is read before the boots");
        assert_eq!(v.dest_folder, "riders/Modern Type 1/paints");
        let _ = fs::remove_dir_all(&root);
    }

    /// With no rider model installed there is nowhere a GP suit will load from, and the app
    /// must say so rather than invent MX Bikes' `default_mx` — which GP Bikes doesn't have.
    #[test]
    fn a_gp_suit_asks_when_no_rider_model_is_installed() {
        let root = tmp("gpsuit-empty");
        let mods_path = root.to_string_lossy().into_owned();
        let pnt = root.join("Alpinestars.pnt");
        write_pnt(&pnt, &["Suit", "Boots"]);

        let v = classify_paint(&pnt, gpb(&mods_path));
        assert!(v.needs_choice, "nothing installed can wear it");
        assert!(v.dest_folder.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    /// The picker is the user's way out of a wrong guess, so it must offer this title's
    /// folders — and only those.
    #[test]
    fn gp_bikes_is_offered_its_own_folders_and_no_others() {
        let root = tmp("gpchoices");
        let mods_path = root.to_string_lossy().into_owned();
        fs::create_dir_all(root.join("mods/rider/riders/Modern Type 1")).unwrap();
        let values: Vec<String> = gear_choices(gpb(&mods_path))
            .into_iter()
            .map(|c| c.value)
            .collect();

        assert!(values.contains(&"riders/Modern Type 1/paints".to_string()), "{values:?}");
        assert!(values.contains(&"riders".to_string()), "somewhere for a new rider model");
        assert!(values.contains(&"animations".to_string()), "riding styles are GP content");
        for absent in ["boots", "protections", "riders/default_mx/paints"] {
            assert!(
                !values.iter().any(|v| v.starts_with(absent)),
                "GP Bikes never reads `{absent}`: {values:?}",
            );
        }
        assert!(
            !values.iter().any(|v| v.ends_with("/gloves") || v.ends_with("/goggles")),
            "gloves are built into the suit, and road helmets have visors: {values:?}",
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// MX Bikes keeps every folder it had, including the ones GP lacks.
    #[test]
    fn mx_bikes_still_gets_its_full_set_of_gear_folders() {
        let root = tmp("mxchoices");
        let mods_path = root.to_string_lossy().into_owned();
        let values: Vec<String> = gear_choices(mx(&mods_path))
            .into_iter()
            .map(|c| c.value)
            .collect();

        for expected in [
            "helmets",
            "boots",
            "protections",
            "riders/default_mx/paints",
            "riders/default_mx/gloves",
        ] {
            assert!(values.contains(&expected.to_string()), "{expected} missing: {values:?}");
        }
        assert!(
            !values.iter().any(|v| v == "protection"),
            "the legacy spelling is read, never written: {values:?}",
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// A bike paint is the fallback, and it still has to ask which bike.
    #[test]
    fn a_bike_paint_is_told_apart_from_an_outfit() {
        let root = tmp("bikepaint");
        let pnt = root.join("96STOCK.pnt");
        write_pnt(&pnt, &["framecompletemap", "wheels"]);
        let v = classify_paint(&pnt, mx(""));
        assert_eq!(v.kind, ContentKind::BikePaint);
        assert!(v.needs_choice, "nothing says which bike");
        let _ = fs::remove_dir_all(&root);
    }

    /// Every row can be re-filed, including ones the classifier placed confidently.
    #[test]
    fn an_identified_track_can_still_be_filed_in_a_folder() {
        let root = tmp("trackfolder");
        let mods = root.join("mods");
        fs::create_dir_all(&mods).unwrap();
        let src = root.join("Hangtown");
        write(&src.join("Hangtown.map"), "m");

        let units = units_in(&src, &mods, mx(""), "Hangtown", 0);
        let (item, _) = to_item(
            units.into_iter().next().unwrap(),
            mx(""),
            &mods,
            "Hangtown",
            "Hangtown",
        );
        assert_eq!(item.kind, ContentKind::Track);
        assert!(!item.needs_choice);
        assert!(
            item.choices.iter().any(|c| c.subpath == "mods/tracks"),
            "a confidently-placed track still offers its folders: {:?}",
            item.choices.iter().map(|c| &c.label).collect::<Vec<_>>()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_destination_cannot_climb_out_of_the_mods_folder() {
        assert!(guard_segments("MX1OEM/paints").is_ok());
        assert!(guard_segments("../../Windows/System32").is_err());
        assert!(guard_segments("..\\..\\evil").is_err());
    }

    #[test]
    fn install_refuses_an_item_with_no_destination() {
        let root = tmp("nodest");
        let mods = root.join("mods");
        fs::create_dir_all(&mods).unwrap();
        let err = install_one(&mods, "", &root, "", "", "x").unwrap_err();
        assert!(err.to_string().contains("no destination"), "{err}");
    }
}
