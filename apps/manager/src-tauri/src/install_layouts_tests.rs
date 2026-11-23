use crate::dropzone::{self, CommitItem, ContentKind};
use crate::install;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

struct Layout {
    title: &'static str,
    source_name: &'static str,
    entries: &'static [&'static str],
    staged_file: Option<&'static str>,
    type_folder: &'static str,
    dest_folder: &'static str,
    slug: &'static str,
    review: &'static [(&'static str, &'static str)],
    check_direct_route: bool,
}

const KX_SOUND: &[&str] = &[
    "engine.scl",
    "KX250F_idle.wav",
    "KX250F_limiter.wav",
    "KX250F_onhigh.wav",
    "KX250F_onidle.wav",
    "KX250F_onlowmid.wav",
    "READ ME.txt",
    "sfx.cfg",
    "YZ250F_offidle.wav",
    "YZ250F_offlow.wav",
    "YZ250F_offmid.wav",
    "YZ250F_offveryhigh.wav",
    "YZ250F_offverylow.wav",
];

const TIM_125: &[&str] = &[
    "Injection 125 sound mod with rev limiter/2st125cc_idle.wav",
    "Injection 125 sound mod with rev limiter/2st125cc_onhigh.wav",
    "Injection 125 sound mod with rev limiter/2st125cc_onlow.wav",
    "Injection 125 sound mod with rev limiter/2st125cc_onveryhigh.wav",
    "Injection 125 sound mod with rev limiter/2st250cc_offhigh.wav",
    "Injection 125 sound mod with rev limiter/2st250cc_offmid.wav",
    "Injection 125 sound mod with rev limiter/engine.scl",
    "Injection 125 sound mod with rev limiter/sfx.cfg",
];

const HONDA_SOUND: &[&str] = &[
    "sounds/",
    "sounds/RM-Z250_idle.wav",
    "sounds/RM-Z250_limiter.wav",
    "sounds/RM-Z250_onhigh.wav",
    "sounds/RM-Z250_onlow.wav",
    "sounds/RM-Z250_onmid.wav",
    "sounds/RM-Z250_onverylow.wav",
    "sounds/YZ250F_offidle.wav",
    "sounds/YZ250F_offlow.wav",
    "sounds/YZ250F_offmid.wav",
    "sounds/YZ250F_offveryhigh.wav",
    "sounds/YZ250F_offverylow.wav",
    "sounds/YZ250F_onidle.wav",
    "engine.scl",
    "sfx.cfg",
];

const FACTORY_KTM: &[&str] = &[
    "Raffy Factory KTM 125 2T SoundMod/",
    "Raffy Factory KTM 125 2T SoundMod/INSTALL_INTO_YOUR_BIKE_FOLDER/",
    "Raffy Factory KTM 125 2T SoundMod/INSTALL_INTO_YOUR_BIKE_FOLDER/engine.scl",
    "Raffy Factory KTM 125 2T SoundMod/mods/",
    "Raffy Factory KTM 125 2T SoundMod/mods/bikes/",
    "Raffy Factory KTM 125 2T SoundMod/mods/bikes/soundmods/",
    "Raffy Factory KTM 125 2T SoundMod/mods/bikes/soundmods/samples/",
    "Raffy Factory KTM 125 2T SoundMod/mods/bikes/soundmods/samples/sx125_idle.wav",
    "Raffy Factory KTM 125 2T SoundMod/mods/bikes/soundmods/samples/sx125_offhigh.wav",
    "Raffy Factory KTM 125 2T SoundMod/mods/bikes/soundmods/samples/sx125_offidle.wav",
    "Raffy Factory KTM 125 2T SoundMod/mods/bikes/soundmods/samples/sx125_offmid.wav",
    "Raffy Factory KTM 125 2T SoundMod/mods/bikes/soundmods/samples/sx125_onhigh.wav",
    "Raffy Factory KTM 125 2T SoundMod/mods/bikes/soundmods/samples/sx125_onlimit.wav",
    "Raffy Factory KTM 125 2T SoundMod/mods/bikes/soundmods/samples/sx125_onlow.wav",
    "Raffy Factory KTM 125 2T SoundMod/mods/bikes/soundmods/samples/sx125_onmid.wav",
    "Raffy Factory KTM 125 2T SoundMod/mods/bikes/soundmods/samples/sx125_onveryhigh.wav",
    "Raffy Factory KTM 125 2T SoundMod/READ ME.txt",
];

