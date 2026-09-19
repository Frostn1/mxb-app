//! A track somebody already compiled, brought in as something to build on.
//!
//! The Studio builds from a [`TrackProgram`] and nothing else, so until now there was no way
//! to start from a track that exists — not one of PiBoSo's, not a published mod, not an
//! earlier build of your own. This is that way in, and it deliberately arrives at the same
//! place the lidar path does: a plot of ground in the store, a lap as straights and arcs, and
//! a `TrackProgram` pointing at both. Everything downstream — the editor, `synthesise`,
//! `build_track` — then treats it as an ordinary track, because it is one.
//!
//! **What comes across, and what does not.** A compiled track holds its terrain (`.trh`), the
//! surfaces painted on it, and — because `tracked -merge` writes it there — the centreline its
//! builder typed. Those are recoverable and they are what this reads. Its *look* is not: the
//! `.map` is a bake, and the source it was baked from (`.hmf`, `.tht`, the sheets, the shader
//! set) is not in the archive and has no reader here. So an imported track keeps its layout,
//! its elevation and its footprint, and wears the Studio's own ground and scenery. Say that in
//! the UI rather than letting someone discover it after a twenty-minute compile.
//!
//! This reads the player's own installed copy, on their own machine. Nothing it produces is
//! PiBoSo's file: it is a new track generated in the shape of one.

use crate::{trackground, trackprog, trackstats};
use anyhow::{anyhow, bail, Context, Result};
use mxb_core::{heightfield, track};
use std::path::Path;

/// What a compiled track gave up, before it becomes a program.
pub struct Imported {
    /// The stored plot's id, for [`trackprog::GroundRef`].
    pub id: String,
    pub ground: trackground::Ground,
    /// The lap as its builder typed it, when the `.trh` still carries one.
    pub lap: Option<crate::trackline::Lap>,
    /// What the track calls itself, from its `.ini`, falling back to the folder name.
    pub name: String,
    /// The surfaces the track paints, in the order its material table names them. Reported so
    /// the UI can say what was read rather than only what was kept.
    pub surfaces: Vec<String>,
}

/// Read a compiled track out of `path` — a `.pkz`, or one track's `prefix` inside a shared one.
pub fn read(path: &Path, prefix: Option<&str>) -> Result<Imported> {
    let names = track::entry_names_under(path, prefix)
        .with_context(|| format!("read {}", path.display()))?;
    let entry = track::heightfield_entries(&names)
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("no terrain file in this track"))?;
    let bytes = track::read_entry(path, &entry)?;

    let layout = heightfield::probe(&bytes, None)
        .ok_or_else(|| anyhow!("{entry} doesn't read as a terrain grid"))?;
    // Both of these are stated by a real `.trh` and guessed at by nothing. Without them the
    // grid is a picture of some relief in unknown units on a plot of unknown size, which is
    // fine to *draw* — the viewer does it — and useless to build on.
    let Some(mps) = layout.metres_per_sample else {
        bail!("{entry} states no footprint, so there is no telling how big this track is");
    };
    if layout.height_scale.is_none() {
        bail!("{entry} states no height scale, so its relief would mean nothing");
    }

    // Native resolution: this is the track, and resampling it here would be throwing away the
    // very thing that was worth importing. `read_grid` only reduces when asked to.
    let (gw, gh, z) = heightfield::read_grid(&bytes, &layout, layout.width.max(layout.height));
    if gw < 2 || gh < 2 {
        bail!("{entry} reads as a {gw}x{gh} grid, which is not a track");
    }

    let block_at = layout.offset + layout.width as usize * layout.height as usize * layout.sample.size();
    let block = bytes.get(block_at..).unwrap_or(&[]);
    let surfaces = trackstats::material_names(block);
    let cover = cover_from_masks(block, &surfaces, gw as usize, gh as usize);
    let lap = crate::trackline::read(block);

    // The generator's height budget is a range, not an elevation, and it quantises against it
    // — so the plot is dropped to its own floor exactly as an imported scan is.
    let base = z.iter().copied().fold(f32::MAX, f32::min);
    let z: Vec<f32> = z.iter().map(|v| v - base).collect();

    let ground = trackground::Ground {
        dim_x: gw as usize,
        dim_z: gh as usize,
        size_x: mps * (gw - 1) as f32,
        size_z: mps * (gh - 1) as f32,
        z,
        cover,
        base_m: base,
    };
    let id = trackground::store(&ground)?;

    Ok(Imported {
        id,
        ground,
        lap,
        name: track_name(path, prefix, &names).unwrap_or_else(|| folder_of(prefix, &entry)),
        surfaces,
    })
}

