//! The one-click graphics presets: "MXB Clean" for racing and "MXB Photo" for screenshots.
//!
//! [`crate::reshade`] manages a ReShade the player installed themselves. This module goes one
//! step further: it puts ReShade in place, with a small, fixed set of effects and two presets
//! tuned for MX Bikes, and takes all of it away again on "Off" — putting back whatever the
//! player had before.
//!
//! ## Nothing of ReShade's ships in this app
//!
//! reshade.me asks that its binaries not be mirrored, so the app carries none. On install it
//! downloads the official `ReShade_Setup_<version>.exe` from reshade.me — the same file a
//! player would — checks it against a pinned SHA-256, and takes `ReShade64.dll` out of the
//! zip that installer carries (checked against its own pinned digest). The effects come the
//! same way: raw files from the repositories ReShade's own installer pulls from, each at a
//! pinned commit and SHA-256. A digest that doesn't match stops the install before anything
//! in the game folder is touched.
//!
//! Only lightweight colour, sharpening, anti-aliasing and bloom effects are used. No ambient
//! occlusion, global illumination, ray tracing or other depth-buffer work, and no add-on
//! build: those are what sat under the ReShade crashes players have reported. The one
//! exception is MXB Photo's depth of field (CinematicDOF), and it only loads while MXB Photo
//! is the active preset (`SkipLoadingDisabledEffects=1`).
//!
//! ## Photo mode
//!
//! In game, F9 flips between MXB Clean and MXB Photo, F6 turns the depth of field on and off,
//! and F11 saves a PNG into `<game>/MXB Photos`. All three are ReShade's own keys, written into
//! `ReShade.ini` and the Photo preset; see [`KEY_PHOTO`] for why these keys.
//!
//! ## On disk
//!
//! ```text
//! <game>/
//!   opengl32.dll                     ReShade 6.8.0 (64-bit), from reshade.me
//!   ReShade.ini                      written here: search paths, preset, keys, screenshots
//!   MXB Photos/                      screenshots; made by ReShade, kept on Off
//!   FrostMod ReShade/                the ReShade library (the player's presets live here)
//!     MXB Pack/
//!       install.json                 the record uninstall works from
//!       Presets/MXB Clean.ini        the two presets (authored here, see PRESET_CLEAN)
//!       Presets/MXB Photo.ini
//!       Shaders/*.fx, *.fxh          only the effects the presets use
//!       Textures/                    SMAA's lookup tables, CinematicDOF's noise and shapes
//!       backup/                      whatever was in the way, moved aside untouched
//! ```
//!
//! The effects live in their own folder rather than `reshade-shaders/`, so a player's own
//! shader collection is never written into, and `ReShade.ini` points at that folder alone.
//!
//! ## Backup and restore
//!
//! Before anything is written, every file the install would replace — `opengl32.dll` (ReShade
//! or not), `opengl32.dll.off`, `ReShade.ini` and `ReShade.log` — is
//! *moved* into `MXB Pack/backup/`. Moving rather than copying means the restore puts back
//! the very same bytes. `install.json` is written first and lists what will be moved, so an
//! install interrupted half-way can still be undone. Turning the preset Off removes what this
//! module put down and moves the backup back.

use crate::library::resolve_child;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Pins
// ---------------------------------------------------------------------------

/// The ReShade release installed. The standard build, not the add-on one: add-ons are
/// exactly the extra surface this feature exists to avoid.
pub const RESHADE_VERSION: &str = "6.8.0";
const RESHADE_SETUP_URL: &str = "https://reshade.me/downloads/ReShade_Setup_6.8.0.exe";
const RESHADE_SETUP_SHA256: &str =
    "207aea16205fbf952bc8fe1879966672454cf04002e7ad34237c7990a5b3c0b4";
/// MX Bikes is a 64-bit OpenGL program, so this is the DLL it loads as `opengl32.dll`.
const RESHADE_DLL_IN_SETUP: &str = "ReShade64.dll";
const RESHADE_DLL_SHA256: &str =
    "b2945c29e7095491a901746b400e58db9b1592ab092bacf2a888ce37f02d08da";

/// One effect file, from where ReShade's own installer gets it, at a fixed commit.
struct Pinned {
    /// Where it goes, relative to [`PACK_DIR`].
    dest: &'static str,
    url: &'static str,
    sha256: &'static str,
}

/// crosire/reshade-shaders `slim` — ReShade's standard headers (ReShade.fxh is CC0-1.0).
macro_rules! slim {
    ($f:literal) => {
        concat!(
            "https://raw.githubusercontent.com/crosire/reshade-shaders/",
            "fd0022170615ce0d8162d219bff07232fa6dd84f/Shaders/",
            $f
        )
    };
}
/// CeeJayDK/SweetFX — MIT. CAS.fx (AMD, MIT) and SMAA (Jimenez et al., MIT) ship in it too.
macro_rules! sweetfx {
    ($dir:literal, $f:literal) => {
        concat!(
            "https://raw.githubusercontent.com/CeeJayDK/SweetFX/",
            "93ddf39b357f5da534ed6d34ba4ec8cc7dcfa361/",
            $dir,
            $f
        )
    };
}
/// FransBouma/OtisFX — CinematicDOF by Frans Bouma (Otis_Inf), BSD-2-Clause in its header,
/// MIT for the repository.
macro_rules! otisfx {
    ($f:literal) => {
        concat!(
            "https://raw.githubusercontent.com/FransBouma/OtisFX/",
            "193aa0bf07ee82fbd5f142b5b813b19c5d169745/",
            $f
        )
    };
}
/// crosire/reshade-shaders `legacy` — MagicBloom by luluco250, MIT.
macro_rules! legacy {
    ($f:literal) => {
        concat!(
            "https://raw.githubusercontent.com/crosire/reshade-shaders/",
            "bcb5ba54199f4455026dd8ba66dc1b74461d3152/Shaders/",
            $f
        )
    };
}

