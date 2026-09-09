//! Picking which ReShade preset the game renders with.
//!
//! ReShade is a third-party post-processing injector. It is **not** ours and is never shipped
//! with this app: reshade.me asks explicitly that its binaries and shader files not be
//! redistributed, and that users be linked to the site instead. So everything here is
//! read-only about ReShade itself — detect it, name its version — and read/write only about
//! *presets*, which are plain `.ini` files the site explicitly invites people to share.
//!
//! ## How it hangs together
//!
//! ReShade attaches by sitting next to the executable under the name of a graphics DLL the
//! game loads. MX Bikes and GP Bikes are OpenGL — both binaries import `OPENGL32.dll` and no
//! `d3d*`/`dxgi`/`vulkan` at all — so the file that matters is `<game>/opengl32.dll`. A
//! ReShade installed as `dxgi.dll` here is a real and common mistake (the installer asks which
//! API and offers DirectX first), which is why [`Status::wrong_api`] exists: the game will
//! simply never load it, and "nothing happens" deserves a better answer than silence.
//!
//! ```text
//! <game_path>/
//!   opengl32.dll          ReShade itself — detected, never written
//!   ReShade.ini           its config — we rewrite exactly one key, PresetPath
//!   reshade-shaders/
//!     Shaders/*.fx        the effects presets ask for
//!     Textures/*          their lookup textures
//!   FrostMod ReShade/     this app's preset library
//!     Off.ini             authored here: a preset that runs no techniques
//!     <Name>.ini
//! ```
//!
//! The active preset is not tracked by a marker file the way [`crate::soundmods`] tracks the
//! active sound set. It can't be: ReShade reads `ReShade.ini`'s `PresetPath` and nothing else,
//! so that key *is* the state, and a second copy of it here could only ever disagree.
//!
//! Presets already sitting loose in the game folder — where every by-hand install leaves them
//! — are listed where they lie rather than moved. Someone who set ReShade up before this app
//! existed should open the card and see their own presets, not an empty list.

use crate::library::{is_simple_name, resolve_child};
use crate::presets::IniDoc;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// This app's preset library, under the game's install dir.
///
/// Named like [`crate::soundmods::SOUND_LIB_DIR`] so a player browsing their game folder can
/// tell at a glance which folders MXB App made.
pub const PRESET_DIR: &str = "FrostMod ReShade";

/// ReShade's own configuration file. Never a preset, even though it is also an `.ini` with a
/// `[GENERAL]` section — hence the explicit exclusion in [`is_preset_file`].
const CONFIG: &str = "ReShade.ini";

/// Where ReShade looks for effects and their textures.
const SHADER_DIR: &str = "reshade-shaders";

/// The name of the no-effects preset, and the file it lives in.
///
/// ReShade has no "off" state in its config — the nearest honest thing is a preset that runs
/// no techniques, which this app writes itself. Disabling by renaming `opengl32.dll` was the
/// alternative and is worse: it fails while the game holds the file open, and it is the kind
/// of change a player won't connect back to a dropdown in a settings page.
pub const OFF: &str = "Off";
const OFF_FILE: &str = "Off.ini";

/// The DLL ReShade must be installed as for these games, and the ones that mean it was
/// installed for an API the game never calls.
const OPENGL_DLL: &str = "opengl32.dll";
const WRONG_API_DLLS: [&str; 5] = ["dxgi.dll", "d3d9.dll", "d3d10.dll", "d3d11.dll", "d3d12.dll"];

/// Proof that a DLL is ReShade's rather than some other wrapper that happens to share the
/// name — RivaTuner, a Steam overlay shim, a game's own bundled `opengl32.dll`.
const RESHADE_MARKER: &[u8] = b"ReShade";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub name: String,
    pub path: String,
    pub active: bool,
    /// Lives in [`PRESET_DIR`], so this app put it there and may remove it. A preset found
    /// loose in the game folder is someone else's file and is only ever read.
    pub managed: bool,
    /// Effect files the preset asks for that aren't installed under `reshade-shaders/`.
    ///
    /// A preset naming an effect ReShade doesn't have doesn't fail — it silently drops that
    /// technique, which reads to the player as "the preset did nothing". Empty when the
    /// shaders folder is missing entirely; that case is [`Status::has_shaders`], reported
    /// once rather than repeated on every preset.
    pub missing_effects: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The folder we looked in. Empty when none is configured yet, which is the one state
    /// where nothing below can be trusted.
    pub game_dir: String,
    /// That folder came from the player's `reshade_path` override rather than from the
    /// game's install dir. Set by the command layer, which is the only side that knows.
    pub custom: bool,
    /// A folder *is* configured, and it isn't there. Distinct from the empty `game_dir`
    /// above: "the folder you picked has moved" and "you haven't picked one" need
    /// different answers, and both used to come back as the latter.
    pub folder_missing: bool,
    /// A ReShade `opengl32.dll` is in place — the game will load it.
    pub installed: bool,
    /// ReShade is here, but as this DLL, which these games never load. Set only when the
    /// OpenGL one is absent, because a correct install alongside a leftover DirectX one is
    /// working fine and shouldn't be nagged about.
    pub wrong_api: Option<String>,
    /// Best-effort, from the DLL. `None` costs nothing but a missing line in the UI.
    pub version: Option<String>,
    /// `reshade-shaders/Shaders` exists and holds at least one `.fx`.
    pub has_shaders: bool,
    /// Name of the active preset, or empty when `PresetPath` names a file that isn't there.
    pub active: String,
    pub presets: Vec<Preset>,
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