/// The program that rebuilds this track, with `jumps` deciding what the ground arrives as.
///
/// Shaped after [`trackground::program_for`], and for the same reasons: the relief knobs are
/// zeroed because nothing is being invented any more, and the wear and roughness are turned
/// down because the terrain already carries what they exist to put back.
///
/// [`trackprog::ScanJumps::Rut`] is the exception and the interesting one — it wants the
/// generator's rut pass to run at full strength, because laying ruts over a track that has
/// none is the whole point of asking for it.
pub fn program_for(imp: &Imported, jumps: trackprog::ScanJumps) -> Result<trackprog::TrackProgram> {
    use trackprog::*;

    let Some(lap) = imp.lap.as_ref() else {
        bail!(
            "this track's terrain file carries no centreline, so there is no lap to rebuild \
             it around. `tracked -merge` is what writes one, and not every track was finished \
             with it."
        );
    };
    let segments = lap.program_segments();
    if segments.is_empty() {
        bail!("this track's centreline is empty");
    }

    // Headroom over the ground's own relief, and the quantisation step with it — the same
    // sum the scanned path makes.
    let scale = (imp.ground.relief() * 1.15 + 4.0).ceil().max(8.0);
    let ruts = jumps == ScanJumps::Rut;

    Ok(TrackProgram {
        name: imp.name.clone(),
        author: String::new(),
        location: String::new(),
        terrain: Terrain {
            size_x: imp.ground.size_x,
            size_z: imp.ground.size_z,
            samples: DEFAULT_SAMPLES,
            scale,
            relief: Relief {
                amplitude: 0.0,
                wavelength: 180.0,
                seed: 1,
                texture: 0.0,
                tilt: 0.0,
                tilt_angle: 0.0,
                landforms: 0,
                landform_height: 0.0,
            },
            surface: Surface::Soil,
            texture: Default::default(),
            // Asking for ruts means asking for the pass that cuts them, at its own strength.
            // Keeping the ground as found means the opposite: the terrain is the answer.
            wear: if ruts { default_wear() } else { 0.0 },
            roughness: if ruts { default_roughness() } else { 0.0 },
            ground: Some(GroundRef {
                id: imp.id.clone(),
                place: imp.name.clone(),
                source: "Imported track".into(),
                collected: String::new(),
                licence: String::new(),
                jumps,
            }),
        },
        // The pose the centreline states, not one fitted to it: the `.trh` carries where its
        // builder put the start and which way it faced, so there is nothing here to infer.
        start: Start { x: lap.start.0, z: lap.start.1, angle: lap.heading },
        segments,
        width: DEFAULT_WIDTH_M,
        // Nothing invented on top. The jumps are already in the terrain — that is what makes
        // this an import rather than a layout — and a `Feature` here would stamp a second one
        // beside the real thing.
        features: Vec::new(),
        blend: default_blend(),
        elevation: Vec::new(),
        discipline: Discipline::Mx,
        border: Default::default(),
        venue: VenueKind::Open,
    })
}

/// Track width when the source doesn't say.
///
/// A compiled `.trh`'s centreline records length, radius and angle per segment and no width —
/// the corridor it was benched into is in the terrain, not in the numbers. On a raw-scan
/// import this only drives the masks and the racing line, never a terrain sample, so a
/// sensible middle beats pretending to have measured it.
const DEFAULT_WIDTH_M: f32 = 12.0;