const FILES: &[Pinned] = &[
    Pinned {
        dest: "Shaders/ReShade.fxh",
        url: slim!("ReShade.fxh"),
        sha256: "6dabfbbaf968c3871905d2ea17f96572ff7b1cec01310b5d0e5252b66b30174f",
    },
    Pinned {
        dest: "Shaders/ReShadeUI.fxh",
        url: slim!("ReShadeUI.fxh"),
        sha256: "78adf672df47460297eb9fe6dd238d2aafa24510b52b84feb1a745dff70eb901",
    },
    Pinned {
        dest: "Shaders/CAS.fx",
        url: sweetfx!("Shaders/SweetFX/", "CAS.fx"),
        sha256: "5cbf1468b75d0d221067bdc7bca658f531816c3b24e0eef8654d9d2a7d1a1c46",
    },
    Pinned {
        dest: "Shaders/SMAA.fx",
        url: sweetfx!("Shaders/SweetFX/", "SMAA.fx"),
        sha256: "fd6591eeb70ff584ac4d774ed3c79af5efac34f5de1a36f8f873e959259c0007",
    },
    Pinned {
        dest: "Shaders/SMAA.fxh",
        url: sweetfx!("Shaders/SweetFX/", "SMAA.fxh"),
        sha256: "ee18b0f376585338cdb477a0900757cebac86d3c476ef99955348b55818c8310",
    },
    Pinned {
        dest: "Shaders/Tonemap.fx",
        url: sweetfx!("Shaders/SweetFX/", "Tonemap.fx"),
        sha256: "bbe2e7eb8d6f9fb4316bad8c85b5bfd1d98f0903d2cd962fd5d4c0ebbde3c5f3",
    },
    Pinned {
        dest: "Shaders/Curves.fx",
        url: sweetfx!("Shaders/SweetFX/", "Curves.fx"),
        sha256: "8368029d2254856505abf7dd789342024465403da49434dd776c6d1af4079a98",
    },
    Pinned {
        dest: "Shaders/LiftGammaGain.fx",
        url: sweetfx!("Shaders/SweetFX/", "LiftGammaGain.fx"),
        sha256: "5a6b6c85d0e09ed4c507f3b5f1b3b2f4f0f1268e24bb0ec81edbd431e28566df",
    },
    Pinned {
        dest: "Shaders/Vibrance.fx",
        url: sweetfx!("Shaders/SweetFX/", "Vibrance.fx"),
        sha256: "b9189a28ca4a645a0e188f8396c81889d217e3c706e8900dfe6594d43e7c33eb",
    },
    Pinned {
        dest: "Shaders/Vignette.fx",
        url: sweetfx!("Shaders/SweetFX/", "Vignette.fx"),
        sha256: "a7358b592830fa74a0a50a842682c99db751e35666e49c229567daf9ea59afa2",
    },
    Pinned {
        dest: "Shaders/MagicBloom.fx",
        url: legacy!("MagicBloom.fx"),
        sha256: "00db2d3822b7c1a03e4bf2bb6499cd5bc1636c63cf072edbb84c445715e8ab8d",
    },
    Pinned {
        dest: "Shaders/CinematicDOF.fx",
        url: otisfx!("Shaders/CinematicDOF.fx"),
        sha256: "742f65b37c015ceb54cb8abaf522055203885ca3838a0cae0a13a25d30af05d2",
    },
    Pinned {
        dest: "Textures/AreaTex.png",
        url: sweetfx!("Textures/SweetFX/", "AreaTex.png"),
        sha256: "0044e6b420a8d98a9dc6a60bedbd43f75cd15ac6052fdedfb2f2f55d3077beed",
    },
    Pinned {
        dest: "Textures/SearchTex.png",
        url: sweetfx!("Textures/SweetFX/", "SearchTex.png"),
        sha256: "55a97853ab64174245701accf315ab63d41776c9cb2aa14c675e653684fd96f3",
    },
    // CinematicDOF's noise, and the six bokeh shapes it compiles in on OpenGL
    // (`__RENDERER__ >= 0xa000`) even though MXB Photo uses the plain circle.
    Pinned {
        dest: "Textures/monochrome_gaussnoise.png",
        url: otisfx!("Textures/monochrome_gaussnoise.png"),
        sha256: "70f200a55be9f83829b26a1420470c53c595c959bdfff6e501dcdf433e4f6ebd",
    },
    Pinned {
        dest: "Textures/bokehshapes/moyheart.png",
        url: otisfx!("Textures/bokehshapes/moyheart.png"),
        sha256: "3bf9233d822b3c4e08017ed0e5a07a8b97ad0805bedb9aa9494da0b06e23173b",
    },
    Pinned {
        dest: "Textures/bokehshapes/hex.png",
        url: otisfx!("Textures/bokehshapes/hex.png"),
        sha256: "619fc48ba05c59a509652503f88890509076b1f9588741ecc62fa8e617c00ce2",
    },
    Pinned {
        dest: "Textures/bokehshapes/fringy_soft_chr_rb.png",
        url: otisfx!("Textures/bokehshapes/fringy_soft_chr_rb.png"),
        sha256: "0bbb4128258348202248bdcb4d157faeb727a5469368a0308b50bc570c4e63e5",
    },
    Pinned {
        dest: "Textures/bokehshapes/hex_fringy_soft.png",
        url: otisfx!("Textures/bokehshapes/hex_fringy_soft.png"),
        sha256: "60156fb14e8139b9ea1399431b28c4d8c33d487a4ba0cd2b6f96410027418290",
    },
    Pinned {
        dest: "Textures/bokehshapes/cutestar.png",
        url: otisfx!("Textures/bokehshapes/cutestar.png"),
        sha256: "bc754b73614115ddd600e61931c1e949d6300d4260ccf9f29ea5c71faa8c9223",
    },
    Pinned {
        dest: "Textures/bokehshapes/square.png",
        url: otisfx!("Textures/bokehshapes/square.png"),
        sha256: "4db68f41531322b98998d67c1aec47dee00338a65a8c82b80a8ea946340ed97e",
    },
    Pinned {
        dest: "LICENSE-OtisFX.txt",
        url: otisfx!("LICENSE"),
        sha256: "b4199c61fd8d700762cc7dfd777d7b0df3fef58e155aaef160c8b239e9e361ef",
    },
    Pinned {
        dest: "LICENSE-SweetFX.txt",
        url: sweetfx!("", "LICENSE"),
        sha256: "9b2e0b3ff53493f211e39390947716cd16ee559303dac0713ad4b12228dbce51",
    },
];

// ---------------------------------------------------------------------------
// Presets — authored here
// ---------------------------------------------------------------------------
//
// Order matters: anti-aliasing on the raw frame, then (Photo only) depth of field, then bloom,
// then the grade, sharpening last so it isn't softened again. Preprocessor switches turn off
// MagicBloom's lens dirt (a 570 KB texture) and its eye adaptation (an extra render target and
// a readback).
//
// The RESHADE_DEPTH_* switches tell ReShade.fxh how to read MX Bikes' depth buffer, which only
// the depth of field uses:
// - not reversed: the game draws with glDepthFunc LEQUAL (FrostMod's depth probe logs
//   `depth=1`), into a 24-bit buffer, so not logarithmic either;
// - not upside down: ReShade's OpenGL path flips every copy out of the default framebuffer
//   (`flip_y` in its opengl_impl_command_list.cpp), so effects get depth top-down like colour;
// - far plane 10000: ReShade.fxh linearises as if the near plane were 1, so this has to be
//   far/near. MX Bikes' draw distance is 1000 m; its near plane isn't known, ~0.1 m is assumed.
//   That puts CinematicDOF's depth in about metres. Check it in game with the effect's
//   "Show CoC values".

/// For racing: barely-there grade, light sharpening, AA, a whisper of bloom.
const PRESET_CLEAN: &str = "\
PreprocessorDefinitions=MAGICBLOOM_NODIRT=1,MAGICBLOOM_NOADAPT=1,RESHADE_DEPTH_INPUT_IS_REVERSED=0,RESHADE_DEPTH_INPUT_IS_UPSIDE_DOWN=0,RESHADE_DEPTH_INPUT_IS_LOGARITHMIC=0,RESHADE_DEPTH_LINEARIZATION_FAR_PLANE=10000.0
Techniques=SMAA@SMAA.fx,MagicBloom@MagicBloom.fx,Tonemap@Tonemap.fx,Curves@Curves.fx,LiftGammaGain@LiftGammaGain.fx,Vibrance@Vibrance.fx,ContrastAdaptiveSharpen@CAS.fx
TechniqueSorting=SMAA@SMAA.fx,MagicBloom@MagicBloom.fx,Tonemap@Tonemap.fx,Curves@Curves.fx,LiftGammaGain@LiftGammaGain.fx,Vibrance@Vibrance.fx,ContrastAdaptiveSharpen@CAS.fx

[CAS.fx]
Contrast=0.000000
Sharpening=0.400000

[Curves.fx]
Contrast=0.150000
Formula=4
Mode=0

[LiftGammaGain.fx]
RGB_Gain=1.020000,1.000000,0.975000
RGB_Gamma=1.000000,1.000000,1.000000
RGB_Lift=0.985000,0.985000,0.985000

[MagicBloom.fx]
fBloom_Intensity=0.050000
fBloom_Threshold=6.000000
iDebug=0

[SMAA.fx]
CornerRounding=25
DebugOutput=0
EdgeDetectionThreshold=0.100000
EdgeDetectionType=1
MaxSearchSteps=32
MaxSearchStepsDiagonal=16
PredicationEnabled=0

[Tonemap.fx]
Bleach=0.000000
Defog=0.000000
Exposure=0.000000
FogColor=0.000000,0.000000,1.000000
Gamma=1.000000
Saturation=0.100000

[Vibrance.fx]
Vibrance=0.200000
VibranceRGBBalance=1.000000,1.000000,1.000000
Vibrance_Luma=0
";

/// For screenshots: depth of field, the same chain pushed further, plus a vignette. Not meant
/// for racing. CinematicDOF auto-focuses on the screen centre (not the mouse, which sits
/// wherever the replay controls left it) and its click overlay is off so it can't end up in a
/// shot. F6 toggles it (`Key` + technique@file is how ReShade 6 stores a technique's key).
const PRESET_PHOTO: &str = "\
PreprocessorDefinitions=MAGICBLOOM_NODIRT=1,MAGICBLOOM_NOADAPT=1,RESHADE_DEPTH_INPUT_IS_REVERSED=0,RESHADE_DEPTH_INPUT_IS_UPSIDE_DOWN=0,RESHADE_DEPTH_INPUT_IS_LOGARITHMIC=0,RESHADE_DEPTH_LINEARIZATION_FAR_PLANE=10000.0
Techniques=SMAA@SMAA.fx,CinematicDOF@CinematicDOF.fx,MagicBloom@MagicBloom.fx,Tonemap@Tonemap.fx,Curves@Curves.fx,LiftGammaGain@LiftGammaGain.fx,Vibrance@Vibrance.fx,Vignette@Vignette.fx,ContrastAdaptiveSharpen@CAS.fx
TechniqueSorting=SMAA@SMAA.fx,CinematicDOF@CinematicDOF.fx,MagicBloom@MagicBloom.fx,Tonemap@Tonemap.fx,Curves@Curves.fx,LiftGammaGain@LiftGammaGain.fx,Vibrance@Vibrance.fx,Vignette@Vignette.fx,ContrastAdaptiveSharpen@CAS.fx
KeyCinematicDOF@CinematicDOF.fx=117,0,0,0

[CAS.fx]
Contrast=0.200000
Sharpening=0.750000