fn library_dir(game_dir: &Path) -> PathBuf {
    resolve_child(game_dir, PRESET_DIR)
}

fn config_path(game_dir: &Path) -> PathBuf {
    resolve_child(game_dir, CONFIG)
}

fn shaders_dir(game_dir: &Path) -> PathBuf {
    resolve_child(&resolve_child(game_dir, SHADER_DIR), "Shaders")
}

/// `.\FrostMod ReShade\Name.ini` — how a preset path is written into `ReShade.ini`.
///
/// Relative, with backslashes, deliberately. ReShade resolves relative paths against the
/// executable's folder, so this is the one form that is right on both Windows and Linux: under
/// Proton the game reads this file as a Windows program and would make nothing of the
/// `/home/…` absolute path this side of the wine boundary would produce.
fn rel_preset_path(game_dir: &Path, preset: &Path) -> String {
    let rel = preset.strip_prefix(game_dir).unwrap_or(preset);
    let joined = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\\");
    format!(".\\{joined}")
}

/// The inverse of [`rel_preset_path`], tolerant of everything ReShade or a human might have
/// written: absolute Windows paths, `./` or `.\` prefixes, either separator, quotes.
fn resolve_preset_path(game_dir: &Path, raw: &str) -> Option<PathBuf> {
    let raw = raw.trim().trim_matches('"');
    if raw.is_empty() {
        return None;
    }
    let norm = raw.replace('\\', "/");
    let rel = norm.trim_start_matches("./");
    // An absolute path from a Windows install (`C:\Games\…`) can still be ours: take its last
    // segments relative to the game dir when they match, and otherwise trust it as written.
    if rel.contains(':') || rel.starts_with('/') {
        let p = PathBuf::from(raw.replace('\\', std::path::MAIN_SEPARATOR_STR));
        return p.exists().then_some(p);
    }
    let mut p = game_dir.to_path_buf();
    for seg in rel.split('/').filter(|s| !s.is_empty() && *s != ".") {
        p = resolve_child(&p, seg);
    }
    Some(p)
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Is this file ReShade's own DLL, and if so which version?
///
/// `Some(None)` means "ReShade, version unknown" — a real answer, distinct from the `None`
/// that means "not ReShade". Both questions read the same bytes, so they are asked together:
/// these DLLs run to tens of megabytes and `status` would otherwise read each one twice.
///
/// A substring scan rather than a version-resource parse. The marker has to survive whatever
/// the build was, and neither the overlay's title string nor the resource layout is a
/// documented interface — so a miss costs a missing line in the UI, never a wrong write.
fn probe_dll(path: &Path) -> Option<Option<String>> {
    let bytes = fs::read(path).ok()?;
    if contains(&bytes, RESHADE_MARKER) {
        return Some(version_near_marker(&bytes));
    }
    // Strings in the PE version resource are UTF-16LE, and some builds carry the name only
    // there. Drop the high byte off the ASCII range and re-run both scans over that rather
    // than writing wide-character variants of each.
    let narrowed = narrow_utf16(&bytes);
    contains(&narrowed, RESHADE_MARKER).then(|| version_near_marker(&narrowed))
}

/// Is the file at `path` ReShade's own DLL?
///
/// ReShade installs under whatever name the game's loader preloads (`opengl32.dll` for an
/// OpenGL title like MX Bikes), so the name alone proves nothing. This reads the file and
/// looks for ReShade's own marks instead. Public because more than one caller needs to tell
/// a real ReShade install apart from an unrelated DLL wearing the same name.
pub fn is_reshade_dll(path: &Path) -> bool {
    probe_dll(path).is_some()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// UTF-16LE text flattened to its ASCII bytes, so one scan covers both encodings.
fn narrow_utf16(bytes: &[u8]) -> Vec<u8> {
    bytes
        .chunks_exact(2)
        .filter(|c| c[1] == 0)
        .map(|c| c[0])
        .collect()
}

fn version_near_marker(bytes: &[u8]) -> Option<String> {
    for (i, w) in bytes.windows(RESHADE_MARKER.len()).enumerate() {
        if w != RESHADE_MARKER {
            continue;
        }
        let rest = &bytes[i + RESHADE_MARKER.len()..];
        let tail: Vec<u8> = rest.iter().take(16).copied().collect();
        let text = String::from_utf8_lossy(&tail);
        let digits: String = text
            .trim_start_matches([' ', 'v', 'V'])
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        // `6.8` is the shortest thing worth showing; `6` alone is noise from some other string.
        if digits.contains('.') && digits.starts_with(|c: char| c.is_ascii_digit()) {
            return Some(digits.trim_end_matches('.').to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------

/// The key every ReShade preset carries and `ReShade.ini` never does.
///
/// A preset's shape is a list of techniques plus one section per effect file; ReShade's own
/// config is `[GENERAL]`/`[INPUT]`/`[OVERLAY]`. So this is what tells the two apart when both
/// are `.ini` files sitting in the same folder.
const TECHNIQUES_KEY: &str = "techniques";

fn is_preset_text(text: &str) -> bool {
    text.lines().any(|line| {
        line.split_once('=')
            .is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(TECHNIQUES_KEY))
    })
}

/// True when `path` is a ReShade preset. Public so the dropzone can classify a dropped `.ini`
/// with the same rule this module lists by — one definition, no drift.
pub fn is_preset_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("ini"))
    {
        return false;
    }
    if path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(CONFIG))
    {
        return false;
    }
    fs::read_to_string(path).is_ok_and(|t| is_preset_text(&t))
}

/// The techniques a preset actually turns on, as `(technique name, effect file if named)`.
///
/// `Techniques=` and nothing else. `TechniqueSorting=` sits right beside it, looks identical
/// and is tempting to read as well — but it is the sort order for *every technique ReShade
/// knows about*, not the enabled ones. Real presets list several hundred entries there while
/// switching five on, so reading it would report almost every preset as missing effects it
/// never asked for.
///
/// Both spellings of an entry occur in the wild, and the bare one is the more common:
///
/// ```text
/// Techniques=MagicBloom,Vibrance,HDR                        // by technique name
/// Techniques=SSR@qUINT_ssr.fx,Curves@Curves.fx              // by name and file
/// ```
fn techniques_used(text: &str) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case(TECHNIQUES_KEY) {
            continue;
        }
        for entry in value.split(',') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            let (name, file) = match entry.split_once('@') {
                Some((n, f)) => (n.trim().to_string(), Some(f.trim().to_string())),
                None => (entry.to_string(), None),
            };
            if !name.is_empty() && !out.iter().any(|(n, _)| n.eq_ignore_ascii_case(&name)) {
                out.push((name, file));
            }
        }
    }
    out
}