/// The surface each cell is painted with, as the class the ground layers paint in.
///
/// Better evidence than the lidar path gets for the same field: it classifies an aerial
/// photograph by luminance and has to guess, while a compiled track states outright which
/// surface its builder painted where. The masks are one byte per cell per surface, so the
/// answer per cell is whichever covers it most.
///
/// Empty when the track paints nothing the mapping recognises — which is a real case, not a
/// failure, and leaves the ground uncoloured rather than wrongly coloured.
fn cover_from_masks(block: &[u8], surfaces: &[String], gw: usize, gh: usize) -> Vec<u8> {
    let masks = track::coverage_masks(block);
    if masks.is_empty() || surfaces.is_empty() {
        return Vec::new();
    }
    let mut out = vec![trackground::Cover::SoilMid.id(); gw * gh];
    let mut best = vec![0u8; gw * gh];
    let mut painted = false;
    for m in &masks {
        let (mw, mh) = (m.width as usize, m.height as usize);
        if mw == 0 || mh == 0 {
            continue;
        }
        let Some(class) = surfaces.get(m.id as usize).map(|n| cover_of(n).id()) else {
            continue;
        };
        let Some(bytes) = block.get(m.at..m.at + mw * mh) else {
            continue;
        };
        painted = true;
        // The masks are half a step coarser than the heightfield — 2048 against 2049 — so
        // each cell reads the mask cell it falls in rather than the two being indexed alike.
        for y in 0..gh {
            let my = y * mh / gh;
            for x in 0..gw {
                let v = bytes[my * mw + x * mw / gw];
                let i = y * gw + x;
                if v > best[i] {
                    best[i] = v;
                    out[i] = class;
                }
            }
        }
    }
    if painted { out } else { Vec::new() }
}

/// One of the game's surface names, as a ground cover class.
///
/// The names come out of the track's own material table, so this matches on what PiBoSo's
/// compilers write. Anything unrecognised is ordinary worked dirt, which is what most of a
/// motocross venue is and the least wrong guess available.
fn cover_of(name: &str) -> trackground::Cover {
    let n = name.trim().to_ascii_lowercase();
    match n.as_str() {
        "grass" => trackground::Cover::Vegetation,
        "soft soil" => trackground::Cover::SoilDark,
        "compact soil" | "sand" => trackground::Cover::SoilLight,
        // `kerb` is in there with the rest: a stock track's table reads
        // `asphalt, grass, sand, kerb, soil, concrete`, which is the runtime's list and
        // wider than the one `terrained` writes into a `.tht`.
        "gravel" | "rock" | "asphalt" | "concrete" | "kerb" => trackground::Cover::Hard,
        _ => trackground::Cover::SoilMid,
    }
}

/// The name the track calls itself, out of its own `.ini`.
fn track_name(path: &Path, prefix: Option<&str>, names: &[String]) -> Option<String> {
    let meta = match prefix {
        Some(p) => mxb_core::pkz::read_meta_and_preview_under(path, p).ok()?.0,
        None => mxb_core::pkz::read_meta(path).ok()?,
    };
    let _ = names;
    meta.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty())
}