[CinematicDOF.fx]
AutoFocusPoint=0.500000,0.500000
AutoFocusTransitionSpeed=0.200000
BlurQuality=8.000000
BokehBusyFactor=0.500000
FarPlaneMaxBlur=1.000000
FNumber=2.800000
FocalLength=120.000000
HighlightBoost=0.500000
HighlightGammaFactor=2.200000
HighlightShape=0
ManualFocusPlane=10.000000
MitigateUndersampling=1
NearFarDistanceCompensation=1.000000
NearPlaneMaxBlur=0.600000
PostBlurSmoothing=0.000000
ShowCoCValues=0
ShowOutOfFocusPlaneOnMouseDown=0
UseAutoFocus=1
UseMouseDrivenAutoFocus=0

[Curves.fx]
Contrast=0.320000
Formula=4
Mode=0

[LiftGammaGain.fx]
RGB_Gain=1.030000,1.000000,0.970000
RGB_Gamma=1.000000,1.000000,1.000000
RGB_Lift=0.975000,0.975000,0.990000

[MagicBloom.fx]
fBloom_Intensity=0.100000
fBloom_Threshold=5.000000
iDebug=0

[SMAA.fx]
CornerRounding=25
DebugOutput=0
EdgeDetectionThreshold=0.080000
EdgeDetectionType=1
MaxSearchSteps=64
MaxSearchStepsDiagonal=20
PredicationEnabled=0

[Tonemap.fx]
Bleach=0.000000
Defog=0.000000
Exposure=0.000000
FogColor=0.000000,0.000000,1.000000
Gamma=1.020000
Saturation=0.150000

[Vibrance.fx]
Vibrance=0.300000
VibranceRGBBalance=1.000000,1.000000,1.000000
Vibrance_Luma=0

[Vignette.fx]
Amount=-0.350000
Center=0.500000,0.500000
Radius=1.800000
Ratio=1.000000
Slope=2
Type=0
";

/// Which preset is wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Choice {
    Off,
    Clean,
    Photo,
}

impl Choice {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "clean" => Some(Self::Clean),
            "photo" => Some(Self::Photo),
            _ => None,
        }
    }

    fn file(self) -> Option<&'static str> {
        match self {
            Self::Off => None,
            Self::Clean => Some("MXB Clean.ini"),
            Self::Photo => Some("MXB Photo.ini"),
        }
    }

    fn text(self) -> &'static str {
        match self {
            Self::Off => "",
            Self::Clean => PRESET_CLEAN,
            Self::Photo => PRESET_PHOTO,
        }
    }
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

const LIB_DIR: &str = crate::reshade::PRESET_DIR;
pub const PACK_DIR: &str = "MXB Pack";
/// Our two presets, in a folder of their own. ReShade's preset-switch key cycles through every
/// preset in a folder, and the library folder above also holds the player's own presets, so
/// ours live apart and one key flips between exactly these two.
const PRESETS_DIR: &str = "Presets";
/// Where ReShade saves screenshots, relative to the game folder. ReShade creates it on the
/// first shot; the install doesn't, and Off never removes it: those are the player's photos.
const PHOTOS_DIR: &str = "MXB Photos";
const MANIFEST: &str = "install.json";
/// The install layout. 0 (a record without the field) is the first test builds', with the
/// presets loose in the library folder; 2 has them in [`PRESETS_DIR`] plus the photo-mode keys.
const LAYOUT: u32 = 2;
const BACKUP_DIR: &str = "backup";
const OPENGL_DLL: &str = "opengl32.dll";
const OPENGL_DLL_OFF: &str = "opengl32.dll.off";
const CONFIG: &str = "ReShade.ini";
const LOG: &str = "ReShade.log";

/// Game-root files the install replaces, in the order they are moved aside.
const ROOT_FILES: [&str; 4] = [OPENGL_DLL, OPENGL_DLL_OFF, CONFIG, LOG];

fn lib_dir(game: &Path) -> PathBuf {
    resolve_child(game, LIB_DIR)
}

/// `<game>/FrostMod ReShade/MXB Pack` — public so [`crate::reshade`] can scan its shaders.
pub fn pack_dir(game: &Path) -> PathBuf {
    resolve_child(&lib_dir(game), PACK_DIR)
}

fn presets_dir(game: &Path) -> PathBuf {
    resolve_child(&pack_dir(game), PRESETS_DIR)
}

fn manifest_path(game: &Path) -> PathBuf {
    pack_dir(game).join(MANIFEST)
}

fn backup_dir(game: &Path) -> PathBuf {
    pack_dir(game).join(BACKUP_DIR)
}

/// `PresetPath` as written into `ReShade.ini`: relative with backslashes, the form that also
/// survives Proton (see `reshade::rel_preset_path`).
fn preset_path_value(choice: Choice) -> String {
    format!(".\\{LIB_DIR}\\{PACK_DIR}\\{PRESETS_DIR}\\{}", choice.file().unwrap_or_default())
}

/// Where the first test builds pointed `PresetPath`, before the presets had their own folder.
fn old_preset_path_value(choice: Choice) -> String {
    format!(".\\{LIB_DIR}\\{}", choice.file().unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Photo mode: keys and screenshots
// ---------------------------------------------------------------------------
//
// Keys are ReShade's `vk,ctrl,shift,alt`. MX Bikes binds F8 (VR reset), letters, the arrows,
// Ins/Del/PgUp/PgDn and Left Shift in replays, and F10 takes its own high-res shot; FrostMod
// uses F7 (hide its overlay), F8 (menu) and Ctrl+Shift+S. Print Screen opens the Snipping
// Tool on Windows 11 and F12 is Steam's screenshot key.

/// F9: MXB Clean ⇄ MXB Photo. A preset shortcut whose path is a folder makes ReShade step to
/// the next preset in that folder, and [`PRESETS_DIR`] holds just our two.
const KEY_PHOTO: [u32; 4] = [120, 0, 0, 0];
/// F6: depth of field on/off inside MXB Photo.
const KEY_DOF: [u32; 4] = [117, 0, 0, 0];
/// F11: ReShade's screenshot, effects included, at the game's resolution.
const KEY_SCREENSHOT: [u32; 4] = [122, 0, 0, 0];
/// Home: ReShade's own default for its menu, written so the UI can name it.
const KEY_OVERLAY: [u32; 4] = [36, 0, 0, 0];
/// How ReShade 6 stores a technique's toggle key in a preset: `Key` + technique@file.
const DOF_KEY_NAME: &str = "KeyCinematicDOF@CinematicDOF.fx";

fn key_value(k: [u32; 4]) -> String {
    k.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
}

/// Every key photo mode sets in `ReShade.ini`: (section, key, value).
fn photo_mode_keys() -> Vec<(&'static str, &'static str, String)> {
    vec![
        ("GENERAL", "PresetShortcutKeys", key_value(KEY_PHOTO)),
        ("GENERAL", "PresetShortcutPaths", format!(".\\{LIB_DIR}\\{PACK_DIR}\\{PRESETS_DIR}")),
        ("GENERAL", "PresetTransitionDuration", "0".into()),
        ("INPUT", "KeyOverlay", key_value(KEY_OVERLAY)),
        ("INPUT", "KeyScreenshot", key_value(KEY_SCREENSHOT)),
        // PNG: lossless.
        ("SCREENSHOT", "FileFormat", "1".into()),
        ("SCREENSHOT", "SavePath", format!(".\\{PHOTOS_DIR}")),
        ("SCREENSHOT", "ClearAlpha", "1".into()),
        ("SCREENSHOT", "SaveBeforeShot", "0".into()),
        ("SCREENSHOT", "SaveOverlayShot", "0".into()),
        ("SCREENSHOT", "SavePresetFile", "0".into()),
    ]
}

/// Set the photo-mode keys on an existing `ReShade.ini`, leaving every other line alone.
fn apply_photo_mode(doc: &mut crate::presets::IniDoc) {
    for (section, key, value) in photo_mode_keys() {
        doc.set(section, key, &value);
    }
}

fn config_text(choice: Choice) -> String {
    let mut doc = crate::presets::IniDoc::parse(&format!(
        "[GENERAL]\r\n\
         EffectSearchPaths=.\\{LIB_DIR}\\{PACK_DIR}\\Shaders\\**\r\n\
         TextureSearchPaths=.\\{LIB_DIR}\\{PACK_DIR}\\Textures\\**\r\n\
         PresetPath={}\r\n\
         PerformanceMode=1\r\n\
         SkipLoadingDisabledEffects=1\r\n\
         CheckForUpdates=0\r\n\
         NoDebugInfo=1\r\n\
         \r\n\
         [OVERLAY]\r\n\
         TutorialProgress=4\r\n",
        preset_path_value(choice)
    ));
    apply_photo_mode(&mut doc);
    let mut text = doc.render();
    if !text.ends_with("\r\n") {
        text.push_str("\r\n");
    }
    text
}

/// A ReShade key as the player reads it: "F9", "Ctrl+Shift+P", "Home". Empty when unbound.
pub fn key_label(k: &[u32; 4]) -> String {
    let vk = k[0];
    if vk == 0 {
        return String::new();
    }
    let name = match vk {
        0x08 => "Backspace".to_string(),
        0x09 => "Tab".to_string(),
        0x0D => "Enter".to_string(),
        0x13 => "Pause".to_string(),
        0x1B => "Esc".to_string(),
        0x20 => "Space".to_string(),
        0x21 => "Page Up".to_string(),
        0x22 => "Page Down".to_string(),
        0x23 => "End".to_string(),
        0x24 => "Home".to_string(),
        0x25 => "Left".to_string(),
        0x26 => "Up".to_string(),
        0x27 => "Right".to_string(),
        0x28 => "Down".to_string(),
        0x2C => "Print Screen".to_string(),
        0x2D => "Insert".to_string(),
        0x2E => "Delete".to_string(),
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk).map(String::from).unwrap_or_default(),
        0x60..=0x69 => format!("Num {}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x91 => "Scroll Lock".to_string(),
        _ => format!("Key {vk}"),
    };
    let mut out = String::new();
    for (on, m) in [(k[1], "Ctrl+"), (k[2], "Shift+"), (k[3], "Alt+")] {
        if on != 0 {
            out.push_str(m);
        }
    }
    out + &name
}

/// `1,2,3,4` (ReShade's form; it may carry more than four numbers) into the first key.
fn parse_key(v: &str) -> Option<[u32; 4]> {
    let n: Vec<u32> = v.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    (n.len() >= 4).then(|| [n[0], n[1], n[2], n[3]])
}

/// The photo-mode keys as the player sees them in the UI.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Hotkeys {
    /// MXB Clean ⇄ MXB Photo.
    pub photo: String,
    /// Depth of field on/off in MXB Photo.
    pub dof: String,
    /// ReShade's screenshot.
    pub screenshot: String,
    /// ReShade's menu.
    pub overlay: String,
}

/// Read back what is actually bound — the player can change any of these in ReShade's menu —
/// falling back to ours where a value is missing or unreadable.
fn hotkeys(game: &Path) -> Hotkeys {
    let cfg = fs::read_to_string(resolve_child(game, CONFIG)).unwrap_or_default();
    let doc = crate::presets::IniDoc::parse(&cfg);
    let read = |section: &str, key: &str, dflt: [u32; 4]| {
        doc.get(section, key).map_or(dflt, |v| {
            if v.trim().is_empty() { [0; 4] } else { parse_key(&v).unwrap_or(dflt) }
        })
    };
    let preset = fs::read_to_string(presets_dir(game).join("MXB Photo.ini")).unwrap_or_default();
    let dof = preset
        .lines()
        .find_map(|l| l.strip_prefix(DOF_KEY_NAME)?.trim_start().strip_prefix('='))
        .and_then(parse_key)
        .unwrap_or(KEY_DOF);
    Hotkeys {
        photo: key_label(&read("GENERAL", "PresetShortcutKeys", KEY_PHOTO)),
        dof: key_label(&dof),
        screenshot: key_label(&read("INPUT", "KeyScreenshot", KEY_SCREENSHOT)),
        overlay: key_label(&read("INPUT", "KeyOverlay", KEY_OVERLAY)),
    }
}

/// The folder ReShade saves screenshots into: `[SCREENSHOT] SavePath`, against the game folder
/// when relative.
fn photo_folder(game: &Path) -> PathBuf {
    let cfg = fs::read_to_string(resolve_child(game, CONFIG)).unwrap_or_default();
    let raw = crate::presets::IniDoc::parse(&cfg)
        .get("SCREENSHOT", "SavePath")
        .map(|v| v.trim().trim_matches('"').to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| PHOTOS_DIR.to_string());
    let p = Path::new(&raw);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    let rel = raw.trim_start_matches(".\\").trim_start_matches("./");
    rel.split(['\\', '/'])
        .filter(|s| !s.is_empty() && *s != ".")
        .fold(game.to_path_buf(), |acc, seg| acc.join(seg))
}

// ---------------------------------------------------------------------------
// GPU vendor, for the super-resolution tip
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
}

