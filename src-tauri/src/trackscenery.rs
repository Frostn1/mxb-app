//! What a generated track stands beside its riding line.
//!
//! `terrained.exe` places objects from `scene<N>` blocks in the `.hmf` and bakes their meshes
//! into the `.map`; the same blocks in the `.tht` give them collision. So this module has two
//! jobs: build the `.edf` models ([`crate::edfwrite`]) and decide where they go.
//!
//! Where they go is not invented. [`crate::trackobjects`] measured twelve published tracks and
//! every figure below is one of its numbers — the offset from the centreline, the gap round
//! the lap, the height. The single clearest of them is that **every track lines its riding
//! line with something about a metre tall, 7.5 m out, one every three metres**, and on most of
//! them it runs the whole lap. That is the edge a rider sees, and it is the first thing here.
//!
//! Everything of one kind is merged into **one node of one model**, placed once at the origin
//! in world metres. That is how published tracks do it — their `main_track_objects_c` is a
//! single baked mesh, which is why a `.map` comes apart into a hundred thousand islands and
//! not a hundred thousand draw calls — and it means a lap's worth of markers costs one
//! `scene` block rather than six hundred.

#![allow(dead_code)]

use crate::edfwrite::{self, Mesh, Part, Texture};
use crate::trackprog::TrackProgram;
use crate::tracksynth::Synth;

/// The line of stakes that marks the edge of the riding line, and what one is.
///
/// Measured off Indiana's `.map` piece by piece — `trackobjects`' `edge_marking` test — rather
/// than pooled over the corpus, because the corpus clusters and clustering loses a stake. It
/// carries 96 of them, evenly both sides, and its own `.rdf` puts the finish line at ±7 m: the
/// stake line *is* the track edge, not something set back from it.
///
/// The four figures move together. A stake at Indiana is **0.66 m tall and 4 cm wide**, half
/// the size this used to build, and that is what lets it stand every 6.2 m without the line
/// reading as a fence — which is the thing the gap was widened to 15 m to escape. Shrink the
/// post and the real spacing works.
const STAKE_OFF_M: f32 = 7.0;
const STAKE_GAP_M: f32 = 6.5;
const STAKE_H_M: f32 = 0.7;
const STAKE_W_M: f32 = 0.045;

/// Banners are the big printed panels, and there are few of them.
///
/// Indiana's are 4–7 m wide on posts about 5.7 m tall, and every one of them is a **wordmark
/// on a solid ground** — fourteen sponsors baked into one atlas, the title sponsor repeated
/// down the lap. [`BANNER_PANELS`] is the same idea with our own names on it.
const BANNER_OFF_M: f32 = 9.5;
const BANNER_GAP_M: f32 = 55.0;
const BANNER_W_M: f32 = 4.0;
const BANNER_H_M: f32 = 1.2;
/// How far off the ground the panel is slung.
const BANNER_LIFT_M: f32 = 0.35;

/// One printed banner: a wordmark on a coloured ground, and whether it carries the app's mark.
///
/// The anatomy is taken from the real ones. Every sponsor panel on Indiana's atlas is a mark,
/// a wordmark and a small strapline under the name — RACE TECH over "THE SCIENCE OF
/// SUSPENSION", and thirteen others follow the same rule.
///
/// The wordmark is **artwork**, not type drawn here. A banner is somebody's brand: MXB App's
/// is Barlow Condensed on `--primary`, Creste's is Cormorant Garamond over Hanken Grotesk in
/// its own `--ink` and `--accent`, and nothing in this crate can rasterise a `.ttf`. Drawing a
/// look-alike face was tried and it is exactly as convincing as a look-alike logo. So each
/// lockup is set once in the real faces and committed beside the icon, which is what a sponsor
/// hands a track builder anyway.
struct Panel {
    /// The name of the artwork in [`artwork`], which is also what the banner says.
    art: &'static str,
    ground: [u8; 3],
    /// The app's snowflake at the left. Off for a panel that is not ours.
    mark: bool,
}

/// What the banners say.
///
/// A lap cycles through them, which is why the app's own name is on two of the five — that is
/// how a title sponsor reads at a real meeting, not an accident of the list. Every ground is a
/// token straight out of `src/index.css`, except Creste's, which is its own `--paper`.
const BANNER_PANELS: [Panel; 5] = [
    // `--primary` on `--primary-foreground`.
    Panel { art: "mxbapp-ice", ground: [13, 18, 22], mark: true },
    // `--foreground` on `--secondary`, so the two dark panels don't read as one.
    Panel { art: "frost", ground: [38, 38, 44], mark: true },
    // Creste's own `--paper`.
    Panel { art: "creste", ground: [20, 19, 16], mark: false },
    // The light theme's `--primary`.
    Panel { art: "frostmod", ground: [28, 120, 151], mark: true },
    // The icon's own way round: dark on the ice blue.
    Panel { art: "mxbapp-dark", ground: [156, 207, 236], mark: true },
];

/// The banner atlas is one square sheet: four printed cells stacked down it, and a plain band
/// at the foot for the posts to wear. Cells rather than a sheet each because a model takes one
/// material — see the note where the kinds are assembled — and because it is what a published
/// track does.
const BANNER_CELLS: usize = BANNER_PANELS.len();
const BANNER_ATLAS_PX: u32 = 1024;
/// Rows per printed cell. Five of these plus a 124-row post band fills the square exactly.
///
/// A thousand pixels across rather than five hundred because the letters are drawn from
/// strokes: at half this the diagonals stair-step and the type goes back to looking pixelled,
/// which is the whole thing it is here to avoid. Published tracks bake 1024-square sheets.
const BANNER_CELL_PX: u32 = 180;

/// How far off the edge of the start straight anything trackside has to stand.
const OFF_THE_START_M: f32 = 2.5;

/// The fence, at the corpus median of 19.2 m and 2.9 m tall.
const FENCE_OFF_M: f32 = 19.2;
const FENCE_H_M: f32 = 2.9;
/// One panel. Wide, because a fence built from narrow boards reads as a crowd of boards.
const FENCE_PANEL_M: f32 = 4.0;

/// Bales guard what a rider would otherwise hit. Corpus offset runs 10.9–34.6; the near end
/// of that is where they do any good.
const BALE_OFF_M: f32 = 7.5;
const BALE_W_M: f32 = 1.2;
const BALE_H_M: f32 = 1.0;
const BALE_D_M: f32 = 0.8;

/// What the tracks people rate actually carry, per kilometre of lap, and how far off the
/// racing line they put it. Measured with `trackobjects` over Rancho, Lakewood, Mt Morris and
/// Southwick:
///
/// ```text
///   structure  234-324 /km  at  4-6 m    the marker boards down both sides
///   fence       35-58  /km  at  8-18 m
///   bale        13-14  /km  at  8-15 m
///   banner      12-22  /km  at 20-37 m
///   pole        17-135 /km  at 18-26 m
///   vehicle     22-36  /km  at 15-25 m
///   tree         2-5   /km  at 50-55 m
/// ```
///
/// Trackside trees are *rare* — five a kilometre at Lakewood. What that track has instead is
/// **15,341 trees beyond 60 m**: the wood is a backdrop, not furniture, and a track with a few
/// dozen trees dotted along it reads as a field with sticks in it.
const TREE_FROM_M: f32 = 34.0;
const TREE_TO_M: f32 = 58.0;
const TREE_SPACING_M: f32 = 45.0;
const TREE_H_M: f32 = 8.0;

/// How high the sky band reaches, in degrees above the horizon — below the sun, which stands
/// at 54°. See `dome_mesh`.
const SKY_TOP_DEG: f32 = 34.0;

/// How tall a feature has to be before it is worth marking.
///
/// At 0.9 m nearly every jump on a lap took a pair of posts and the track ended up flagged
/// from end to end, which says nothing. These are for the ones a rider needs warning of.
const JUMPMARK_FROM_M: f32 = 1.5;

/// A jump post: a stake's thickness, taller than the edge markers so it stands out of them.
const JUMPMARK_W_M: f32 = 0.09;
const JUMPMARK_H_M: f32 = 1.5;

/// The pennant on top of it, along its longest edge.
const JUMPMARK_FLAG_M: f32 = 0.42;

/// How far apart the poles and the parked vans go, from the per-kilometre counts above.
const POLE_GAP_M: f32 = 50.0;
const POLE_H_M: f32 = 7.5;
const VAN_GAP_M: f32 = 40.0;

/// The backdrop: where the wood starts, and how thickly it stands out to the edge of the plot.
const WOOD_FROM_M: f32 = 64.0;
const WOOD_STEP_M: f32 = 9.0;

/// How close to the track a tree may stand when it is inside the lap rather than behind it,
/// and how few of the candidates there are taken.
const INFIELD_TREE_FROM_M: f32 = 30.0;
const INFIELD_TREE_SHARE: f32 = 0.16;

/// How much of the wood is pine. A wood of one tree is wallpaper; the venues this is measured
/// against are half conifer, and the silhouette is most of what says where you are.
const PINE_SHARE: f32 = 0.55;

/// Which way to turn a mesh so its own X axis runs along a heading.
///
/// [`edfwrite::turned`] sends local X to `right_vector(deg)` and a card's normal to
/// `heading_vector(deg)`. So a thing whose length should run *along* the track — a fence
/// panel, a bale, a board facing the rider — turns by `heading + 90`, and a thing that should
/// span *across* it — the gantry beam — turns by `heading`. Getting that backwards is what
/// laid the finish gantry along the track instead of over it, and stood every bale end-on.
fn along(heading_rad: f32) -> f32 {
    heading_rad.to_degrees() + 90.0
}

fn across(heading_rad: f32) -> f32 {
    heading_rad.to_degrees()
}