/// What the installed effects offer: every `.fx` file name, and every technique they declare.
///
/// Both are needed because a preset may name either one. Recursive — shader packs nest a
/// folder deep, and ReShade's own search path (`Shaders\**`) is recursive too.
#[derive(Default)]
struct Available {
    files: Vec<String>,
    techniques: Vec<String>,
}

impl Available {
    fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    fn has_file(&self, want: &str) -> bool {
        self.files.iter().any(|f| f.eq_ignore_ascii_case(want))
    }

    fn has_technique(&self, want: &str) -> bool {
        self.techniques.iter().any(|t| t.eq_ignore_ascii_case(want))
    }
}

fn scan_available(shaders: &Path) -> Available {
    fn walk(dir: &Path, out: &mut Available, depth: usize) {
        if depth > 8 {
            return;
        }
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out, depth + 1);
                continue;
            }
            if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fx")) {
                continue;
            }
            if let Some(n) = p.file_name() {
                out.files.push(n.to_string_lossy().into_owned());
            }
            if let Ok(text) = fs::read_to_string(&p) {
                out.techniques.extend(declared_techniques(&text));
            }
        }
    }
    let mut out = Available::default();
    walk(shaders, &mut out, 0);
    out
}

/// Technique names an effect file declares — the `technique <Name>` lines ReShade itself
/// resolves a preset's bare names against.
fn declared_techniques(fx: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in fx.lines() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix("technique") else {
            continue;
        };
        // `technique` has to be its own word: `techniques_are_cool` is not a declaration.
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let name: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            out.push(name);
        }
    }
    out
}

/// The effects a preset switches on that aren't installed — named as the preset named them,
/// so the message matches what the player would search for.
fn missing_effects(text: &str, have: &Available) -> Vec<String> {
    techniques_used(text)
        .into_iter()
        .filter_map(|(name, file)| match file {
            Some(f) => (!have.has_file(&f)).then_some(f),
            None => (!have.has_technique(&name)).then_some(name),
        })
        .collect()
}

fn preset_name(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn ini_files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if is_preset_file(&p) {
                out.push(p);
            }
        }
    }
    out.sort_by_key(|p| preset_name(p).to_lowercase());
    out
}

// ---------------------------------------------------------------------------
// Public surface
// ---------------------------------------------------------------------------

/// Everything the ReShade card renders, in one pass over the folder ReShade lives in —
/// [`crate::config::AppConfig::reshade_dir`], normally the game's install dir.
pub fn status(reshade_path: &str) -> Status {
    let reshade_path = reshade_path.trim();
    if reshade_path.is_empty() {
        return Status::default();
    }
    let game_dir = PathBuf::from(reshade_path);
    if !game_dir.is_dir() {
        // Named rather than blanked. A folder that has been picked and has since moved is
        // a different problem from one that was never set, and only the path says which.
        return Status {
            game_dir: reshade_path.to_string(),
            folder_missing: true,
            ..Status::default()
        };
    }

    let gl = probe_dll(&resolve_child(&game_dir, OPENGL_DLL));
    let installed = gl.is_some();
    let wrong_api = if installed {
        None
    } else {
        WRONG_API_DLLS
            .iter()
            .find(|n| is_reshade_dll(&resolve_child(&game_dir, n)))
            .map(|n| (*n).to_string())
    };

    let have = scan_available(&shaders_dir(&game_dir));
    let has_shaders = !have.is_empty();

    let active_path = fs::read_to_string(config_path(&game_dir))
        .ok()
        .and_then(|t| IniDoc::parse(&t).get("GENERAL", "PresetPath"))
        .and_then(|raw| resolve_preset_path(&game_dir, &raw));

    let lib = library_dir(&game_dir);
    let mut presets: Vec<Preset> = Vec::new();
    for (managed, path) in ini_files(&lib)
        .into_iter()
        .map(|p| (true, p))
        .chain(ini_files(&game_dir).into_iter().map(|p| (false, p)))
    {
        let text = fs::read_to_string(&path).unwrap_or_default();
        let missing_effects = if has_shaders {
            missing_effects(&text, &have)
        } else {
            Vec::new()
        };
        presets.push(Preset {
            active: active_path.as_deref().is_some_and(|a| same_file(a, &path)),
            name: preset_name(&path),
            path: path.to_string_lossy().into_owned(),
            managed,
            missing_effects,
        });
    }

    let active = presets
        .iter()
        .find(|p| p.active)
        .map(|p| p.name.clone())
        .unwrap_or_default();

    Status {
        game_dir: game_dir.to_string_lossy().into_owned(),
        custom: false,
        folder_missing: false,
        installed,
        wrong_api,
        version: gl.flatten(),
        has_shaders,
        active,
        presets,
    }
}