/// The folder a track sits in, as a last resort for its name.
fn folder_of(prefix: Option<&str>, entry: &str) -> String {
    prefix
        .and_then(|p| p.rsplit('/').find(|s| !s.is_empty()))
        .map(str::to_string)
        .unwrap_or_else(|| {
            entry
                .rsplit('/')
                .next()
                .and_then(|f| f.split('.').next())
                .unwrap_or("Imported track")
                .to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_names_map_to_the_classes_the_ground_paints_in() {
        use trackground::Cover;
        assert_eq!(cover_of("grass"), Cover::Vegetation);
        assert_eq!(cover_of("Soft Soil"), Cover::SoilDark);
        assert_eq!(cover_of("soil"), Cover::SoilMid);
        assert_eq!(cover_of("compact soil"), Cover::SoilLight);
        assert_eq!(cover_of("gravel"), Cover::Hard);
        // Not in the game's own list, so it lands on the least wrong answer rather than
        // being dropped.
        assert_eq!(cover_of("something new"), Cover::SoilMid);
    }

    #[test]
    fn a_track_with_no_centreline_says_what_is_missing() {
        let imp = Imported {
            id: "x".into(),
            ground: trackground::Ground {
                dim_x: 2,
                dim_z: 2,
                size_x: 10.0,
                size_z: 10.0,
                z: vec![0.0; 4],
                cover: Vec::new(),
                base_m: 0.0,
            },
            lap: None,
            name: "Nameless".into(),
            surfaces: Vec::new(),
        };
        let err = program_for(&imp, trackprog::ScanJumps::Keep).unwrap_err().to_string();
        assert!(err.contains("centreline"), "{err}");
    }

    /// What [`trackprog::ScanJumps::Rut`] promises, pinned: the scan back verbatim *except*
    /// for the rut layer.
    ///
    /// Both halves matter and they fail in opposite directions. If the ground off the line
    /// moves at all, something from the corridor pass is reaching terrain samples it has no
    /// business touching and the track is no longer the one that was imported. If nothing
    /// moves anywhere, the mode is an expensive alias for `Keep` and the ruts never arrived.
    ///
    /// Compared against a `Keep` build of the same program, so the only difference between
    /// the two runs is the one flag.
    #[test]
    fn ruts_over_a_scan_touch_the_line_and_nothing_else() {
        let ground = fixture_ground();
        let id = trackground::store(&ground).expect("stored");
        let imp = Imported {
            id,
            ground,
            lap: None,
            name: "Fixture".into(),
            surfaces: Vec::new(),
        };
        // No centreline in a fixture, so the program is built by hand around a lap the
        // generator can walk — this is about the terrain pass, not about reading a `.trh`.
        let base = fixture_program(&imp);

        let mut keep = base.clone();
        keep.terrain.ground.as_mut().unwrap().jumps = trackprog::ScanJumps::Keep;
        let mut rut = base;
        rut.terrain.ground.as_mut().unwrap().jumps = trackprog::ScanJumps::Rut;

        let a = crate::tracksynth::synthesise(&keep).expect("keep synthesises");
        let b = crate::tracksynth::synthesise(&rut).expect("rut synthesises");
        assert_eq!(a.heights.len(), b.heights.len());

        // Compared as a *shape*, not sample for sample. Both builds re-floor the grid to their
        // own minimum — the height budget is a range and samples quantise against it — so
        // cutting a rut anywhere lowers the minimum and lifts every sample by the same amount.
        // That datum shift is not the claim; what the ground does relative to itself is. The
        // median of the untouched field is the datum, because most of the field is untouched.
        let diff: Vec<f32> = a.heights.iter().zip(&b.heights).map(|(x, y)| y - x).collect();
        let mut off: Vec<f32> = diff
            .iter()
            .zip(&a.corridor)
            .filter(|(_, on)| !**on)
            .map(|(d, _)| *d)
            .collect();
        assert!(!off.is_empty(), "the fixture's corridor covers the whole plot");
        off.sort_by(|p, q| p.partial_cmp(q).unwrap());
        let datum = off[off.len() / 2];

        // Measured: 99% of the field off the corridor moves by *exactly* the datum and not a
        // float ulp more. The remainder is the fringe just beyond the corridor's boolean edge,
        // where `RUT_BLEND_M` fades a rut wall out into the ground beside it — that fade is
        // the point of blending the layer, and it reached 10.5 cm here against the 53 cm the
        // ruts cut on the line. A tenth of the plot moving, or the fringe rivalling the cut,
        // would both mean something other than the rut layer is reaching the scan.
        let untouched = off.iter().filter(|d| **d == datum).count();
        assert!(
            untouched * 100 >= off.len() * 97,
            "only {untouched} of {} samples off the corridor were left alone — something \
             other than the rut layer is reaching the scan",
            off.len(),
        );
        let fringe = off
            .iter()
            .map(|d| (d - datum).abs())
            .fold(0.0f32, f32::max);
        assert!(fringe < 0.15, "the rut layer bled {fringe:.3} m into the ground beside it");

        let cut = diff
            .iter()
            .zip(&a.corridor)
            .map(|(d, on)| if *on { (d - datum).abs() } else { 0.0 })
            .fold(0.0f32, f32::max);
        assert!(
            cut > 0.05,
            "the deepest thing the corridor gained was {cut:.3} m: the rut layer never really \
             reached the ground, and Rut is an expensive alias for Keep",
        );
        assert!(cut > fringe * 2.0, "the fringe {fringe:.3} m rivals the cut {cut:.3} m");
    }

    fn fixture_ground() -> trackground::Ground {
        // Flat, so every millimetre of difference between the two builds is the rut layer's.
        let (dim, size) = (257usize, 256.0f32);
        trackground::Ground {
            dim_x: dim,
            dim_z: dim,
            size_x: size,
            size_z: size,
            z: vec![0.0; dim * dim],
            cover: Vec::new(),
            base_m: 0.0,
        }
    }

    fn fixture_program(imp: &Imported) -> trackprog::TrackProgram {
        // A lap the walker can follow: two straights and two 180s, which is what any
        // generated MX layout reduces to. Stated as `LineSegment`s so it arrives through
        // `program_for` the way a read-back `.trh` does, rather than being patched in after.
        let seg = |length: f32, radius: f32, angle: f32| crate::trackline::LineSegment {
            length,
            radius,
            angle,
            elevation: 0.0,
            at: 0.0,
            x: 0.0,
            z: 0.0,
            heading: 0.0,
        };
        program_for(
            &Imported {
                id: imp.id.clone(),
                ground: imp.ground.clone(),
                lap: Some(crate::trackline::Lap {
                    start: (40.0, 40.0),
                    heading: 0.0,
                    length: 365.6,
                    segments: vec![
                        seg(120.0, 0.0, 0.0),
                        seg(62.8, 20.0, 180.0),
                        seg(120.0, 0.0, 0.0),
                        seg(62.8, 20.0, 180.0),
                    ],
                }),
                name: imp.name.clone(),
                surfaces: Vec::new(),
            },
            trackprog::ScanJumps::Rut,
        )
        .expect("a lap is enough to make a program")
    }

    /// The real thing, when there is an install to point at. Proves what no fixture can: that
    /// a track PiBoSo compiled reads back as a plot and a lap, in metres, the right way up.
    #[test]
    #[ignore = "needs a game install"]
    fn a_stock_track_reads_back_as_a_program() {
        let Ok(install) = std::env::var("MXB_INSTALL_DIR") else {
            eprintln!("set MXB_INSTALL_DIR to a folder holding tracks.pkz");
            return;
        };
        let archive = mxb_core::trackstock::archive_path(&install).expect("tracks.pkz");
        let track = mxb_core::trackstock::find(&install, "forest").expect("forest is stock");
        let imp = read(&archive, Some(&track.prefix())).expect("forest reads back");

        assert!(imp.ground.dim_x > 512, "grid came back {}", imp.ground.dim_x);
        let (sx, sz) = (imp.ground.size_x, imp.ground.size_z);
        assert!((100.0..4000.0).contains(&sx), "plot is {sx} by {sz} m");
        let relief = imp.ground.relief();
        assert!((1.0..200.0).contains(&relief), "relief is {relief} m");

        let prog = program_for(&imp, trackprog::ScanJumps::Rut).expect("and becomes a program");
        assert!(prog.is_raw_scan(), "a Rut import is still the scan, not a cut corridor");
        assert!(!prog.segments.is_empty());
        println!(
            "{}: {}x{} over {sx:.0}x{sz:.0} m, {relief:.1} m relief, {} segments, surfaces {:?}",
            imp.name,
            imp.ground.dim_x,
            imp.ground.dim_z,
            prog.segments.len(),
            imp.surfaces,
        );
    }
}