/// Deterministic noise, so a track built twice is the same track.
fn rnd(seed: u32, i: u32) -> f32 {
    let mut h = seed ^ i.wrapping_mul(0x9E37_79B9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 16;
    (h % 100_000) as f32 / 100_000.0
}

/// One model placed in the world.
#[derive(Clone, Debug)]
pub struct Scene {
    /// The `.edf` beside the `.hmf`.
    pub file: String,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
}

/// What a track's scenery amounts to: the files to write, and the blocks that place them.
pub struct Scenery {
    /// `name.edf` against its bytes.
    pub files: Vec<(String, Vec<u8>)>,
    /// Placed in the `.hmf`, so drawn.
    pub drawn: Vec<Scene>,
    /// Placed in the `.tht` as well, so ridden into. Only what should stop a bike.
    pub solid: Vec<Scene>,
    /// What went where, for the log and for measuring the result back.
    pub tally: Vec<(&'static str, usize)>,
}

/// Height of the built ground at a world point, bilinear.
fn ground(syn: &Synth, x: f32, z: f32) -> f32 {
    let (gx, gz) = (x / syn.mps, z / syn.mps);
    let (x0, z0) = (gx.floor() as isize, gz.floor() as isize);
    let (fx, fz) = (gx - x0 as f32, gz - z0 as f32);
    let at = |ix: isize, iz: isize| -> f32 {
        let ix = ix.clamp(0, syn.gw as isize - 1) as usize;
        let iz = iz.clamp(0, syn.gh as isize - 1) as usize;
        syn.heights[iz * syn.gw + ix]
    };
    let (a, b) = (at(x0, z0), at(x0 + 1, z0));
    let (c, d) = (at(x0, z0 + 1), at(x0 + 1, z0 + 1));
    (a + (b - a) * fx) * (1.0 - fz) + (c + (d - c) * fx) * fz
}

/// How far a point is from the nearest part of the riding line — any part of it, not the
/// station it was placed from.
///
/// A lap folds back on itself. A tree put twenty-six metres to the side of one station lands
/// a metre from another, and measured only against its own station it looks correctly placed
/// right up until you ride into it. This is the check that catches that, and everything is
/// held to it.
fn clearance(coarse: &[crate::trackprog::Station], x: f32, z: f32) -> f32 {
    coarse
        .iter()
        .map(|st| (st.x - x).powi(2) + (st.z - z).powi(2))
        .fold(f32::INFINITY, f32::min)
        .sqrt()
}

/// The lowest ground under a footprint `r` metres across.
///
/// A card is placed by one point but stands on a width of ground, and on anything sloping the
/// far corner is the one that matters: sampling the centre alone buried a seven-metre tree
/// card a metre into a hillside. Taking the minimum leaves a gap under the high side, which
/// is what a real tree looks like anyway.
fn ground_min(syn: &Synth, x: f32, z: f32, r: f32) -> f32 {
    let mut lo = f32::INFINITY;
    for (dx, dz) in [(0.0, 0.0), (-r, 0.0), (r, 0.0), (0.0, -r), (0.0, r), (-r, -r), (r, r), (-r, r), (r, -r)] {
        lo = lo.min(ground(syn, x + dx, z + dz));
    }
    lo
}

/// The lowest and highest ground under a panel of length `len` centred on `(x, z)` and
/// running along `deg`.
///
/// A disc of samples is the wrong shape for a fence panel: it is a line, and what buries it
/// is the ground at its two ends. Sampled as a disc a panel across a two-metre step read as
/// level and sank a metre into the bank.
fn ground_span(syn: &Synth, x: f32, z: f32, deg: f32, len: f32) -> (f32, f32) {
    let (s, c) = deg.to_radians().sin_cos();
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for k in 0..=8 {
        let t = (k as f32 / 8.0 - 0.5) * len;
        let g = ground(syn, x + c * t, z - s * t);
        lo = lo.min(g);
        hi = hi.max(g);
    }
    (lo, hi)
}

/// The lowest and highest ground under a rectangle `w` by `d`, turned to `deg`.
fn ground_foot(syn: &Synth, x: f32, z: f32, deg: f32, w: f32, d: f32) -> (f32, f32) {
    let (s, c) = deg.to_radians().sin_cos();
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for a in -1..=1 {
        for b in -1..=1 {
            let (u, v) = (a as f32 * w * 0.5, b as f32 * d * 0.5);
            let g = ground(syn, x + c * u + s * v, z - s * u + c * v);
            lo = lo.min(g);
            hi = hi.max(g);
        }
    }
    (lo, hi)
}

/// Whether a world point is inside the terrain square, with a margin.
fn inside(prog: &TrackProgram, x: f32, z: f32, margin: f32) -> bool {
    x > margin
        && z > margin
        && x < prog.terrain.size_x - margin
        && z < prog.terrain.size_z - margin
}

// ---------------------------------------------------------------------------
// Sheets
// ---------------------------------------------------------------------------

/// Build a square sheet from a function of `(u, v)`.
///
/// `v` runs **down**: zero is the top of whatever wears it, which is the convention
/// [`edfwrite::card`] uses — its `v` is zero at the card's top edge. Published tracks use the
/// opposite one, and [`crate::map::textures`] flips a sheet's rows on the way in to suit it.
/// So a sheet written here comes back out of a compiled `.map` looking upside down, and it is
/// right anyway: the two inversions cancel, because ours flips the pixels *and* the UVs.
///
/// Proved rather than argued — `trackobjects`' `edge_marking` test rebuilds a banner from the
/// compiled map through its own UVs and its own sheet, and the wordmark comes out upright on
/// a generated track and on Indiana alike. Do not "fix" this by flipping the pixels alone:
/// that puts the letters on their heads in the game and leaves them looking right in every
/// dump, which is the worst way round to have it.
fn sheet(name: &str, dim: u32, f: impl Fn(f32, f32) -> [u8; 4]) -> Texture {
    let mut rgba = Vec::with_capacity((dim * dim * 4) as usize);
    for y in 0..dim {
        for x in 0..dim {
            let (u, v) = (x as f32 / dim as f32, y as f32 / dim as f32);
            rgba.extend_from_slice(&f(u, v));
        }
    }
    Texture { name: name.into(), width: dim, height: dim, rgba }
}

fn grain(u: f32, v: f32, seed: u32, scale: f32) -> f32 {
    let i = ((u * scale) as u32) ^ (((v * scale) as u32) << 8);
    rnd(seed, i)
}

/// The app's own mark, traced out of the icon the app actually ships.
///
/// Drawing a snowflake by hand gets the idea and not the logo — the barbs came out as a blob
/// and it was recognisably *a* snowflake rather than *ours*. This lifts the silhouette
/// straight from `icons/128x128@2x.png`: the mark is the dark shape on the icon's blue, so
/// anything opaque and dark is the logo and everything else is the tile behind it. Cropped to
/// its own bounds and cached, because it is the same picture on every panel.
///
/// Returns `(width, height, coverage)` — one byte a pixel, 255 where the mark is.
fn mark_mask() -> &'static (u32, u32, Vec<u8>) {
    static MARK: std::sync::OnceLock<(u32, u32, Vec<u8>)> = std::sync::OnceLock::new();
    MARK.get_or_init(|| {
        const ICON: &[u8] = include_bytes!("../icons/128x128@2x.png");
        let Ok(img) = image::load_from_memory(ICON) else {
            return (0, 0, Vec::new());
        };
        let img = img.to_rgba8();
        let (w, h) = img.dimensions();
        let dark = |x: u32, y: u32| -> u8 {
            let p = img.get_pixel(x, y).0;
            if p[3] < 160 {
                return 0;
            }
            let luma = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
            // The icon's flake is near-black on a light blue tile; the midpoint separates
            // them with room to spare, and the ramp keeps the antialiased edge.
            (((110.0 - luma) / 40.0).clamp(0.0, 1.0) * 255.0) as u8
        };
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
        for y in 0..h {
            for x in 0..w {
                if dark(x, y) > 20 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        if x1 <= x0 || y1 <= y0 {
            return (0, 0, Vec::new());
        }
        let (mw, mh) = (x1 - x0 + 1, y1 - y0 + 1);
        let mut mask = Vec::with_capacity((mw * mh) as usize);
        for y in y0..=y1 {
            for x in x0..=x1 {
                mask.push(dark(x, y));
            }
        }
        (mw, mh, mask)
    })
}

/// Brand artwork, decoded once and cached.
///
/// Creste's wordmark is Cormorant Garamond over Hanken Grotesk, and nothing in this crate can
/// rasterise a `.ttf`. So the lockup is set once in the real faces and committed as artwork —
/// which is what a sponsor hands a track builder anyway. The stroked face this module draws is
/// for the moto brands; a house serif is not something to approximate.
///
/// Returns `(width, height, rgba)`.
fn artwork(name: &str) -> Option<&'static (u32, u32, Vec<u8>)> {
    macro_rules! sheet_of {
        ($cell:ident, $file:literal) => {{
            static $cell: std::sync::OnceLock<(u32, u32, Vec<u8>)> = std::sync::OnceLock::new();
            Some($cell.get_or_init(|| match image::load_from_memory(include_bytes!($file)) {
                Ok(img) => {
                    let img = img.to_rgba8();
                    let (w, h) = img.dimensions();
                    (w, h, img.into_raw())
                }
                Err(_) => (0, 0, Vec::new()),
            }))
        }};
    }
    match name {
        "mxbapp-ice" => sheet_of!(A, "../assets/mxbapp-ice.png"),
        "frost" => sheet_of!(B, "../assets/frost.png"),
        "creste" => sheet_of!(C, "../assets/creste.png"),
        "frostmod" => sheet_of!(D, "../assets/frostmod.png"),
        "mxbapp-dark" => sheet_of!(E, "../assets/mxbapp-dark.png"),
        _ => None,
    }
}

/// Sample artwork at `(u, v)`, both 0..1 across its own box. Bilinear, premultiplied by its
/// own alpha so the edges stay clean over any ground.
fn art_at(art: &(u32, u32, Vec<u8>), u: f32, v: f32) -> ([f32; 3], f32) {
    let (w, h, px) = art;
    if *w == 0 || !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
        return ([0.0; 3], 0.0);
    }
    let (fx, fy) = (u * (*w - 1) as f32, v * (*h - 1) as f32);
    let (x0, y0) = (fx.floor() as u32, fy.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let at = |x: u32, y: u32, k: usize| px[((y * w + x) * 4) as usize + k] as f32;
    let mix = |k: usize| {
        let top = at(x0, y0, k) * (1.0 - tx) + at(x1, y0, k) * tx;
        let bot = at(x0, y1, k) * (1.0 - tx) + at(x1, y1, k) * tx;
        top * (1.0 - ty) + bot * ty
    };
    (std::array::from_fn(mix), mix(3) / 255.0)
}

/// How much of the mark covers `(u, v)`, both 0..1 across its own box. Bilinear, so the edge
/// stays smooth however large it is drawn.
fn mark_at(u: f32, v: f32) -> f32 {
    let (w, h, mask) = mark_mask();
    if *w == 0 || !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
        return 0.0;
    }
    let (fx, fy) = (u * (*w - 1) as f32, v * (*h - 1) as f32);
    let (x0, y0) = (fx.floor() as u32, fy.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let at = |x: u32, y: u32| mask[(y * w + x) as usize] as f32 / 255.0;
    let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
    let bot = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
    top * (1.0 - ty) + bot * ty
}


/// A stake: white plastic, scuffed at the foot. What marks the edge of a motocross track.
///
/// Not bare timber with a painted tip, which is what this used to draw. Indiana's stakes
/// measure a flat light grey — rgb(138,140,135) sampled through their own UVs — with no
/// second population beside them, so the line is all one thing and the variety comes from
/// how they lean rather than from what they are made of.
fn stake_sheet() -> Texture {
    sheet("stake_c", 32, |u, v| {
        let g = grain(u, v * 0.25, 0x33A1, 26.0);
        // Ground-in dirt up the bottom third, because that is the half a rider sees.
        let dirt = ((v - 0.62) / 0.38).clamp(0.0, 1.0) * (0.55 + 0.45 * g);
        let s = 0.94 + 0.06 * g;
        let mix = |clean: f32, soil: f32| ((clean * s) * (1.0 - dirt) + soil * dirt) as u8;
        [mix(238.0, 132.0), mix(240.0, 106.0), mix(236.0, 74.0), 255]
    })
}

/// Where cell `i` of the banner atlas sits down the sheet, as a `v` range.
fn banner_cell(i: usize) -> (f32, f32) {
    let h = BANNER_CELL_PX as f32 / BANNER_ATLAS_PX as f32;
    let top = i as f32 * h;
    (top, top + h)
}

/// The plain band at the foot of the atlas that the posts wear.
fn banner_post_band() -> (f32, f32) {
    (BANNER_CELLS as f32 * BANNER_CELL_PX as f32 / BANNER_ATLAS_PX as f32, 1.0)
}

/// The banner atlas: [`BANNER_PANELS`] printed one under another, and a post band under them.
///
/// One sheet rather than one per panel because a model takes a single material — see the note
/// where the kinds are assembled — and because it is how a published track carries a lap's
/// worth of sponsors. Each panel is a wordmark on a solid ground with a keyline round it,
/// which is what every one of Indiana's fourteen is.
fn banner_sheet() -> Texture {
    let (band_top, _) = banner_post_band();
    // The cell's height over its width. Everything laid out below is in cell coordinates, and
    // artwork has to be told the difference or it comes out stretched.
    let aspect = BANNER_CELL_PX as f32 / BANNER_ATLAS_PX as f32;
    // Decode once up front rather than inside the pixel loop's first call.
    let _ = mark_mask();
    for p in &BANNER_PANELS {
        let _ = artwork(p.art);
    }
    sheet("banner_c", BANNER_ATLAS_PX, |u, v| {
        if v >= band_top {
            // The posts: dull galvanised, no print.
            let g = grain(u, v, 0x71B4, 40.0);
            let s = 0.86 + 0.20 * g;
            return [(150.0 * s) as u8, (152.0 * s) as u8, (156.0 * s) as u8, 255];
        }
        let cell = ((v / aspect) as usize).min(BANNER_CELLS - 1);
        let (top, bot) = banner_cell(cell);
        let cv = (v - top) / (bot - top);
        let p = &BANNER_PANELS[cell];

        let g = grain(u, cv, 0x5C2D + cell as u32, 30.0);
        let s = 0.94 + 0.08 * g;
        let mut base: [f32; 3] = std::array::from_fn(|k| p.ground[k] as f32);
        // A hem all the way round, darker than the ground, so a panel has an edge.
        if cv < 0.055 || cv > 0.945 || u < 0.010 || u > 0.990 {
            base = std::array::from_fn(|k| p.ground[k] as f32 * 0.7);
            return [(base[0] * s) as u8, (base[1] * s) as u8, (base[2] * s) as u8, 255];
        }

        // Everything sits to the right of the mark, where there is one.
        let left = if p.mark { 0.225 } else { 0.05 };
        if p.mark {
            let (mw, mh) = (0.15f32, 0.78f32);
            let a = mark_at((u - 0.045) / mw, (cv - 0.11) / mh);
            // The mark takes the wordmark's own ink, which every lockup shares with it.
            let ink = mark_ink(p.art);
            for k in 0..3 {
                base[k] = base[k] * (1.0 - a) + ink[k] as f32 * a;
            }
        }

        // The lockup, fitted inside what is left of the panel and never stretched.
        if let Some(art) = artwork(p.art) {
            let (aw, ah) = (art.0 as f32, art.1 as f32);
            let (bu0, bu1) = (left, 0.965);
            let (box_w, box_h) = (bu1 - bu0, 0.74f32);
            let scale = (box_w / aw).min(box_h * aspect / ah);
            let (fw, fh) = (aw * scale, ah * scale / aspect);
            let (au, av) = (
                (u - (bu0 + bu1) * 0.5 + fw * 0.5) / fw,
                (cv - 0.5 + fh * 0.5) / fh,
            );
            let (col, a) = art_at(art, au, av);
            for k in 0..3 {
                base[k] = base[k] * (1.0 - a) + col[k] * a;
            }
        }
        [(base[0] * s) as u8, (base[1] * s) as u8, (base[2] * s) as u8, 255]
    })
}

/// The colour the mark is printed in on a given panel — the wordmark's own ink.
fn mark_ink(art: &str) -> [u8; 3] {
    match art {
        "mxbapp-ice" => [156, 207, 236],
        "mxbapp-dark" => [13, 18, 22],
        _ => [242, 243, 245],
    }
}

/// Mesh fencing: a grid you can see through, which is the whole reason it is a cut-out.
fn fence_sheet() -> Texture {
    sheet("fence_c_a", 64, |u, v| {
        let line = |t: f32| (t * 16.0).fract() < 0.22;
        // Posts down the sides and a rail top and bottom keep the panel readable at distance.
        let frame = v < 0.05 || v > 0.95 || u < 0.03 || u > 0.97;
        if frame {
            [120, 122, 126, 255]
        } else if line(u) || line(v) {
            [150, 152, 156, 210]
        } else {
            [0, 0, 0, 0]
        }
    })
}

/// A block: a white body on a dark base, which is what stops it reading as a paper cube.
fn bale_mesh() -> Mesh {
    let mut m = edfwrite::moved(
        &edfwrite::cuboid(BALE_W_M, BALE_H_M * 0.82, BALE_D_M),
        [0.0, BALE_H_M * 0.09, 0.0],
    );
    m.append(&edfwrite::moved(
        &edfwrite::cuboid(BALE_W_M * 1.04, BALE_H_M * 0.18, BALE_D_M * 1.04),
        [0.0, -BALE_H_M * 0.41, 0.0],
    ));
    m
}

fn bale_sheet() -> Texture {
    sheet("bale_c", 64, |u, v| {
        let g = grain(u, v, 0x51A7, 32.0);
        // White, the way a modern track's blocks are — hay-coloured ones read as a yellow lump
        // from any distance — over a dark foot, which is what gives it an edge to see.
        let base = if v > 0.86 {
            [58, 60, 64]
        } else if v < 0.06 {
            [206, 208, 210]
        } else {
            [238, 240, 242]
        };
        [
            (base[0] as f32 * (0.86 + 0.2 * g)) as u8,
            (base[1] as f32 * (0.86 + 0.2 * g)) as u8,
            (base[2] as f32 * (0.86 + 0.2 * g)) as u8,
            255,
        ]
    })
}

/// A tree's sheet, in two halves: bark on the left, foliage on the right.
///
/// The tree is solid geometry now rather than crossed cards, so nothing here is cut out — a
/// canopy built as a shape does not need an alpha channel to stop being a slab.
fn tree_sheet() -> Texture {
    sheet("tree_c", 128, |u, v| {
        if u < 0.5 {
            // Bark: vertical grain, because that is the one thing that makes a trunk read.
            let streak = grain(u * 6.0, v, 0x77B1, 30.0);
            let s = 0.72 + 0.46 * streak;
            [(96.0 * s) as u8, (72.0 * s) as u8, (52.0 * s) as u8, 255]
        } else {
            let g = grain(u, v, 0x3D19, 34.0);
            let h = grain(u, v, 0x1A55, 9.0);
            let s = 0.58 + 0.34 * g + 0.16 * h;
            [(62.0 * s) as u8, (112.0 * s) as u8, (50.0 * s) as u8, 255]
        }
    })
}

fn gate_sheet() -> Texture {
    sheet("gate_c", 64, |u, v| {
        let g = grain(u, v, 0x9E11, 24.0);
        if v < 0.28 {
            [(30.0 + 30.0 * g) as u8, (60.0 + 40.0 * g) as u8, (120.0 + 40.0 * g) as u8, 255]
        } else {
            [(190.0 + 40.0 * g) as u8, (190.0 + 40.0 * g) as u8, (186.0 + 40.0 * g) as u8, 255]
        }
    })
}

// ---------------------------------------------------------------------------
// Placement
// ---------------------------------------------------------------------------

/// A tree with a shape rather than a picture of one.
///
/// Crossed cards are what published tracks use and they read fine at a distance, but close up
/// they are two flat pictures and you can see them turn as you ride past. This is a tapered
/// trunk and a canopy of stacked rings — about forty triangles, solid from every angle, and
/// no alpha channel needed because a shape does not have to be cut out of anything.
///
/// UVs put the trunk in the left half of the sheet and the foliage in the right; see
/// [`tree_sheet`].
/// The sky, as a band round the horizon rather than a lid over the plot.
///
/// A closed dome is what a track ships — Mt Morris carries `dome_R26.edf` — but a closed dome
/// built here came out *dark*: TerrainEd bakes shadow volumes from every scene in the file,
/// the sun stands 54° up, and a lid over the whole plot puts the entire track in its shadow.
/// The riding line went from dark brown to unreadable.
///
/// Open above [`SKY_TOP_DEG`] and the sun comes through, while the part a rider actually sees
/// — the horizon and a little way up from it — is still the track's own. Looking straight up
/// gets the game's sky, which is no loss on a bike.
fn dome_mesh(radius: f32) -> Mesh {
    const RINGS: usize = 5;
    const SIDES: usize = 24;
    let mut m = Mesh::default();
    let top = SKY_TOP_DEG.to_radians().tan();
    let mut ring_at = |mesh: &mut Mesh, t: f32| -> u32 {
        let start = mesh.vertex_count() as u32;
        let y = radius * top * t;
        for k in 0..=SIDES {
            let a = std::f32::consts::TAU * k as f32 / SIDES as f32;
            let (x, z) = (a.sin() * radius, a.cos() * radius);
            mesh.positions.extend_from_slice(&[x, y, z]);
            let l = (x * x + z * z).sqrt().max(1e-4);
            mesh.normals.extend_from_slice(&[-x / l, 0.0, -z / l]);
            mesh.uvs.extend_from_slice(&[k as f32 / SIDES as f32 * 4.0, 1.0 - t]);
        }
        start
    };
    let mut lo = ring_at(&mut m, 0.0);
    for i in 1..=RINGS {
        let hi = ring_at(&mut m, i as f32 / RINGS as f32);
        for k in 0..SIDES as u32 {
            let (a, b) = (lo + k, lo + k + 1);
            let (c, d) = (hi + k, hi + k + 1);
            m.indices.extend_from_slice(&[a, c, b]);
            m.indices.extend_from_slice(&[b, c, d]);
        }
        lo = hi;
    }
    m
}

/// The sky's own sheet: blue overhead, pale at the horizon, with cloud banded across it.
fn dome_sheet() -> Texture {
    sheet("sky_c", 256, |u, v| {
        // v is 0 at the zenith and 1 at the horizon — see `dome_mesh`'s uvs.
        let up = 1.0 - v;
        let base = [
            60.0 + 130.0 * (1.0 - up).powf(1.6),
            110.0 + 120.0 * (1.0 - up).powf(1.4),
            180.0 + 55.0 * (1.0 - up).powf(1.2),
        ];
        // Cloud: two scales of noise, thresholded so it comes out as banks rather than fog,
        // and thinned towards the top where a flat sheet would read as a ceiling.
        let n = 0.6 * grain(u * 3.0, v * 6.0, 0x9A11, 5.0) + 0.4 * grain(u * 9.0, v * 14.0, 0x9A12, 11.0);
        let cloud = ((n - 0.52) * 3.4).clamp(0.0, 1.0) * (0.35 + 0.65 * (1.0 - up));
        let c = |i: usize| (base[i] * (1.0 - cloud) + 246.0 * cloud).clamp(0.0, 255.0) as u8;
        [c(0), c(1), c(2), 255]
    })
}

/// The yellow post that stands beside a jump's takeoff.
///
/// A post and not a board: every track marks its jumps, and what it marks them with is a
/// stake — a rider coming at a blind crest reads the line of colour, not a sign.
fn jumpmark_mesh(h: f32) -> Mesh {
    let mut m = edfwrite::cuboid(JUMPMARK_W_M, h, JUMPMARK_W_M);
    // A pennant at the top: a triangle off one side of the post, doubled so it reads from
    // both. It is the flag a rider picks up out of the corner of an eye, not the post.
    let (w, flag_h) = (JUMPMARK_FLAG_M, JUMPMARK_FLAG_M * 0.62);
    let y = h * 0.5 - flag_h * 0.5;
    let mut flag = Mesh::default();
    for (px, py) in [(0.0, y + flag_h * 0.5), (0.0, y - flag_h * 0.5), (w, y)] {
        flag.positions.extend_from_slice(&[px, py, 0.0]);
        flag.normals.extend_from_slice(&[0.0, 0.0, 1.0]);
        flag.uvs.extend_from_slice(&[px / w, 0.5 - py * 0.1]);
    }
    flag.indices.extend_from_slice(&[0, 1, 2]);
    m.append(&edfwrite::double_sided(&flag));
    m
}

fn jumpmark_sheet() -> Texture {
    sheet("jumpmark_c", 32, |u, v| {
        let g = grain(u, v * 0.3, 0x8B31, 24.0);
        let k = 0.90 + 0.16 * g;
        [(242.0 * k) as u8, (198.0 * k) as u8, (28.0 * k) as u8, 255]
    })
}

/// A pole: a post with a crossbar near the top. Power, floodlight or flag — at the distance
/// these stand it is a vertical, and what it does is break up the skyline.
fn pole_mesh(h: f32) -> Mesh {
    let mut m = edfwrite::cuboid(0.22, h, 0.22);
    m.append(&edfwrite::moved(&edfwrite::cuboid(1.8, 0.14, 0.14), [0.0, h * 0.86, 0.0]));
    m
}

/// A box van, parked. Two boxes and nothing else: at twenty metres it is a white slab with a
/// dark cab, and every paddock on every track is full of them.
fn van_mesh() -> Mesh {
    let mut m = edfwrite::moved(&edfwrite::cuboid(6.0, 2.5, 2.4), [0.0, 1.55, 0.0]);
    m.append(&edfwrite::moved(&edfwrite::cuboid(2.1, 1.7, 2.3), [-3.6, 1.15, 0.0]));
    m
}

fn pole_sheet() -> Texture {
    sheet("pole_c", 32, |u, v| {
        let g = grain(u, v * 0.3, 0x71B3, 24.0);
        let s = 0.82 + 0.24 * g;
        [(142.0 * s) as u8, (140.0 * s) as u8, (134.0 * s) as u8, 255]
    })
}

/// A van's paint: white with a band of colour through it, which is what a team truck is
/// without being anybody's actual livery.
fn van_sheet() -> Texture {
    sheet("van_c", 64, |u, v| {
        let g = grain(u, v, 0x24C9, 28.0);
        let base = if (0.30..0.46).contains(&v) {
            [190.0, 54.0, 44.0]
        } else if v < 0.18 {
            [56.0, 58.0, 62.0]
        } else {
            [236.0, 238.0, 240.0]
        };
        let s = 0.90 + 0.12 * g;
        [(base[0] * s) as u8, (base[1] * s) as u8, (base[2] * s) as u8, 255]
    })
}

/// A tree of one kind or the other, so the wood comes out mixed.
fn stand(h: f32, seed: u32, key: u32) -> Mesh {
    if rnd(seed ^ 0x46, key) < PINE_SHARE {
        pine_mesh(h * 1.25, seed, key)
    } else {
        tree_mesh(h, seed, key)
    }
}

/// A conifer: a straight trunk and three skirts of needles narrowing to a point.
///
/// Its own mesh rather than a taller broadleaf, because at a distance a tree is a silhouette
/// and nothing else — a ridge of pines behind a track is most of what tells you where it is.
fn pine_mesh(h: f32, seed: u32, i: u32) -> Mesh {
    const SIDES: usize = 6;
    let mut m = Mesh::default();
    let trunk_h = h * 0.22;
    let r_trunk = (h * 0.028).max(0.05);

    let ring = |mesh: &mut Mesh, y: f32, r: f32, v: f32| -> u32 {
        let start = mesh.vertex_count() as u32;
        for s in 0..SIDES {
            let a = std::f32::consts::TAU * s as f32 / SIDES as f32;
            let (sx, sz) = (a.sin() * r, a.cos() * r);
            mesh.positions.extend_from_slice(&[sx, y, sz]);
            let l = (sx * sx + sz * sz).sqrt().max(1e-4);
            mesh.normals.extend_from_slice(&[sx / l, 0.0, sz / l]);
            mesh.uvs.extend_from_slice(&[0.06 + 0.10 * (s as f32 / SIDES as f32), v]);
        }
        start
    };
    let mut band = |mesh: &mut Mesh, lo: u32, hi: u32| {
        for s in 0..SIDES as u32 {
            let n = (s + 1) % SIDES as u32;
            mesh.indices.extend_from_slice(&[lo + s, lo + n, hi + s]);
            mesh.indices.extend_from_slice(&[hi + s, lo + n, hi + n]);
            mesh.indices.extend_from_slice(&[hi + s, lo + n, lo + s]);
            mesh.indices.extend_from_slice(&[hi + n, lo + n, hi + s]);
        }
    };
    let base = ring(&mut m, 0.0, r_trunk, 0.02);
    let top = ring(&mut m, trunk_h, r_trunk * 0.8, 0.22);
    band(&mut m, base, top);

    // Three skirts, each narrower and higher than the last.
    let spread = 0.85 + 0.30 * rnd(seed ^ 0xC1, i);
    for k in 0..3u32 {
        let f = k as f32 / 3.0;
        let y0 = trunk_h + (h - trunk_h) * f * 0.62;
        let y1 = y0 + (h - y0) * 0.78;
        let r = (h * 0.30 * (1.0 - f * 0.55) * spread).max(0.3);
        let skirt = ring(&mut m, y0, r, 0.45 + 0.15 * f);
        let tip = m.vertex_count() as u32;
        m.positions.extend_from_slice(&[0.0, y1, 0.0]);
        m.normals.extend_from_slice(&[0.0, 1.0, 0.0]);
        m.uvs.extend_from_slice(&[0.62, 0.95]);
        for sd in 0..SIDES as u32 {
            let n = (sd + 1) % SIDES as u32;
            m.indices.extend_from_slice(&[skirt + sd, skirt + n, tip]);
            m.indices.extend_from_slice(&[tip, skirt + n, skirt + sd]);
        }
    }
    m
}

fn tree_mesh(h: f32, seed: u32, i: u32) -> Mesh {
    const SIDES: usize = 7;
    let mut m = Mesh::default();
    let trunk_h = h * 0.42;
    let r_base = (h * 0.045).max(0.06);

    let ring = |mesh: &mut Mesh, y: f32, r: f32, u: f32, v: f32, wobble: f32, k: u32| -> u32 {
        let start = mesh.vertex_count() as u32;
        for s in 0..SIDES {
            let a = std::f32::consts::TAU * s as f32 / SIDES as f32;
            let rr = r * (1.0 - wobble * 0.5 + wobble * rnd(seed ^ 0xB1, k * 31 + s as u32));
            let (sx, sz) = (a.sin() * rr, a.cos() * rr);
            mesh.positions.extend_from_slice(&[sx, y, sz]);
            // Unit length, and not merely pointing the right way: `map::parse` rejects a
            // whole `.map` whose normals aren't unit, because that is its one cheap check
            // that a block really is a vertex block. Tilted outward-and-up by eye and left
            // un-normalised, these came out 1.031 long and TerrainEd's output — which was
            // otherwise perfect — would not read back at all.
            let (ox, oy, oz) = (sx, 0.25 * rr.max(0.01), sz);
            let l = (ox * ox + oy * oy + oz * oz).sqrt().max(1e-4);
            mesh.normals.extend_from_slice(&[ox / l, oy / l, oz / l]);
            mesh.uvs.extend_from_slice(&[u + 0.18 * (s as f32 / SIDES as f32), v]);
        }
        start
    };
    // Wound the way `cuboid` is — see its note. Outward faces come after the reverse.
    let mut band = |mesh: &mut Mesh, lo: u32, hi: u32| {
        for s in 0..SIDES as u32 {
            let n = (s + 1) % SIDES as u32;
            mesh.indices.extend_from_slice(&[lo + s, lo + n, hi + s]);
            mesh.indices.extend_from_slice(&[hi + s, lo + n, hi + n]);
        }
    };

    // Trunk: two rings, tapering.
    let t0 = ring(&mut m, 0.0, r_base, 0.04, 0.95, 0.15, i * 3);
    let t1 = ring(&mut m, trunk_h, r_base * 0.72, 0.04, 0.05, 0.15, i * 3 + 1);
    band(&mut m, t0, t1);

    // Canopy: three rings and a cap, widest a third of the way up.
    let cw = h * 0.34;
    let c0 = ring(&mut m, trunk_h * 0.82, cw * 0.55, 0.55, 0.96, 0.30, i * 3 + 2);
    let c1 = ring(&mut m, trunk_h + (h - trunk_h) * 0.30, cw, 0.55, 0.62, 0.30, i * 5);
    let c2 = ring(&mut m, trunk_h + (h - trunk_h) * 0.68, cw * 0.74, 0.55, 0.30, 0.30, i * 5 + 1);
    band(&mut m, c0, c1);
    band(&mut m, c1, c2);
    let apex = m.vertex_count() as u32;
    m.positions.extend_from_slice(&[0.0, h, 0.0]);
    m.normals.extend_from_slice(&[0.0, 1.0, 0.0]);
    m.uvs.extend_from_slice(&[0.72, 0.04]);
    for s in 0..SIDES as u32 {
        let n = (s + 1) % SIDES as u32;
        m.indices.extend_from_slice(&[c2 + s, c2 + n, apex]);
    }
    m
}

/// A banner: a printed panel slung between two stakes.
/// Squeeze a mesh's `v` into one band of the atlas, leaving `u` alone.
fn in_band(mesh: &Mesh, (top, bot): (f32, f32)) -> Mesh {
    let mut m = mesh.clone();
    for uv in m.uvs.chunks_exact_mut(2) {
        uv[1] = top + uv[1].clamp(0.0, 1.0) * (bot - top);
    }
    m
}

/// A banner: the `cell`-th printed panel slung between two posts.
///
/// The panel takes its own cell of the atlas and the posts take the plain band under them, so
/// a lap's banners are one mesh and one material and still say four different things.
fn banner_mesh(cell: usize) -> Mesh {
    let mut m = in_band(
        &edfwrite::double_sided(&edfwrite::moved(
            &edfwrite::card(BANNER_W_M, BANNER_H_M),
            [0.0, BANNER_LIFT_M, 0.0],
        )),
        banner_cell(cell % BANNER_CELLS),
    );
    for side in [-1.0f32, 1.0] {
        m.append(&in_band(
            &edfwrite::moved(
                &edfwrite::cuboid(0.08, BANNER_LIFT_M + BANNER_H_M, 0.08),
                [side * BANNER_W_M * 0.5, 0.0, 0.0],
            ),
            banner_post_band(),
        ));
    }
    m
}

/// Build a track's scenery: the models, and where they stand.
pub fn build(prog: &TrackProgram, syn: &Synth) -> Scenery {
    let seed = prog.terrain.relief.seed;
    let lap = prog.lap_length();
    let half = prog.width * 0.5;
    let stations = prog.stations(0.5);
    let at = |s: f32| -> crate::trackprog::Station {
        let i = ((s / 0.5) as usize).min(stations.len().saturating_sub(1));
        stations[i]
    };
    // Every candidate is measured against the whole lap, not against the station it came
    // from. Two metres a step is finer than anything being placed is wide.
    let coarse = prog.stations(2.0);

    // Nothing is planted on the start straight. It is track — forty riders leave the gate
    // across the whole width of it — and the scenery is laid out from the lap, which knows
    // nothing about a spur running beside it.
    let clear_of_the_start = |x: f32, z: f32| -> bool {
        syn.outside_the_start(x, z).map(|e| e > OFF_THE_START_M).unwrap_or(true)
    };

    let mut stakes = Mesh::default();
    let mut banners = Mesh::default();
    let mut fence = Mesh::default();
    let mut bales = Mesh::default();
    let mut trees = Mesh::default();
    let mut poles = Mesh::default();
    let mut jumpmarks = Mesh::default();
    let mut vans = Mesh::default();
    let mut gate = Mesh::default();
    let mut tally = Vec::new();

    // 1. The edge line: little white plastic stakes at the track edge, which is how a
    // motocross track is marked and what Indiana measures — see [`STAKE_OFF_M`]. Thin and
    // low, so what you see down the track is a line of points rather than a wall.
    let stake_off = (half + 1.0).max(STAKE_OFF_M);
    let mut n = 0usize;
    let mut s = 0.0f32;
    while s < lap {
        let st = at(s);
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        for side in [-1.0f32, 1.0] {
            let (x, z) = (st.x + rx * stake_off * side, st.z + rz * stake_off * side);
            if !inside(prog, x, z, 2.0)
                || clearance(&coarse, x, z) < stake_off - 1.0
                || !clear_of_the_start(x, z)
            {
                continue;
            }
            let key = (s / STAKE_GAP_M) as u32 * 2 + (side > 0.0) as u32;
            let h = STAKE_H_M * (0.88 + 0.24 * rnd(seed ^ 0x12, key));
            // They all wear the same plastic, so the variety is in the lean — which is what a
            // line of stakes actually looks like once a meeting has been run on it. Wider than
            // it was, because a shorter stake needs more of it to read as knocked about.
            let lean = (rnd(seed ^ 0x13, key) - 0.5) * 26.0;
            let post = edfwrite::cuboid(STAKE_W_M, h, STAKE_W_M);
            stakes.append(&edfwrite::moved(
                &edfwrite::turned(&post, lean),
                [x, ground(syn, x, z) - 0.03, z],
            ));
            n += 1;
        }
        s += STAKE_GAP_M;
    }
    tally.push(("stakes", n));

    // 2. Banners: few, big, and facing the rider.
    let mut n = 0usize;
    let mut s = BANNER_GAP_M * 0.5;
    while s < lap {
        let st = at(s);
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        let key = (s / BANNER_GAP_M) as u32;
        let side = if rnd(seed ^ 0x51, key) < 0.5 { -1.0f32 } else { 1.0 };
        let off = BANNER_OFF_M.max(half + 2.5);
        let (x, z) = (st.x + rx * off * side, st.z + rz * off * side);
        if inside(prog, x, z, 3.0)
            && clearance(&coarse, x, z) > off - 1.0
            && clear_of_the_start(x, z)
        {
            let (lo, hi) = ground_span(syn, x, z, along(st.heading), BANNER_W_M);
            if hi - lo < 1.0 {
                // Which panel: walked round the lap rather than drawn at random, so the
                // same name never lands twice in a row where a rider would see both.
                banners.append(&edfwrite::moved(
                    &edfwrite::turned(&banner_mesh(n), along(st.heading)),
                    [x, (lo + hi) * 0.5 - 0.05, z],
                ));
                n += 1;
            }
        }
        s += BANNER_GAP_M;
    }
    tally.push(("banners", n));



    // 3. No fence. It ran along the lap and closed the start straight off — the spur runs
    //    beside the circuit and a fence between the two is a wall across where the field
    //    leaves the gate. Dropped everywhere for now rather than routed round the start,
    //    which is the smaller change and the one that can be judged from a bike.
    let _ = &mut fence;


    // 4. Bales where a rider leaves the track fastest: the outside of every corner. Their
    // long side runs along the track edge, which is what `along` is for.
    let mut n = 0usize;
    let mut s = 0.0f32;
    while s < lap {
        let st = at(s);
        if st.curvature.abs() > 1.0 / 45.0 {
            let side = -st.curvature.signum();
            let (rx, rz) = crate::trackprog::right_vector(st.heading);
            // A fixed offset and no jitter: a row of blocks is a *row*, and a row that
            // wanders reads as rubbish left at the edge of a corner rather than as something
            // somebody laid out.
            let off = BALE_OFF_M;
            let (x, z) = (st.x + rx * off * side, st.z + rz * off * side);
            // Just off the edge of the track — which is where a block goes, and it is also
            // why the clearance bar is low: at seven and a half metres from the line, a bar of
            // "a track's width plus two" threw every one of them away.
            if inside(prog, x, z, 3.0)
                && clearance(&coarse, x, z) > half + 0.5
                && clear_of_the_start(x, z)
            {
                let deg = along(st.heading);
                let (lo, hi) = ground_foot(syn, x, z, deg, BALE_W_M, BALE_D_M);
                if hi - lo > 0.8 {
                    s += BALE_W_M + 0.35;
                    continue;
                }
                // One in three. Laid end to end they are a wall; what a corner has is a few
                // blocks with ground between them, and a rider reads the line from the gaps.
                if n % 3 == 0 {
                    bales.append(&edfwrite::moved(
                        &edfwrite::turned(&bale_mesh(), deg),
                        [x, (lo + hi) * 0.5, z],
                    ));
                }
                n += 1;
            }
        }
        s += BALE_W_M + 0.35;
    }
    tally.push(("bales", n / 3));


    // 4b. A yellow board either side of every jump, at the takeoff — which is where a rider
    //     needs to know one is coming. Nothing on the small stuff: a roller with a sign on it
    //     is a track that has run out of things to say.
    let mut n = 0usize;
    for f in &prog.features {
        let takeoff = match f {
            // The top of the takeoff face, which is where a rider needs it — a marker before
            // the ramp is a marker for the ground in front of the jump.
            crate::trackprog::Feature::Tabletop { at, height, length, .. } => {
                let (up, top, _) = crate::trackprog::tabletop_faces(*height, *length);
                Some((at + up + top * 0.15, *height))
            }
            crate::trackprog::Feature::Double { at, height, lip, .. } => {
                // The lip is where the rider leaves the ground, which is the end of the ramp.
                let ramp = crate::trackprog::double_faces(*height, *lip).ramp;
                Some((at + ramp, *height))
            }
            crate::trackprog::Feature::StepUp { at, length, height, .. } => {
                Some((at + length * 0.8, *height))
            }
            _ => None,
        };
        let Some((crest, height)) = takeoff else { continue };
        if height.abs() < JUMPMARK_FROM_M {
            continue;
        }
        let st = at(crest % lap);
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        for side in [-1.0f32, 1.0] {
            let off = half + 0.4;
            let (x, z) = (st.x + rx * off * side, st.z + rz * off * side);
            if !inside(prog, x, z, 2.0) || !clear_of_the_start(x, z) {
                continue;
            }
            // Tall enough to reach the top of the face. A jump's shape fades out across the
            // width of the track, so the ground a post stands on at the edge is the *foot* of
            // the jump — and a flag down there is a flag on the run-up, which is what it
            // looked like. The post grows with the jump so the pennant sits at the crest.
            let h = JUMPMARK_H_M + height.abs() * 0.9;
            jumpmarks.append(&edfwrite::moved(
                &edfwrite::turned(&jumpmark_mesh(h), along(st.heading)),
                [x, ground(syn, x, z) - 0.05, z],
            ));
            n += 1;
        }
    }
    tally.push(("jump boards", n));

    // 5. Trees, out past the fence, thinned so they don't line up with the lap.
    let mut n = 0usize;
    let mut placed: Vec<(f32, f32)> = Vec::new();
    let mut s = 0.0f32;
    while s < lap {
        let st = at(s);
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        // Keyed on where round the lap we are, not on how many have been placed. Keyed on
        // the count, every station drew the same three numbers — because the count only
        // moves when a tree is accepted — so every station after the first proposed trees in
        // the same three spots and the spacing rule threw them all away. One tree on the
        // whole track, and it looked like the spacing rule was too strict.
        let step = (s / TREE_SPACING_M) as u32;
        for k in 0..3u32 {
            let i = step * 7 + k;
            if rnd(seed ^ 0x41, i) > 0.45 {
                continue;
            }
            let side = if rnd(seed ^ 0x42, i) < 0.5 { -1.0 } else { 1.0 };
            let off = TREE_FROM_M + (TREE_TO_M - TREE_FROM_M) * rnd(seed ^ 0x43, i);
            let (x, z) = (st.x + rx * off * side, st.z + rz * off * side);
            if !inside(prog, x, z, 6.0)
                || clearance(&coarse, x, z) < TREE_FROM_M - 6.0
                || !clear_of_the_start(x, z)
            {
                continue;
            }
            // Nothing where the track is, and nothing on top of another tree.
            if placed
                .iter()
                .any(|(px, pz)| (px - x).powi(2) + (pz - z).powi(2) < TREE_SPACING_M.powi(2))
            {
                continue;
            }
            placed.push((x, z));
            let h = TREE_H_M * (0.7 + 0.6 * rnd(seed ^ 0x44, i));
            trees.append(&edfwrite::moved(
                &edfwrite::turned(&stand(h, seed, i), 360.0 * rnd(seed ^ 0x45, i)),
                [x, ground_min(syn, x, z, h * 0.2) - 0.05, z],
            ));
            n += 1;
        }
        s += TREE_SPACING_M;
    }
    tally.push(("trees", n));

    // 5b. The wood behind them, which is the part that makes a place: a grid of trees from
    //     past the furniture out to the edge of the plot, thinned by noise so it reads as a
    //     wood rather than as an orchard, and kept off everything else.
    let mut n = 0usize;
    let (sx, sz) = (prog.terrain.size_x, prog.terrain.size_z);
    let mut gz = WOOD_STEP_M * 0.5;
    while gz < sz {
        let mut gx = WOOD_STEP_M * 0.5;
        while gx < sx {
            let key = (gz / WOOD_STEP_M) as u32 * 4096 + (gx / WOOD_STEP_M) as u32;
            // Jittered off the grid, or the wood comes out in rows.
            let x = gx + (rnd(seed ^ 0x61, key) - 0.5) * WOOD_STEP_M * 0.9;
            let z = gz + (rnd(seed ^ 0x62, key) - 0.5) * WOOD_STEP_M * 0.9;
            gx += WOOD_STEP_M;
            if rnd(seed ^ 0x63, key) > 0.62 || !inside(prog, x, z, 4.0) {
                continue;
            }
            if !clear_of_the_start(x, z) {
                continue;
            }
            // The wood proper stands past everything else. Inside that — the infield, and the
            // pockets the lap folds round — a *few* trees, thinned right down: they are what
            // stops the middle of a track being a bare field, and any more would be a hedge
            // across the view of the next corner.
            let room = clearance(&coarse, x, z);
            if room < INFIELD_TREE_FROM_M {
                continue;
            }
            if room < WOOD_FROM_M && rnd(seed ^ 0x66, key) > INFIELD_TREE_SHARE {
                continue;
            }
            let h = TREE_H_M * (0.75 + 0.7 * rnd(seed ^ 0x64, key));
            trees.append(&edfwrite::moved(
                &edfwrite::turned(&stand(h, seed, key), 360.0 * rnd(seed ^ 0x65, key)),
                [x, ground_min(syn, x, z, h * 0.2) - 0.05, z],
            ));
            n += 1;
        }
        gz += WOOD_STEP_M;
    }
    tally.push(("wood", n));

    // 6. Poles and vans — twenty and twenty-five a kilometre on the tracks measured, standing
    //    18–26 m and 15–25 m off the line. Neither is scenery you look at; together they are
    //    what stops the ground beside a track reading as an empty field.
    let mut n = 0usize;
    let mut s = 0.0f32;
    while s < lap {
        let st = at(s);
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        let key = (s / POLE_GAP_M) as u32;
        let side = if rnd(seed ^ 0x71, key) < 0.5 { -1.0f32 } else { 1.0 };
        let off = 18.0 + 8.0 * rnd(seed ^ 0x72, key);
        let (x, z) = (st.x + rx * off * side, st.z + rz * off * side);
        if inside(prog, x, z, 3.0) && clearance(&coarse, x, z) > off - 2.0 && clear_of_the_start(x, z)
        {
            let h = POLE_H_M * (0.85 + 0.4 * rnd(seed ^ 0x73, key));
            poles.append(&edfwrite::moved(&pole_mesh(h), [x, ground(syn, x, z) - 0.1, z]));
            n += 1;
        }
        s += POLE_GAP_M;
    }
    tally.push(("poles", n));

    // No parked vans. They are what a paddock is full of, but ours stood close enough to the
    // riding line to be something a rider hits — a white box with a stripe down it, solid, in
    // the way. Out until they can be put somewhere that is actually a paddock.
    let _ = &mut vans;


    // 7. The sky over all of it.
    let (sx, sz) = (prog.terrain.size_x, prog.terrain.size_z);
    let sky = edfwrite::moved(
        &dome_mesh(sx.max(sz) * 0.95),
        [sx * 0.5, ground(syn, sx * 0.5, sz * 0.5) - 2.0, sz * 0.5],
    );
    tally.push(("sky", 1));

    // 6. The start gantry, over the gate row — which is on the start straight, off to the
    //    side of the lap, not on the lap itself. It spans the whole row, and the row is 54 m
    //    wide where the gates stand.
    //
    // Each post stands on its own ground: level them together and one is buried and the other
    // is in the air. The beam clears the higher of the two, and it spans *across* the track,
    // so it turns by `across` — turned by `along` it lay down the track instead of over it,
    // which is the ninety degrees you could see.
    let (st, gate_span) = match &syn.spur {
        Some(spur) => {
            let g = spur.gate_at();
            let i = ((g / 0.5) as usize).min(spur.stations.len().saturating_sub(1));
            (spur.stations[i], spur.at(g) + 3.0)
        }
        None => (at(2.0), half + 3.0),
    };
    let (rx, rz) = crate::trackprog::right_vector(st.heading);
    let mut highest = f32::NEG_INFINITY;
    for side in [-1.0f32, 1.0] {
        let (x, z) = (st.x + rx * gate_span * side, st.z + rz * gate_span * side);
        let foot = ground(syn, x, z);
        highest = highest.max(foot);
        gate.append(&edfwrite::moved(&edfwrite::cuboid(0.5, 5.0, 0.5), [x, foot, z]));
    }
    let span = gate_span * 2.0;
    let beam = edfwrite::turned(&edfwrite::cuboid(span, 1.2, 0.3), across(st.heading));
    gate.append(&edfwrite::moved(&beam, [st.x, highest + 4.6, st.z]));
    tally.push(("gate", 1));

    // One model a kind, each with its own single sheet. Not one model of several materials:
    // TerrainEd faults on the second material in a model whatever the geometry — see
    // `edfwrite`'s note and the case-by-case test behind it. PiBoSo's own example track is
    // built the same way, three `scene` blocks for three objects.
    let kinds: Vec<(&str, Mesh, Texture, bool)> = vec![
        ("stakes", stakes, stake_sheet(), false),
        ("banners", banners, banner_sheet(), true),
        ("bales", bales, bale_sheet(), true),
        // Not solid: clipping a marker board should cost a rider nothing.
        ("jumpmarks", jumpmarks, jumpmark_sheet(), false),
        ("trees", trees, tree_sheet(), true),
        ("poles", poles, pole_sheet(), false),
        // The sky is drawn and nothing else: a dome you can ride into is not a sky.
        ("sky", sky, dome_sheet(), false),
        ("gate", gate, gate_sheet(), true),
    ];

    let mut files = Vec::new();
    let mut drawn = Vec::new();
    let mut solid = Vec::new();
    for (name, mesh, sheet, is_solid) in kinds {
        if mesh.vertex_count() < 8 {
            continue;
        }
        let file = format!("{name}.edf");
        let bytes = edfwrite::write(name, &[Part { name: name.into(), mesh, texture: 0, normal: None }], &[sheet]);
        files.push((file.clone(), bytes));
        let at = Scene { file, pos: [0.0, 0.0, 0.0], rot: [0.0, 0.0, 0.0] };
        // Collision only for what should stop a bike. A stake snaps and the fence is behind
        // the run-off, so neither is a wall; a tree, a bale and the gantry are.
        if is_solid {
            solid.push(at.clone());
        }
        drawn.push(at);
    }

    Scenery { files, drawn, solid, tally }
}

/// The `scene<N>` blocks, in the form TerrainEd reads them.
pub fn blocks(scenes: &[Scene]) -> String {
    let mut s = String::new();
    for (i, sc) in scenes.iter().enumerate() {
        s.push_str(&format!(
            "\nscene{i}\n{{\n\tname = {}\n\tpos\n\t{{\n\t\tx = {:.3}\n\t\ty = {:.3}\n\t\tz = {:.3}\n\t}}\n\
             \trot\n\t{{\n\t\tx = {:.3}\n\t\ty = {:.3}\n\t\tz = {:.3}\n\t}}\n}}\n",
            sc.file, sc.pos[0], sc.pos[1], sc.pos[2], sc.rot[0], sc.rot[1], sc.rot[2],
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> (TrackProgram, Synth) {
        let p: TrackProgram = serde_json::from_str(crate::trackprog::EXAMPLE).unwrap();
        let s = crate::tracksynth::synthesise(&p).unwrap();
        (p, s)
    }

    #[test]
    fn a_lap_gets_an_edge_line_the_whole_way_round() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let stakes = sc.tally.iter().find(|(k, _)| *k == "stakes").unwrap().1;
        let want = (p.lap_length() / STAKE_GAP_M * 2.0) as usize;
        assert!(
            stakes > want * 8 / 10,
            "{stakes} stakes for a {:.0} m lap, expected about {want}",
            p.lap_length()
        );
    }

    /// Every panel says something, and no two of them say the same thing in the same place.
    ///
    /// The failure this guards against is silent: a cell whose text renders as nothing, or a
    /// UV band a rounding off, leaves a lap of blank coloured slabs that still builds and
    /// still ships. Ink coverage is the cheapest proof that a wordmark is actually printed.
    #[test]
    fn every_banner_panel_is_printed_and_they_all_differ() {
        let t = banner_sheet();
        assert_eq!(t.width, BANNER_ATLAS_PX);
        assert_eq!(t.height, BANNER_ATLAS_PX);
        let row = |v: f32| (v * t.height as f32) as u32;

        let mut seen: Vec<Vec<u8>> = Vec::new();
        for cell in 0..BANNER_CELLS {
            let (top, bot) = banner_cell(cell);
            // The middle band of the cell, where the line of text sits.
            let (y0, y1) = (row(top + (bot - top) * 0.3), row(top + (bot - top) * 0.7));
            let mut ink = 0usize;
            let mut total = 0usize;
            let mut hist = std::collections::BTreeMap::<[u8; 3], usize>::new();
            for y in y0..y1 {
                for x in 0..t.width {
                    let o = ((y * t.width + x) * 4) as usize;
                    let px = [t.rgba[o], t.rgba[o + 1], t.rgba[o + 2]];
                    *hist.entry(px.map(|c| c / 32 * 32)).or_default() += 1;
                    total += 1;
                }
            }
            // The commonest colour is the ground; anything else is print.
            let ground = *hist.iter().max_by_key(|(_, n)| **n).unwrap().0;
            for (px, n) in &hist {
                if *px != ground {
                    ink += n;
                }
            }
            let share = ink as f32 / total as f32;
            let text = BANNER_PANELS[cell].art;
            assert!(
                (0.05..0.60).contains(&share),
                "panel {cell} ({text}) is {:.0}% ink — blank or solid, not printed",
                share * 100.0
            );
            let strip: Vec<u8> = (0..t.width)
                .map(|x| t.rgba[(((y0 + y1) / 2 * t.width + x) * 4) as usize])
                .collect();
            assert!(
                !seen.contains(&strip),
                "panel {cell} ({text}) prints the same line as one before it"
            );
            seen.push(strip);
        }

        // And the posts wear the plain band, not a slice of somebody's name.
        let (band, _) = banner_post_band();
        let m = banner_mesh(0);
        let post_vs: Vec<f32> = m.uvs.chunks_exact(2).skip(8).map(|uv| uv[1]).collect();
        assert!(
            !post_vs.is_empty() && post_vs.iter().all(|v| *v >= band - 1e-4),
            "the posts sample the printed cells"
        );
    }

    /// Write the sheets out as PNGs, which is the only way to judge whether a banner reads.
    ///
    /// ```text
    /// FROST_SHEETS=/tmp/gen cargo test --bin mxb-app -- --ignored --nocapture the_sheets
    /// ```
    #[test]
    #[ignore = "writes PNGs — set FROST_SHEETS"]
    fn the_sheets() {
        let dir = std::env::var("FROST_SHEETS").expect("set FROST_SHEETS");
        std::fs::create_dir_all(&dir).unwrap();
        for t in [banner_sheet(), stake_sheet(), jumpmark_sheet(), bale_sheet()] {
            let file = format!("{dir}/{}.png", t.name);
            image::RgbaImage::from_raw(t.width, t.height, t.rgba.clone())
                .unwrap()
                .save(&file)
                .unwrap();
            println!("{file} {}x{}", t.width, t.height);
        }
    }

    /// A lap shows more than one banner. One cell used everywhere is the same failure as a
    /// blank cell, and it only shows up on the bike.
    #[test]
    fn a_lap_cycles_through_the_panels() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let banners = sc.tally.iter().find(|(k, _)| *k == "banners").unwrap().1;
        assert!(banners >= BANNER_CELLS, "{banners} banners — too few to cycle");
        let edf = &sc.files.iter().find(|(f, _)| f == "banners.edf").unwrap().1;
        let nodes = crate::edf::parse_world(edf);
        assert!(!nodes.is_empty(), "the banner model reads back");
        let bands: std::collections::BTreeSet<u32> = nodes
            .iter()
            .flat_map(|n| n.uvs.chunks_exact(2))
            .map(|uv| (uv[1] * BANNER_ATLAS_PX as f32 / BANNER_CELL_PX as f32) as u32)
            .collect();
        assert!(
            bands.len() > BANNER_CELLS,
            "banners sample {} bands of the atlas — the lap is not cycling",
            bands.len()
        );
    }

    #[test]
    fn jumps_are_marked_and_rollers_are_not() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let boards = sc.tally.iter().find(|(k, _)| *k == "jump boards").unwrap().1;
        // Two boards a jump, and only for the ones worth marking.
        let worth = p
            .features
            .iter()
            .filter(|f| {
                f.height().abs() >= JUMPMARK_FROM_M
                    && matches!(
                        f,
                        crate::trackprog::Feature::Tabletop { .. }
                            | crate::trackprog::Feature::Double { .. }
                            | crate::trackprog::Feature::StepUp { .. }
                    )
            })
            .count();
        assert!(worth > 0, "the demo has jumps worth marking");
        assert!(
            boards >= worth && boards <= worth * 2,
            "{boards} boards for {worth} jumps"
        );
    }

    #[test]
    fn a_lap_gets_a_wood_behind_it_and_not_one_tree() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let count = |k: &str| sc.tally.iter().find(|(n, _)| *n == k).map(|(_, v)| *v).unwrap_or(0);
        let km = p.lap_length() / 1000.0;

        // Trackside trees are rare on the tracks this is measured against — 2.4–5 a kilometre
        // — and what makes those places is the wood behind them.
        let near = count("trees") as f32 / km;
        assert!(near > 1.0 && near < 12.0, "{near:.1} trees/km trackside");

        // The wood itself. Lakewood stands fifteen thousand; a 500 m plot has room for
        // hundreds, and the failure this catches is the one that happened — randomness keyed
        // on the placed count, so every station drew the same numbers and the whole lap ended
        // up with a single tree.
        let wood = count("wood");
        assert!(wood > 200, "the backdrop is {wood} trees");
    }

    #[test]
    fn nothing_stands_on_the_riding_line() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let nodes: Vec<crate::edf::EdfNode> = sc
            .files
            .iter()
            .flat_map(|(_, b)| crate::edf::parse_world(b))
            .collect();
        assert!(nodes.len() >= 4, "a kind a model: {}", nodes.len());

        // Every vertex, against the corridor the track was built with. The edge line sits
        // just outside the shoulder; nothing may sit inside the riding surface itself.
        let stations = p.stations(1.0);
        let half = p.width * 0.5;
        // Anything a rider's head would pass under may cross the line — the start gantry is
        // meant to. What matters is that nothing is in the way at riding height.
        let ride_h = 3.0;
        let mut worst = f32::INFINITY;
        for n in &nodes {
            for v in n.positions.chunks_exact(3) {
                let g = ground(&s, v[0], v[2]);
                if v[1] - g > ride_h {
                    continue;
                }
                let d = stations
                    .iter()
                    .map(|st| ((st.x - v[0]).powi(2) + (st.z - v[2]).powi(2)).sqrt())
                    .fold(f32::INFINITY, f32::min);
                worst = worst.min(d);
            }
        }
        assert!(
            worst > half,
            "something stands {worst:.1} m from the centreline, inside a {half:.1} m half-width"
        );
    }

    #[test]
    fn everything_stands_on_the_ground_it_was_placed_on() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let nodes = crate::edf::parse_world(&sc.files[0].1);

        // How far a thing's foot may sit under the surface, by kind. Not one number: a
        // seven-metre foliage card is placed on the lowest ground it covers precisely so it
        // never floats, and its far corner going under a rise is what that costs — and what
        // you want, because a tree hovering over a bank is the thing you would notice. A
        // fence panel or a gate post is built, flat-footed and man-high, and one sunk half a
        // metre reads as broken.
        let allowed = |name: &str| if name == "trees" { 1.5f32 } else { 0.6 };

        let mut worst: (f32, String, [f32; 3]) = (0.0, String::new(), [0.0; 3]);
        for n in &nodes {
            let limit = allowed(&n.name);
            for v in n.positions.chunks_exact(3) {
                let under = ground(&s, v[0], v[2]) - v[1];
                if under > limit && under - limit > worst.0 - allowed(&worst.1) {
                    worst = (under, n.name.clone(), [v[0], v[1], v[2]]);
                }
            }
        }
        assert!(
            worst.1.is_empty(),
            "{} is buried {:.2} m at {:?}, over its {:.1} m allowance",
            worst.1, worst.0, worst.2, allowed(&worst.1)
        );
    }

    #[test]
    fn every_normal_is_unit_length() {
        // `map::parse` refuses a map whose normals are not unit — it is how it tells a vertex
        // block from noise — so a model that ships a normal of length 1.03 makes TerrainEd's
        // output unreadable even though TerrainEd compiled it perfectly.
        let (p, s) = demo();
        let sc = build(&p, &s);
        for (name, bytes) in &sc.files {
            for n in crate::edf::parse_world(bytes) {
                for v in n.normals.chunks_exact(3) {
                    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                    assert!(
                        (l - 1.0).abs() < 1e-3,
                        "{name}/{} has a normal {l:.4} long: {v:?}",
                        n.name
                    );
                }
            }
        }
    }

    #[test]
    fn collision_is_only_what_should_stop_a_bike() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let named = |v: &[Scene]| -> Vec<String> { v.iter().map(|s| s.file.clone()).collect() };
        let drawn = named(&sc.drawn);
        let solid = named(&sc.solid);
        // No fence: it ran along the lap and closed the start straight off. See `build`.
        // No banners and no fence — see `build`.
        for want in ["stakes.edf", "bales.edf", "trees.edf", "gate.edf"] {
            assert!(drawn.contains(&want.to_string()), "{want} not drawn: {drawn:?}");
        }
        // A tree, a bale and the gantry stop a bike.
        for want in ["bales.edf", "trees.edf", "gate.edf"] {
            assert!(solid.contains(&want.to_string()), "{want} should be solid: {solid:?}");
        }
        // A stake snaps rather than stopping you, and the fence run has gaps where the ground
        // steps — as collision that is a wall with holes in it, which is worse than no wall.
        assert!(!solid.contains(&"stakes.edf".to_string()), "{solid:?}");
        assert!(!drawn.contains(&"fence.edf".to_string()), "the fence is dropped: {drawn:?}");
        // And every placed model is one the export actually writes.
        for f in solid.iter().chain(drawn.iter()) {
            assert!(sc.files.iter().any(|(n, _)| n == f), "{f} is placed but never written");
        }
    }

    #[test]
    fn the_blocks_read_the_way_terrained_writes_them() {
        let s = blocks(&[
            Scene { file: "trees.edf".into(), pos: [1.0, 2.0, 3.0], rot: [0.0, 90.0, 0.0] },
            Scene { file: "bales.edf".into(), pos: [0.0, 0.0, 0.0], rot: [0.0, 0.0, 0.0] },
        ]);
        assert!(s.contains("scene0") && s.contains("scene1"), "{s}");
        assert!(s.contains("name = trees.edf") && s.contains("name = bales.edf"));
        assert!(s.contains("x = 1.000") && s.contains("y = 2.000") && s.contains("z = 3.000"));
        assert!(s.contains("y = 90.000"));
    }
}