/// Same file on disk, tolerating the case differences Windows and macOS don't care about and
/// the `..`-free canonicalization failures a missing file would cause.
fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy()),
    }
}

/// Point ReShade at `name`, by rewriting `PresetPath` and nothing else.
///
/// Takes effect when the game next reads its config; ReShade picks the preset up live in
/// recent versions, and the UI says "next launch" while the game is running rather than
/// promising either way.
pub fn apply(game_path: &str, name: &str) -> anyhow::Result<()> {
    let game_dir = game_dir_of(game_path)?;
    if !is_reshade_dll(&resolve_child(&game_dir, OPENGL_DLL)) {
        anyhow::bail!("ReShade isn't installed in {}", game_dir.display());
    }
    let target = if name == OFF {
        ensure_off_preset(&game_dir)?
    } else {
        find_preset(&game_dir, name)?
    };

    let cfg = config_path(&game_dir);
    // ReShade writes this file itself on first run. If it hasn't yet, the search paths have to
    // come with the preset or ReShade would start with nowhere to look for effects.
    let existing = fs::read_to_string(&cfg).unwrap_or_default();
    let mut doc = IniDoc::parse(&existing);
    if existing.trim().is_empty() {
        doc.set("GENERAL", "EffectSearchPaths", ".\\reshade-shaders\\Shaders\\**");
        doc.set("GENERAL", "TextureSearchPaths", ".\\reshade-shaders\\Textures\\**");
    }
    doc.set("GENERAL", "PresetPath", &rel_preset_path(&game_dir, &target));
    fs::write(&cfg, doc.render())?;
    Ok(())
}

/// Remove a preset this app installed. Presets loose in the game folder are left alone —
/// they're not ours to delete. Deleting the active one falls back to [`OFF`].
pub fn delete(game_path: &str, name: &str) -> anyhow::Result<()> {
    let game_dir = game_dir_of(game_path)?;
    if !is_simple_name(name) || name == OFF {
        anyhow::bail!("can't delete preset {name:?}");
    }
    let path = resolve_child(&library_dir(&game_dir), &format!("{name}.ini"));
    if !path.is_file() {
        anyhow::bail!("preset {name:?} isn't in the app's library");
    }
    let was_active = status(game_path).active == name;
    fs::remove_file(&path)?;
    if was_active {
        apply(game_path, OFF)?;
    }
    Ok(())
}

fn game_dir_of(game_path: &str) -> anyhow::Result<PathBuf> {
    let game_path = game_path.trim();
    if game_path.is_empty() {
        anyhow::bail!("the game folder isn't set");
    }
    let dir = PathBuf::from(game_path);
    if !dir.is_dir() {
        anyhow::bail!("no game folder at {}", dir.display());
    }
    Ok(dir)
}

fn find_preset(game_dir: &Path, name: &str) -> anyhow::Result<PathBuf> {
    if !is_simple_name(name) {
        anyhow::bail!("bad preset name {name:?}");
    }
    let file = format!("{name}.ini");
    for dir in [library_dir(game_dir), game_dir.to_path_buf()] {
        let p = resolve_child(&dir, &file);
        if is_preset_file(&p) {
            return Ok(p);
        }
    }
    anyhow::bail!("no preset named {name:?}")
}

/// The no-effects preset, written on first use. Ours to author — it contains no ReShade code,
/// only the two empty keys that mean "run nothing".
fn ensure_off_preset(game_dir: &Path) -> anyhow::Result<PathBuf> {
    let dir = library_dir(game_dir);
    fs::create_dir_all(&dir)?;
    let path = dir.join(OFF_FILE);
    if !path.is_file() {
        fs::write(&path, "Techniques=\nTechniqueSorting=\n")?;
    }
    Ok(path)
}

// ---------------------------------------------------------------------------
// Installing a downloaded or dropped preset
// ---------------------------------------------------------------------------

/// The `installSubpath` that routes an install here instead of into the mods tree.
///
/// Every other kind of content lands under `<mods_path>/mods/<something>`; a ReShade preset
/// is the one thing that belongs in the *install* dir instead. It carries no `mods/` prefix
/// precisely so it can never be mistaken for one of those.
pub const SUBPATH: &str = "reshade";

pub fn is_reshade_subpath(subpath: &str) -> bool {
    subpath.trim().eq_ignore_ascii_case(SUBPATH)
}

/// What an install put where, so the UI can say more than "done".
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub presets: Vec<String>,
    pub effects: usize,
    pub textures: usize,
}