const TST_TRACK: &[&str] = &[
    "tst/gate.edf",
    "tst/gfx.cfg",
    "tst/marshals.cfg",
    "tst/sky_5BC5FF671EE108B41D5F95BD2C7AE3FC241FBA91E9D05822D07DFC3B8070D397.edf",
    "tst/sound_90C6832101526B22503507846C8BB94B7F9055FAEEA339CB2A8DDA2F764ED691.wav",
    "tst/sound_CAF1CD68A5B453E462819533BDD33FD570225B1286D71209A27E248867624F25.wav",
    "tst/track_image.tga",
    "tst/track_map.tga",
    "tst/tst.amb",
    "tst/tst.ini",
    "tst/tst.map",
    "tst/tst.rdf",
    "tst/tst.ssc",
    "tst/tst.trh",
    "tst/tst.tsc",
];

const JASES_TRACK: &[&str] = &[
    "jasestrack/gate.edf",
    "jasestrack/jasestrack.amb",
    "jasestrack/jasestrack.ini",
    "jasestrack/jasestrack.map",
    "jasestrack/jasestrack.rdf",
    "jasestrack/jasestrack.trh",
    "jasestrack/jasestrack.tsc",
    "jasestrack/marshals.cfg",
    "jasestrack/track_image.tga",
    "jasestrack/track_map.tga",
    "jasestrack/layout_pitbike_track/gate.edf",
    "jasestrack/layout_pitbike_track/layout_pitbike_track.ini",
    "jasestrack/layout_pitbike_track/layout_pitbike_track.rdf",
    "jasestrack/layout_pitbike_track/layout_pitbike_track.trh",
    "jasestrack/layout_pitbike_track/track_image.tga",
    "jasestrack/layout_pitbike_track/track_map.tga",
];

const MX207_TRACK: &[&str] = &[
    "wheeler_springsmx_v2/gate.edf",
    "wheeler_springsmx_v2/marshals.cfg",
    "wheeler_springsmx_v2/sky_DA5E62D6CB34D3CB13637200563F856AC66D26F335D9A9499CA6787A2849CB6A.edf",
    "wheeler_springsmx_v2/track_image.tga",
    "wheeler_springsmx_v2/track_map.tga",
    "wheeler_springsmx_v2/wheeler_springsmx_v2.amb",
    "wheeler_springsmx_v2/wheeler_springsmx_v2.ini",
    "wheeler_springsmx_v2/wheeler_springsmx_v2.map",
    "wheeler_springsmx_v2/wheeler_springsmx_v2.rdf",
    "wheeler_springsmx_v2/wheeler_springsmx_v2.trh",
    "wheeler_springsmx_v2/wheeler_springsmx_v2.tsc",
];

const KRONOS_TRACK: &[&str] = &[
    "kronos/gate.edf",
    "kronos/gfx.cfg",
    "kronos/kronos.amb",
    "kronos/kronos.ini",
    "kronos/kronos.map",
    "kronos/kronos.rdf",
    "kronos/kronos.trh",
    "kronos/kronos.tsc",
    "kronos/marshals.cfg",
    "kronos/track_image.tga",
    "kronos/track_map.tga",
];

const TYRE: &[&str] = &[
    "oem_mx/",
    "oem_mx/c_p_mx.edf",
    "oem_mx/fwheel.hrc",
    "oem_mx/gfx.cfg",
    "oem_mx/p_mx.edf",
    "oem_mx/p_mx_as.edf",
    "oem_mx/p_mx_s.edf",
    "oem_mx/rwheel.hrc",
];

const HOK_HELMETS: &[&str] = &[
    "HOK BM10 HS APEX COBALT.pnt",
    "HOK BM10 HS APEX SPECTRUM.pnt",
    "HOK BM10 HS ARTIC GHOST.pnt",
    "HOK BM10 HS CYBER FUCHSIA.pnt",
    "HOK BM10 HS DARK PHANTOM.pnt",
    "HOK BM10 HS GLACIER VELOCITY.pnt",
    "HOK BM10 HS HYPER MINT.pnt",
    "HOK BM10 HS IGNITION RED.pnt",
    "HOK BM10 HS ONYX FLARE.pnt",
    "HOK BM10 HS PLATINUM BRONZE.pnt",
    "HOK BM10 HS SHADOW VIOLET.pnt",
    "HOK BM10 HS STEALTH OXIDE.pnt",
    "HOK BM10 HS ULTRA VOLTAGE.pnt",
    "HOK BM10 HS VOLT SURGE.pnt",
    "HOK BM10 HS WR SHIFT.pnt",
];