#[cfg(test)]
mod built {
    use super::*;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn u32b(v: usize) -> [u8; 4] {
        (v as u32).to_le_bytes()
    }

    #[test]
    #[ignore = "writes and compiles a track — set FROST_BUILD"]
    fn builds_a_track_with_objects() {
        let dir = PathBuf::from(std::env::var("FROST_BUILD").expect("set FROST_BUILD"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // `FROST_PROGRAM` builds a track of your own; without it, the example.
        let src = match std::env::var("FROST_PROGRAM") {
            Ok(f) => std::fs::read_to_string(f).expect("the program file"),
            Err(_) => crate::trackprog::EXAMPLE.to_string(),
        };
        let mut p: TrackProgram = serde_json::from_str(&src).unwrap();
        for note in crate::trackllm::repair_for_tests(&mut p) {
            println!("  repair: {note}");
        }
        let syn = crate::tracksynth::synthesise(&p).unwrap();
        let slug = crate::tracksynth::write_source(&p, &syn, &dir).unwrap();
        println!("{}: {:.0} m lap, {} files", p.name, p.lap_length(), slug.len());

        let sc = build(&p, &syn);
        for (k, n) in &sc.tally {
            println!("  {k:<14} {n}");
        }
        for (name, bytes) in &sc.files {
            println!("  {name:<14} {} KB", bytes.len() / 1024);
        }

        let tools = PathBuf::from(std::env::var("FROST_TOOLS").expect("set FROST_TOOLS"));
        let t = crate::trackbuild::find(&tools).expect("compilers under FROST_TOOLS");
        let name = crate::tracksynth::slug(&p.name);
        println!("slug: {name}");

        let map_rel = format!("{name}/{name}.map");
        let trh_rel = format!("{name}/{name}.trh");
        std::fs::create_dir_all(dir.join(&name)).ok();
        for (label, args) in [
            ("map", vec!["track.hmf", map_rel.as_str(), "params.ini"]),
            ("trh", vec!["track.tht", trh_rel.as_str(), "trh_params.ini"]),
        ] {
            let out = run(&t.terrained, &args, &dir);
            println!("--- {label} ---\n{out}");
        }
        if let Some(tracked) = &t.tracked {
            let out = run(
                tracked,
                &["-merge", &trh_rel, "cl", "track.tcl", "sa", "track_start.tcl"],
                &dir,
            );
            println!("--- centreline ---\n{out}");
        }

        let map_path = dir.join(&map_rel);
        assert!(map_path.is_file(), "no {map_rel} was written");
        let mb = std::fs::read(&map_path).unwrap();
        let mesh = crate::map::parse(&mb).expect("the compiled .map parses");
        let sheets = crate::map::declared(&mb);
        println!(
            "\ncompiled: {} materials, {} vertices, {} triangles, {} islands\nsheets: {:?}",
            mesh.materials,
            mesh.vertex_count(),
            mesh.triangle_count(),
            mesh.objects.len(),
            sheets.iter().map(|(n, ..)| n).collect::<Vec<_>>()
        );
        assert!(mesh.triangle_count() > 1000, "the scenery didn't reach the map");

        // Package, and install where the game lists it.
        let pkz = dir.join(format!("{name}.pkz"));
        let size = crate::trackbuild::package(&dir, &name, &pkz).unwrap();
        println!("packaged {} KB", size / 1024);
        if let Ok(to) = std::env::var("FROST_INSTALL") {
            let at = crate::trackbuild::install(&pkz, Path::new(&to)).unwrap();
            println!("installed {at:?}");
        }

        dump(&dir.join("scene.bin"), &p, &syn, &mesh, &mb);
        println!("dumped {:?}", dir.join("scene.bin"));
    }

    /// Everything a renderer needs, in one file: the ground, the riding line, the scenery
    /// mesh and the sheets it wears. Written so the picture is of what was *compiled*, not of
    /// what we meant to compile.
    fn dump(
        to: &Path,
        p: &TrackProgram,
        syn: &Synth,
        mesh: &crate::map::MapMesh,
        map_bytes: &[u8],
    ) {
        let mut f = std::fs::File::create(to).unwrap();
        let mut w = |b: &[u8]| f.write_all(b).unwrap();
        w(b"SDMP");
        w(&u32b(syn.gw));
        w(&u32b(syn.gh));
        w(&syn.mps.to_le_bytes());
        w(&p.terrain.size_x.to_le_bytes());
        for v in &syn.heights {
            w(&v.to_le_bytes());
        }
        for c in &syn.corridor {
            w(&[*c as u8]);
        }

        // Sheets, reduced so the dump stays small — enough to tell a leaf from a fence.
        const DIM: u32 = 64;
        let textures = crate::map::textures(map_bytes, 256);
        let count = mesh.materials.max(1) as usize;
        w(&u32b(count));
        for m in 0..count {
            let t = textures.iter().find(|t| t.material == m as u32);
            w(&u32b(DIM as usize));
            for y in 0..DIM {
                for x in 0..DIM {
                    let px = match t {
                        Some(t) if t.width > 0 && t.height > 0 => {
                            let sx = (x * t.width / DIM).min(t.width - 1) as usize;
                            let sy = (y * t.height / DIM).min(t.height - 1) as usize;
                            let i = (sy * t.width as usize + sx) * 4;
                            [t.rgba[i], t.rgba[i + 1], t.rgba[i + 2], t.rgba[i + 3]]
                        }
                        _ => [170, 170, 170, 255],
                    };
                    w(&px);
                }
            }
        }

        w(&u32b(mesh.vertex_count()));
        for v in &mesh.positions {
            w(&v.to_le_bytes());
        }
        for v in &mesh.uvs {
            w(&v.to_le_bytes());
        }
        // One material per triangle, from the group runs.
        let tris = mesh.triangle_count();
        let mut mat = vec![0u32; tris];
        for g in &mesh.groups {
            for t in g.tri_start as usize..(g.tri_start + g.tri_count) as usize {
                if t < tris {
                    mat[t] = g.material;
                }
            }
        }
        w(&u32b(tris));
        for i in &mesh.indices {
            w(&i.to_le_bytes());
        }
        for m in &mat {
            w(&m.to_le_bytes());
        }
    }

    fn run(exe: &Path, args: &[&str], dir: &Path) -> String {
        let mut cmd = if cfg!(target_os = "windows") {
            std::process::Command::new(exe)
        } else {
            let mut c = std::process::Command::new(
                std::env::var("FROST_WINE").expect("set FROST_WINE"),
            );
            c.env("WINEPREFIX", std::env::var("FROST_PREFIX").expect("set FROST_PREFIX"));
            c.env("WINEDEBUG", "-all");
            c.arg(exe);
            c
        };
        cmd.args(args).current_dir(dir);
        let out = cmd.output().expect("running the compiler");
        format!(
            "exit {:?}\n{}{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    }
}