/// Unpack an extracted preset download into the game folder.
///
/// Community preset archives are not uniform — some are a bare `.ini`, some carry the effects
/// they depend on, some bundle screenshots and a readme, and a few include ReShade itself. So
/// files are routed by what they are rather than by where the archive happened to put them:
///
/// - a preset `.ini` → the app's library folder
/// - `.fx` / `.fxh` → `reshade-shaders/Shaders`, keeping any folder structure below a
///   `Shaders` directory so a pack's internal includes still resolve
/// - images **inside a `Textures` folder** → `reshade-shaders/Textures`; images anywhere else
///   are the screenshots that sell the preset, and go nowhere
/// - everything else, including any bundled DLL and any bundled `ReShade.ini`, is ignored
///
/// Those last two exclusions are the important ones. Copying a bundled `ReShade.ini` would
/// overwrite the player's own keybinds and settings, and this app does not install ReShade
/// binaries at all — see the module header.
pub fn install_extracted(extracted: &Path, game_path: &str) -> anyhow::Result<Installed> {
    let game_dir = game_dir_of(game_path)?;
    let mut out = Installed::default();

    for src in walk_files(extracted) {
        let Some(name) = src.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        let ext = src
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        let dest = match ext.as_str() {
            "ini" if is_preset_file(&src) => {
                out.presets.push(preset_name(&src));
                library_dir(&game_dir).join(&name)
            }
            "fx" | "fxh" => {
                out.effects += 1;
                shaders_dir(&game_dir).join(under_shaders(extracted, &src))
            }
            "png" | "jpg" | "jpeg" | "bmp" | "dds" | "tga" if in_textures_folder(&src) => {
                out.textures += 1;
                resolve_child(&resolve_child(&game_dir, SHADER_DIR), "Textures").join(&name)
            }
            _ => continue,
        };

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&src, &dest)?;
    }

    if out.presets.is_empty() && out.effects == 0 {
        anyhow::bail!("no ReShade preset found in the download");
    }
    out.presets.sort();
    Ok(out)
}

/// A `.fx`'s path relative to the `Shaders` folder inside the archive, so a pack that nests
/// its includes keeps that shape. Falls back to the bare file name.
fn under_shaders(root: &Path, file: &Path) -> PathBuf {
    let rel = file.strip_prefix(root).unwrap_or(file);
    let mut comps = rel.components().peekable();
    let mut after: Option<PathBuf> = None;
    while let Some(c) = comps.next() {
        if c.as_os_str().to_string_lossy().eq_ignore_ascii_case("Shaders") {
            after = Some(comps.map(|c| c.as_os_str()).collect());
            break;
        }
    }
    after
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from(file.file_name().unwrap_or_default()))
}

/// Is this image a lookup texture the preset needs, rather than a screenshot of it?
fn in_textures_folder(file: &Path) -> bool {
    file.parent()
        .and_then(|p| p.file_name())
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("Textures"))
}