/// The vendor named in some text (a renderer string, a driver list). A laptop lists its
/// integrated Intel GPU next to the one the game runs on, so NVIDIA wins over AMD over Intel.
pub fn gpu_vendor_in(text: &str) -> Option<GpuVendor> {
    let t = text.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| t.contains(w));
    if has(&["nvidia", "geforce", "quadro"]) {
        Some(GpuVendor::Nvidia)
    } else if has(&["amd", "radeon", "advanced micro devices", "ati technologies"]) {
        Some(GpuVendor::Amd)
    } else if has(&["intel"]) {
        Some(GpuVendor::Intel)
    } else {
        None
    }
}

/// ReShade logs the renderer the game really uses ("Running on NVIDIA GeForce …"), so that
/// comes first; before the game has run with it, Windows' display-adapter list.
fn gpu_vendor(game: &Path) -> Option<GpuVendor> {
    let log = fs::read(resolve_child(game, LOG)).unwrap_or_default();
    let log = String::from_utf8_lossy(&log);
    if let Some(v) = log.lines().rev().find(|l| l.contains("Running on ")).and_then(gpu_vendor_in) {
        return Some(v);
    }
    display_adapters().as_deref().and_then(gpu_vendor_in)
}

#[cfg(windows)]
fn display_adapters() -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}",
            "/s",
            "/v",
            "DriverDesc",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(not(windows))]
fn display_adapters() -> Option<String> {
    None
}

// ---------------------------------------------------------------------------
// The install record
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    reshade: String,
    /// Game-root files moved into `backup/`, by their original name.
    backed_up: Vec<String>,
    /// Presets in the library folder that shared a name with ours, moved into `backup/`
    /// under `presets/`. Only layout 0 did this: since [`PRESETS_DIR`] nothing is in the way.
    backed_up_presets: Vec<String>,
    /// See [`LAYOUT`]. Missing in records written by the first test builds, which reads as 0.
    #[serde(default)]
    layout: u32,
}

fn read_manifest(game: &Path) -> Option<Manifest> {
    let text = fs::read_to_string(manifest_path(game)).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_manifest(game: &Path, m: &Manifest) -> anyhow::Result<()> {
    fs::write(manifest_path(game), serde_json::to_string_pretty(m)?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/// A ReShade (or other `opengl32.dll`) the player already had, which an install would move
/// aside. The UI asks before doing that.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Existing {
    /// A ReShade DLL is there (on, or turned off as `opengl32.dll.off`).
    pub reshade: bool,
    pub version: Option<String>,
    /// An `opengl32.dll` that isn't ReShade — some other wrapper.
    pub other_dll: bool,
    /// A `ReShade.ini` with the player's own settings.
    pub config: bool,
}

impl Existing {
    fn any(&self) -> bool {
        self.reshade || self.other_dll || self.config
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub game_dir: String,
    /// The folder holds the game's executable, so installing here can work at all.
    pub game_found: bool,
    /// Our pack is installed (its record is there).
    pub installed: bool,
    /// Which of our presets `ReShade.ini` points at; `Off` when not installed. `None` when
    /// installed but pointed at some other preset by hand.
    pub choice: Option<Choice>,
    /// Our ReShade has been renamed to `opengl32.dll.off` by the health check's off switch.
    pub disabled: bool,
    /// Only meaningful while not installed: what an install would move aside.
    pub existing: Option<Existing>,
    pub reshade_version: String,
    /// Where ReShade saves screenshots. Empty when the game folder isn't found.
    pub photo_folder: String,
    /// The photo-mode keys, as bound right now.
    pub hotkeys: Hotkeys,
    /// For the super-resolution tip (NVIDIA DSR / AMD VSR). `None` when it can't be told.
    pub gpu: Option<GpuVendor>,
    /// MX Bikes' own high-res screenshots (F10): `<user dir>/hrsshots`. Filled in by the
    /// command layer, which knows the user folder; see [`hires_shots`].
    pub hires_folder: String,
    /// F10 shots there still only as TGA.
    pub hires_pending: u32,
}

// ---------------------------------------------------------------------------
// MX Bikes' own high-res shots
// ---------------------------------------------------------------------------
//
// F10 in MX Bikes renders one large frame off-screen at core.ini's `[hrs]` size and saves it as
// an uncompressed TGA (about 25 MB at 4K) into `hrsshots/`. It is drawn before ReShade and
// without the HUD. The app converts those to PNG beside them; the TGAs are left alone.

/// The TGAs in `dir` with no PNG of the same name next to them yet.
pub fn hires_shots(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("tga"))
                && !p.with_extension("png").exists()
        })
        .collect();
    out.sort();
    out
}

/// What [`convert_hires_shots`] did.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HiresConverted {
    pub converted: u32,
    /// Files that wouldn't decode, by name.
    pub failed: Vec<String>,
}