const WHITE_KTM: &[&str] = &["White_KTM.pnt"];
const PURPLE_DEVIL: &[&str] = &["Purple Devil.pnt"];

fn layouts() -> Vec<Layout> {
    vec![
        Layout {
            title: "KX Engine sound mod",
            source_name: "KX Engine sound mod",
            entries: KX_SOUND,
            staged_file: None,
            type_folder: "bikes",
            dest_folder: "KTM_SX125",
            slug: "kx-engine-sound",
            review: &[],
            check_direct_route: false,
        },
        Layout {
            title: "125 Sound Mod v1 By Tim",
            source_name: "125 sound mod with limiter.zip",
            entries: TIM_125,
            staged_file: None,
            type_folder: "bikes",
            dest_folder: "KTM_SX125",
            slug: "125-sound-mod",
            review: &[("", "")],
            check_direct_route: true,
        },
        Layout {
            title: "Honda CRF250R 2023 SoundMod",
            source_name: "Honda CRF250R 2023 SoundMod.zip",
            entries: HONDA_SOUND,
            staged_file: None,
            type_folder: "bikes",
            dest_folder: "HONDA_CRF250",
            slug: "honda-crf250-sound",
            review: &[],
            check_direct_route: false,
        },
        Layout {
            title: "Factory KTM 125 2T SoundMod",
            source_name: "Raffy Factory KTM 125 2T SoundMod.zip",
            entries: FACTORY_KTM,
            staged_file: None,
            type_folder: "bikes",
            dest_folder: "KTM_SX125",
            slug: "factory-ktm-125-sound",
            review: &[("mods", ""), ("", "")],
            check_direct_route: true,
        },
        Layout {
            title: "the sand track MX",
            source_name: "tst.pkz",
            entries: TST_TRACK,
            staged_file: Some("tst.pkz"),
            type_folder: "tracks",
            dest_folder: "",
            slug: "tst",
            review: &[],
            check_direct_route: false,
        },
        Layout {
            title: "jases compound",
            source_name: "jasestrack.pkz",
            entries: JASES_TRACK,
            staged_file: Some("jasestrack.pkz"),
            type_folder: "tracks",
            dest_folder: "",
            slug: "jasestrack",
            review: &[],
            check_direct_route: false,
        },
        Layout {
            title: "MX 207 2026",
            source_name: "wheeler_springsmx_v2.pkz",
            entries: MX207_TRACK,
            staged_file: Some("wheeler_springsmx_v2.pkz"),
            type_folder: "tracks",
            dest_folder: "",
            slug: "wheeler_springsmx_v2",
            review: &[],
            check_direct_route: false,
        },
        Layout {
            title: "Kronos MX",
            source_name: "kronos.pkz",
            entries: KRONOS_TRACK,
            staged_file: Some("kronos.pkz"),
            type_folder: "tracks",
            dest_folder: "",
            slug: "kronos",
            review: &[],
            check_direct_route: false,
        },
        Layout {
            title: "Dunlop MX-14",
            source_name: "Dunlop MX14.zip",
            entries: TYRE,
            staged_file: None,
            type_folder: "bikes",
            dest_folder: "",
            slug: "dunlop-mx14",
            review: &[],
            check_direct_route: false,
        },
        Layout {
            title: "Pirelli MX32",
            source_name: "Pirelli MX32 Scorpion by LC Creations.zip",
            entries: TYRE,
            staged_file: None,
            type_folder: "bikes",
            dest_folder: "",
            slug: "pirelli-mx32",
            review: &[],
            check_direct_route: false,
        },
        Layout {
            title: "HOK BM10 HYPERSHIFT",
            source_name: "HOK BM10 HYPERSHIFT.zip",
            entries: HOK_HELMETS,
            staged_file: None,
            type_folder: "rider",
            dest_folder: "helmets/HOK_BM10/paints",
            slug: "hok-bm10-hypershift",
            review: &[],
            check_direct_route: false,
        },
        Layout {
            title: "White KTM",
            source_name: "White_KTM.pnt",
            entries: WHITE_KTM,
            staged_file: None,
            type_folder: "bikes",
            dest_folder: "KTM_SX125/paints",
            slug: "white-ktm",
            review: &[],
            check_direct_route: false,
        },
        Layout {
            title: "Purple Devil Stark Varg",
            source_name: "Purple Devil Stark Varg",
            entries: PURPLE_DEVIL,
            staged_file: None,
            type_folder: "bikes",
            dest_folder: "KTM_SX125/paints",
            slug: "purple-devil",
            review: &[],
            check_direct_route: false,
        },
    ]
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn stage(layout: &Layout, extracted: &Path) {
    std::fs::create_dir_all(extracted).unwrap();
    if let Some(file) = layout.staged_file {
        write(&extracted.join(file), format!("fixture:{file}"));
        return;
    }
    for entry in layout.entries {
        let path = extracted.join(entry.trim_end_matches('/'));
        if entry.ends_with('/') {
            std::fs::create_dir_all(path).unwrap();
        } else {
            write(&path, format!("fixture:{entry}"));
        }
    }
}

fn fake_game(root: &Path) -> (String, PathBuf) {
    let game = root.join("game");
    let mods = game.join("mods");
    for bike in ["KTM_SX125", "HONDA_CRF250"] {
        let dir = mods.join("bikes").join(bike);
        write(
            &dir.join(format!("{bike}.ini")),
            format!("[info]\nname = {bike}\n"),
        );
        write(&dir.join(format!("{bike}.cfg")), format!("ID = {bike}\n"));
        write(&dir.join("engine.scl"), b"stock engine");
        write(&dir.join("sfx.cfg"), b"stock sfx");
    }
    (game.to_string_lossy().into_owned(), mods)
}

fn rel(path: &Path, mods: &Path) -> String {
    path.strip_prefix(mods)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}

fn expected(layout: &Layout) -> BTreeSet<String> {
    if let Some(file) = layout.staged_file {
        return [format!("tracks/{file}")].into_iter().collect();
    }
    layout
        .entries
        .iter()
        .filter(|entry| !entry.ends_with('/'))
        .filter_map(|entry| {
            let dst = match layout.title {
                "KX Engine sound mod" => format!("bikes/KTM_SX125/{entry}"),
                "125 Sound Mod v1 By Tim" => format!(
                    "bikes/KTM_SX125/{}",
                    entry
                        .strip_prefix("Injection 125 sound mod with rev limiter/")
                        .unwrap()
                ),
                "Honda CRF250R 2023 SoundMod" => format!("bikes/HONDA_CRF250/{entry}"),
                "Factory KTM 125 2T SoundMod" => {
                    const ROOT: &str = "Raffy Factory KTM 125 2T SoundMod/";
                    let inner = entry.strip_prefix(ROOT).unwrap();
                    if let Some(path) = inner.strip_prefix("mods/") {
                        path.to_string()
                    } else if let Some(path) = inner.strip_prefix("INSTALL_INTO_YOUR_BIKE_FOLDER/")
                    {
                        format!("bikes/KTM_SX125/{path}")
                    } else {
                        return None;
                    }
                }
                "Dunlop MX-14" | "Pirelli MX32" => format!("tyres/{entry}"),
                "HOK BM10 HYPERSHIFT" => format!("rider/helmets/HOK_BM10/paints/{entry}"),
                "White KTM" | "Purple Devil Stark Varg" => {
                    format!("bikes/KTM_SX125/paints/{entry}")
                }
                _ => unreachable!("unexpected fixture {}", layout.title),
            };
            Some(dst)
        })
        .collect()
}

fn writes(route: &install::Route, mods: &Path) -> BTreeSet<String> {
    install::writes_for(&route.placement)
        .into_iter()
        .map(|(_, dst)| rel(&dst, mods))
        .collect()
}

fn installed_fixture_files(mods: &Path) -> BTreeSet<String> {
    walkdir::WalkDir::new(mods)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            std::fs::read(entry.path())
                .ok()
                .filter(|bytes| bytes.starts_with(b"fixture:"))
                .map(|_| rel(entry.path(), mods))
        })
        .collect()
}