fn walk_files(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
        if depth > 8 {
            return;
        }
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out, depth + 1);
            } else {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out, 0);
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(tmp: &Path) -> String {
        tmp.to_string_lossy().into_owned()
    }

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("frost-reshade-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// A folder that looks like a ReShade-equipped MX Bikes install.
    fn fixture(name: &str, marker_dll: &str) -> PathBuf {
        let dir = tmp(name);
        let mut dll = b"MZ\x90\x00 fake pe ".to_vec();
        dll.extend_from_slice(b"ReShade 6.8.0 ");
        dll.extend_from_slice(&[0u8; 64]);
        fs::write(dir.join(marker_dll), dll).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    const PRESET: &str = "Techniques=Clarity@Clarity.fx,Vibrance@Vibrance.fx\n\
                          TechniqueSorting=Clarity@Clarity.fx,Vibrance@Vibrance.fx\n\
                          \n[Clarity.fx]\nClarityStrength=0.400000\n";


    /// `ReShade.ini` is an `.ini` in the same folder as the presets and must never be offered
    /// as one — applying it would point ReShade at its own config.
    #[test]
    fn config_is_never_a_preset() {
        let dir = fixture("cfg-not-preset", OPENGL_DLL);
        write(&dir.join(CONFIG), "[GENERAL]\nPresetPath=.\\A.ini\n");
        write(&dir.join("A.ini"), PRESET);

        let s = status(&game(&dir));
        assert_eq!(
            s.presets.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            vec!["A"],
        );
    }

    /// The whole point of reusing `IniDoc`: the player's keybinds and overlay settings have to
    /// come back byte-identical, because we only ever meant to change one key.
    #[test]
    fn applying_touches_only_preset_path() {
        let dir = fixture("only-presetpath", OPENGL_DLL);
        let before = "[GENERAL]\r\n\
                      EffectSearchPaths=.\\reshade-shaders\\Shaders\\**\r\n\
                      PresetPath=.\\Old.ini\r\n\
                      \r\n\
                      [INPUT]\r\n\
                      KeyOverlay=36,0,0,0\r\n\
                      KeyEffects=145,0,0,0\r\n";
        write(&dir.join(CONFIG), before);
        write(&dir.join(PRESET_DIR).join("Realistic MXB.ini"), PRESET);

        apply(&game(&dir), "Realistic MXB").unwrap();

        let after = fs::read_to_string(dir.join(CONFIG)).unwrap();
        assert_eq!(
            after,
            before.replace(
                "PresetPath=.\\Old.ini",
                "PresetPath=.\\FrostMod ReShade\\Realistic MXB.ini",
            ),
            "only PresetPath may change — CRLF, key order and [INPUT] included",
        );
    }

    /// A preset picked in the app has to come back as the active one, which only works if
    /// what `apply` writes is what `status` reads.
    #[test]
    fn apply_then_status_round_trips() {
        let dir = fixture("round-trip", OPENGL_DLL);
        write(&dir.join(PRESET_DIR).join("Cinematic.ini"), PRESET);
        write(&dir.join("Loose.ini"), PRESET);

        apply(&game(&dir), "Cinematic").unwrap();
        assert_eq!(status(&game(&dir)).active, "Cinematic");

        apply(&game(&dir), "Loose").unwrap();
        let s = status(&game(&dir));
        assert_eq!(s.active, "Loose");
        assert!(
            !s.presets.iter().find(|p| p.name == "Loose").unwrap().managed,
            "a preset found loose in the game folder isn't ours to manage",
        );
    }

    /// Off is a real preset file with no techniques, authored on demand.
    #[test]
    fn off_runs_nothing() {
        let dir = fixture("off", OPENGL_DLL);
        apply(&game(&dir), OFF).unwrap();

        let off = fs::read_to_string(dir.join(PRESET_DIR).join(OFF_FILE)).unwrap();
        assert!(techniques_used(&off).is_empty());
        assert_eq!(status(&game(&dir)).active, OFF);
    }

    /// The silent failure this feature exists to make loud: a preset asking for an effect
    /// that isn't installed renders as if nothing happened.
    #[test]
    fn missing_effects_are_named() {
        let dir = fixture("missing-fx", OPENGL_DLL);
        write(&dir.join(PRESET_DIR).join("Half.ini"), PRESET);
        write(
            &dir.join(SHADER_DIR)
                .join("Shaders")
                .join("pack")
                .join("Clarity.fx"),
            "technique Clarity\n{\n}\n",
        );

        let s = status(&game(&dir));
        let p = s.presets.iter().find(|p| p.name == "Half").unwrap();
        assert!(s.has_shaders);
        assert_eq!(
            p.missing_effects,
            vec!["Vibrance.fx"],
            "Clarity.fx nests a folder deep and is installed",
        );
    }

    /// The form real presets are actually written in: technique names with no `@file.fx`.
    /// These resolve against the `technique <Name>` lines inside the installed effects, so
    /// without that lookup the warning could never fire on a real preset at all.
    #[test]
    fn bare_technique_names_resolve_against_installed_effects() {
        let dir = fixture("bare-techniques", OPENGL_DLL);
        write(
            &dir.join(PRESET_DIR).join("Real.ini"),
            "PreprocessorDefinitions=\n\
             Techniques=MagicBloom,Vibrance,HDR\n\
             TechniqueSorting=MagicBloom,Vibrance,HDR,Clarity,SMAA,Cartoon\n\
             \n[MagicBloom.fx]\nfMagicBloom_Intensity=0.500000\n",
        );
        let shaders = dir.join(SHADER_DIR).join("Shaders");
        write(
            &shaders.join("MagicBloom.fx"),
            "technique MagicBloom < ui_tooltip = \"Bloom\"; >\n{\n}\n",
        );
        write(&shaders.join("Misc.fx"), "technique Vibrance\n{\n}\n");

        let p = &status(&game(&dir)).presets[0];
        assert_eq!(
            p.missing_effects,
            vec!["HDR"],
            "MagicBloom and Vibrance are declared by installed effects; HDR is not",
        );
    }

    /// `TechniqueSorting` lists every technique ReShade knows, not the enabled ones — real
    /// presets carry several hundred entries there. Reading it would report almost every
    /// preset as broken.
    #[test]
    fn technique_sorting_is_not_a_list_of_requirements() {
        let sorting = (0..300)
            .map(|i| format!("Effect{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let text = format!("Techniques=OnlyThis\nTechniqueSorting={sorting}\n");

        let used = techniques_used(&text);
        assert_eq!(used.len(), 1);
        assert_eq!(used[0].0, "OnlyThis");
    }

    /// A `.fx` that merely mentions the word must not be read as declaring a technique.
    #[test]
    fn only_real_technique_declarations_count() {
        let fx = "// techniques are great\n\
                  uniform float techniqueScale;\n\
                  technique  Spaced_Out < ui = 1; >\n\
                  technique Plain\n";
        assert_eq!(declared_techniques(fx), vec!["Spaced_Out", "Plain"]);
    }

    /// No shaders at all is one message about the install, not a wall of warnings on every
    /// preset.
    #[test]
    fn no_shader_folder_reports_once() {
        let dir = fixture("no-shaders", OPENGL_DLL);
        write(&dir.join(PRESET_DIR).join("Half.ini"), PRESET);

        let s = status(&game(&dir));
        assert!(!s.has_shaders);
        assert!(s.presets[0].missing_effects.is_empty());
    }

    /// MX Bikes is OpenGL. ReShade installed for DirectX loads for nothing, and saying so is
    /// the whole difference between a fixable mistake and a mystery.
    #[test]
    fn directx_install_is_reported_as_wrong_api() {
        let dir = fixture("wrong-api", "dxgi.dll");
        let s = status(&game(&dir));
        assert!(!s.installed);
        assert_eq!(s.wrong_api.as_deref(), Some("dxgi.dll"));
    }

    /// A correct install with a leftover DirectX one beside it is working — don't nag.
    #[test]
    fn a_working_install_is_never_called_wrong_api() {
        let dir = fixture("both-apis", OPENGL_DLL);
        write(&dir.join("dxgi.dll"), "MZ ReShade 6.8.0");
        let s = status(&game(&dir));
        assert!(s.installed);
        assert!(s.wrong_api.is_none());
    }

    /// An unrelated `opengl32.dll` — a wrapper, an overlay shim — is not ReShade.
    #[test]
    fn other_opengl_wrappers_are_not_reshade() {
        let dir = tmp("other-wrapper");
        write(&dir.join(OPENGL_DLL), "MZ some other wrapper");
        let s = status(&game(&dir));
        assert!(!s.installed);
        assert!(s.wrong_api.is_none());
    }

    #[test]
    fn version_comes_off_the_dll() {
        let dir = fixture("version", OPENGL_DLL);
        assert_eq!(status(&game(&dir)).version.as_deref(), Some("6.8.0"));
    }

    /// The version resource stores its strings as UTF-16, which is where a real DLL carries
    /// this — the plain-bytes scan alone would miss it.
    #[test]
    fn version_is_found_in_utf16_too() {
        let dir = tmp("version-utf16");
        let mut dll = b"MZ\x90\x00".to_vec();
        for b in b"ReShade 6.8.0" {
            dll.push(*b);
            dll.push(0);
        }
        fs::write(dir.join(OPENGL_DLL), dll).unwrap();
        assert_eq!(status(&game(&dir)).version.as_deref(), Some("6.8.0"));
    }

    /// Relative-with-backslashes is the one form that survives Proton, where the game reads
    /// this file as a Windows program.
    #[test]
    fn preset_path_is_written_relative() {
        let dir = Path::new("/games/MX Bikes");
        assert_eq!(
            rel_preset_path(dir, &dir.join(PRESET_DIR).join("A.ini")),
            ".\\FrostMod ReShade\\A.ini",
        );
        assert_eq!(rel_preset_path(dir, &dir.join("A.ini")), ".\\A.ini");
    }

    /// Whatever ReShade or a human wrote in there has to resolve back to the same file.
    #[test]
    fn preset_path_reads_back_in_any_form() {
        let dir = fixture("path-forms", OPENGL_DLL);
        let target = dir.join(PRESET_DIR).join("A.ini");
        write(&target, PRESET);

        for raw in [
            ".\\FrostMod ReShade\\A.ini",
            "./FrostMod ReShade/A.ini",
            "FrostMod ReShade\\A.ini",
            "\"FrostMod ReShade\\A.ini\"",
        ] {
            let got = resolve_preset_path(&dir, raw).unwrap();
            assert!(same_file(&got, &target), "{raw} should resolve to the preset");
        }
    }

    /// A name from the frontend is joined onto a path, so it has to stay inside the folder.
    #[test]
    fn preset_names_cant_escape_the_folder() {
        let dir = fixture("traversal", OPENGL_DLL);
        for bad in ["../../evil", "..\\evil", "C:\\evil", ".", ".."] {
            assert!(apply(&game(&dir), bad).is_err(), "{bad} must be refused");
            assert!(delete(&game(&dir), bad).is_err(), "{bad} must be refused");
        }
    }

    /// Deleting what the game is currently rendering with must leave a valid state behind.
    #[test]
    fn deleting_the_active_preset_falls_back_to_off() {
        let dir = fixture("delete-active", OPENGL_DLL);
        write(&dir.join(PRESET_DIR).join("Gone.ini"), PRESET);
        apply(&game(&dir), "Gone").unwrap();

        delete(&game(&dir), "Gone").unwrap();
        assert_eq!(status(&game(&dir)).active, OFF);
    }

    /// Someone else's file in the game root is listed, but never deleted by us.
    #[test]
    fn loose_presets_are_not_deletable() {
        let dir = fixture("loose-keep", OPENGL_DLL);
        let loose = dir.join("Theirs.ini");
        write(&loose, PRESET);

        assert!(delete(&game(&dir), "Theirs").is_err());
        assert!(loose.is_file());
    }

    /// Applying before ReShade exists would write a config for something that can't read it.
    #[test]
    fn apply_needs_reshade_installed() {
        let dir = tmp("no-reshade");
        write(&dir.join(PRESET_DIR).join("A.ini"), PRESET);
        assert!(apply(&game(&dir), "A").is_err());
        assert!(!dir.join(CONFIG).exists());
    }

    /// A config ReShade hasn't written yet needs the search paths, or it starts with nowhere
    /// to look for the effects the preset names.
    #[test]
    fn fresh_config_gets_search_paths() {
        let dir = fixture("fresh-config", OPENGL_DLL);
        write(&dir.join(PRESET_DIR).join("A.ini"), PRESET);
        apply(&game(&dir), "A").unwrap();

        let text = fs::read_to_string(dir.join(CONFIG)).unwrap();
        let doc = IniDoc::parse(&text);
        assert_eq!(
            doc.get("GENERAL", "EffectSearchPaths").as_deref(),
            Some(".\\reshade-shaders\\Shaders\\**"),
        );
        assert_eq!(
            doc.get("GENERAL", "PresetPath").as_deref(),
            Some(".\\FrostMod ReShade\\A.ini"),
        );
    }

    /// The shape a real preset download arrives in: the preset, the effects it needs, its
    /// lookup textures, and the screenshots and readme that sell it.
    #[test]
    fn install_sorts_an_archive_by_what_the_files_are() {
        let dir = fixture("install-archive", OPENGL_DLL);
        let src = tmp("install-archive-src");
        write(&src.join("Realistic MXB.ini"), PRESET);
        write(&src.join("Shaders").join("qUINT").join("Clarity.fx"), "// fx");
        write(&src.join("Shaders").join("Clarity.fxh"), "// include");
        write(&src.join("Textures").join("lut.png"), "texture bytes");
        write(&src.join("screenshot.png"), "a picture of the preset");
        write(&src.join("readme.txt"), "install instructions");

        let out = install_extracted(&src, &game(&dir)).unwrap();

        assert_eq!(out.presets, vec!["Realistic MXB"]);
        assert_eq!(out.effects, 2);
        assert_eq!(out.textures, 1, "the screenshot is not a texture");
        assert!(dir.join(PRESET_DIR).join("Realistic MXB.ini").is_file());
        assert!(
            dir.join(SHADER_DIR)
                .join("Shaders")
                .join("qUINT")
                .join("Clarity.fx")
                .is_file(),
            "a pack's folder structure has to survive or its includes stop resolving",
        );
        assert!(dir.join(SHADER_DIR).join("Textures").join("lut.png").is_file());
        assert!(
            !dir.join(SHADER_DIR).join("Textures").join("screenshot.png").exists(),
            "an image outside Textures/ is a screenshot",
        );
    }

    /// Some archives bundle ReShade itself, config and all. Copying that `ReShade.ini` would
    /// wipe the player's keybinds and settings, and this app installs no ReShade binaries.
    #[test]
    fn install_never_overwrites_the_config_or_lays_down_binaries() {
        let dir = fixture("install-safety", OPENGL_DLL);
        let mine = "[INPUT]\nKeyOverlay=36,0,0,0\n";
        write(&dir.join(CONFIG), mine);

        let src = tmp("install-safety-src");
        write(&src.join("A.ini"), PRESET);
        write(&src.join(CONFIG), "[INPUT]\nKeyOverlay=999,0,0,0\n");
        write(&src.join(OPENGL_DLL), "MZ ReShade 6.8.0");

        install_extracted(&src, &game(&dir)).unwrap();

        assert_eq!(
            fs::read_to_string(dir.join(CONFIG)).unwrap(),
            mine,
            "the player's own ReShade.ini must be untouched",
        );
        assert!(
            !dir.join(PRESET_DIR).join(CONFIG).exists(),
            "ReShade.ini is not a preset and must not be filed as one",
        );
    }

    /// An archive with nothing of ours in it should say so rather than report success.
    #[test]
    fn install_rejects_an_archive_with_no_preset() {
        let dir = fixture("install-empty", OPENGL_DLL);
        let src = tmp("install-empty-src");
        write(&src.join("readme.txt"), "no preset here");
        assert!(install_extracted(&src, &game(&dir)).is_err());
    }

    /// Run the classifier over real preset files rather than the tidy ones above.
    ///
    /// `MXB_REAL_PRESET=<file-or-dir> cargo test real_presets -- --ignored --nocapture`
    ///
    /// Worth doing before believing anything about how presets are written: the hand-made
    /// fixtures here all name their effects `Name@File.fx`, and the presets people actually
    /// share mostly use bare technique names.
    #[test]
    #[ignore]
    fn real_presets_are_recognised() {
        let Ok(arg) = std::env::var("MXB_REAL_PRESET") else {
            eprintln!("set MXB_REAL_PRESET to a preset .ini or a folder of them");
            return;
        };
        let root = PathBuf::from(&arg);
        let files: Vec<PathBuf> = if root.is_dir() {
            walk_files(&root)
        } else {
            vec![root]
        };

        let mut seen = 0;
        for f in files.iter().filter(|f| {
            f.extension().is_some_and(|e| e.eq_ignore_ascii_case("ini"))
        }) {
            let text = fs::read_to_string(f).unwrap_or_default();
            let used = techniques_used(&text);
            eprintln!(
                "{}: preset={} techniques={} named_by_file={}",
                f.file_name().unwrap_or_default().to_string_lossy(),
                is_preset_file(f),
                used.len(),
                used.iter().filter(|(_, file)| file.is_some()).count(),
            );
            if is_preset_file(f) {
                seen += 1;
                assert!(
                    !used.is_empty(),
                    "{} is a preset but enables nothing",
                    f.display(),
                );
            }
        }
        assert!(seen > 0, "no presets found under {arg}");
    }

    /// No game folder set is the one state where every answer is "don't know" — it must not
    /// look like "ReShade isn't installed", which would send the user off to install it again.
    #[test]
    fn unset_game_folder_is_empty_not_negative() {
        let s = status("   ");
        assert!(s.game_dir.is_empty());
        assert!(!s.folder_missing);
        assert!(s.presets.is_empty());
    }

    /// A folder that was picked and has since moved is a different problem from one that was
    /// never picked — and it used to come back as the latter, with the path thrown away.
    #[test]
    fn a_configured_folder_that_is_gone_is_named() {
        let dir = tmp("folder-gone");
        let path = dir.join("moved-away");
        let s = status(&path.to_string_lossy());
        assert!(s.folder_missing);
        assert_eq!(s.game_dir, path.to_string_lossy());
        assert!(!s.installed);
    }

    /// The whole point of the override: presets are read from wherever the player pointed us,
    /// not from the game's install dir.
    #[test]
    fn presets_come_from_whichever_folder_it_is_given() {
        let install = fixture("override-install", OPENGL_DLL);
        let elsewhere = fixture("override-elsewhere", OPENGL_DLL);
        write(&install.join(PRESET_DIR).join("FromInstall.ini"), PRESET);
        write(&elsewhere.join(PRESET_DIR).join("FromElsewhere.ini"), PRESET);

        let names = |dir: &Path| {
            status(&game(dir))
                .presets
                .into_iter()
                .map(|p| p.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&install), vec!["FromInstall"]);
        assert_eq!(names(&elsewhere), vec!["FromElsewhere"]);

        // And applying writes the config next to the ReShade that will read it.
        apply(&game(&elsewhere), "FromElsewhere").unwrap();
        assert!(elsewhere.join(CONFIG).is_file());
        assert!(!install.join(CONFIG).exists());
    }
}