/// Write a PNG beside every TGA in `dir` that has none. A PNG is written to a temporary name
/// first, so a half-written file never passes for a converted shot.
pub fn convert_hires_shots(dir: &Path) -> HiresConverted {
    let mut out = HiresConverted::default();
    for tga in hires_shots(dir) {
        let name = tga.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let png = tga.with_extension("png");
        let tmp = tga.with_extension("png.part");
        let done = image::open(&tga)
            .map_err(anyhow::Error::from)
            .and_then(|img| {
                img.to_rgb8().save_with_format(&tmp, image::ImageFormat::Png)?;
                fs::rename(&tmp, &png)?;
                Ok(())
            });
        match done {
            Ok(()) => out.converted += 1,
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                log::warn!("[reshade] couldn't convert {name}: {e:#}");
                out.failed.push(name);
            }
        }
    }
    out
}

fn existing_setup(game: &Path) -> Existing {
    let mut out = Existing::default();
    for name in [OPENGL_DLL, OPENGL_DLL_OFF] {
        let p = resolve_child(game, name);
        if !p.is_file() {
            continue;
        }
        if crate::reshade::is_reshade_dll(&p) {
            out.reshade = true;
            if out.version.is_none() {
                out.version = crate::reshade::health(&game.to_string_lossy()).version;
            }
        } else if name == OPENGL_DLL {
            out.other_dll = true;
        }
    }
    out.config = resolve_child(game, CONFIG).is_file();
    out
}

fn active_choice(game: &Path) -> Option<Choice> {
    let text = fs::read_to_string(resolve_child(game, CONFIG)).ok()?;
    let raw = crate::presets::IniDoc::parse(&text).get("GENERAL", "PresetPath")?;
    let norm = raw.trim().trim_matches('"').replace('/', "\\").to_ascii_lowercase();
    [Choice::Clean, Choice::Photo].into_iter().find(|c| {
        norm == preset_path_value(*c).to_ascii_lowercase()
            || norm == old_preset_path_value(*c).to_ascii_lowercase()
    })
}

/// Is this the game's install folder? ReShade has to sit beside the executable.
fn has_game_exe(game: &Path, exe: &str) -> bool {
    !exe.is_empty() && resolve_child(game, exe).is_file()
}

pub fn status(game_dir: &str, exe: &str) -> Status {
    let game_dir = game_dir.trim();
    let mut out = Status {
        game_dir: game_dir.to_string(),
        reshade_version: RESHADE_VERSION.to_string(),
        ..Status::default()
    };
    if game_dir.is_empty() {
        return out;
    }
    let game = PathBuf::from(game_dir);
    out.game_found = game.is_dir() && has_game_exe(&game, exe);
    if !out.game_found {
        return out;
    }
    if read_manifest(&game).is_some() {
        out.installed = true;
        out.choice = active_choice(&game);
        out.disabled = !resolve_child(&game, OPENGL_DLL).is_file()
            && resolve_child(&game, OPENGL_DLL_OFF).is_file();
        out.photo_folder = photo_folder(&game).to_string_lossy().into_owned();
        out.hotkeys = hotkeys(&game);
        out.gpu = gpu_vendor(&game);
    } else {
        out.choice = Some(Choice::Off);
        let e = existing_setup(&game);
        out.existing = e.any().then_some(e);
        // What an install will set up; the ReShade.ini here, if any, is the player's.
        out.photo_folder = game.join(PHOTOS_DIR).to_string_lossy().into_owned();
        out.hotkeys = Hotkeys {
            photo: key_label(&KEY_PHOTO),
            dof: key_label(&KEY_DOF),
            screenshot: key_label(&KEY_SCREENSHOT),
            overlay: key_label(&KEY_OVERLAY),
        };
    }
    out
}

/// The folder photo mode saves into, for the app's "Open photos folder". `None` when the game
/// folder isn't found.
pub fn photos_dir(game_dir: &str, exe: &str) -> Option<PathBuf> {
    let s = status(game_dir, exe);
    (s.game_found && !s.photo_folder.is_empty()).then(|| PathBuf::from(s.photo_folder))
}

/// Our pack is what's installed here. Read by the health check, which doesn't nag about a
/// ReShade the player switched on from this very app.
pub fn is_installed(game_dir: &str) -> bool {
    let d = game_dir.trim();
    !d.is_empty() && read_manifest(Path::new(d)).is_some()
}

// ---------------------------------------------------------------------------
// Downloading
// ---------------------------------------------------------------------------

/// Everything an install writes that came from the network, already verified.
pub struct Payload {
    dll: Vec<u8>,
    files: Vec<(&'static str, Vec<u8>)>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn verify(what: &str, bytes: &[u8], want: &str) -> anyhow::Result<()> {
    let got = sha256_hex(bytes);
    if !got.eq_ignore_ascii_case(want) {
        anyhow::bail!("{what} didn't match its pinned checksum (got {got}) — nothing was changed");
    }
    Ok(())
}

/// Pull `ReShade64.dll` out of the official setup. The setup is an executable with a zip
/// appended, which is how the installer itself carries both DLLs.
fn dll_from_setup(setup: &[u8], dll_name: &str) -> anyhow::Result<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(setup))?;
    let mut entry = zip.by_name(dll_name)?;
    let mut out = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut out)?;
    Ok(out)
}

async fn get(client: &reqwest::Client, url: &str) -> anyhow::Result<Vec<u8>> {
    let res = client.get(url).send().await?.error_for_status()?;
    Ok(res.bytes().await?.to_vec())
}

/// Download ReShade from reshade.me and the effects from their repositories, verifying each
/// against its pin. Nothing touches the game folder here.
pub async fn fetch(client: &reqwest::Client) -> anyhow::Result<Payload> {
    let setup = get(client, RESHADE_SETUP_URL).await?;
    verify("The ReShade download", &setup, RESHADE_SETUP_SHA256)?;
    let dll = dll_from_setup(&setup, RESHADE_DLL_IN_SETUP)?;
    verify("ReShade64.dll", &dll, RESHADE_DLL_SHA256)?;

    let mut files = Vec::with_capacity(FILES.len());
    for f in FILES {
        let bytes = get(client, f.url).await?;
        verify(f.dest, &bytes, f.sha256)?;
        files.push((f.dest, bytes));
    }
    Ok(Payload { dll, files })
}

// ---------------------------------------------------------------------------
// Install / switch / restore
// ---------------------------------------------------------------------------

/// What [`set`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Done; the folder is now in the state asked for.
    Done,
    /// The player has their own ReShade (or another `opengl32.dll`) here. Nothing was
    /// touched; ask, then call again with `replace_existing`.
    NeedsConfirm(Existing),
    /// Installing needs the download — call [`fetch`] and come back with the payload.
    NeedsPayload,
}

/// Bring `game_dir` to `choice`. Switching between our presets only rewrites `PresetPath`;
/// going from Off installs; going to Off restores everything.
///
/// The caller checks the game isn't running: the DLL is held open while it is.
pub fn set(
    game_dir: &str,
    exe: &str,
    choice: Choice,
    replace_existing: bool,
    payload: Option<&Payload>,
) -> anyhow::Result<Outcome> {
    let game = PathBuf::from(game_dir.trim());
    if game_dir.trim().is_empty() || !game.is_dir() {
        anyhow::bail!("the game folder isn't set");
    }
    if !has_game_exe(&game, exe) {
        anyhow::bail!("{} isn't in {} — is this the game folder?", exe, game.display());
    }

    let manifest = read_manifest(&game);
    let installed = manifest.is_some();
    match (installed, choice) {
        (false, Choice::Off) => Ok(Outcome::Done),
        (true, Choice::Off) => {
            restore(&game)?;
            Ok(Outcome::Done)
        }
        (true, c) if manifest.as_ref().is_some_and(|m| m.layout < LAYOUT) => {
            migrate(&game, c, manifest.unwrap_or_default())?;
            Ok(Outcome::Done)
        }
        (true, c) => {
            // Already ours: only the preset changes. Rewrite the preset file too, in case it
            // was deleted from the ReShade card in the meantime.
            write_preset(&game, c)?;
            point_config_at(&game, c)?;
            Ok(Outcome::Done)
        }
        (false, c) => {
            let existing = existing_setup(&game);
            if existing.any() && !replace_existing {
                return Ok(Outcome::NeedsConfirm(existing));
            }
            let Some(payload) = payload else {
                return Ok(Outcome::NeedsPayload);
            };
            if let Err(e) = install(&game, c, payload) {
                // Put back whatever had been moved before the failure. `restore` only acts
                // once the install record exists, so a failure before that touches nothing.
                let _ = restore(&game);
                return Err(e);
            }
            Ok(Outcome::Done)
        }
    }
}