#[test]
fn real_mxbmods_layouts_land_where_mx_bikes_loads_them() {
    let root = std::env::temp_dir().join(format!(
        "mxb-real-layouts-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();

    for (index, layout) in layouts().iter().enumerate() {
        let case = root.join(index.to_string());
        let work = case.join("work");
        let extracted = work.join(install::STAGED_DIR);
        stage(layout, &extracted);
        let (mods_path, mods) = fake_game(&case);
        let expected = expected(layout);

        let planned =
            dropzone::plan_extracted(&mods_path, &extracted, work.clone(), layout.source_name)
                .unwrap_or_else(|error| panic!("{}: planning failed: {error:#}", layout.title));

        // The download path can deliberately offer a review, but placement still has to be
        // safe when a caller already has the user's bike choice and places the mod whole.
        if layout.check_direct_route {
            let route = install::plan_placement(
                &extracted,
                &mods,
                layout.type_folder,
                layout.dest_folder,
                layout.slug,
            );
            assert_eq!(
                writes(&route, &mods),
                expected,
                "{}: direct paths",
                layout.title
            );
        }

        if let Some(plan) = planned {
            let review: Vec<(&str, &str)> = plan
                .items
                .iter()
                .map(|item| (item.subpath.as_str(), item.dest_folder.as_str()))
                .collect();
            assert_eq!(
                review, layout.review,
                "{}: review destinations",
                layout.title
            );

            let choices: Vec<CommitItem> = plan
                .items
                .iter()
                .map(|item| match item.kind {
                    ContentKind::ModsTree => CommitItem {
                        id: item.id.clone(),
                        subpath: item.subpath.clone(),
                        dest_folder: item.dest_folder.clone(),
                    },
                    ContentKind::SoundSet => CommitItem {
                        id: item.id.clone(),
                        subpath: "mods/bikes".into(),
                        dest_folder: layout.dest_folder.into(),
                    },
                    _ => CommitItem {
                        id: item.id.clone(),
                        subpath: format!("mods/{}", layout.type_folder),
                        dest_folder: layout.dest_folder.into(),
                    },
                })
                .collect();
            let outcome = dropzone::commit(&mods_path, &plan.id, &choices).unwrap();
            assert!(
                outcome.failed.is_empty(),
                "{}: {:?}",
                layout.title,
                outcome.failed
            );
            assert_eq!(
                installed_fixture_files(&mods),
                expected,
                "{}: committed paths",
                layout.title
            );
            dropzone::cancel(&plan.id);
        } else {
            assert!(
                layout.review.is_empty(),
                "{}: expected a review plan",
                layout.title
            );
            let route = install::plan_placement(
                &extracted,
                &mods,
                layout.type_folder,
                layout.dest_folder,
                layout.slug,
            );
            assert_eq!(
                writes(&route, &mods),
                expected,
                "{}: planned paths",
                layout.title
            );
        }
    }

    let _ = std::fs::remove_dir_all(root);
}

fn route_targets(extracted: &Path, mods: &Path, type_folder: &str) -> BTreeSet<String> {
    let route = install::plan_placement(extracted, mods, type_folder, "", "slug");
    install::writes_for(&route.placement)
        .into_iter()
        .map(|(_, dst)| rel(&dst, mods))
        .collect()
}

/// An unpacked bike can carry wheel meshes beside its own model. Its `<Bike>.ini`/`.cfg`
/// say it is a bike, so it must never be filed under `tyres`.
#[test]
fn an_unpacked_bike_with_wheel_meshes_is_not_a_tyre_set() {
    let root = std::env::temp_dir().join(format!("frost-layout-bike-wheels-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let extracted = root.join("extracted");
    for f in ["MyBike/MyBike.ini", "MyBike/MyBike.cfg", "MyBike/fwheel.hrc", "MyBike/rwheel.hrc", "MyBike/model.edf"] {
        write(&extracted.join(f), "x");
    }
    let mods = root.join("mods");
    let targets = route_targets(&extracted, &mods, "bikes");
    assert!(targets.iter().all(|t| !t.starts_with("tyres/")), "{targets:?}");
    let _ = std::fs::remove_dir_all(root);
}

/// Tyre files loose at the archive root have no author-chosen folder name; they must not be
/// filed as `mods/tyres/extracted`, which no bike would ever ask for.
#[test]
fn loose_tyre_files_are_never_named_after_the_staging_folder() {
    let root = std::env::temp_dir().join(format!("frost-layout-loose-tyre-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let extracted = root.join("extracted");
    for f in ["fwheel.hrc", "rwheel.hrc", "p_mx.edf", "gfx.cfg"] {
        write(&extracted.join(f), "x");
    }
    let mods = root.join("mods");
    let targets = route_targets(&extracted, &mods, "bikes");
    assert!(targets.iter().all(|t| !t.starts_with("tyres/extracted")), "{targets:?}");
    let _ = std::fs::remove_dir_all(root);
}