/// Bring an install from the first test builds (layout 0: presets loose in the library
/// folder, no photo-mode keys) up to [`LAYOUT`]. Our old preset files there are ours by name:
/// that install moved any player file of the same name into `backup/presets/`, and Off still
/// puts it back.
fn migrate(game: &Path, c: Choice, mut manifest: Manifest) -> anyhow::Result<()> {
    for old in [Choice::Clean, Choice::Photo] {
        if let Some(f) = old.file() {
            let p = resolve_child(&lib_dir(game), f);
            if p.is_file() {
                fs::remove_file(&p)?;
            }
        }
    }
    write_preset(game, Choice::Clean)?;
    write_preset(game, Choice::Photo)?;
    let cfg = resolve_child(game, CONFIG);
    let existing = fs::read_to_string(&cfg).unwrap_or_default();
    if existing.trim().is_empty() {
        fs::write(&cfg, config_text(c))?;
    } else {
        let mut doc = crate::presets::IniDoc::parse(&existing);
        doc.set("GENERAL", "PresetPath", &preset_path_value(c));
        apply_photo_mode(&mut doc);
        fs::write(&cfg, doc.render())?;
    }
    manifest.layout = LAYOUT;
    write_manifest(game, &manifest)
}

fn write_preset(game: &Path, c: Choice) -> anyhow::Result<()> {
    let dir = presets_dir(game);
    fs::create_dir_all(&dir)?;
    if let Some(file) = c.file() {
        fs::write(dir.join(file), c.text())?;
    }
    Ok(())
}

/// Change `PresetPath` and nothing else — ReShade saves its own settings into this file as
/// the player uses the overlay, and those stay.
fn point_config_at(game: &Path, c: Choice) -> anyhow::Result<()> {
    let cfg = resolve_child(game, CONFIG);
    let existing = fs::read_to_string(&cfg).unwrap_or_default();
    if existing.trim().is_empty() {
        fs::write(&cfg, config_text(c))?;
        return Ok(());
    }
    let mut doc = crate::presets::IniDoc::parse(&existing);
    doc.set("GENERAL", "PresetPath", &preset_path_value(c));
    fs::write(&cfg, doc.render())?;
    Ok(())
}

fn install(game: &Path, c: Choice, payload: &Payload) -> anyhow::Result<()> {
    let pack = pack_dir(game);
    // A pack folder with no record is a half-finished earlier attempt; it is ours by name.
    if pack.exists() {
        fs::remove_dir_all(&pack)?;
    }
    let backup = backup_dir(game);
    fs::create_dir_all(&backup)?;

    // The record goes down first, listing what is about to move, so an install that dies
    // part-way can still be undone by `restore`. Our presets have their own folder, so a
    // player's preset of the same name in the library folder isn't in the way.
    let backed_up: Vec<String> = ROOT_FILES
        .iter()
        .filter(|n| resolve_child(game, n).exists())
        .map(|n| (*n).to_string())
        .collect();
    let manifest = Manifest {
        reshade: RESHADE_VERSION.to_string(),
        backed_up: backed_up.clone(),
        backed_up_presets: Vec::new(),
        layout: LAYOUT,
    };
    write_manifest(game, &manifest)?;

    for name in &backed_up {
        fs::rename(resolve_child(game, name), backup.join(name))?;
    }

    for (dest, bytes) in &payload.files {
        let path = dest.split('/').fold(pack.clone(), |p, seg| p.join(seg));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, bytes)?;
    }
    write_preset(game, Choice::Clean)?;
    write_preset(game, Choice::Photo)?;
    fs::write(game.join(CONFIG), config_text(c))?;
    // The DLL last: until it is there the game doesn't load any of this.
    fs::write(game.join(OPENGL_DLL), &payload.dll)?;
    Ok(())
}

/// Remove what [`install`] put down and move the backup back. Safe on a half-finished
/// install and safe to run twice.
fn restore(game: &Path) -> anyhow::Result<()> {
    // No record, nothing of ours: every file in the root belongs to the player.
    let Some(manifest) = read_manifest(game) else {
        return Ok(());
    };
    let backup = backup_dir(game);

    // Ours, in the game root. The DLL goes only if it is ReShade: anything else there was
    // put down after us, by someone else.
    for name in [OPENGL_DLL, OPENGL_DLL_OFF] {
        let p = resolve_child(game, name);
        if p.is_file() && crate::reshade::is_reshade_dll(&p) {
            fs::remove_file(&p)?;
        }
    }
    for name in [CONFIG, LOG] {
        let p = resolve_child(game, name);
        if p.is_file() {
            fs::remove_file(&p)?;
        }
    }
    // Layout 0 kept our presets in the library folder (theirs of that name went to backup);
    // since then they live in the pack folder and a same-named file there is the player's.
    if manifest.layout < LAYOUT {
        for c in [Choice::Clean, Choice::Photo] {
            if let Some(f) = c.file() {
                let p = resolve_child(&lib_dir(game), f);
                if p.is_file() {
                    fs::remove_file(&p)?;
                }
            }
        }
    }

    // Theirs, back where it was.
    for name in &manifest.backed_up {
        let from = backup.join(name);
        let to = game.join(name);
        if from.exists() && !to.exists() {
            fs::rename(&from, &to)?;
        }
    }
    for name in &manifest.backed_up_presets {
        let from = backup.join("presets").join(name);
        let to = lib_dir(game).join(name);
        if from.exists() && !to.exists() {
            fs::rename(&from, &to)?;
        }
    }

    // Anything still in backup/ couldn't be put back (its spot was taken) — keep the pack
    // folder then, rather than delete the player's file.
    if dir_has_files(&backup) {
        anyhow::bail!(
            "restored what could be; some of your files are still in {} because something \
             else now sits where they were",
            backup.display()
        );
    }
    let pack = pack_dir(game);
    if pack.exists() {
        fs::remove_dir_all(&pack)?;
    }
    // The library folder goes too when nothing else is in it.
    let lib = lib_dir(game);
    if fs::read_dir(&lib).is_ok_and(|mut rd| rd.next().is_none()) {
        let _ = fs::remove_dir(&lib);
    }
    Ok(())
}

fn dir_has_files(dir: &Path) -> bool {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .flatten()
        .any(|e| e.file_type().is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const EXE: &str = "mxbikes.exe";

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("mxb-pack-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join(EXE), b"MZ game").unwrap();
        p
    }

    fn g(p: &Path) -> String {
        p.to_string_lossy().into_owned()
    }

    /// A payload shaped like the real one, without the network.
    fn payload() -> Payload {
        let mut dll = b"MZ\x90\x00 fake ".to_vec();
        dll.extend_from_slice(b"ReShade 6.8.0 ");
        Payload {
            dll,
            files: FILES.iter().map(|f| (f.dest, format!("// {}", f.dest).into_bytes())).collect(),
        }
    }

    /// Every file in a folder, with its bytes — the "byte-identical after restore" check.
    fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut out: Vec<(String, Vec<u8>)> = walkdir::WalkDir::new(dir)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| {
                (
                    e.path().strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/"),
                    fs::read(e.path()).unwrap(),
                )
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn install_on_a_clean_game_and_off_leaves_nothing_behind() {
        let dir = tmp("clean");
        let before = snapshot(&dir);

        assert_eq!(
            set(&g(&dir), EXE, Choice::Clean, false, None).unwrap(),
            Outcome::NeedsPayload,
            "nothing in the way, so it goes straight to the download",
        );
        assert_eq!(snapshot(&dir), before, "asking for the payload changes nothing");

        let p = payload();
        assert_eq!(set(&g(&dir), EXE, Choice::Clean, false, Some(&p)).unwrap(), Outcome::Done);
        assert!(crate::reshade::is_reshade_dll(&dir.join(OPENGL_DLL)));
        let pack = dir.join(LIB_DIR).join(PACK_DIR);
        for f in FILES {
            assert!(pack.join(f.dest).is_file(), "{} missing", f.dest);
        }
        let cfg = fs::read_to_string(dir.join(CONFIG)).unwrap();
        assert!(cfg.contains("PresetPath=.\\FrostMod ReShade\\MXB Pack\\Presets\\MXB Clean.ini"));
        assert!(cfg.contains("EffectSearchPaths=.\\FrostMod ReShade\\MXB Pack\\Shaders\\**"));
        assert!(cfg.contains("PerformanceMode=1"));

        let s = status(&g(&dir), EXE);
        assert!(s.installed && s.existing.is_none());
        assert_eq!(s.choice, Some(Choice::Clean));
        assert!(is_installed(&g(&dir)));

        // ReShade writes a log and a screenshot-free run leaves only that; Off removes it.
        fs::write(dir.join(LOG), "log").unwrap();
        set(&g(&dir), EXE, Choice::Off, false, None).unwrap();
        assert_eq!(snapshot(&dir), before, "Off must leave the folder exactly as it was");
        assert!(!dir.join(LIB_DIR).exists(), "an emptied library folder goes too");
        assert_eq!(status(&g(&dir), EXE).choice, Some(Choice::Off));
    }

    #[test]
    fn switching_presets_only_moves_preset_path_and_keeps_overlay_settings() {
        let dir = tmp("switch");
        set(&g(&dir), EXE, Choice::Clean, false, Some(&payload())).unwrap();
        // ReShade saves the player's own overlay settings into the same file.
        let mut cfg = fs::read_to_string(dir.join(CONFIG)).unwrap();
        cfg.push_str("\r\n[INPUT]\r\nKeyOverlay=113,0,0,0\r\n");
        fs::write(dir.join(CONFIG), &cfg).unwrap();

        assert_eq!(set(&g(&dir), EXE, Choice::Photo, false, None).unwrap(), Outcome::Done);
        let after = fs::read_to_string(dir.join(CONFIG)).unwrap();
        assert_eq!(after, cfg.replace("MXB Clean.ini", "MXB Photo.ini"));
        assert_eq!(status(&g(&dir), EXE).choice, Some(Choice::Photo));
    }

    #[test]
    fn an_existing_reshade_is_asked_about_then_backed_up_and_restored_byte_for_byte() {
        let dir = tmp("existing");
        fs::write(dir.join(OPENGL_DLL), b"MZ ReShade 6.7.3 the player's own").unwrap();
        fs::write(dir.join(CONFIG), "[GENERAL]\r\nPresetPath=.\\Mine.ini\r\n").unwrap();
        fs::write(dir.join(LOG), "their log").unwrap();
        fs::write(dir.join("Mine.ini"), "Techniques=Clarity\n").unwrap();
        fs::create_dir_all(dir.join("reshade-shaders/Shaders")).unwrap();
        fs::write(dir.join("reshade-shaders/Shaders/Clarity.fx"), "technique Clarity {}").unwrap();
        fs::create_dir_all(dir.join(LIB_DIR)).unwrap();
        fs::write(dir.join(LIB_DIR).join("MXB Clean.ini"), "Techniques=TheirOwn\n").unwrap();
        fs::write(dir.join(LIB_DIR).join("Other.ini"), "Techniques=Other\n").unwrap();
        let before = snapshot(&dir);

        let s = status(&g(&dir), EXE);
        let e = s.existing.clone().expect("their setup is reported");
        assert!(e.reshade && e.config && !e.other_dll);
        assert_eq!(e.version.as_deref(), Some("6.7.3"));

        // Without the go-ahead, and even with the payload in hand, nothing moves.
        let out = set(&g(&dir), EXE, Choice::Clean, false, Some(&payload())).unwrap();
        assert!(matches!(out, Outcome::NeedsConfirm(_)));
        assert_eq!(snapshot(&dir), before);

        set(&g(&dir), EXE, Choice::Clean, true, Some(&payload())).unwrap();
        assert_ne!(fs::read(dir.join(OPENGL_DLL)).unwrap(), b"MZ ReShade 6.7.3 the player's own");
        assert_eq!(
            fs::read_to_string(dir.join("reshade-shaders/Shaders/Clarity.fx")).unwrap(),
            "technique Clarity {}",
            "their shader collection is never written into",
        );
        assert_eq!(
            fs::read_to_string(dir.join(LIB_DIR).join("MXB Clean.ini")).unwrap(),
            "Techniques=TheirOwn\n",
            "a preset of theirs that shares our name isn't in the way any more",
        );
        let ours = dir.join(LIB_DIR).join(PACK_DIR).join(PRESETS_DIR).join("MXB Clean.ini");
        assert!(fs::read_to_string(ours).unwrap().contains("SMAA"));

        set(&g(&dir), EXE, Choice::Off, false, None).unwrap();
        assert_eq!(snapshot(&dir), before, "everything of theirs comes back unchanged");
    }

    #[test]
    fn a_foreign_opengl32_and_a_turned_off_reshade_both_count_as_existing() {
        let dir = tmp("foreign");
        fs::write(dir.join(OPENGL_DLL), b"MZ some other wrapper").unwrap();
        let e = status(&g(&dir), EXE).existing.unwrap();
        assert!(e.other_dll && !e.reshade);

        let dir = tmp("off-copy");
        fs::write(dir.join(OPENGL_DLL_OFF), b"MZ ReShade 6.8.0").unwrap();
        let before = snapshot(&dir);
        assert!(status(&g(&dir), EXE).existing.unwrap().reshade);
        set(&g(&dir), EXE, Choice::Photo, true, Some(&payload())).unwrap();
        assert!(!dir.join(OPENGL_DLL_OFF).exists(), "moved aside so it can't clash");
        set(&g(&dir), EXE, Choice::Off, false, None).unwrap();
        assert_eq!(snapshot(&dir), before);
    }

    /// The health check's off switch renames our DLL; Off still has to clean that up.
    #[test]
    fn off_after_the_health_check_turned_reshade_off() {
        let dir = tmp("toggled");
        let before = snapshot(&dir);
        set(&g(&dir), EXE, Choice::Clean, false, Some(&payload())).unwrap();
        crate::reshade::set_enabled(&g(&dir), false, false).unwrap();
        assert!(status(&g(&dir), EXE).disabled);
        set(&g(&dir), EXE, Choice::Off, false, None).unwrap();
        assert_eq!(snapshot(&dir), before);
    }

    /// An install that dies half-way — here, a file that can't be written — must leave the
    /// player's files where they were.
    #[test]
    fn a_failed_install_rolls_back() {
        let dir = tmp("rollback");
        fs::write(dir.join(OPENGL_DLL), b"MZ ReShade 6.7.3 mine").unwrap();
        fs::write(dir.join(CONFIG), "mine").unwrap();
        let before = snapshot(&dir);

        let mut bad = payload();
        // A path segment Windows and Unix both refuse as a file name.
        bad.files.push(("Shaders/\0bad.fx", b"x".to_vec()));
        assert!(set(&g(&dir), EXE, Choice::Clean, true, Some(&bad)).is_err());
        assert_eq!(snapshot(&dir), before);
        assert!(!is_installed(&g(&dir)));
    }

    #[test]
    fn refuses_a_folder_without_the_game() {
        let dir = tmp("not-game");
        fs::remove_file(dir.join(EXE)).unwrap();
        assert!(set(&g(&dir), EXE, Choice::Clean, false, Some(&payload())).is_err());
        assert!(!dir.join(OPENGL_DLL).exists());
        assert!(!status(&g(&dir), EXE).game_found);
    }

    #[test]
    fn a_fresh_install_sets_up_photo_mode() {
        let dir = tmp("photo-mode");
        let before = snapshot(&dir);
        set(&g(&dir), EXE, Choice::Clean, false, Some(&payload())).unwrap();

        let cfg = fs::read_to_string(dir.join(CONFIG)).unwrap();
        for line in [
            "PresetShortcutKeys=120,0,0,0",
            "PresetShortcutPaths=.\\FrostMod ReShade\\MXB Pack\\Presets",
            "KeyScreenshot=122,0,0,0",
            "KeyOverlay=36,0,0,0",
            "FileFormat=1",
            "SavePath=.\\MXB Photos",
        ] {
            assert!(cfg.contains(line), "{line} missing from ReShade.ini:\n{cfg}");
        }
        assert!(cfg.ends_with("\r\n") && !cfg.replace("\r\n", "").contains('\n'), "CRLF throughout");

        // Exactly our two presets in their folder: the switch key cycles through it.
        let presets = dir.join(LIB_DIR).join(PACK_DIR).join(PRESETS_DIR);
        let mut names: Vec<String> = fs::read_dir(&presets)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["MXB Clean.ini", "MXB Photo.ini"]);
        let photo = fs::read_to_string(presets.join("MXB Photo.ini")).unwrap();
        assert!(photo.contains("KeyCinematicDOF@CinematicDOF.fx=117,0,0,0"));
        assert!(photo.contains("RESHADE_DEPTH_INPUT_IS_REVERSED=0"));
        assert!(photo.contains("UseMouseDrivenAutoFocus=0"));
        assert!(!dir.join(PHOTOS_DIR).exists(), "ReShade makes the photos folder on the first shot");

        let s = status(&g(&dir), EXE);
        assert_eq!(
            s.hotkeys,
            Hotkeys {
                photo: "F9".into(),
                dof: "F6".into(),
                screenshot: "F11".into(),
                overlay: "Home".into()
            }
        );
        assert_eq!(PathBuf::from(&s.photo_folder), dir.join(PHOTOS_DIR));

        // The player rebinds the screenshot in ReShade's menu: the app shows their key.
        let rebound = cfg.replace("KeyScreenshot=122,0,0,0", "KeyScreenshot=80,1,1,0");
        fs::write(dir.join(CONFIG), rebound).unwrap();
        assert_eq!(status(&g(&dir), EXE).hotkeys.screenshot, "Ctrl+Shift+P");

        set(&g(&dir), EXE, Choice::Off, false, None).unwrap();
        assert_eq!(snapshot(&dir), before);
    }

    #[test]
    fn photos_taken_survive_off() {
        let dir = tmp("photos");
        set(&g(&dir), EXE, Choice::Photo, false, Some(&payload())).unwrap();
        fs::create_dir_all(dir.join(PHOTOS_DIR)).unwrap();
        fs::write(dir.join(PHOTOS_DIR).join("mxbikes 2026-10-27.png"), b"PNG").unwrap();
        set(&g(&dir), EXE, Choice::Off, false, None).unwrap();
        assert_eq!(fs::read(dir.join(PHOTOS_DIR).join("mxbikes 2026-10-27.png")).unwrap(), b"PNG");
        assert!(!dir.join(OPENGL_DLL).exists());
    }

    /// The first test builds kept our presets loose in the library folder and had no photo
    /// keys. Choosing a preset brings such an install up to date; Off still restores exactly.
    #[test]
    fn an_install_from_the_first_test_builds_is_migrated_and_still_restores() {
        let dir = tmp("migrate");
        fs::create_dir_all(dir.join(LIB_DIR)).unwrap();
        fs::write(dir.join(LIB_DIR).join("MXB Photo.ini"), "Techniques=Theirs\n").unwrap();
        fs::write(dir.join(LIB_DIR).join("Other.ini"), "Techniques=Other\n").unwrap();
        let before = snapshot(&dir);

        // What that build left on disk.
        let pack = dir.join(LIB_DIR).join(PACK_DIR);
        fs::create_dir_all(pack.join("backup/presets")).unwrap();
        fs::rename(
            dir.join(LIB_DIR).join("MXB Photo.ini"),
            pack.join("backup/presets/MXB Photo.ini"),
        )
        .unwrap();
        fs::write(
            pack.join(MANIFEST),
            r#"{"reshade":"6.8.0","backedUp":[],"backedUpPresets":["MXB Photo.ini"]}"#,
        )
        .unwrap();
        for f in FILES {
            let p = pack.join(f.dest);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "x").unwrap();
        }
        fs::write(dir.join(LIB_DIR).join("MXB Clean.ini"), PRESET_CLEAN).unwrap();
        fs::write(dir.join(LIB_DIR).join("MXB Photo.ini"), PRESET_PHOTO).unwrap();
        fs::write(
            dir.join(CONFIG),
            "[GENERAL]\r\nPresetPath=.\\FrostMod ReShade\\MXB Clean.ini\r\n\r\n[INPUT]\r\nKeyReload=116,0,0,0\r\n",
        )
        .unwrap();
        fs::write(dir.join(OPENGL_DLL), b"MZ ReShade 6.8.0").unwrap();
        assert_eq!(status(&g(&dir), EXE).choice, Some(Choice::Clean), "the old path still reads");

        set(&g(&dir), EXE, Choice::Photo, false, None).unwrap();
        assert!(!dir.join(LIB_DIR).join("MXB Clean.ini").exists());
        assert!(!dir.join(LIB_DIR).join("MXB Photo.ini").exists());
        assert!(pack.join(PRESETS_DIR).join("MXB Photo.ini").is_file());
        let cfg = fs::read_to_string(dir.join(CONFIG)).unwrap();
        assert!(cfg.contains("PresetShortcutKeys=120,0,0,0"));
        assert!(cfg.contains("KeyReload=116,0,0,0"), "the player's own keys stay");
        assert_eq!(status(&g(&dir), EXE).choice, Some(Choice::Photo));
        assert_eq!(read_manifest(&dir).unwrap().layout, LAYOUT);

        set(&g(&dir), EXE, Choice::Off, false, None).unwrap();
        assert_eq!(snapshot(&dir), before, "their preset is back, ours are gone");
    }

    #[test]
    fn f10_shots_become_pngs_and_the_tgas_stay() {
        let dir = tmp("hrs");
        let img = image::RgbImage::from_fn(8, 4, |x, y| image::Rgb([x as u8 * 30, y as u8 * 60, 7]));
        img.save_with_format(dir.join("screen000.tga"), image::ImageFormat::Tga).unwrap();
        img.save_with_format(dir.join("screen001.tga"), image::ImageFormat::Tga).unwrap();
        fs::write(dir.join("screen001.png"), b"already").unwrap();
        fs::write(dir.join("broken.tga"), b"not a tga").unwrap();
        assert_eq!(hires_shots(&dir).len(), 2, "screen001 already has its PNG");

        let r = convert_hires_shots(&dir);
        assert_eq!(r.converted, 1);
        assert_eq!(r.failed, ["broken.tga"]);
        let back = image::open(dir.join("screen000.png")).unwrap().to_rgb8();
        assert_eq!(back, img, "lossless");
        assert!(dir.join("screen000.tga").is_file());
        assert_eq!(fs::read(dir.join("screen001.png")).unwrap(), b"already");
        assert!(!dir.join("broken.png").exists() && !dir.join("broken.png.part").exists());
        assert!(hires_shots(&tmp("hrs-none")).is_empty());
    }

    #[test]
    fn key_labels() {
        assert_eq!(key_label(&[120, 0, 0, 0]), "F9");
        assert_eq!(key_label(&[80, 1, 1, 0]), "Ctrl+Shift+P");
        assert_eq!(key_label(&[36, 0, 0, 0]), "Home");
        assert_eq!(key_label(&[0, 0, 0, 0]), "");
        assert_eq!(key_label(&[0x2C, 0, 0, 1]), "Alt+Print Screen");
        assert_eq!(parse_key("117,0,0,0,9,9,9,9"), Some([117, 0, 0, 0]));
        assert_eq!(parse_key("117,0"), None);
    }

    #[test]
    fn gpu_vendors() {
        use GpuVendor::*;
        assert_eq!(gpu_vendor_in("Running on NVIDIA GeForce RTX 5070/PCIe/SSE2."), Some(Nvidia));
        assert_eq!(gpu_vendor_in("AMD Radeon RX 7800 XT"), Some(Amd));
        assert_eq!(
            gpu_vendor_in(
                "DriverDesc REG_SZ Intel(R) UHD Graphics\r\nDriverDesc REG_SZ NVIDIA GeForce RTX 4060 Laptop GPU"
            ),
            Some(Nvidia)
        );
        assert_eq!(gpu_vendor_in("Intel(R) Arc(TM) A770 Graphics"), Some(Intel));
        assert_eq!(gpu_vendor_in("Microsoft Basic Display Adapter"), None);

        // ReShade's log names the GPU the game actually ran on.
        let dir = tmp("gpu");
        set(&g(&dir), EXE, Choice::Clean, false, Some(&payload())).unwrap();
        fs::write(dir.join(LOG), "x | INFO  | Running on AMD Radeon RX 6600.\r\n").unwrap();
        assert_eq!(status(&g(&dir), EXE).gpu, Some(Amd));
    }

    #[test]
    fn checksums_are_enforced() {
        assert!(verify("x", b"abc", RESHADE_DLL_SHA256).is_err());
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify("x", b"abc", abc).is_ok());
    }

    /// The real setup is an exe with a zip on the end; the reader has to find it past the
    /// executable bytes in front.
    #[test]
    fn the_dll_comes_out_of_a_zip_appended_to_an_exe() {
        let mut zipped = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut zipped);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            w.start_file("ReShade32.dll", opts).unwrap();
            w.write_all(b"thirty-two").unwrap();
            w.start_file("ReShade64.dll", opts).unwrap();
            w.write_all(b"sixty-four").unwrap();
            w.finish().unwrap();
        }
        let mut setup = b"MZ\x90\x00 an installer stub ".repeat(100);
        setup.extend_from_slice(zipped.get_ref());
        assert_eq!(dll_from_setup(&setup, "ReShade64.dll").unwrap(), b"sixty-four");
    }

    /// Every technique a preset turns on is declared by a file the pack installs — so a
    /// typo here can't ship as a preset that silently does nothing.
    #[test]
    fn presets_only_name_effects_the_pack_installs() {
        let installed: Vec<&str> = FILES
            .iter()
            .filter_map(|f| f.dest.strip_prefix("Shaders/"))
            .collect();
        for text in [PRESET_CLEAN, PRESET_PHOTO] {
            let line = text.lines().find(|l| l.starts_with("Techniques=")).unwrap();
            for entry in line["Techniques=".len()..].split(',') {
                let (_, file) = entry.split_once('@').expect("name@file form");
                assert!(installed.contains(&file), "{file} isn't installed by the pack");
            }
        }
    }

    /// Hits reshade.me and GitHub: every pin still resolves to the bytes it names.
    /// `cargo test reshade_pack::tests::pins_match_the_network -- --ignored`
    #[test]
    #[ignore]
    fn pins_match_the_network() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let client = reqwest::Client::builder().user_agent("mxb-app").build().unwrap();
        let p = rt.block_on(fetch(&client)).unwrap();
        assert!(crate::reshade::is_reshade_dll_bytes(&p.dll));
        assert_eq!(p.files.len(), FILES.len());

        // And the real thing installs and restores like the fake one.
        let dir = tmp("network");
        let before = snapshot(&dir);
        set(&g(&dir), EXE, Choice::Clean, false, Some(&p)).unwrap();
        assert_eq!(crate::reshade::health(&g(&dir)).version.as_deref(), Some("6.8.0"));
        set(&g(&dir), EXE, Choice::Off, false, None).unwrap();
        assert_eq!(snapshot(&dir), before);
    }
}
