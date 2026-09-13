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
const STAKE_GAP_M: f32 = 6.2;
const STAKE_H_M: f32 = 0.7;
const STAKE_W_M: f32 = 0.045;

/// The advertising boards, and where they run.
///
/// Not banners slung one at a time every fifty metres, which is what this used to build. What
/// a track carries is a **continuous hoarding**: printed plastic panels bolted edge to edge
/// into a run, sharing an upright at every join. Indiana measures the same thing — a line
/// 1.35 m tall at 10.8 m from the centreline, in ~1.1 m pieces, both sides, covering 99% of
/// the lap — and it is the one object that lines the whole circuit.
///
/// So the numbers here are its numbers: 1.35 m tall, panels butted with no gap.
///
/// How far past the track edge the line stands: Indiana's netting is ~12.5 m out from its ±7 m
/// edge. Measured from our edge, so a wider track doesn't swallow it; at 20 m flat it stood
/// 12 m back from a 16 m track.
// Hugging the track, just outside the stakes: at 5.5 m out it marked nothing.
const EDGE_LINE_OUT_M: f32 = 2.0;
/// Most ground a panel may cross end to end before it is left out rather than hung in the air.
const EDGE_LINE_STEP_M: f32 = 0.6;
const BANNER_W_M: f32 = 4.0;
const BANNER_H_M: f32 = 1.35;
/// How thick a board is.
///
/// A printed sheet on a frame, not a sheet on its own: 6 cm is the frame, and it is the
/// smallest thickness that still shows an edge from a bike at ten metres. It is also what
/// stops a board vanishing edge-on — a plane has no width when you look along it, so a run
/// of them used to wink out one by one as you rode past.
const BANNER_D_M: f32 = 0.06;
/// How far off the ground the bottom rail sits. Low: a hoarding is a wall, not a sling.
const BANNER_LIFT_M: f32 = 0.12;
/// Boards in a run, and metres of clear ground between one run and the next. A hoarding that
/// never breaks is a fence; what a track has is runs with the gate, the crossings and the
/// marshal posts between them. A run of tiled banner is the same *length* in metres, which is
/// more pieces because a piece is narrower.
const BANNER_RUN_MIN: usize = 5;
const BANNER_RUN_MAX: usize = 14;
const BANNER_BREAK_MIN_M: f32 = 22.0;
const BANNER_BREAK_MAX_M: f32 = 70.0;

/// The other thing a track lines its lap with: one design printed over and over on piece after
/// piece of the same plastic, strung along stakes rather than bolted to uprights.
///
/// Indiana's is `inflate_tilable_c` and its pieces measure 0.9–1.9 m wide, butted at 1.0–2.1 m,
/// 0.9–1.4 m tall, every one printing the *same* window of the sheet — `trackobjects`'
/// `which_way_a_banner_faces`.
const TILE_H_M: f32 = 1.05;
/// Pieces to a print. Indiana cuts its into 5 and into 20; a whole number puts the seam on a
/// piece's edge, and two is the fewest that still follows ground a board would bridge.
const TILE_PER_PRINT: usize = 2;
/// How far along a run the design comes round again — the cell's own proportions, so a lockup
/// drawn for a 4 m board is not squashed onto a narrower piece.
const TILE_PRINT_M: f32 = BANNER_W_M * TILE_H_M / BANNER_H_M;
const TILE_W_M: f32 = TILE_PRINT_M / TILE_PER_PRINT as f32;
/// Pieces between uprights. A hoarding has one at every join; a strung banner does not, and
/// putting one there is what made this read as a fence the first time.
const TILE_POST_EVERY: usize = 3;
/// How much of a lap's printed line is tiled banner rather than sponsors' boards.
const TILED_SHARE: f32 = 0.5;

/// One printed board: a wordmark on a coloured ground, and whether it carries the app's mark.
///
/// The anatomy is taken from the real ones. Every sponsor panel on Indiana's atlas is a mark,
/// a wordmark and a small strapline under the name — RACE TECH over "THE SCIENCE OF
/// SUSPENSION", and thirteen others follow the same rule.
///
/// The wordmark is **artwork**, not type drawn here. A board is somebody's brand: Frost's Mod Manager's is
/// Barlow Condensed on `--primary`, Creste's is Cormorant Garamond over Hanken Grotesk in its
/// own `--ink` and `--accent`, and nothing in this crate can rasterise a `.ttf`. Drawing a
/// look-alike face was tried and it is exactly as convincing as a look-alike logo. So each
/// lockup is set once in the real faces and committed beside the icon, which is what a sponsor
/// hands a track builder anyway.
struct Panel {
    /// The name of the artwork in [`artwork`], which is also what the board says.
    art: &'static str,
    ground: [u8; 3],
    /// The app's snowflake at the left. Off for a board that is not ours.
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
/// How far outside the track's own edge a bale stands.
///
/// Off the edge, not a fixed distance from the line. It was 7.5 m flat, against a clearance
/// bar of `half a track's width + 0.5` — so the two crossed at a 14 m track and every bale on
/// anything wider was thrown away by the bar for standing too near the corner it belongs to.
/// The old 12 m example cleared it by a metre, which is why nothing showed.
const BALE_OFF_M: f32 = 1.0;
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
// Fewer and further: 32 a km within 60 m against the 2-5 published tracks carry.
const TREE_FROM_M: f32 = 40.0;
const TREE_TO_M: f32 = 58.0;
const TREE_SPACING_M: f32 = 110.0;
const TREE_H_M: f32 = 8.0;

/// How high the sky band reaches, in degrees above the horizon — below the sun, which stands
/// at 54°. See `dome_mesh`.
pub(crate) const SKY_TOP_DEG: f32 = 34.0;

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
// Forty-odd a kilometre, alternating sides, as the published tracks carry them.
const POLE_GAP_M: f32 = 25.0;
const POLE_H_M: f32 = 7.5;
const VAN_GAP_M: f32 = 40.0;

/// The backdrop: where the wood starts, and how thickly it stands out to the edge of the plot.
const WOOD_FROM_M: f32 = 64.0;

/// The backdrop beyond the plot: how far out it reaches, how high it rises at its tallest and
/// least, how many rays make it round, and how far apart its trees stand.
const BACKDROP_REACH_M: f32 = 110.0;
const BACKDROP_RISE_M: (f32, f32) = (8.0, 20.0);
const BACKDROP_RAYS: usize = 256;
const BACKDROP_TREE_M: f32 = 8.0;
// Denser than 9 m at 62%: Oakhanger's whole wood came to 250 trees.
const WOOD_STEP_M: f32 = 6.0;

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

/// A point `s` metres along a polyline, and which way to turn a piece standing on it.
///
/// `cursor` is carried between calls because `s` only ever moves forwards; without it this is
/// a scan of the whole lap for every board.
/// Where a fence stands beside the start straight: on the side the lap runs, just past the
/// straight's edge, from the gate row to a gap at the join. With both open, a rider joining the
/// lap could not tell which way it went; fenced, the only way out is the lap's own direction.
fn spur_fence_line(syn: &Synth, coarse: &[crate::trackprog::Station]) -> Vec<(f32, f32)> {
    let Some(spur) = syn.spur.as_ref() else {
        return Vec::new();
    };
    let (from, to) = (spur.gate_at().max(0.0), spur.length() - SPUR_FENCE_GAP_M);
    spur.stations
        .iter()
        .filter(|q| q.s >= from && q.s <= to)
        .filter_map(|q| {
            let near = coarse
                .iter()
                .min_by(|a, b| (a.x - q.x).hypot(a.z - q.z).total_cmp(&(b.x - q.x).hypot(b.z - q.z)))?;
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            let side = if (near.x - q.x) * rx + (near.z - q.z) * rz >= 0.0 { 1.0 } else { -1.0 };
            let out = spur.at(q.s) + SPUR_FENCE_OUT_M;
            Some((q.x + rx * out * side, q.z + rz * out * side))
        })
        .collect()
}

/// The start fence: how far short of the join it stops, and how far past the straight's edge.
const SPUR_FENCE_GAP_M: f32 = 12.0;
const SPUR_FENCE_OUT_M: f32 = 1.5;

fn along_line(
    line: &[(f32, f32)],
    acc: &[f32],
    cursor: &mut usize,
    s: f32,
) -> (f32, f32, f32) {
    while *cursor + 2 < line.len() && acc[*cursor + 1] < s {
        *cursor += 1;
    }
    let (a, b) = (line[*cursor], line[*cursor + 1]);
    let seg = (acc[*cursor + 1] - acc[*cursor]).max(1e-6);
    let t = ((s - acc[*cursor]) / seg).clamp(0.0, 1.0);
    let deg = (b.0 - a.0).atan2(b.1 - a.1).to_degrees() + 90.0;
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, deg)
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
    /// Each model as built, world metres, with the sheet it wears — what the track's picture
    /// draws, so it shows the scenery the game will.
    pub models: Vec<(Mesh, edfwrite::Texture)>,
}

/// Height of the built ground at a world point, bilinear.
/// A lifted piece set on our ground vertex by vertex: it is stored as height above the donor's.
fn draped(mesh: &Mesh, x: f32, z: f32, lift: f32, syn: &Synth) -> Mesh {
    let mut m = mesh.clone();
    // Never above the ground everywhere: a piece whose donor ground was misread came out
    // floating by that much.
    let low = m.positions.chunks_exact(3).map(|v| v[1]).fold(f32::INFINITY, f32::min);
    let sink = if low.is_finite() { low.max(0.0) } else { 0.0 };
    for v in m.positions.chunks_exact_mut(3) {
        let (wx, wz) = (v[0] + x, v[2] + z);
        v[1] += ground(syn, wx, wz) + lift - sink;
        v[0] = wx;
        v[2] = wz;
    }
    m
}

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
    // The cell's height over its width, which is how far down the sheet a cell reaches.
    let aspect = BANNER_CELL_PX as f32 / BANNER_ATLAS_PX as f32;
    // And the *board's*, which is the one artwork is fitted against: a cell is 1024 by 180 and
    // the board is 4 m by 1.35, so measuring the lockup against the pixels draws it half as
    // wide as it should be, in the left third of the board.
    let board = BANNER_H_M / BANNER_W_M;
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
            // Square on the board, so square here means as many metres across as down.
            let mw = 0.15f32;
            let mh = mw / board;
            let a = mark_at((u - 0.045) / mw, (cv - (1.0 - mh) * 0.5) / mh);
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
            let scale = (box_w / aw).min(box_h * board / ah);
            let (fw, fh) = (aw * scale, ah * scale / board);
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
/// the sun stands 40° up, and a lid over the whole plot puts the entire track in its shadow.
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
    // The drawn sheet is noise, tiled four times round and stretched over the band. A
    // photograph is one exposure that runs horizon to zenith, so it goes round once and the
    // band takes only the slice of it that it actually covers — the first
    // [`SKY_TOP_DEG`] degrees — or the whole sky ends up squeezed into 34°.
    let photo = dome_photo().is_some();
    let wraps = if photo { 1.0 } else { 4.0 };
    let mut ring_at = |mesh: &mut Mesh, t: f32| -> u32 {
        let start = mesh.vertex_count() as u32;
        let y = radius * top * t;
        let v = if photo {
            1.0 - (top * t).atan().to_degrees() / 90.0
        } else {
            1.0 - t
        };
        for k in 0..=SIDES {
            let a = std::f32::consts::TAU * k as f32 / SIDES as f32;
            let (x, z) = (a.sin() * radius, a.cos() * radius);
            mesh.positions.extend_from_slice(&[x, y, z]);
            let l = (x * x + z * z).sqrt().max(1e-4);
            mesh.normals.extend_from_slice(&[-x / l, 0.0, -z / l]);
            mesh.uvs.extend_from_slice(&[k as f32 / SIDES as f32 * wraps, v]);
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

/// The sky as a file of its own: `dome.edf`, named by the `.amb`.
///
/// The sheet is [`dome_photo`] where it loads, and a drawn blue-to-pale gradient with banded
/// cloud behind it otherwise.
///
/// The band in the map exists because TerrainEd bakes shadow volumes from every mesh in the
/// scene, so a lid over the plot put the whole track in shadow. That reasoning does not apply
/// here — a `.edf` the game loads at runtime never goes near the compiler — so this one is
/// closed all the way over, which is what a published track ships and what a rider looking up
/// expects to see.
pub fn dome_file(radius: f32) -> Vec<u8> {
    let sky = dome_sheet();
    let mut m = Mesh::default();
    const RINGS: usize = 8;
    const SIDES: usize = 32;
    // How many times the sheet goes round. A drawn sky is noise and tiling it four times
    // costs nothing, but a photograph is one 360° exposure with a sun in it — wrap that four
    // times and the track gets four suns.
    let wraps = if sky.width == sky.height { 4.0 } else { 1.0 };
    // Ring by ring from the horizon to the pole, facing inwards.
    let ring = |mesh: &mut Mesh, t: f32| -> u32 {
        let start = mesh.vertex_count() as u32;
        let phi = t * std::f32::consts::FRAC_PI_2;
        let (y, r) = (radius * phi.sin(), radius * phi.cos());
        for k in 0..=SIDES {
            let a = std::f32::consts::TAU * k as f32 / SIDES as f32;
            let (x, z) = (a.sin() * r, a.cos() * r);
            mesh.positions.extend_from_slice(&[x, y, z]);
            // Outward, and the mesh is doubled below so the inside still draws.
            //
            // Pointing them inwards is the obvious thing — that is the side a rider sees —
            // and it renders the sky at ambient only: `clear`'s ambient is 0.4, so a day sky
            // comes out at forty per cent of itself, which from the seat is night.
            let l = (x * x + y * y + z * z).sqrt().max(1e-4);
            mesh.normals.extend_from_slice(&[x / l, y / l, z / l]);
            mesh.uvs.extend_from_slice(&[k as f32 / SIDES as f32 * wraps, 1.0 - t]);
        }
        start
    };
    let mut prev = ring(&mut m, 0.0);
    for i in 1..=RINGS {
        let t = i as f32 / RINGS as f32;
        let next = ring(&mut m, t);
        for k in 0..SIDES as u32 {
            // Wound so the inside faces are the ones drawn.
            m.indices.extend_from_slice(&[prev + k, next + k, prev + k + 1]);
            m.indices.extend_from_slice(&[prev + k + 1, next + k, next + k + 1]);
        }
        prev = next;
    }
    let part = Part {
        name: "sky".into(),
        mesh: crate::edfwrite::double_sided(&m),
        texture: 0,
        normal: None,
    };
    crate::edfwrite::write("dome", &[part], &[sky])
}

/// A published track's sky, as a photograph.
///
/// Indiana Pro's own dome sheet, lifted out of its `dome.edf` by [`tests::dump_ground_sheets`]
/// the same way `assets/ground/*.jpg` were lifted from its `.map`. One 360° exposure, 8192 by
/// 2048, zenith on the first row — which is the way round [`sheet`] writes and so the way
/// round `dome_file`'s uvs read it, no flip needed.
///
/// A sky is smooth, so it keeps all eight thousand pixels for about a megabyte of JPEG.
fn dome_photo() -> Option<&'static Texture> {
    static SKY: std::sync::OnceLock<Option<Texture>> = std::sync::OnceLock::new();
    SKY.get_or_init(|| {
        let img = image::load_from_memory(include_bytes!("../assets/sky/dome.jpg")).ok()?;
        let img = img.to_rgba8();
        Some(Texture {
            name: "sky_c".into(),
            width: img.width(),
            height: img.height(),
            rgba: img.into_raw(),
        })
    })
    .as_ref()
}

fn dome_sheet() -> Texture {
    if let Some(photo) = dome_photo() {
        return photo.clone();
    }
    sheet("sky_c", 256, |u, v| {
        // v is 0 at the zenith and 1 at the horizon — see `dome_mesh`'s uvs.
        let up = 1.0 - v;
        // A day. Bright enough that whatever light the track's own ambient puts on it, it
        // still reads as sky rather than as dusk.
        let base = [
            105.0 + 120.0 * (1.0 - up).powf(1.6),
            155.0 + 90.0 * (1.0 - up).powf(1.4),
            215.0 + 35.0 * (1.0 - up).powf(1.2),
        ];
        // Cloud: two scales of noise, thresholded so it comes out as banks rather than fog,
        // and thinned towards the top where a flat sheet would read as a ceiling.
        let n = 0.6 * grain(u * 3.0, v * 6.0, 0x9A11, 5.0) + 0.4 * grain(u * 9.0, v * 14.0, 0x9A12, 11.0);
        let cloud = ((n - 0.52) * 3.4).clamp(0.0, 1.0) * (0.35 + 0.65 * (1.0 - up));
        let c = |i: usize| (base[i] * (1.0 - cloud) + 246.0 * cloud).clamp(0.0, 255.0) as u8;
        [c(0), c(1), c(2), 255]
    })
}

/// What a tyre leaves, as a texture: the print of a knobbly.
///
/// Asked for outright after two rides — "tyre marks don't mean grooves everywhere, I meant
/// the actual tyre mark texture". Grooves are the *shape* a hundred passes cut into the
/// ground, and they are worth having, but they are not a tyre mark. A mark is the print of
/// the knobs: two rows of blocks either side of a centre line, staggered, about four
/// centimetres apart, dark where the rubber pressed the dirt down.
///
/// Alpha-cut, so what is not a knob is not drawn at all — the ground shows through between
/// them, which is what makes it read as a print rather than as a stripe.
fn tyre_sheet() -> Texture {
    sheet("tyre_c_a", 256, |u, v| {
        // Four prints side by side, the same print at four ages. A ribbon takes whichever
        // lane it is given and can change lane part way along, which is how a mark fades out
        // in the middle of itself without a second material or a vertex colour to do it with.
        let lane = (u * TYRE_FADES as f32) as usize;
        let u = (u * TYRE_FADES as f32).fract();
        let fade = [1.0f32, 0.68, 0.42, 0.22][lane.min(TYRE_FADES - 1)];
        let across = (u - 0.5) * 2.0; // -1 at one edge, +1 at the other
        // Two rows of knobs, offset half a step from each other, plus a centre block.
        let row = |lane: f32, phase: f32| -> f32 {
            let d = (across - lane).abs();
            if d > 0.30 {
                return 0.0;
            }
            let along = (v * 3.0 + phase).fract();
            let block = if (0.12..0.62).contains(&along) { 1.0 } else { 0.0 };
            // Softened at the edges of each block so it is a print in dirt, not a stamp.
            let edge = ((0.30 - d) / 0.12).clamp(0.0, 1.0);
            block * edge
        };
        let knob = row(-0.52, 0.0).max(row(0.52, 0.5)).max(row(0.0, 0.25) * 0.85);
        // Ragged: a print in soil is never the shape of the block that made it.
        let torn = 0.72 + 0.5 * grain(u * 2.0, v * 2.0, 0x7A31, 40.0);
        let a = (knob * torn).clamp(0.0, 1.0) * fade;
        if a < 0.10 {
            return [0, 0, 0, 0];
        }
        // Pressed dirt: darker than what it is printed on, and slightly wet-looking.
        let g = 0.85 + 0.3 * grain(u, v, 0x7A32, 26.0);
        [
            (44.0 * g) as u8,
            (32.0 * g) as u8,
            (22.0 * g) as u8,
            (215.0 * a) as u8,
        ]
    })
}

/// A ribbon of tyre prints laid on the ground, following the line a rider took.
///
/// It has to be a mesh rather than part of the terrain's own paint. A terrain layer tiles in
/// world space — north-south, always — so tread baked into a ground sheet points the same way
/// everywhere and is wrong wherever the track is not going north. A card knows which way it
/// is pointing.
///
/// The strip follows the ground sample by sample rather than being a flat quad: a two-metre
/// card laid across a rut stands off it at one end and buries itself at the other.
fn tyre_ribbon(
    syn: &Synth,
    stations: &[crate::trackprog::Station],
    airborne: &dyn Fn(f32) -> bool,
    lat_at: impl Fn(usize) -> f32,
    width: f32,
    seed: u32,
) -> Mesh {
    let mut m = Mesh::default();
    let mut v_at = 0.0f32;
    let mut prev: Option<u32> = None;
    for (i, st) in stations.iter().enumerate() {
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        let lat = lat_at(i);
        let (cx, cz) = (st.x + rx * lat, st.z + rz * lat);
        // Three reasons a pass is not printed here.
        //
        // Over a jump it is not printed because the rider is in the air: a table with tyre
        // marks across its deck is a table nobody jumped. Off the corridor it is not printed
        // because nobody rode there. And along its own length a mark comes and goes, which is
        // what a pass laid on ground that was damp in places actually looks like.
        // Snapped into whatever groove is nearest.
        //
        // A pass laid at a fixed offset from the line crosses grooves rather than running
        // down one, and prints strewn across a rut is not what a rut looks like: the marks
        // pile up *inside* it. So each pass looks half a metre either side of where it was
        // going to go and takes the deepest ground it finds — which is the floor of a groove
        // if there is one near, and where it was going otherwise.
        let sample_rut = |t: f32| -> f32 {
            let (x, z) = (st.x + rx * t, st.z + rz * t);
            let (gx, gz) = ((x / syn.mps) as usize, (z / syn.mps) as usize);
            -syn.rut
                .get(gz.min(syn.gh - 1) * syn.gw + gx.min(syn.gw - 1))
                .copied()
                .unwrap_or(0.0)
        };
        let mut best = (sample_rut(lat), lat);
        let mut k = -4i32;
        while k <= 4 {
            let t = lat + k as f32 * 0.13;
            let v = sample_rut(t);
            if v > best.0 {
                best = (v, t);
            }
            k += 1;
        }
        let (groove, lat) = best;
        let (cx, cz) = (st.x + rx * lat, st.z + rz * lat);
        let coming = crate::tracksynth::fbm(st.s / 34.0, seed as f32 * 0.37, seed ^ 0x5A11);
        // A groove keeps a mark going: the deepest part of a line is where the prints pile
        // up, which is the whole reason a rut reads as ridden rather than as a ditch.
        let over_a_jump = airborne(st.s);
        let on = lat.abs() < width * 6.0 && (coming > -0.15 || groove > 0.25);
        if !on {
            prev = None;
            v_at += 0.0;
            continue;
        }
        // Which of the four ages this stretch of the pass is in. It moves along the ribbon,
        // so one mark is crisp at the corner and gone by the exit.
        // Over a jump the marks stay, at the faintest age there is. A rider does leave the
        // ground somewhere on a takeoff and the prints that carried him there are on it —
        // but nothing on a deck is a fresh print, because nobody is on the ground for it.
        let age = if over_a_jump {
            TYRE_FADES - 1
        } else {
            ((0.5 - 0.5 * coming) * 3.4 - groove * 1.6).clamp(0.0, 3.0) as usize
        };
        let u0 = age.min(TYRE_FADES - 1) as f32 / TYRE_FADES as f32;
        let du = 1.0 / TYRE_FADES as f32;
        let half = width * 0.5;
        let start = m.vertex_count() as u32;
        for side in [-1.0f32, 1.0] {
            let (x, z) = (cx + rx * half * side, cz + rz * half * side);
            m.positions.extend_from_slice(&[x, ground(syn, x, z) + TYRE_LIFT_M, z]);
            m.normals.extend_from_slice(&[0.0, 1.0, 0.0]);
            m.uvs.extend_from_slice(&[u0 + if side < 0.0 { 0.0 } else { du }, v_at]);
        }
        if let Some(prev_start) = prev {
            m.indices.extend_from_slice(&[prev_start, prev_start + 1, start]);
            m.indices.extend_from_slice(&[start, prev_start + 1, start + 1]);
        }
        let step = if i + 1 < stations.len() {
            (stations[i + 1].x - st.x).hypot(stations[i + 1].z - st.z)
        } else {
            0.0
        };
        prev = Some(start);
        v_at += step / TYRE_TREAD_M;
    }
    m
}

/// How many ages of print the sheet carries, how many passes are laid, and how far the
/// outermost of them sits off the line.
const TYRE_FADES: usize = 4;
const TYRE_PASSES: usize = 27;
const TYRE_SPREAD_M: f32 = 2.4;

/// How wide a print is, how far the tread repeats in, and how far the card floats over the
/// ground so it draws in front of it without standing off it.
/// A print is a tyre wide, not a pencil line — "too thin, too little in count, should overlap
/// each other, some more faded some less", from a rider on the track.
const TYRE_W_M: f32 = 0.34;
const TYRE_TREAD_M: f32 = 0.42;
const TYRE_LIFT_M: f32 = 0.035;

/// The ground beyond the plot, and the pines standing on it.
///
/// Built outward from the plot's centre, one ray at a time: each ray leaves the square exactly
/// at its edge, at the edge's own height, and climbs from there, so the bank meets the terrain
/// with no seam and no gap at the corners. The rise varies round the ring, so the horizon is a
/// line of hills rather than a wall.
fn backdrop(prog: &TrackProgram, syn: &Synth, seed: u32) -> (Mesh, Vec<Plant>) {
    let (sx, sz) = (prog.terrain.size_x, prog.terrain.size_z);
    let (cx, cz) = (sx * 0.5, sz * 0.5);
    const OUT: [f32; 7] = [0.0, 8.0, 20.0, 38.0, 60.0, 85.0, BACKDROP_REACH_M];
    // Where a ray from the centre at angle `a` leaves the square.
    let exit = |a: f32| -> (f32, f32, f32, f32) {
        let (dx, dz) = (a.cos(), a.sin());
        let tx = if dx.abs() > 1e-6 { (if dx > 0.0 { sx - cx } else { -cx }) / dx } else { f32::MAX };
        let tz = if dz.abs() > 1e-6 { (if dz > 0.0 { sz - cz } else { -cz }) / dz } else { f32::MAX };
        let t = tx.min(tz);
        (cx + dx * t, cz + dz * t, dx, dz)
    };
    let rise = |a: f32| -> f32 {
        let n = 0.5 + 0.5 * noise1(a * 3.0, seed ^ 0xBAC0);
        BACKDROP_RISE_M.0 + (BACKDROP_RISE_M.1 - BACKDROP_RISE_M.0) * n
    };
    let height = |a: f32, d: f32| -> f32 {
        let (ex, ez, dx, dz) = exit(a);
        let edge = ground(syn, ex - dx, ez - dz);
        let t = (d / BACKDROP_REACH_M).clamp(0.0, 1.0);
        edge - 0.1 + rise(a) * t * t * (3.0 - 2.0 * t)
    };
    let point = |a: f32, d: f32| -> [f32; 3] {
        let (ex, ez, dx, dz) = exit(a);
        [ex + dx * d, height(a, d), ez + dz * d]
    };

    let mut ground_mesh = Mesh::default();
    for k in 0..=BACKDROP_RAYS {
        let a = std::f32::consts::TAU * k as f32 / BACKDROP_RAYS as f32;
        for &d in &OUT {
            let p = point(a, d);
            // Lit off its own slope, from the neighbours round and out.
            let da = std::f32::consts::TAU / BACKDROP_RAYS as f32;
            let (pa, pb) = (point(a - da, d), point(a + da, d));
            let (po, pi) = (point(a, d + 4.0), point(a, (d - 4.0).max(0.0)));
            let (u, w) = ([pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]], [po[0] - pi[0], po[1] - pi[1], po[2] - pi[2]]);
            let mut nrm = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
            if nrm[1] < 0.0 {
                nrm = [-nrm[0], -nrm[1], -nrm[2]];
            }
            let l = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt().max(1e-6);
            ground_mesh.positions.extend_from_slice(&p);
            ground_mesh.normals.extend_from_slice(&[nrm[0] / l, nrm[1] / l, nrm[2] / l]);
            ground_mesh.uvs.extend_from_slice(&[p[0] / 12.0, p[2] / 12.0]);
        }
    }
    let per = OUT.len() as u32;
    for k in 0..BACKDROP_RAYS as u32 {
        for j in 0..per - 1 {
            let (a, b) = (k * per + j, (k + 1) * per + j);
            ground_mesh.indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }

    // Pines on it, jittered off a grid in (angle, distance) so they read as a wood.
    let mut plants = Vec::new();
    let mut d = 6.0f32;
    while d < BACKDROP_REACH_M - 4.0 {
        let (ex, ez, _, _) = exit(0.0);
        let r = ((ex - cx).hypot(ez - cz) + d).max(1.0);
        let steps = ((std::f32::consts::TAU * r) / BACKDROP_TREE_M) as u32;
        for k in 0..steps {
            let key = (d as u32) * 65536 + k;
            if rnd(seed ^ 0xBAC1, key) > 0.8 {
                continue;
            }
            let a = std::f32::consts::TAU * (k as f32 + rnd(seed ^ 0xBAC2, key) - 0.5) / steps as f32;
            let dd = d + (rnd(seed ^ 0xBAC3, key) - 0.5) * BACKDROP_TREE_M;
            let p = point(a, dd.clamp(2.0, BACKDROP_REACH_M));
            plants.push(Plant {
                x: p[0],
                z: p[2],
                foot: p[1] - 0.1,
                yaw: 360.0 * rnd(seed ^ 0xBAC5, key),
                key: key ^ 0x6000_0000,
                far: true,
            });
        }
        d += BACKDROP_TREE_M;
    }
    (ground_mesh, plants)
}

/// A smooth wander in one dimension, -1 to 1.
fn noise1(x: f32, seed: u32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let a = rnd(seed, i as i32 as u32) * 2.0 - 1.0;
    let b = rnd(seed, (i as i32 + 1) as u32) * 2.0 - 1.0;
    let t = f * f * (3.0 - 2.0 * f);
    a + (b - a) * t
}

/// The backdrop's ground: grass going brown, so it sits behind the site's own turf.
fn backdrop_sheet() -> Texture {
    sheet("backdrop_c", 64, |u, v| {
        let g = grain(u, v, 0x5AC7, 16.0);
        let s = 0.78 + 0.3 * g;
        [(70.0 * s) as u8, (84.0 * s) as u8, (44.0 * s) as u8, 255]
    })
}

/// A box van, parked. Two boxes and nothing else: at twenty metres it is a white slab with a
/// dark cab, and every paddock on every track is full of them.
fn van_mesh() -> Mesh {
    let mut m = edfwrite::moved(&edfwrite::cuboid(6.0, 2.5, 2.4), [0.0, 1.55, 0.0]);
    m.append(&edfwrite::moved(&edfwrite::cuboid(2.1, 1.7, 2.3), [-3.6, 1.15, 0.0]));
    m
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

/// Squeeze a mesh's `v` into one band of the atlas, leaving `u` alone.
fn in_band(mesh: &Mesh, (top, bot): (f32, f32)) -> Mesh {
    in_cell(mesh, (top, bot), (0.0, 1.0))
}

/// Squeeze a mesh into one band of the atlas and one window across it.
///
/// The window is what makes a tiled banner tile: a piece two metres into a run samples the
/// two metres of print that belong there, and `u` past 1 wraps — which is not a guess, it is
/// what Indiana's own pieces do, at `v` up to 2.0.
fn in_cell(mesh: &Mesh, (top, bot): (f32, f32), (u0, u1): (f32, f32)) -> Mesh {
    let mut m = mesh.clone();
    for uv in m.uvs.chunks_exact_mut(2) {
        uv[0] = u0 + uv[0].clamp(0.0, 1.0) * (u1 - u0);
        uv[1] = top + uv[1].clamp(0.0, 1.0) * (bot - top);
    }
    m
}

/// The two ways a track carries a printed line, and the difference is the repeat.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Print {
    /// Sponsors' boards, bolted edge to edge, a different name on each.
    Boards,
    /// One design, printed over and over on piece after piece of the same plastic.
    Tiled,
}

impl Print {
    fn size(self) -> (f32, f32) {
        match self {
            Print::Boards => (BANNER_W_M, BANNER_H_M),
            Print::Tiled => (TILE_W_M, TILE_H_M),
        }
    }

    /// The upright that holds a piece up at its leading end, and how thick it is. A hoarding
    /// shares one at every join; a strung banner is held every few pieces.
    fn post_at(self, piece: usize) -> Option<f32> {
        match self {
            Print::Boards => Some(0.09),
            Print::Tiled if piece % TILE_POST_EVERY == 0 => Some(0.06),
            Print::Tiled => None,
        }
    }

    /// Which window of the sheet the `piece`-th piece of a run prints. A board prints the
    /// whole cell; a tiled piece prints the slice that belongs where it stands.
    fn window(self, piece: usize) -> (f32, f32) {
        match self {
            Print::Boards => (0.0, 1.0),
            Print::Tiled => {
                let du = 1.0 / TILE_PER_PRINT as f32;
                let u0 = (piece % TILE_PER_PRINT) as f32 * du;
                (u0, u0 + du)
            }
        }
    }
}

/// A printed board: a panel with a front, a back and four edges you can see the thickness of.
///
/// Not a card. A zero-thickness quad printed on both sides is what a *flag* is — hang it
/// beside a track and it reads as a tarpaulin, which is what a rider's-eye render shows and
/// what it was called. A trackside board is a rigid sheet on a frame, and the give-away that
/// it is one is the sliver of edge you see wherever it is not square-on to you.
///
/// The two big faces print; the four edges take the plain band the uprights use. Their `u`
/// runs opposite ways in world space — [`edfwrite::cuboid`] lays every face out from its own
/// outward normal — which is exactly a double-sided print, and it is why this replaces
/// [`edfwrite::printed_both_sides`] rather than wrapping it: that gave both copies the same
/// world-space `u`, so whichever face pointed at the track from the far side of the lap
/// showed the wordmark backwards. Half the boards on a track were unreadable.
fn banner_slab(w: f32, h: f32, cell: (f32, f32), window: (f32, f32)) -> Mesh {
    let box_ = edfwrite::moved(&edfwrite::cuboid(w, h, BANNER_D_M), [0.0, BANNER_LIFT_M, 0.0]);
    // `cuboid` writes +z first and -z second, four vertices each: the printed faces are the
    // first eight, and everything after them is edge.
    const PRINTED: usize = 8;
    let mut m = in_cell(&box_, cell, window);
    let edges = in_band(&box_, banner_post_band());
    for (i, uv) in m.uvs.chunks_exact_mut(2).enumerate() {
        if i < PRINTED {
            // Mirrored, both faces. A face laid out from its own outward normal runs `u`
            // against the direction its viewer reads in — measured, not derived, and the same
            // finding [`edfwrite::printed_both_sides`] recorded: the winding argues the
            // opposite. Flipping one face and not the other is what makes half a lap
            // unreadable, so both flip and the two stay opposite in world space.
            uv[0] = window.0 + window.1 - uv[0];
        } else {
            uv.copy_from_slice(&edges.uvs[i * 2..i * 2 + 2]);
        }
    }
    m
}

/// One piece of a printed line: the `cell`-th panel of the atlas, in the window this piece
/// prints, with an upright at its leading end.
///
/// The leading end only — the pieces are butted and the next one's upright is this one's far
/// post. `cap` closes the end left bare, which is behind the run's *first* piece.
fn banner_piece(style: Print, cell: usize, piece: usize, cap: bool) -> Mesh {
    let (w, h) = style.size();
    let mut m = banner_slab(
        w,
        h,
        banner_cell(cell % BANNER_CELLS),
        style.window(piece),
    );
    let mut post = |x: f32, t: f32| {
        m.append(&in_band(
            &edfwrite::moved(
                &edfwrite::cuboid(t, BANNER_LIFT_M + h + 0.06, t),
                [x, 0.0, 0.0],
            ),
            banner_post_band(),
        ));
    };
    if let Some(t) = style.post_at(piece) {
        post(-w * 0.5, t);
    }
    if cap {
        post(w * 0.5, style.post_at(0).unwrap_or(0.06));
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
    let pits = Pits::of(prog);

    let mut stakes = Mesh::default();
    let mut banners = Mesh::default();
    let mut fence = Mesh::default();
    let mut plants: Vec<Plant> = Vec::new();
    // A lifted venue, if one is installed — and with it a real start/finish arch, which
    // replaces the two box gantries this used to draw.
    let lib = crate::trackprops::load();
    let arch = lib.as_ref().and_then(|l| l.props.iter().find(|p| p.id == "finish_arch" && whole(p)));
    let mut arch_kind: Option<(String, Mesh, Texture, bool)> = None;
    // Indiana's own edge pieces, when the library carries them: its stake, and one piece of
    // the barrier that lines its lap, repeated where ours drew boxes and printed boards.
    let edge_stake = lib.as_ref().and_then(|l| l.props.iter().find(|p| p.id == "edge_stake"));
    let edge_barrier = lib.as_ref().and_then(|l| l.props.iter().find(|p| p.id == "edge_barrier"));
    let edge_post = lib.as_ref().and_then(|l| l.props.iter().find(|p| p.id == "edge_post"));
    let mut barrier_kind: Option<(String, Mesh, Texture, bool)> = None;
    let mut posts_kind: Option<(String, Mesh, Texture, bool)> = None;
    let mut vans = Mesh::default();
    let mut gate = Mesh::default();
    let mut tally = Vec::new();

    // 1. The edge line: little white plastic stakes at the track edge, which is how a
    // motocross track is marked and what Indiana measures — see [`STAKE_OFF_M`]. Thin and
    // low, so what you see down the track is a line of points rather than a wall.
    let stake_off = (half + 1.0).max(STAKE_OFF_M);
    let mut n = 0usize;
    // Along each edge's own line, by arc length: stepped along the centreline they bunched on
    // the inside of every bend and spread on the outside.
    for side in [-1.0f32, 1.0] {
        let line: Vec<(f32, f32)> = stations
            .iter()
            .map(|st| {
                let (rx, rz) = crate::trackprog::right_vector(st.heading);
                (st.x + rx * stake_off * side, st.z + rz * stake_off * side)
            })
            .collect();
        let mut acc = vec![0.0f32];
        for w in line.windows(2) {
            acc.push(acc.last().unwrap() + (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1));
        }
        let total = *acc.last().unwrap();
        let (mut cursor, mut s, mut i) = (0usize, 0.0f32, 0u32);
        let mut placed: Vec<(f32, f32)> = Vec::new();
        while s < total {
            let (x, z, _) = along_line(&line, &acc, &mut cursor, s);
            s += STAKE_GAP_M;
            i += 1;
            if !inside(prog, x, z, 2.0)
                || clearance(&coarse, x, z) < stake_off - 1.0
                || !clear_of_the_start(x, z)
                // Where the line folds round a tight inside, it comes back past itself.
                || placed.iter().rev().take(64).any(|&(px, pz)| (px - x).hypot(pz - z) < STAKE_GAP_M * 0.5)
            {
                continue;
            }
            placed.push((x, z));
            let key = i * 2 + (side > 0.0) as u32;
            let h = STAKE_H_M * (0.88 + 0.24 * rnd(seed ^ 0x12, key));
            // They all wear the same plastic, so the variety is in the lean.
            let lean = (rnd(seed ^ 0x13, key) - 0.5) * 26.0;
            let post = match edge_stake {
                Some(p) => p.mesh.clone(),
                None => edfwrite::cuboid(STAKE_W_M, h, STAKE_W_M),
            };
            stakes.append(&edfwrite::moved(&edfwrite::turned(&post, lean), [x, ground(syn, x, z) - 0.03, z]));
            n += 1;
        }
    }
    tally.push(("stakes", n));

    if let (Some(piece), Some(lib)) = (edge_barrier, lib.as_ref()) {
        // The real barrier, one piece after another along both sides, where the printed
        // boards ran. Turned by our heading less the piece's own, like any lifted instance.
        let off = half + EDGE_LINE_OUT_M;
        let step = piece.span.max(0.5);
        let mut mesh = Mesh::default();
        let mut post_mesh = Mesh::default();
        let mut n = 0usize;
        for side in [-1.0f32, 1.0] {
            let line: Vec<(f32, f32)> = stations
                .iter()
                .map(|st| {
                    let (rx, rz) = crate::trackprog::right_vector(st.heading);
                    (st.x + rx * off * side, st.z + rz * off * side)
                })
                .collect();
            let mut acc = Vec::with_capacity(line.len());
            acc.push(0.0f32);
            for w in line.windows(2) {
                let d = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
                acc.push(acc.last().unwrap() + d);
            }
            let total = *acc.last().unwrap();
            let (mut cursor, mut s) = (0usize, 0.0f32);
            while s < total {
                // A post where the panel starts, then the panel.
                let (qx, qz, qdeg) = along_line(&line, &acc, &mut cursor, s);
                let (x, z, deg) = along_line(&line, &acc, &mut cursor, s + step * 0.5);
                s += step;
                if !inside(prog, x, z, 3.0)
                    || clearance(&coarse, x, z) < off - 1.5
                    || !clear_of_the_start(x, z)
                {
                    continue;
                }
                // Seated along its length; a panel across a step would hang off one end.
                let (lo, hi) = ground_span(syn, x, z, deg, step);
                if hi - lo > EDGE_LINE_STEP_M {
                    continue;
                }
                let yaw = deg - 90.0 - piece.axis_ref.to_degrees();
                let panel = edfwrite::moved(&edfwrite::turned(&piece.mesh, yaw), [x, (lo + hi) * 0.5 - 0.05, z]);
                // Never over another leg of the lap or the start pad.
                if on_riding_surface(syn, half, &panel) || pits.on_lane(lap, syn, &panel) {
                    continue;
                }
                mesh.append(&panel);
                if let Some(p) = edge_post {
                    let yaw = qdeg - 90.0 - p.axis_ref.to_degrees();
                    post_mesh.append(&edfwrite::moved(&edfwrite::turned(&p.mesh, yaw), [qx, ground(syn, qx, qz), qz]));
                }
                n += 1;
            }
        }
        // The start straight fenced off from the lap beside it, up to a gap at the join.
        let mut fence = 0usize;
        let line = spur_fence_line(syn, &coarse);
        if line.len() >= 2 {
            let mut acc = vec![0.0f32];
            for w in line.windows(2) {
                acc.push(acc.last().unwrap() + (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1));
            }
            let total = *acc.last().unwrap();
            let (mut cursor, mut s) = (0usize, 0.0f32);
            while s + step <= total {
                let (qx, qz, qdeg) = along_line(&line, &acc, &mut cursor, s);
                let (x, z, deg) = along_line(&line, &acc, &mut cursor, s + step * 0.5);
                s += step;
                if !inside(prog, x, z, 3.0) {
                    continue;
                }
                let (lo, hi) = ground_span(syn, x, z, deg, step);
                if hi - lo > EDGE_LINE_STEP_M {
                    continue;
                }
                let yaw = deg - 90.0 - piece.axis_ref.to_degrees();
                let panel = edfwrite::moved(&edfwrite::turned(&piece.mesh, yaw), [x, (lo + hi) * 0.5 - 0.05, z]);
                if on_riding_surface(syn, half, &panel) || pits.on_lane(lap, syn, &panel) {
                    continue;
                }
                mesh.append(&panel);
                if let Some(p) = edge_post {
                    let yaw = qdeg - 90.0 - p.axis_ref.to_degrees();
                    post_mesh.append(&edfwrite::moved(&edfwrite::turned(&p.mesh, yaw), [qx, ground(syn, qx, qz), qz]));
                }
                fence += 1;
            }
        }
        tally.push(("start fence", fence));
        if let Some((name, w, h, rgba)) = lib.sheets.iter().find(|s| s.0 == piece.sheet) {
            let tex = Texture { name: name.clone(), width: *w, height: *h, rgba: rgba.clone() };
            barrier_kind = Some(("barrier".into(), mesh, tex, true));
        }
        if let Some((name, w, h, rgba)) = edge_post.and_then(|p| lib.sheets.iter().find(|s| s.0 == p.sheet)) {
            let tex = Texture { name: name.clone(), width: *w, height: *h, rgba: rgba.clone() };
            posts_kind = Some(("barrier_posts".into(), post_mesh, tex, true));
        }
        tally.push(("barrier pieces", n));
    } else {
    // 2. The printed line down both sides, in runs, of the two kinds a track carries: a
    //    hoarding of sponsors' boards bolted edge to edge, and a tiled banner — see [`Print`].
    //
    //    Walked along its *own* offset line rather than along the centreline, for the reason
    //    the fence below spells out: stepping the centreline and offsetting each step spaces
    //    pieces by the centreline's arc length, and the offset line's is longer round the
    //    outside of a corner and shorter round the inside — so a run gaps through every turn
    //    one way and piles up the other. A printed line is the shape that shows that up worst,
    //    because its pieces touch.
    //
    //    And by arc length, not by adding stations up until they pass a piece's width: on
    //    half-metre stations that leaves daylight at every join.
    let mut n = 0usize;
    let mut runs = 0usize;
    let mut tiled = 0usize;
    let banner_off = half + EDGE_LINE_OUT_M;
    for side in [-1.0f32, 1.0] {
        let line: Vec<(f32, f32)> = stations
            .iter()
            .map(|st| {
                let (rx, rz) = crate::trackprog::right_vector(st.heading);
                (st.x + rx * banner_off * side, st.z + rz * banner_off * side)
            })
            .collect();
        let mut acc = Vec::with_capacity(line.len());
        acc.push(0.0f32);
        for w in line.windows(2) {
            let d = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
            acc.push(acc.last().unwrap() + d);
        }
        let total = *acc.last().unwrap();

        // Where this side's first run starts, so the two sides don't break in the same places.
        let mut s = BANNER_BREAK_MIN_M * (0.5 + 0.5 * rnd(seed ^ 0x61, side as u32));
        let mut cursor = 0usize;
        let mut left_in_run = 0usize;
        let mut run_key = side as u32;
        let mut placed: Option<(f32, f32)> = None;
        let mut style = Print::Boards;
        let mut cell = 0usize;
        let mut piece = 0usize;

        while s < total {
            // What this run is, decided before the first piece of it is measured out, because
            // how far to step depends on how wide a piece is.
            if left_in_run == 0 {
                run_key = run_key.wrapping_mul(2_654_435_761).wrapping_add(1);
                style = if rnd(seed ^ 0x64, run_key) < TILED_SHARE {
                    Print::Tiled
                } else {
                    Print::Boards
                };
                // A tiled run is one sponsor's, so it picks its panel once.
                cell = (rnd(seed ^ 0x65, run_key) * BANNER_CELLS as f32) as usize % BANNER_CELLS;
                let boards = BANNER_RUN_MIN
                    + ((rnd(seed ^ 0x62, run_key) * (BANNER_RUN_MAX - BANNER_RUN_MIN) as f32)
                        as usize);
                // The run is that many metres long whichever kind it is.
                left_in_run =
                    ((boards as f32 * BANNER_W_M / style.size().0).round() as usize).max(2);
                piece = 0;
            }
            let (pw, _) = style.size();
            if s + pw > total {
                break;
            }
            let (x, z, deg) = along_line(&line, &acc, &mut cursor, s + pw * 0.5);

            // A piece needs level ground under its whole width and room to stand clear.
            let ok = inside(prog, x, z, 3.0)
                && clearance(&coarse, x, z) > banner_off - 1.5
                && clear_of_the_start(x, z);
            let (lo, hi) = ground_span(syn, x, z, deg, pw);
            if !ok || hi - lo > 0.9 {
                // Break the run here rather than leaving a piece hanging in the air.
                left_in_run = 0;
                placed = None;
                s += BANNER_BREAK_MIN_M;
                continue;
            }
            // Where the two sides fold back on each other, one side's run can land inside the
            // other's. Spacing along a run does not catch it — the runs are walked separately.
            if let Some((px, pz)) = placed {
                if (px - x).powi(2) + (pz - z).powi(2) < (pw * 0.55).powi(2) {
                    s += pw;
                    continue;
                }
            }
            // A hoarding cycles its sponsors; a tiled run prints the one it picked.
            let printed = match style {
                Print::Boards => n,
                Print::Tiled => cell,
            };
            let board = edfwrite::moved(
                &edfwrite::turned(&banner_piece(style, printed, piece, piece == 0), deg),
                [x, (lo + hi) * 0.5 - 0.04, z],
            );
            // Never over another leg of the lap or the start pad: break the run there.
            if on_riding_surface(syn, half, &board) || pits.on_lane(lap, syn, &board) {
                left_in_run = 0;
                placed = None;
                s += BANNER_BREAK_MIN_M;
                continue;
            }
            placed = Some((x, z));
            banners.append(&board);
            n += 1;
            piece += 1;
            left_in_run -= 1;
            s += pw;
            if left_in_run == 0 {
                runs += 1;
                tiled += (style == Print::Tiled) as usize;
                let t = rnd(seed ^ 0x63, run_key ^ 0x9E37);
                s += BANNER_BREAK_MIN_M + t * (BANNER_BREAK_MAX_M - BANNER_BREAK_MIN_M);
                placed = None;
            }
        }
    }
    tally.push(("banner boards", n));
    tally.push(("banner runs", runs));
    tally.push(("tiled runs", tiled));
    }



    // 3. No fence. It ran along the lap and closed the start straight off — the spur runs
    //    beside the circuit and a fence between the two is a wall across where the field
    //    leaves the gate. Dropped everywhere for now rather than routed round the start,
    //    which is the smaller change and the one that can be judged from a bike.
    let _ = &mut fence;





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
            plants.push(Plant {
                x,
                z,
                foot: ground_min(syn, x, z, 1.0) - 0.05,
                yaw: 360.0 * rnd(seed ^ 0x45, i),
                key: i,
                far: false,
            });
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
            if rnd(seed ^ 0x63, key) > 0.8 || !inside(prog, x, z, 4.0) {
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
            plants.push(Plant {
                x,
                z,
                foot: ground_min(syn, x, z, 1.0) - 0.05,
                yaw: 360.0 * rnd(seed ^ 0x65, key),
                key: key ^ 0x5000_0000,
                far: false,
            });
            n += 1;
        }
        gz += WOOD_STEP_M;
    }
    tally.push(("wood", n));

    // 5c. The backdrop: a bank of ground beyond the plot, rising away from it, with a wood on
    //     it. The plot ends forty metres from the line in places, and past it there was only
    //     the sky — ridden as "scenery could use a backdrop and some more objects in the
    //     background". Drawn and never solid: nobody rides out there.
    let (backdrop, far) = backdrop(prog, syn, seed);
    let n = far.len();
    plants.extend(far);
    tally.push(("backdrop trees", n));


    // No parked vans. They are what a paddock is full of, but ours stood close enough to the
    // riding line to be something a rider hits — a white box with a stripe down it, solid, in
    // the way. Out until they can be put somewhere that is actually a paddock.
    let _ = &mut vans;


    // No tyre marks. Asked for and then asked out again: a ribbon of prints down the racing
    // line reads as a stripe painted on the ground rather than as ground anyone has ridden,
    // and no published track lays anything like it. Indiana carries none.
    let tyre = Mesh::default();

    // No sky among the scenery. It is `dome.edf`, which the `.amb` loads at run time; placed
    // here as well, TerrainEd baked its shadow over a third of the plot at a 40° sun.

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
    if arch.is_none() {
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
    }

    // And one over the finish line itself, which is where the finish jump lands. The same
    // shape as the gate's, narrower — it spans the racing line rather than a row of forty
    // gates — and taller, because what comes under it has just come off a three-metre
    // tabletop.
    if let (Some(arch), Some(lib)) = (arch, lib.as_ref()) {
        // The real one, scaled evenly until its posts clear our track: Indiana's stand 11 m
        // apart, and a national here is wider than that.
        let st = at(crate::tracksynth::finish_at(prog));
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        let feet = |k: f32| {
            let post = arch.span * 0.39 * k;
            [-1.0f32, 1.0]
                .iter()
                .map(|side| ground(syn, st.x + rx * post * side, st.z + rz * post * side))
                // The lower post's ground: on the higher, the other post hung in the air.
                .fold(f32::INFINITY, f32::min)
        };
        // Wide enough to clear the track, and tall enough that the header stands 5 m over the
        // lip it straddles: the line is on the take-off, and riders leave the ground under it.
        // Indiana's header is 4.5 m clear on 6.6 m.
        let lip = ground(syn, st.x, st.z);
        let mut k = ((prog.width + 3.0) / (arch.span * 0.75)).max(1.0);
        for _ in 0..2 {
            k = k.max((lip - feet(k) + 5.0) / (arch.height * 0.68));
        }
        let foot = feet(k);
        let mut scaled = arch.mesh.clone();
        scaled.positions.iter_mut().for_each(|v| *v *= k);
        let deg = (st.heading - arch.axis_ref).to_degrees();
        let mesh = edfwrite::moved(&edfwrite::turned(&scaled, deg), [st.x, foot, st.z]);
        if let Some((name, w, h, rgba)) = lib.sheets.iter().find(|s| s.0 == arch.sheet) {
            let tex = Texture { name: name.clone(), width: *w, height: *h, rgba: rgba.clone() };
            // Drawn, not solid: whoever is high off the lip should fly through, not into it.
            arch_kind = Some(("finish_arch".into(), mesh, tex, false));
            tally.push(("finish arch", 1));
        }
    } else {
        let st = at(crate::tracksynth::finish_at(prog));
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        let span = half + 4.0;
        let feet: Vec<(f32, f32)> = [-1.0f32, 1.0]
            .iter()
            .map(|side| (st.x + rx * span * side, st.z + rz * span * side))
            .collect();
        if feet.iter().all(|(x, z)| inside(prog, *x, *z, 2.0)) {
            let mut highest = f32::NEG_INFINITY;
            for (x, z) in &feet {
                let foot = ground(syn, *x, *z);
                highest = highest.max(foot);
                gate.append(&edfwrite::moved(&edfwrite::cuboid(0.5, 6.0, 0.5), [*x, foot, *z]));
            }
            let beam =
                edfwrite::turned(&edfwrite::cuboid(span * 2.0, 1.2, 0.3), across(st.heading));
            gate.append(&edfwrite::moved(&beam, [st.x, highest + 5.6, st.z]));
            tally.push(("finish gantry", 1));
        }
    }

    // One model a kind, each with its own single sheet. Not one model of several materials:
    // TerrainEd faults on the second material in a model whatever the geometry — see
    // `edfwrite`'s note and the case-by-case test behind it. PiBoSo's own example track is
    // built the same way, three `scene` blocks for three objects.
    let kinds: Vec<(String, Mesh, Texture, bool)> = vec![
        ("stakes".into(), stakes, stake_sheet(), false),
        ("banners".into(), banners, banner_sheet(), true),
        // The bank beyond the plot and the wood on it: drawn, never solid.
        ("backdrop".into(), backdrop, backdrop_sheet(), false),
        ("gate".into(), gate, gate_sheet(), true),
        // Drawn, never solid: a mark is paint on the ground, not a kerb.
    ];

    // A lifted venue, if one is installed. Absence is ordinary: a library is baked from a
    // donor archive the user already has, so most builds have none and place nothing.
    let mut kinds = kinds;
    // A mat and a stand at every stall the game spawns a rider in.
    let (stands, stalls) = pit_stands(&pits, prog, syn);
    tally.push(("pit stands", stalls));
    kinds.push(("pit_stands".into(), stands, pit_sheet(), false));
    kinds.extend(arch_kind);
    kinds.extend(barrier_kind);
    kinds.extend(posts_kind);
    // The stakes wear the real stake's sheet when that is what they are.
    if let (Some(p), Some(lib)) = (edge_stake, lib.as_ref()) {
        if let Some((name, w, h, rgba)) = lib.sheets.iter().find(|s| s.0 == p.sheet) {
            if let Some(k) = kinds.iter_mut().find(|k| k.0 == "stakes") {
                k.2 = Texture { name: name.clone(), width: *w, height: *h, rgba: rgba.clone() };
            }
        }
    }
    if let Some(lib) = lib.as_ref() {
        let from = kinds.len();
        if let Some((name, w, h, rgba)) =
            lib.sheets.iter().find(|s| crate::trackprops::SCATTER_SHEETS.contains(&s.0.as_str()))
        {
            let mesh = tearoffs(prog, syn, seed);
            tally.push(("tearoffs", mesh.triangle_count() / 4));
            let tex = Texture { name: name.clone(), width: *w, height: *h, rgba: rgba.clone() };
            // Drawn, never solid: a strip of film in the dirt.
            kinds.push(("tearoffs".into(), mesh, tex, false));
        }
        for (name, mesh, tex, is_solid) in plant_trees(lib, &plants) {
            tally.push(("real trees", mesh.triangle_count()));
            kinds.push((name, mesh, tex, is_solid));
        }
        let (placed, got) = lifted_counted(lib, prog, syn);
        tally.push(("arches", got.arches));
        tally.push(("bales", got.bales));
        tally.push(("parked cars", got.parked));
        tally.push(("turn markers", got.turn_markers));
        tally.push(("pit vehicles", got.pit_vehicles));
        for (name, mesh, tex, is_solid) in placed {
            tally.push(("lifted", mesh.triangle_count()));
            kinds.push((name, mesh, tex, is_solid));
        }
        log::info!(
            "placed {} lifted models from {}",
            kinds.len() - from,
            lib.donor
        );
    }

    // The paddock, its road to the pits, and the sponsor wall behind the gates.
    crate::trackvenue::dress(prog, syn, lib.as_ref(), &mut kinds, &mut tally);

    let mut files = Vec::new();
    let mut drawn = Vec::new();
    let mut solid = Vec::new();
    let mut models = Vec::new();
    // The game indexes a draw group in 16 bits and TerrainEd makes one group of a model, so a
    // model past that lost what lay beyond its 65,535th vertex: the arch, on Northgate.
    let kinds: Vec<(String, Mesh, Texture, bool)> = kinds
        .into_iter()
        .flat_map(|(name, mesh, sheet, is_solid)| {
            split_for_draw(&mesh, MODEL_MAX_VERTS)
                .into_iter()
                .enumerate()
                .map(|(k, part)| (if k == 0 { name.clone() } else { format!("{name}_{k}") }, part, sheet.clone(), is_solid))
                .collect::<Vec<_>>()
        })
        .collect();
    for (name, mesh, sheet, is_solid) in kinds {
        if mesh.vertex_count() < 8 {
            continue;
        }
        let file = format!("{name}.edf");
        let part = Part { name: name.clone(), mesh, texture: 0, normal: None };
        let bytes =
            edfwrite::write(&name, std::slice::from_ref(&part), std::slice::from_ref(&sheet));
        files.push((file.clone(), bytes));
        models.push((part.mesh, sheet));
        let at = Scene { file, pos: [0.0, 0.0, 0.0], rot: [0.0, 0.0, 0.0] };
        // Collision only for what should stop a bike. A stake snaps and the fence is behind
        // the run-off, so neither is a wall; a tree, a bale and the gantry are.
        if is_solid {
            solid.push(at.clone());
        }
        drawn.push(at);
    }

    // In the build output, so what stood can be checked without opening the track.
    let shown: Vec<_> = tally.iter().filter(|(k, _)| *k != "lifted" && *k != "real trees").collect();
    println!("  scenery {shown:?}");
    log::info!("scenery {shown:?}");
    Scenery { files, drawn, solid, tally, models }
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

    /// The start straight is fenced from the lap beside it, and the join is left open.
    #[test]
    fn the_start_straight_is_fenced_up_to_its_join() {
        let (p, s) = demo();
        let Some(spur) = s.spur.as_ref() else {
            return;
        };
        let line = spur_fence_line(&s, &p.stations(2.0));
        assert!(line.len() > 4, "no fence beside the start straight");
        let end = spur.stations.last().unwrap();
        for &(x, z) in &line {
            assert!(s.outside_the_start(x, z).map_or(true, |e| e > 0.5), "the fence stands on the start straight");
            assert!((x - end.x).hypot(z - end.z) > SPUR_FENCE_GAP_M - 2.0, "the fence closes the join");
        }
    }


    /// Tear-offs lie in patches at the corners, on or beside the track, and never on the start.
    #[test]
    fn tear_offs_lie_in_the_corners() {
        let prog: TrackProgram = serde_json::from_str(crate::trackprog::EXAMPLE).unwrap();
        let syn = crate::tracksynth::synthesise(&prog).unwrap();
        let m = tearoffs(&prog, &syn, 7);
        let strips = m.vertex_count() / 4;
        assert!(strips > 200, "only {strips} tear-offs");
        let st = prog.stations(1.0);
        for v in m.positions.chunks_exact(3).step_by(4) {
            let near = st.iter().map(|q| (q.x - v[0]).hypot(q.z - v[2])).fold(f32::INFINITY, f32::min);
            assert!(near < TEAROFF_OUT_M + 1.0, "a tear-off {near:.1} m from the lap");
            assert!(
                syn.outside_the_start(v[0], v[2]).map(|e| e > OFF_THE_START_M - 0.5).unwrap_or(true),
                "a tear-off on the start"
            );
        }
    }

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
        let m = banner_piece(Print::Boards, 0, 0, true);
        let post_vs: Vec<f32> = m.uvs.chunks_exact(2).skip(8).map(|uv| uv[1]).collect();
        assert!(
            !post_vs.is_empty() && post_vs.iter().all(|v| *v >= band - 1e-4),
            "the uprights sample the printed cells"
        );
    }

    /// A board reads the right way round from both sides, which means its two faces run `u`
    /// A board is a board: it has a front, a back, a thickness, and the wordmark reads the
    /// right way round from either side of it.
    ///
    /// The mirroring is asserted by *position*, not by vertex index, because that is the
    /// property that matters — walk the board in world +X and one printed face's `u` climbs
    /// while the other's falls. Index-paired, the old card made the same claim and was still
    /// showing every board on the far side of the lap backwards.
    #[test]
    fn a_board_is_printed_on_both_faces_and_has_a_thickness() {
        let m = banner_piece(Print::Boards, 0, 0, false);
        // `cuboid` writes +z first and -z second, four vertices each.
        let face = |k: usize| -> Vec<(f32, f32, f32)> {
            (k * 4..k * 4 + 4)
                .map(|i| (m.positions[i * 3], m.positions[i * 3 + 2], m.uvs[i * 2]))
                .collect()
        };
        let (front, back) = (face(0), face(1));
        assert!(
            front.iter().all(|v| v.1 > 0.0) && back.iter().all(|v| v.1 < 0.0),
            "the two printed faces are not either side of the panel: {front:?} / {back:?}"
        );
        let thickness = front[0].1 - back[0].1;
        assert!(
            (thickness - BANNER_D_M).abs() < 1e-5,
            "the board is {thickness:.3} m thick, not {BANNER_D_M}"
        );
        // Same picture, read the same way round from both sides: sorted by world X, the two
        // faces' u run opposite ways.
        let run = |f: &[(f32, f32, f32)]| -> f32 {
            let mut v: Vec<(f32, f32)> = f.iter().map(|p| (p.0, p.2)).collect();
            v.sort_by(|a, b| a.0.total_cmp(&b.0));
            v.last().unwrap().1 - v[0].1
        };
        let (fr, br) = (run(&front), run(&back));
        assert!(
            fr * br < 0.0,
            "both faces grow u the same way across the board — one of them reads backwards \
             ({fr:+.3} and {br:+.3})"
        );
        assert!(fr.abs() > 1e-3, "the print has no width across the board");
        // And the edges are the plain band, not a smear of the wordmark round the rim.
        let (top, bot) = banner_post_band();
        for i in 8..m.vertex_count() {
            let v = m.uvs[i * 2 + 1];
            assert!(
                v >= top.min(bot) - 1e-4 && v <= top.max(bot) + 1e-4,
                "vertex {i} of the frame samples v {v:.3}, outside the post band"
            );
        }
    }

    /// Pieces land exactly a piece apart, so a run is joined up rather than a row of signs.
    #[test]
    fn pieces_land_a_piece_apart() {
        // A 30 m radius corner, sampled the way `stations` samples one.
        let (r, step) = (30.0f32, 0.5f32);
        let line: Vec<(f32, f32)> = (0..400)
            .map(|i| {
                let a = i as f32 * step / r;
                (r * a.sin(), r * a.cos())
            })
            .collect();
        let mut acc = vec![0.0f32];
        for w in line.windows(2) {
            let d = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
            acc.push(acc.last().unwrap() + d);
        }
        for style in [Print::Boards, Print::Tiled] {
            let w = style.size().0;
            let mut cursor = 0usize;
            let mut last: Option<(f32, f32)> = None;
            for k in 0..12 {
                let (x, z, _) = along_line(&line, &acc, &mut cursor, k as f32 * w + w * 0.5);
                if let Some((px, pz)) = last {
                    let d = ((x - px).powi(2) + (z - pz).powi(2)).sqrt();
                    assert!(
                        (d - w).abs() < w * 0.02,
                        "{style:?}: piece {k} sits {d:.3} m from the last, not {w:.3} m"
                    );
                }
                last = Some((x, z));
            }
        }
    }

    /// A lap carries both kinds of printed line, not one of them everywhere.
    #[test]
    fn a_lap_gets_both_hoardings_and_tiled_banner() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let count = |k: &str| sc.tally.iter().find(|(n, _)| *n == k).map(|(_, v)| *v).unwrap_or(0);
        let (runs, tiled) = (count("banner runs"), count("tiled runs"));
        assert!(runs >= 6, "{runs} printed runs on a lap");
        assert!(
            tiled > 0 && tiled < runs,
            "{tiled} of {runs} runs are tiled — a lap wants some of each"
        );
    }

    /// A tiled run prints one design over and over, and the print does not stretch — pieces
    /// each printing the whole cell would squash a 4 m lockup onto a 1.5 m piece.
    #[test]
    fn a_tiled_run_repeats_one_print_without_squashing_it() {
        let (w, h) = Print::Tiled.size();
        // The piece prints as many metres of the design as it is wide, at the design's own
        // proportions — which is the whole reason `TILE_W_M` is derived and not picked.
        assert!(
            ((w / TILE_PRINT_M) - (w * BANNER_H_M / (BANNER_W_M * h))).abs() < 1e-5,
            "a {w:.2} m by {h} m piece does not print at the cell's proportions"
        );
        let mut seen = Vec::new();
        for piece in 0..12 {
            let (u0, u1) = Print::Tiled.window(piece);
            assert!(
                (u1 - u0 - w / TILE_PRINT_M).abs() < 1e-5,
                "piece {piece} prints {:.3} of the cell for {w:.2} m of run — it stretches",
                u1 - u0
            );
            seen.push(u0);
        }
        // It comes round: the window walks the cell and wraps, rather than sitting still.
        assert!(
            (seen[0] - seen[TILE_PER_PRINT]).abs() < 1e-4,
            "the print does not repeat after {TILE_PER_PRINT} pieces: {seen:?}"
        );
        assert!(seen[1] > seen[0], "every piece prints the same slice — nothing moves");

        // A board is the whole cell, and it is the piece that carries an upright at all.
        assert_eq!(Print::Boards.window(3), (0.0, 1.0));
        assert!(Print::Boards.post_at(1).is_some(), "a hoarding posts every join");
        assert!(Print::Tiled.post_at(1).is_none(), "a strung banner does not");
    }

    /// The sky is one exposure, so it goes round exactly once.
    ///
    /// The drawn sheet is noise and tiles four times to get some detail out of 256 pixels.
    /// A photograph has a sun in it, and four wraps put four suns over the track.
    #[test]
    fn the_photographed_sky_wraps_once() {
        let sheet = dome_sheet();
        let photo = dome_photo().is_some();
        assert!(photo, "the dome photograph is bundled and decodes");
        assert_ne!(
            sheet.width, sheet.height,
            "the photograph is a 360 panorama, not a square tile"
        );
        assert_eq!(
            sheet.rgba.len(),
            sheet.width as usize * sheet.height as usize * 4,
            "the sheet is RGBA"
        );

        let bytes = dome_file(1200.0);
        let nodes = crate::edf::parse_world(&bytes);
        assert!(!nodes.is_empty(), "the dome reads back as a model");
        let u_max = nodes
            .iter()
            .flat_map(|n| n.uvs.chunks_exact(2))
            .fold(f32::NEG_INFINITY, |m, uv| m.max(uv[0]));
        assert!(
            (u_max - 1.0).abs() < 1e-3,
            "the sheet goes round {u_max} times — a photographed sky must go round once"
        );

        // And the picture survives the round trip at its full width.
        let tex = crate::edf::embedded_textures(&bytes);
        assert_eq!(tex.len(), 1, "one sheet on the dome");
        assert_eq!((tex[0].width, tex[0].height), (sheet.width, sheet.height));

        // The band baked into the map wears the same sheet, so it wraps once as well — and it
        // reaches only SKY_TOP_DEG, so it must take that slice of the picture rather than
        // stretching the whole sky into it.
        let band = dome_mesh(1200.0);
        let (mut u_hi, mut v_lo, mut v_hi) = (f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY);
        for uv in band.uvs.chunks_exact(2) {
            u_hi = u_hi.max(uv[0]);
            v_lo = v_lo.min(uv[1]);
            v_hi = v_hi.max(uv[1]);
        }
        assert!((u_hi - 1.0).abs() < 1e-3, "the band wraps {u_hi} times");
        assert!((v_hi - 1.0).abs() < 1e-3, "the band starts at the horizon");
        // v is 1 at the horizon and 0 at the zenith, so a band reaching 34 degrees stops at
        // 1 - 34/90. Anything near 0 means it swallowed the whole sky.
        let expect = 1.0 - SKY_TOP_DEG / 90.0;
        assert!(
            (v_lo - expect).abs() < 0.02,
            "the band tops out at v {v_lo}, not {expect} — it is stretching the sky"
        );
    }

    /// Write a generated `dome.edf` out, to look at or to drop into a track by hand.
    ///
    /// ```text
    /// FROST_DUMP=/tmp/gen cargo test --bin mxb-app -- --ignored --nocapture write_the_dome
    /// ```
    #[test]
    #[ignore = "writes a dome.edf — set FROST_DUMP"]
    fn write_the_dome() {
        let dir = std::env::var("FROST_DUMP").expect("set FROST_DUMP");
        std::fs::create_dir_all(&dir).unwrap();
        let bytes = dome_file(1200.0);
        let path = format!("{dir}/dome.edf");
        std::fs::write(&path, &bytes).unwrap();
        println!("{path} {:.2} MB", bytes.len() as f64 / 1_048_576.0);
    }

    /// Write the sheets out as PNGs, which is the only way to judge whether a banner reads.
    ///
    /// ```text
    /// FROST_SHEETS=/tmp/gen cargo test -p mxb-app --bin mxb-app -- --ignored --nocapture the_sheets
    /// ```
    #[test]
    #[ignore = "writes PNGs — set FROST_SHEETS"]
    fn the_sheets() {
        let dir = std::env::var("FROST_SHEETS").expect("set FROST_SHEETS");
        std::fs::create_dir_all(&dir).unwrap();
        for t in [banner_sheet(), stake_sheet()] {
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
        let boards = sc.tally.iter().find(|(k, _)| *k == "banner boards").unwrap().1;
        let runs = sc.tally.iter().find(|(k, _)| *k == "banner runs").unwrap().1;
        assert!(boards >= BANNER_CELLS, "{boards} boards — too few to cycle");
        // A hoarding is boards bolted together, so a run has to be a run. One board per run
        // is the failure the break logic causes when it fires on every step.
        assert!(
            runs > 0 && boards / runs >= BANNER_RUN_MIN / 2,
            "{boards} boards in {runs} runs — the hoarding is not joined up"
        );
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

    /// A lifted prop whose mesh reaches past its own box is a donor's run: never replayed.
    #[test]
    fn a_one_legged_arch_gets_its_other_leg() {
        let at = |c: Mesh, x: f32, y: f32| {
            let (lo, hi) = c.bounds();
            edfwrite::moved(&c, [x - (lo[0] + hi[0]) * 0.5, y - lo[1], -(lo[2] + hi[2]) * 0.5])
        };
        let mut arch = at(edfwrite::cuboid(0.5, 6.0, 0.5), -7.0, 0.0);
        arch.append(&at(edfwrite::cuboid(14.5, 0.5, 0.5), 0.0, 5.5));
        let (both, _, _) = arch_on_legs(&arch).expect("an arch");
        let low: Vec<f32> = both.positions.chunks_exact(3).filter(|v| v[1] < 0.4).map(|v| v[0]).collect();
        assert!(low.iter().any(|&x| x < -6.0), "lost its own leg");
        assert!(low.iter().any(|&x| x > 6.0), "no leg at the far end: {low:?}");
    }

    /// A header wider than its legs still stands on them, and only what hangs low between the
    /// legs goes: a banner hung under it, and a flag stood between them, whole.
    #[test]
    fn an_arch_with_a_wide_header_keeps_its_legs() {
        let at = |c: Mesh, x: f32, y: f32| {
            let (lo, hi) = c.bounds();
            edfwrite::moved(&c, [x - (lo[0] + hi[0]) * 0.5, y - lo[1], -(lo[2] + hi[2]) * 0.5])
        };
        let mut gfx = at(edfwrite::cuboid(0.5, 6.0, 0.5), -8.5, 0.0);
        gfx.append(&at(edfwrite::cuboid(0.5, 6.0, 0.5), 8.5, 0.0));
        // The header overhangs each leg by three metres, as RaceGFX's does.
        gfx.append(&at(edfwrite::cuboid(24.0, 1.0, 0.5), 0.0, 5.5));
        gfx.append(&at(edfwrite::cuboid(14.0, 1.0, 0.1), 0.0, 0.3));
        gfx.append(&at(edfwrite::cuboid(0.1, 4.0, 1.0), 2.0, 0.0));
        let (m, gap, _) = arch_on_legs(&gfx).expect("an arch");
        let feet: Vec<f32> = m.positions.chunks_exact(3).filter(|v| v[1] < 0.4).map(|v| v[0]).collect();
        assert!(feet.iter().any(|&x| x < -8.0) && feet.iter().any(|&x| x > 8.0), "lost a leg: {feet:?}");
        assert!(
            m.positions.chunks_exact(3).all(|v| v[0].abs() > 8.0 || v[1] >= 5.5),
            "a banner or flag still hangs between the legs"
        );
        assert!(m.positions.chunks_exact(3).any(|v| v[0].abs() > 11.0), "lost the header's overhang");
        assert!((gap - (17.0 - 0.5)).abs() < 0.05, "gap {gap}");

        // On our lap: a foot on the ground either side, clear of the edge, and nothing low over it.
        let (p, s) = demo();
        let reach = gfx.positions.chunks_exact(3).map(|v| v[0].hypot(v[2])).fold(0.0f32, f32::max);
        let lib = crate::trackprops::PropLibrary {
            donor: "t".into(),
            donor_lap_m: 1000.0,
            props: vec![crate::trackprops::Prop {
                id: "gfx".into(),
                sheet: "gfx_c".into(),
                class: crate::trackobjects::Class::Structure,
                mesh: gfx,
                height: 6.5,
                span: 24.0,
                reach,
                axis_ref: 0.0,
            }],
            instances: vec![crate::trackprops::Instance { prop: 0, along: 0.5, offset: 0.0, yaw: 0.0, lift: 0.0, near: true }],
            runs: vec![],
            sheets: vec![("gfx_c".into(), 2, 2, vec![200u8; 16])],
        };
        let placed = lifted(&lib, &p, &s);
        let (_, mesh, ..) = placed.iter().find(|k| k.0 == "gfx").expect("the arch was placed");
        let (st, half) = (p.stations(0.5), p.width * 0.5);
        let mut sides = [false; 2];
        for v in edge_points(mesh, 0.5) {
            let up = v[1] - ground(&s, v[0], v[2]);
            let q = st.iter().min_by(|a, b| (a.x - v[0]).hypot(a.z - v[2]).total_cmp(&(b.x - v[0]).hypot(b.z - v[2]))).unwrap();
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            let lat = (v[0] - q.x) * rx + (v[2] - q.z) * rz;
            if up <= RIDE_H_M {
                assert!(lat.abs() > half, "a piece {up:.1} m up stands {:.1} m from the centreline", lat.abs());
            }
            if up < 0.2 {
                assert!(lat.abs() > half + ARCH_LEG_CLEAR_M - 0.3, "a foot {:.1} m out, inside the leg clearance", lat.abs());
                sides[(lat > 0.0) as usize] = true;
            }
        }
        assert!(sides[0] && sides[1], "not standing on a leg either side: {sides:?}");
    }

    /// A pole as tall as the frame, standing between the legs, is not a leg: the outer foot is,
    /// its far twin is mirrored in, and the pole goes whole rather than leave its top in the air.
    #[test]
    fn a_pole_between_the_legs_is_not_a_leg() {
        let at = |c: Mesh, x: f32, y: f32| {
            let (lo, hi) = c.bounds();
            edfwrite::moved(&c, [x - (lo[0] + hi[0]) * 0.5, y - lo[1], -(lo[2] + hi[2]) * 0.5])
        };
        let mut arch = at(edfwrite::cuboid(0.5, 6.0, 0.5), 6.6, 0.0);
        arch.append(&at(edfwrite::cuboid(0.3, 5.5, 0.3), -2.3, 0.0));
        arch.append(&at(edfwrite::cuboid(14.2, 1.0, 0.5), 0.0, 6.0));
        let (m, gap, _) = arch_on_legs(&arch).expect("an arch");
        let feet: Vec<f32> = m.positions.chunks_exact(3).filter(|v| v[1] < 0.4).map(|v| v[0]).collect();
        assert!(feet.iter().any(|&x| x < -6.0) && feet.iter().any(|&x| x > 6.0), "not on two legs: {feet:?}");
        assert!(feet.iter().all(|&x| x.abs() > 6.0), "the pole stood as a leg: {feet:?}");
        assert!(
            m.positions.chunks_exact(3).all(|v| v[0].abs() > 5.0 || v[1] >= 6.0),
            "the pole's top was left in the air"
        );
        assert!((gap - 12.7).abs() < 0.05, "gap {gap}");
    }

    /// Bales stand in rows just past the track edge, never where the donor had them.
    #[test]
    fn bales_stand_beside_the_track() {
        let (p, s) = demo();
        let c = edfwrite::cuboid(1.2, 0.8, 0.8);
        let (lo, _) = c.bounds();
        let bale = edfwrite::moved(&c, [0.0, -lo[1], 0.0]);
        let reach = bale.positions.chunks_exact(3).map(|v| v[0].hypot(v[2])).fold(0.0f32, f32::max);
        let lib = crate::trackprops::PropLibrary {
            donor: "t".into(),
            donor_lap_m: 1000.0,
            props: vec![crate::trackprops::Prop {
                id: "bale".into(),
                sheet: "bale_c".into(),
                class: crate::trackobjects::Class::Bale,
                mesh: bale,
                height: 0.8,
                span: 1.2,
                reach,
                axis_ref: 0.0,
            }],
            // The donor stood them 60 m out, in its trees.
            instances: (0..10)
                .map(|i| crate::trackprops::Instance { prop: 0, along: i as f32 / 10.0, offset: 60.0, yaw: 0.0, lift: 0.0, near: false })
                .collect(),
            runs: vec![],
            sheets: vec![("bale_c".into(), 2, 2, vec![200u8; 16])],
        };
        let (placed, got) = lifted_counted(&lib, &p, &s);
        let bales = got.bales;
        assert!(bales >= 6, "{bales} bales on a lap");
        assert!(
            bales as f32 <= p.lap_length() / 1000.0 * BALE_PER_KM + BALE_ROW.1 as f32,
            "{bales} bales: more than a rated track carries"
        );
        let (_, m, ..) = placed.iter().find(|k| k.0 == "bale").expect("bales placed");
        let (st, half) = (p.stations(0.5), p.width * 0.5);
        for v in m.positions.chunks_exact(3) {
            let d = st.iter().map(|q| (q.x - v[0]).hypot(q.z - v[2])).fold(f32::INFINITY, f32::min);
            assert!(d > half + 0.9 && d < half + 4.5, "a bale {d:.1} m from the centreline, past a {half:.1} m half-width");
            assert!((v[1] - ground(&s, v[0], v[2])).abs() < 1.0, "a bale off the ground");
        }
        // At a corner they stand on its outside, where riders run wide; the inside is the markers'.
        let (mut inner, mut outer) = (0, 0);
        for v in m.positions.chunks_exact(3) {
            let q = st.iter().min_by(|a, b| (a.x - v[0]).hypot(a.z - v[2]).total_cmp(&(b.x - v[0]).hypot(b.z - v[2]))).unwrap();
            if q.curvature.abs() < 1.0 / BALE_CORNER_R_M {
                continue;
            }
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            if ((v[0] - q.x) * rx + (v[2] - q.z) * rz) * q.curvature > 0.0 { inner += 1 } else { outer += 1 }
        }
        assert!(outer > 0 && inner * 3 <= outer, "{outer} bale corners outside a turn, {inner} inside");
    }

    /// One stake a spot, evenly down both edges: none doubled, about every STAKE_GAP_M.
    #[test]
    fn stakes_stand_one_a_spot_down_both_edges() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let (_, bytes) = sc.files.iter().find(|(f, _)| f == "stakes.edf").expect("stakes");
        let mut feet: Vec<[f32; 3]> = Vec::new();
        for n in crate::edf::parse_world(bytes) {
            for v in n.positions.chunks_exact(3) {
                if !feet.iter().any(|f| (f[0] - v[0]).hypot(f[2] - v[2]) < 0.3) {
                    feet.push([v[0], v[1], v[2]]);
                }
            }
        }
        for (i, f) in feet.iter().enumerate() {
            for g in &feet[i + 1..] {
                let d = (f[0] - g[0]).hypot(f[2] - g[2]);
                assert!(d > 3.0, "two stakes {d:.1} m apart at ({:.0}, {:.0})", f[0], f[2]);
            }
        }
        let st = p.stations(0.5);
        let mut sides: [Vec<f32>; 2] = [Vec::new(), Vec::new()];
        for f in &feet {
            let q = st.iter().min_by(|a, b| (a.x - f[0]).hypot(a.z - f[2]).total_cmp(&(b.x - f[0]).hypot(b.z - f[2]))).unwrap();
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            sides[(((f[0] - q.x) * rx + (f[2] - q.z) * rz) > 0.0) as usize].push(q.s);
        }
        let (l, r) = (sides[0].len(), sides[1].len());
        assert!(l.min(r) * 10 >= l.max(r) * 8, "{l} stakes down the left, {r} down the right");
        for side in &mut sides {
            side.sort_by(f32::total_cmp);
            let mut gaps: Vec<f32> = side.windows(2).map(|w| w[1] - w[0]).filter(|g| *g < 20.0).collect();
            gaps.sort_by(f32::total_cmp);
            let med = gaps[gaps.len() / 2];
            assert!((med - STAKE_GAP_M).abs() < 0.8, "stakes {med:.1} m apart, not {STAKE_GAP_M}");
        }
    }

    /// Indiana's stake lifted as a prop is not laid again beside the rule's own.
    #[test]
    fn a_lifted_edge_stake_is_not_laid_twice() {
        let (p, s) = demo();
        let c = edfwrite::cuboid(0.045, 0.76, 0.045);
        let prop = |id: &str| crate::trackprops::Prop {
            id: id.into(),
            sheet: "stake_c".into(),
            class: crate::trackobjects::Class::Structure,
            mesh: c.clone(),
            height: 0.76,
            span: 0.045,
            reach: 0.032,
            axis_ref: 0.0,
        };
        let lib = crate::trackprops::PropLibrary {
            donor: "t".into(),
            donor_lap_m: 1000.0,
            props: vec![prop("edge_stake"), prop("stake_copy")],
            instances: (0..40)
                .map(|i| crate::trackprops::Instance { prop: 1, along: i as f32 / 40.0, offset: if i % 2 == 0 { 5.4 } else { -5.4 }, yaw: 0.0, lift: 0.0, near: true })
                .collect(),
            runs: vec![],
            sheets: vec![("stake_c".into(), 2, 2, vec![200u8; 16])],
        };
        let (placed, _) = lifted_counted(&lib, &p, &s);
        assert!(placed.iter().all(|k| k.0 != "stake"), "a lifted stake stood beside the rule's");
    }

    /// A mat and a stand at every stall the game spawns a rider in, where the `.rdf` says.
    #[test]
    fn a_stand_at_every_stall_the_rdf_spawns() {
        let (p, s) = demo();
        let dir = std::env::temp_dir().join(format!("mxb-pits-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let wrote = crate::tracksynth::write_source(&p, &s, &dir).unwrap();
        let rdf = wrote.iter().find(|f| f.ends_with(".rdf")).expect("an .rdf is written");
        let txt = std::fs::read_to_string(dir.join(rdf)).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let lane = &txt[txt.find("pit_lane").expect("a pit lane")..txt.find("pit_board").unwrap_or(txt.len())];
        let (mut longs, mut lats) = (Vec::new(), Vec::new());
        for line in lane.lines().map(str::trim) {
            if let Some(v) = line.strip_prefix("long = ") { longs.push(v.parse::<f32>().unwrap()); }
            if let Some(v) = line.strip_prefix("lat = ") { lats.push(v.parse::<f32>().unwrap()); }
        }
        let pits = Pits::of(&p);
        assert_eq!(longs.len(), pits.stalls.len(), "the .rdf spawns {} riders, we mark {}", longs.len(), pits.stalls.len());
        for ((l, a), &(sl, sa)) in longs.iter().zip(&lats).zip(&pits.stalls) {
            assert!((l - sl).abs() < 0.01 && (a - sa).abs() < 0.01, "stall at {l}/{a} in the .rdf, {sl}/{sa} here");
        }
        let sc = build(&p, &s);
        assert_eq!(sc.tally.iter().find(|(k, _)| *k == "pit stands").map(|x| x.1), Some(pits.stalls.len()));
        let (_, bytes) = sc.files.iter().find(|(f, _)| f == "pit_stands.edf").expect("stands written");
        let verts: Vec<[f32; 3]> = crate::edf::parse_world(bytes).iter().flat_map(|n| n.positions.chunks_exact(3).map(|v| [v[0], v[1], v[2]]).collect::<Vec<_>>()).collect();
        let (st, half) = (p.stations(0.5), p.width * 0.5);
        for &(long, lat) in &pits.stalls {
            let q = st[((long / 0.5) as usize).min(st.len() - 1)];
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            let (x, z) = (q.x + rx * lat, q.z + rz * lat);
            assert!(verts.iter().any(|v| (v[0] - x).hypot(v[2] - z) < 1.5), "no mat at the stall {long:.0} m round");
        }
        for v in &verts {
            let d = st.iter().map(|q| (q.x - v[0]).hypot(q.z - v[2])).fold(f32::INFINITY, f32::min);
            assert!(d > half + 1.0, "a stand {d:.1} m from the centreline, on the track");
        }
    }

    /// Pit vehicles park in a row behind the stalls, off the lane and the track; a lifted truck
    /// that would have stood by the pits is not replayed there.
    #[test]
    fn pit_vehicles_park_in_a_row_behind_the_stalls() {
        let (p, s) = demo();
        let foot = |c: Mesh| {
            let (lo, hi) = c.bounds();
            edfwrite::moved(&c, [-(lo[0] + hi[0]) * 0.5, -lo[1], -(lo[2] + hi[2]) * 0.5])
        };
        let prop = |id: &str, sheet: &str, class, mesh: Mesh, height: f32, span: f32| {
            let reach = mesh.positions.chunks_exact(3).map(|v| v[0].hypot(v[2])).fold(0.0f32, f32::max);
            crate::trackprops::Prop { id: id.into(), sheet: sheet.into(), class, mesh, height, span, reach, axis_ref: 0.0 }
        };
        let pits = Pits::of(&p);
        let lap = p.lap_length();
        let lib = crate::trackprops::PropLibrary {
            donor: "t".into(),
            donor_lap_m: 1000.0,
            props: vec![
                prop("truck", "truck_c", crate::trackobjects::Class::Vehicle, foot(edfwrite::cuboid(8.0, 3.0, 2.5)), 3.0, 8.0),
                prop("tent", "tent_sides_c", crate::trackobjects::Class::Structure, foot(edfwrite::cuboid(4.0, 2.8, 4.0)), 2.8, 4.0),
            ],
            // The donor's truck, stood on what is our pit lane.
            instances: vec![crate::trackprops::Instance { prop: 0, along: pits.stalls[pits.stalls.len() / 2].0 / lap, offset: pits.stalls[0].1, yaw: 0.0, lift: 0.0, near: true }],
            runs: vec![],
            sheets: ["truck_c", "tent_sides_c"].iter().map(|n| (n.to_string(), 2, 2, vec![200u8; 16])).collect(),
        };
        let (placed, got) = lifted_counted(&lib, &p, &s);
        assert_eq!(got.pit_vehicles, 0, "the paddock parks them now: trackvenue");
        let half = p.width * 0.5;
        for (name, m, ..) in &placed {
            for v in edge_points(m, 0.5) {
                let (past, out) = pits.frame(lap, &s, v[0], v[2]);
                assert!(out > pits.lane + PIT_HALF_M, "{name} stands on the pit lane, {out:.1} m out");
                assert!(out < pits.lane + PIT_HALF_M + PIT_ROW_GAP_M + 5.5, "{name} strays from the row, {out:.1} m out");
                assert!(past < PIT_PARK_PAST_M + 10.0, "{name} parked {past:.0} m past the stalls");
                assert!(s.dist[grid_cell(&s, v[0], v[2])] > half + 1.0, "{name} on the track");
            }
        }
    }

    /// No model reaches past what a 16-bit draw group indexes, and splitting keeps every triangle.
    #[test]
    fn a_big_model_splits_into_draw_sized_parts() {
        let mut m = Mesh::default();
        for i in 0..3000 {
            m.append(&edfwrite::moved(&edfwrite::cuboid(0.5, 0.5, 0.5), [i as f32, 0.0, 0.0]));
        }
        let parts = split_for_draw(&m, 20_000);
        assert!(parts.len() >= 4, "{} parts", parts.len());
        assert!(parts.iter().all(|p| p.vertex_count() <= 20_000), "a part past the cap");
        assert_eq!(parts.iter().map(|p| p.triangle_count()).sum::<usize>(), m.triangle_count(), "triangles lost");
        assert!(MODEL_MAX_VERTS < 65_536);
    }

    /// Draw the pits from above, stall spots ringed, to judge the layout by eye.
    ///
    /// ```text
    /// FROST_PROPS=library.fpl FROST_PIT_PNG=/tmp/pits.png cargo test --bins -- --ignored draw_the_pits
    /// ```
    #[test]
    #[ignore = "draws the pits — set FROST_PROPS and FROST_PIT_PNG"]
    fn draw_the_pits() {
        let path = std::env::var("FROST_PIT_PNG").expect("set FROST_PIT_PNG");
        let m = match crate::tracklayout::search(103, 1) { Ok(m) => m.program, Err(v) => v[0].program.clone() };
        let mut prog = m.clone();
        prog.terrain.surface = serde_json::from_str("\"soil\"").unwrap();
        let prog = crate::tracksynth::with_fitted_budget(&prog).unwrap();
        let syn = crate::tracksynth::synthesise(&prog).unwrap();
        let sc = build(&prog, &syn);
        let pits = Pits::of(&prog);
        let (lap, half, st) = (prog.lap_length(), prog.width * 0.5, prog.stations(0.5));
        let world = |long: f32, lat: f32| {
            let q = st[((long.rem_euclid(lap) / 0.5) as usize).min(st.len() - 1)];
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            (q.x + rx * lat, q.z + rz * lat)
        };
        let mut pts = Vec::new();
        for &(l, a) in &pits.stalls {
            pts.extend([world(l - 40.0, a), world(l + 40.0, a), world(l, a + pits.side * 22.0), world(l, -a)]);
        }
        let x0 = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min);
        let x1 = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
        let z0 = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        let z1 = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
        let ppm = 8.0;
        let (w, h) = (((x1 - x0) * ppm) as u32, ((z1 - z0) * ppm) as u32);
        let mut img = image::RgbaImage::new(w, h);
        for py in 0..h {
            for px in 0..w {
                let (x, z) = (x0 + px as f32 / ppm, z1 - py as f32 / ppm);
                let c = if syn.dist[grid_cell(&syn, x, z)] < half {
                    [150, 110, 70]
                } else if pits.lane_at(lap, &syn, x, z) {
                    [205, 190, 150]
                } else {
                    [70, 110, 60]
                };
                img.put_pixel(px, py, image::Rgba([c[0], c[1], c[2], 255]));
            }
        }
        let to_px = |x: f32, z: f32| ((x - x0) * ppm, (z1 - z) * ppm);
        let colour = |name: &str| -> Option<[u8; 3]> {
            if name.starts_with("vehicles") || name.starts_with("semi_trailers") { Some([40, 90, 200]) }
            else if name.starts_with("tent") || name.starts_with("big_tent") || name.starts_with("easy_ups") { Some([240, 240, 240]) }
            else if name.starts_with("pit_stands") { Some([220, 40, 30]) }
            else if name.starts_with("barrier") { Some([30, 30, 30]) }
            else if name.starts_with("stakes") { Some([250, 210, 20]) }
            else if name.contains("haybale") { Some([200, 150, 60]) }
            else { None }
        };
        for (name, bytes) in &sc.files {
            let Some(c) = colour(name) else { continue };
            for n in crate::edf::parse_world(bytes) {
                for t in n.indices.chunks_exact(3) {
                    let q: Vec<(f32, f32)> = t.iter().map(|&i| to_px(n.positions[i as usize * 3], n.positions[i as usize * 3 + 2])).collect();
                    let (bx0, bx1) = (q.iter().map(|p| p.0).fold(f32::MAX, f32::min).max(0.0), q.iter().map(|p| p.0).fold(f32::MIN, f32::max).min(w as f32 - 1.0));
                    let (by0, by1) = (q.iter().map(|p| p.1).fold(f32::MAX, f32::min).max(0.0), q.iter().map(|p| p.1).fold(f32::MIN, f32::max).min(h as f32 - 1.0));
                    if bx0 > bx1 || by0 > by1 { continue; }
                    let area = (q[1].0 - q[0].0) * (q[2].1 - q[0].1) - (q[2].0 - q[0].0) * (q[1].1 - q[0].1);
                    for py in by0 as u32..=by1 as u32 {
                        for px in bx0 as u32..=bx1 as u32 {
                            let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
                            let e = |a: (f32, f32), b: (f32, f32)| (b.0 - a.0) * (y - a.1) - (x - a.0) * (b.1 - a.1);
                            let (a, b, cc) = (e(q[1], q[2]), e(q[2], q[0]), e(q[0], q[1]));
                            let hit = if area >= 0.0 { a >= -0.5 && b >= -0.5 && cc >= -0.5 } else { a <= 0.5 && b <= 0.5 && cc <= 0.5 };
                            if hit { img.put_pixel(px, py, image::Rgba([c[0], c[1], c[2], 255])); }
                        }
                    }
                }
            }
        }
        // The spawn spots: a white ring round each.
        for &(l, a) in &pits.stalls {
            let (cx, cy) = { let (x, z) = world(l, a); to_px(x, z) };
            for py in (cy - 12.0).max(0.0) as u32..(cy + 12.0).min(h as f32) as u32 {
                for px in (cx - 12.0).max(0.0) as u32..(cx + 12.0).min(w as f32) as u32 {
                    let r = (px as f32 - cx).hypot(py as f32 - cy);
                    if (8.0..11.0).contains(&r) { img.put_pixel(px, py, image::Rgba([255, 255, 255, 255])); }
                }
            }
        }
        img.save(&path).unwrap();
        println!("PIT picture {path}: {w}x{h}, {} stalls, tally {:?}", pits.stalls.len(), sc.tally.iter().filter(|(k, _)| k.starts_with("pit") || *k == "arches" || *k == "bales").collect::<Vec<_>>());
    }

    #[test]
    fn an_arch_that_spanned_the_donor_track_spans_ours() {
        let (p, s) = demo();
        let span = 2.0 * DONOR_HALF_M + 2.0;
        // Two legs and a header: something ridden under, not a wall across the track.
        let foot = |c: Mesh, x: f32, y: f32| {
            let (lo, hi) = c.bounds();
            edfwrite::moved(&c, [x - (lo[0] + hi[0]) * 0.5, y - lo[1], -(lo[2] + hi[2]) * 0.5])
        };
        let mut c = foot(edfwrite::cuboid(0.5, 6.0, 0.5), -span * 0.5 + 0.25, 0.0);
        c.append(&foot(edfwrite::cuboid(0.5, 6.0, 0.5), span * 0.5 - 0.25, 0.0));
        c.append(&foot(edfwrite::cuboid(span, 0.5, 0.5), 0.0, 5.5));
        let xs: Vec<f32> = c.positions.chunks_exact(3).map(|v| v[0]).collect();
        let zs: Vec<f32> = c.positions.chunks_exact(3).map(|v| v[2]).collect();
        let mid = |v: &[f32]| (v.iter().copied().fold(f32::MAX, f32::min) + v.iter().copied().fold(f32::MIN, f32::max)) * 0.5;
        let arch = edfwrite::moved(&c, [-mid(&xs), 0.0, -mid(&zs)]);
        let reach = arch.positions.chunks_exact(3).map(|v| v[0].hypot(v[2])).fold(0.0f32, f32::max);
        let lib = crate::trackprops::PropLibrary {
            donor: "t".into(),
            donor_lap_m: 1000.0,
            props: vec![crate::trackprops::Prop {
                id: "arch".into(),
                sheet: "arch_c".into(),
                class: crate::trackobjects::Class::Structure,
                mesh: arch,
                height: 6.0,
                span,
                reach,
                axis_ref: 0.0,
            }],
            // Yaw 0: the donor ran along z, so this x-long arch stood across it.
            instances: vec![crate::trackprops::Instance {
                prop: 0,
                along: 0.5,
                offset: 0.0,
                yaw: 0.0,
                lift: 0.0,
                near: true,
            }],
            runs: vec![],
            sheets: vec![("arch_c".into(), 2, 2, vec![200u8; 16])],
        };
        let placed = lifted(&lib, &p, &s);
        let (_, mesh, _, _) = placed.iter().find(|k| k.0.contains("arch")).expect("the arch was placed");
        // Across the track, not along it: its legs stand off the riding surface.
        let (st, half) = (p.stations(0.5), p.width * 0.5);
        for v in mesh.positions.chunks_exact(3) {
            if v[1] - ground(&s, v[0], v[2]) > RIDE_H_M {
                continue;
            }
            let d = st.iter().map(|q| (q.x - v[0]).hypot(q.z - v[2])).fold(f32::INFINITY, f32::min);
            assert!(d > half, "a leg stands {d:.1} m from the centreline, on the track");
        }
        // Each copy round the lap on its own: gathered by where they stand, every one centred
        // over the track.
        let mut copies: Vec<(f32, f32, f32)> = Vec::new();
        for v in mesh.positions.chunks_exact(3) {
            match copies.iter_mut().find(|c| (c.0 / c.2 - v[0]).hypot(c.1 / c.2 - v[2]) < 40.0) {
                Some(c) => *c = (c.0 + v[0], c.1 + v[2], c.2 + 1.0),
                None => copies.push((v[0], v[2], 1.0)),
            }
        }
        for (x, z, n) in copies {
            let (cx, cz) = (x / n, z / n);
            let d = p.stations(1.0).iter().map(|q| (q.x - cx).hypot(q.z - cz)).fold(f32::INFINITY, f32::min);
            assert!(d < 2.0, "an arch stands {d:.1} m off the centreline, not over the track");
        }
    }

    /// An "arch" lifted with the boards round it, or a rig too tall to be one, never lays
    /// anything on our track.
    #[test]
    fn an_arch_never_lays_its_boards_on_the_track() {
        let (p, s) = demo();
        let at = |c: Mesh, x: f32, y: f32| {
            let (lo, hi) = c.bounds();
            edfwrite::moved(&c, [x - (lo[0] + hi[0]) * 0.5, y - lo[1], -(lo[2] + hi[2]) * 0.5])
        };
        let span = 2.0 * DONOR_HALF_M + 2.0;
        let arch = |h: f32| {
            let mut m = at(edfwrite::cuboid(0.5, h, 0.5), -span * 0.5, 0.0);
            m.append(&at(edfwrite::cuboid(0.5, h, 0.5), span * 0.5, 0.0));
            m.append(&at(edfwrite::cuboid(span, 0.5, 0.5), 0.0, h - 0.5));
            m
        };
        // A board run clustered in under the header, as Indiana's FXR arch lifts.
        let mut boards = arch(6.0);
        boards.append(&at(edfwrite::cuboid(span - 2.0, 1.0, 0.1), 0.0, 0.0));
        let prop = |id: &str, sheet: &str, mesh: Mesh, height: f32| {
            let reach = mesh.positions.chunks_exact(3).map(|v| v[0].hypot(v[2])).fold(0.0f32, f32::max);
            crate::trackprops::Prop {
                id: id.into(),
                sheet: sheet.into(),
                class: crate::trackobjects::Class::Structure,
                mesh,
                height,
                span: span + 0.5,
                reach,
                axis_ref: 0.0,
            }
        };
        let inst = |prop: usize, along: f32| crate::trackprops::Instance {
            prop,
            along,
            offset: 0.0,
            yaw: 0.0,
            lift: 0.0,
            near: true,
        };
        let lib = crate::trackprops::PropLibrary {
            donor: "t".into(),
            donor_lap_m: 1000.0,
            props: vec![prop("boards", "boards_c", boards, 6.0), prop("rig", "rig_c", arch(20.0), 20.0)],
            instances: vec![inst(0, 0.5), inst(1, 0.25)],
            runs: vec![],
            sheets: ["boards_c", "rig_c"].iter().map(|n| (n.to_string(), 2, 2, vec![200u8; 16])).collect(),
        };
        let (st, half) = (p.stations(0.5), p.width * 0.5);
        for (name, m, ..) in &lifted(&lib, &p, &s) {
            // Along the edges, not only at vertices: the board's middle has none.
            for v in edge_points(m, 0.5) {
                let up = v[1] - ground(&s, v[0], v[2]);
                assert!(up < 12.0, "{name} stands {up:.1} m up: a rig, not an arch");
                if up > RIDE_H_M {
                    continue;
                }
                let d = st.iter().map(|q| (q.x - v[0]).hypot(q.z - v[2])).fold(f32::INFINITY, f32::min);
                assert!(d > half, "{name} has a piece {up:.1} m up, {d:.1} m from the centreline");
            }
        }
    }

    #[test]
    fn a_prop_that_reaches_past_its_box_is_not_replayed() {
        let (p, s) = demo();
        let board = |x: f32| edfwrite::moved(&edfwrite::cuboid(1.0, 1.0, 0.1), [x, 0.0, 0.0]);
        let mut run = board(-15.0);
        run.append(&board(15.0));
        let prop = |id: &str, sheet: &str, mesh: Mesh| {
            let reach = mesh.positions.chunks_exact(3).map(|v| v[0].hypot(v[2])).fold(0.0f32, f32::max);
            crate::trackprops::Prop {
                id: id.into(),
                sheet: sheet.into(),
                class: crate::trackobjects::Class::Structure,
                mesh,
                height: 1.0,
                span: 1.0,
                reach,
                axis_ref: 0.0,
            }
        };
        let lib = crate::trackprops::PropLibrary {
            donor: "t".into(),
            donor_lap_m: 1000.0,
            props: vec![prop("one", "one_c", board(0.0)), prop("run", "run_c", run)],
            instances: (0..20)
                .flat_map(|i| {
                    [0usize, 1].map(|k| crate::trackprops::Instance {
                        prop: k,
                        along: i as f32 / 20.0,
                        offset: 30.0,
                        yaw: 0.0,
                        lift: 0.0,
                        near: true,
                    })
                })
                .collect(),
            runs: vec![],
            sheets: ["one_c", "run_c"].iter().map(|n| (n.to_string(), 2, 2, vec![200u8; 16])).collect(),
        };
        let names: Vec<String> = lifted(&lib, &p, &s).into_iter().map(|k| k.0).collect();
        assert!(names.contains(&"one".to_string()), "a real prop was dropped: {names:?}");
        assert!(!names.contains(&"run".to_string()), "a donor's run was replayed: {names:?}");
    }

    /// The edge line stands just past the track edge, the same distance out the whole way.
    #[test]
    fn the_edge_line_stands_just_past_the_track_edge() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let (name, bytes) = sc
            .files
            .iter()
            .find(|(f, _)| f == "barrier.edf" || f == "banners.edf")
            .expect("an edge line");
        let st = p.stations(1.0);
        let mut d: Vec<f32> = crate::edf::parse_world(bytes)
            .iter()
            .flat_map(|n| n.positions.chunks_exact(3).map(|v| (v[0], v[2])).collect::<Vec<_>>())
            .map(|(x, z)| st.iter().map(|q| (q.x - x).hypot(q.z - z)).fold(f32::INFINITY, f32::min))
            .collect();
        d.sort_by(|a, b| a.total_cmp(b));
        let (half, want) = (p.width * 0.5, p.width * 0.5 + EDGE_LINE_OUT_M);
        let med = d[d.len() / 2];
        assert!((med - want).abs() < 0.75, "{name} stands {med:.1} m out, not {want:.1}");
        assert!(d[0] > half + 1.0, "{name} comes {:.1} m from the centreline, at the track edge", d[0]);
        assert!(d[d.len() * 95 / 100] < want + 2.5, "{name} strays {:.1} m out", d[d.len() * 95 / 100]);
    }

    /// The finish line has a gantry over it, not only the gate row.
    #[test]
    fn a_gantry_stands_over_the_finish_line() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        assert_eq!(
            sc.tally.iter().find(|(k, _)| *k == "finish gantry").map(|(_, v)| *v),
            Some(1),
            "no gantry over the finish line: {:?}",
            sc.tally
        );
        let line = crate::tracksynth::finish_at(&p);
        let st = p
            .stations(0.5)
            .into_iter()
            .min_by(|a, b| (a.s - line).abs().total_cmp(&(b.s - line).abs()))
            .expect("a station at the line");
        let (fx, fz) = crate::trackprog::heading_vector(st.heading);
        // A beam, over the line and over a rider's head. Measured along the lap rather than
        // as a distance: the beam spans the track, so its nearest vertex is out at the post.
        let clear = sc
            .files
            .iter()
            .filter(|(f, _)| f == "gate.edf")
            .flat_map(|(_, b)| crate::edf::parse_world(b))
            .flat_map(|n| n.positions.chunks_exact(3).map(|v| [v[0], v[1], v[2]]).collect::<Vec<_>>())
            .filter(|v| ((v[0] - st.x) * fx + (v[2] - st.z) * fz).abs() < 3.0)
            .map(|v| v[1] - ground(&s, v[0], v[2]))
            .fold(f32::MIN, f32::max);
        assert!(
            clear > 4.5,
            "the highest thing over the finish line is {clear:.1} m up"
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
        // Trees and bales come from the prop library now, which the demo has none of.
        for want in ["stakes.edf", "gate.edf"] {
            assert!(drawn.contains(&want.to_string()), "{want} not drawn: {drawn:?}");
        }
        // The gantry stops a bike.
        for want in ["gate.edf"] {
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
        // What `review` makes of it. `repair` cannot uncross a lap — that is a re-route, and
        // a re-route is the layout — so the complaint has to be read rather than waited for.
        // A hand-written program that skips this builds happily and comes out crossing itself.
        let notes = crate::trackllm::validate(&p);
        if notes.is_empty() {
            println!("  review: nothing to say");
        }
        for note in &notes {
            println!("  review: {note}");
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

        // Sheets, reduced so the dump stays small. Not to 64: at that size a wordmark averages
        // into a grey wash, which is how a lap of backwards banners went unnoticed.
        const DIM: u32 = 256;
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

/// Replay a lifted prop library onto this lap.
///
/// Each instance was recorded against its donor's centreline as a fraction round the lap, a
/// signed lateral offset and a yaw relative to the heading there — see [`crate::trackprops`].
/// Replaying is the same three numbers read the other way: find our station at that fraction,
/// step out by the offset, and turn the prop by the yaw plus our heading.
///
/// The transform is **baked into the geometry** rather than written as a `scene` block's
/// `rot`. TerrainEd bakes every block into the `.map` regardless, so the compiled track is
/// identical either way, and baking uses only the path `builds_a_track_with_objects` already
/// proves. Nothing here has ever written a non-zero `rot` and its sense is unverified; that is
/// an optimisation for the export folder's size, not for the track's.
///
/// Props are merged by sheet, because a model carries one sheet and one only.
/// A lifted object this long and this low is a run of boards or fence, not a prop: the lifter
/// groups a run by proximity, so it comes out bent to the donor's corner, and replayed it stood
/// near our track in that shape instead of along it. Our edge is laid by rule (`lift_edge`).
const LIFT_RUN_SPAN_M: f32 = 8.0;
const LIFT_RUN_HEIGHT_M: f32 = 3.5;
/// A prop's mesh is centred on its box, so it reaches at most `span / √2`; this is float slack.
const LIFT_REACH_SLACK_M: f32 = 0.5;

/// How far an arch sized for a narrower track may be scaled up to span ours.
const ARCH_MAX_SCALE: f32 = 1.8;

/// The parts of a piece taller than `min_h`, each part a connected run of triangles.
fn tall_parts(m: &Mesh, min_h: f32) -> Mesh {
    let part = part_of(m);
    let top = part_tops(m, &part);
    let mut out = Mesh::default();
    let mut remap: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
    for t in m.indices.chunks_exact(3) {
        if top[part[t[0] as usize]] < min_h {
            continue;
        }
        for &i in t {
            let i = i as usize;
            let slot = match remap.get(&i) {
                Some(&v) => v,
                None => {
                    let v = out.vertex_count() as u32;
                    out.positions.extend_from_slice(&m.positions[i * 3..i * 3 + 3]);
                    out.uvs.extend_from_slice(&m.uvs[i * 2..i * 2 + 2]);
                    out.normals.extend_from_slice(&m.normals[i * 3..i * 3 + 3]);
                    remap.insert(i, v);
                    v
                }
            };
            out.indices.push(slot);
        }
    }
    out
}
/// How tall a part of an arch piece must stand to be the arch rather than a run beside it.
const ARCH_PART_MIN_H_M: f32 = 3.0;
/// Which connected part each vertex belongs to, as the index of the part's root vertex.
fn part_of(m: &Mesh) -> Vec<usize> {
    fn root(up: &mut [usize], mut i: usize) -> usize {
        while up[i] != i {
            up[i] = up[up[i]];
            i = up[i];
        }
        i
    }
    let n = m.vertex_count();
    let mut up: Vec<usize> = (0..n).collect();
    for t in m.indices.chunks_exact(3) {
        let a = root(&mut up, t[0] as usize);
        for &j in &t[1..] {
            let b = root(&mut up, j as usize);
            if a != b {
                up[b] = a;
            }
        }
    }
    // Welded by position too: an exporter splits a mesh at every UV seam, and a box's faces
    // share no vertex, so index links alone cut a leg off from its own top.
    let mut seen: std::collections::HashMap<(i32, i32, i32), usize> = std::collections::HashMap::new();
    for i in 0..n {
        let q = |k: usize| (m.positions[i * 3 + k] / 0.01).round() as i32;
        match seen.entry((q(0), q(1), q(2))) {
            std::collections::hash_map::Entry::Occupied(e) => {
                let (a, b) = (root(&mut up, *e.get()), root(&mut up, i));
                if a != b {
                    up[b] = a;
                }
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(i);
            }
        }
    }
    (0..n).map(|i| root(&mut up, i)).collect()
}

/// The highest point of each part, indexed by root.
fn part_tops(m: &Mesh, part: &[usize]) -> Vec<f32> {
    let mut top = vec![f32::MIN; part.len()];
    for (i, &r) in part.iter().enumerate() {
        top[r] = top[r].max(m.positions[i * 3 + 1]);
    }
    top
}

/// How near its lowest point a vertex stands to be a foot, how far apart two feet are to be two
/// legs, how wide a leg column is past its feet, and the narrowest gap that is an arch.
const ARCH_FOOT_M: f32 = 0.4;
const ARCH_LEG_GAP_M: f32 = 2.0;
const ARCH_LEG_W_M: f32 = 0.75;
const ARCH_MIN_GAP_M: f32 = 4.0;
/// The arch's frame is the parts reaching this share of its height: legs and header. Boards and
/// flags lifted with it stand lower.
const ARCH_FRAME_SHARE: f32 = 0.7;
/// How far apart two feet may stand from the header's middle and still be a pair of legs.
const ARCH_LEG_SYM_M: f32 = 1.5;

/// An arch stood on its own feet. Legs are found where its frame touches the ground, not at its
/// box's ends: a header wider than its legs put them metres inside those. A lone leg is mirrored
/// about the header's middle. Only what hangs low between the legs goes — a board or flag whole,
/// so no top of one is left in the air. Centred between the legs, with the clear gap between
/// their inner faces.
fn arch_on_legs(m: &Mesh) -> Option<(Mesh, f32, f32)> {
    let (lo, hi) = m.bounds();
    let ax = if hi[0] - lo[0] >= hi[2] - lo[2] { 0 } else { 2 };
    let other = 2 - ax;
    let n = m.vertex_count();
    let part = part_of(m);
    let top = part_tops(m, &part);
    let frame = |i: usize| top[part[i]] >= lo[1] + (hi[1] - lo[1]) * ARCH_FRAME_SHARE;
    let mut feet: Vec<(f32, usize)> = (0..n)
        .filter(|&i| frame(i) && m.positions[i * 3 + 1] < lo[1] + ARCH_FOOT_M)
        .map(|i| (m.positions[i * 3 + ax], i))
        .collect();
    if feet.is_empty() {
        return None;
    }
    feet.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut clusters: Vec<(f32, f32)> = vec![(feet[0].0, feet[0].0)];
    for &(t, _) in &feet[1..] {
        let c = clusters.last_mut().unwrap();
        if t - c.1 > ARCH_LEG_GAP_M {
            clusters.push((t, t));
        } else {
            c.1 = t;
        }
    }
    // Legs stand either side of the header's middle, as far out as each other. Otherwise the
    // outer foot is the leg, the far one was built apart, and anything nearer is a pole.
    let mid = (lo[ax] + hi[ax]) * 0.5;
    let off = |c: (f32, f32)| (c.0 + c.1) * 0.5 - mid;
    let (l, r) = (clusters[0], *clusters.last().unwrap());
    let paired = off(l) < 0.0 && off(r) > 0.0 && (off(r) + off(l)).abs() <= ARCH_LEG_SYM_M;
    let legs: Vec<(f32, f32)> = if paired { vec![l, r] } else if -off(l) >= off(r) { vec![l] } else { vec![r] };
    let mut is_leg = vec![false; n];
    for &(t, i) in &feet {
        if legs.iter().any(|c| t >= c.0 && t <= c.1) {
            is_leg[part[i]] = true;
        }
    }
    let mut mirror = Mesh::default();
    let (a, b) = if paired {
        (l, r)
    } else {
        // Its own column, mirrored about the header's middle, stands in for the far leg.
        let one = legs[0];
        let (c0, c1) = (one.0 - ARCH_LEG_W_M, one.1 + ARCH_LEG_W_M);
        let within = |i: u32| is_leg[part[i as usize]] && (c0..=c1).contains(&m.positions[i as usize * 3 + ax]);
        for tri in m.indices.chunks_exact(3).filter(|t| t.iter().all(|&i| within(i))) {
            let base = mirror.vertex_count() as u32;
            // Reversed, because a mirror turns the winding inside out.
            for &i in tri.iter().rev() {
                let i = i as usize;
                let mut p = [m.positions[i * 3], m.positions[i * 3 + 1], m.positions[i * 3 + 2]];
                p[ax] = 2.0 * mid - p[ax];
                let mut nv = [m.normals[i * 3], m.normals[i * 3 + 1], m.normals[i * 3 + 2]];
                nv[ax] = -nv[ax];
                mirror.positions.extend_from_slice(&p);
                mirror.normals.extend_from_slice(&nv);
                mirror.uvs.extend_from_slice(&m.uvs[i * 2..i * 2 + 2]);
            }
            mirror.indices.extend([base, base + 1, base + 2]);
        }
        let far = (2.0 * mid - one.1, 2.0 * mid - one.0);
        if far.0 > one.1 { (one, far) } else { (far, one) }
    };
    let gap = b.0 - a.1;
    if gap < ARCH_MIN_GAP_M {
        return None;
    }
    let low = lo[1] + RIDE_H_M;
    let (inner0, inner1) = (a.1 + ARCH_LEG_W_M, b.0 - ARCH_LEG_W_M);
    // A triangle hangs between the legs if any of it is low and it reaches in between them,
    // even from outboard on both sides: a banner strung leg to leg has no vertex in between.
    let hung = |t: &[u32]| {
        let (mut t0, mut t1, mut y) = (f32::MAX, f32::MIN, f32::MAX);
        for &i in t {
            let v = &m.positions[i as usize * 3..i as usize * 3 + 3];
            (t0, t1, y) = (t0.min(v[ax]), t1.max(v[ax]), y.min(v[1]));
        }
        y <= low && t1 > inner0 && t0 < inner1
    };
    // Anything but a leg goes whole if any of it hangs: a board, a flag, a pole.
    let mut hangs = vec![false; n];
    for t in m.indices.chunks_exact(3) {
        let r = part[t[0] as usize];
        if !is_leg[r] && hung(t) {
            hangs[r] = true;
        }
    }
    let mut out = Mesh::default();
    for t in m.indices.chunks_exact(3) {
        let r0 = part[t[0] as usize];
        let keep = if is_leg[r0] { !hung(t) } else { !hangs[r0] };
        if !keep {
            continue;
        }
        let base = out.vertex_count() as u32;
        for &i in t {
            let i = i as usize;
            out.positions.extend_from_slice(&m.positions[i * 3..i * 3 + 3]);
            out.uvs.extend_from_slice(&m.uvs[i * 2..i * 2 + 2]);
            out.normals.extend_from_slice(&m.normals[i * 3..i * 3 + 3]);
        }
        out.indices.extend([base, base + 1, base + 2]);
    }
    out.append(&mirror);
    // Along the track, centred on the legs, so they stand at the station the arch is placed at.
    let leg_feet: Vec<f32> = feet
        .iter()
        .filter(|(t, _)| legs.iter().any(|c| *t >= c.0 && *t <= c.1))
        .map(|&(_, i)| m.positions[i * 3 + other])
        .collect();
    let depth = leg_feet.iter().sum::<f32>() / leg_feet.len().max(1) as f32;
    let shift = (a.0 + b.1) * 0.5;
    for v in out.positions.chunks_exact_mut(3) {
        v[ax] -= shift;
        v[other] -= depth;
    }
    Some((out, gap, (b.1 - a.0) * 0.5))
}

/// A thin tall piece near the donor's track: a cable or bare pole, which floats as a line.
fn thin_near(p: &crate::trackprops::Prop, offset: f32) -> bool {
    use crate::trackobjects::Class;
    !matches!(p.class, Class::Tree | Class::Crowd)
        && p.span < 0.6
        && p.height > 3.0
        && offset.abs() < THIN_NEAR_M
}
const THIN_NEAR_M: f32 = 25.0;

/// An arch or gantry that stood across the donor's track: tall, wide, and centred on its line.
fn spans_track(p: &crate::trackprops::Prop, offset: f32) -> bool {
    p.class == crate::trackobjects::Class::Structure
        && p.height > 4.0
        && p.span >= 2.0 * DONOR_HALF_M
        && offset.abs() < p.span * 0.3
}
/// Real arches stand 5-10 m; taller is a tower or rigging, which a mirrored leg doubles.
const ARCH_MAX_H_M: f32 = 10.5;
/// Half the donor's riding width, and how far an arch's legs stand clear of our edge.
const DONOR_HALF_M: f32 = 7.0;
/// Past the riding margin, so a leg's inner face is off the riding surface too.
const ARCH_LEG_CLEAR_M: f32 = 3.5;
/// Where arches go: straight ground, off jumps, past the start and apart from each other.
const ARCH_STRAIGHT_R_M: f32 = 40.0;
/// How far along the lap either side of an arch the ground must be that straight.
const ARCH_STRAIGHT_ALONG_M: f32 = 15.0;
const ARCH_OFF_FEATURE_M: f32 = 15.0;
const ARCH_FROM_START_M: f32 = 110.0;
const ARCH_APART_M: f32 = 250.0;
/// How many times round the lap one arch may stand.
const ARCH_COPIES: usize = 3;

/// Whether a lifted prop is one object. A mesh reaching past its own box was lifted with its
/// neighbours' triangles (a library baked before `trackprops::lift` was fixed): a donor's run.
fn whole(p: &crate::trackprops::Prop) -> bool {
    p.reach <= p.span * std::f32::consts::FRAC_1_SQRT_2 + LIFT_REACH_SLACK_M
}

/// Riding height, and how far past the half-width the riding surface may wander.
// Above a rider on the bike; arches never stand over a jump, so nobody is in the air under one.
const RIDE_H_M: f32 = 2.5;
const RIDE_MARGIN_M: f32 = 1.0;

/// Whether placed geometry stands at riding height over any leg of the lap or the start pad.
/// Checked on the geometry itself, because an anchor check misses a piece's far end.
fn on_riding_surface(syn: &Synth, half: f32, m: &Mesh) -> bool {
    if m.vertex_count() == 0 {
        return false;
    }
    let cell = |x: f32, z: f32| {
        let gx = (x / syn.mps).round().clamp(0.0, (syn.gw - 1) as f32) as usize;
        let gz = (z / syn.mps).round().clamp(0.0, (syn.gh - 1) as f32) as usize;
        gz * syn.gw + gx
    };
    // Far from every riding surface by more than its own size: nothing to sample.
    let (lo, hi) = m.bounds();
    let (cx, cz) = ((lo[0] + hi[0]) * 0.5, (lo[2] + hi[2]) * 0.5);
    let r = (hi[0] - lo[0]).hypot(hi[2] - lo[2]) * 0.5;
    if syn.dist[cell(cx, cz)] > half + RIDE_MARGIN_M + r
        && syn.outside_the_start(cx, cz).is_none_or(|e| e > RIDE_MARGIN_M + r)
    {
        return false;
    }
    edge_points(m, 1.0).any(|v| {
        v[1] - ground(syn, v[0], v[2]) <= RIDE_H_M
            && (syn.dist[cell(v[0], v[2])] < half + RIDE_MARGIN_M
                || syn.outside_the_start(v[0], v[2]).is_some_and(|e| e < RIDE_MARGIN_M))
    })
}

/// Points along every triangle edge at most `step` apart, and each centroid. A long flat board
/// has vertices only at its ends, and its middle is what lies on the track.
fn edge_points(m: &Mesh, step: f32) -> impl Iterator<Item = [f32; 3]> + '_ {
    m.indices.chunks_exact(3).flat_map(move |t| {
        let p = |i: u32| {
            let i = i as usize * 3;
            [m.positions[i], m.positions[i + 1], m.positions[i + 2]]
        };
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        let mut out = vec![[(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0]];
        for (u, v) in [(a, b), (b, c), (c, a)] {
            let len = ((v[0] - u[0]).powi(2) + (v[1] - u[1]).powi(2) + (v[2] - u[2]).powi(2)).sqrt();
            let n = (len / step).ceil().max(1.0) as usize;
            for k in 0..n {
                let f = k as f32 / n as f32;
                out.push([u[0] + (v[0] - u[0]) * f, u[1] + (v[1] - u[1]) * f, u[2] + (v[2] - u[2]) * f]);
            }
        }
        out
    })
}

pub fn lifted(
    lib: &crate::trackprops::PropLibrary,
    prog: &TrackProgram,
    syn: &Synth,
) -> Vec<(String, Mesh, Texture, bool)> {
    lifted_counted(lib, prog, syn).0
}

/// What [`lifted_counted`] stood by rule rather than replayed.
#[derive(Clone, Copy, Debug, Default)]
pub struct Lifted {
    pub turn_markers: usize,
    pub parked: usize,
    pub arches: usize,
    pub bales: usize,
    pub pit_vehicles: usize,
}

/// [`lifted`], with how many arches, bales and pit vehicles it stood.
pub fn lifted_counted(
    lib: &crate::trackprops::PropLibrary,
    prog: &TrackProgram,
    syn: &Synth,
) -> (Vec<(String, Mesh, Texture, bool)>, Lifted) {
    let lap = prog.lap_length();
    let half = prog.width * 0.5;
    let stations = prog.stations(0.5);
    let coarse = prog.stations(2.0);
    let at = |s: f32| -> crate::trackprog::Station {
        let i = ((s / 0.5) as usize).min(stations.len().saturating_sub(1));
        stations[i]
    };

    let sheet_rgba: std::collections::HashMap<&str, &(String, u32, u32, Vec<u8>)> =
        lib.sheets.iter().map(|s| (s.0.as_str(), s)).collect();
    let mut by_sheet: std::collections::HashMap<String, Mesh> = std::collections::HashMap::new();
    let wrap = |d: f32| {
        let d = d.rem_euclid(lap);
        d.min(lap - d)
    };
    // Where an arch can stand across our track, near `s0`: on a straight, off every jump, past
    // the start, apart from the arches already up, and clear of any other leg of the lap.
    // `fits` has the last word: the arch as it would stand there, nothing on a riding surface.
    let over_track = |s0: f32, reach: f32, placed: &[f32], fits: &dyn Fn(f32) -> bool| -> Option<f32> {
        for k in 0..=(lap / 6.0) as usize {
            for dir in [1.0f32, -1.0] {
                let s = (s0 + dir * k as f32 * 3.0).rem_euclid(lap);
                if s < ARCH_FROM_START_M || s > lap - 20.0 {
                    continue;
                }
                // Straight along the lap either side of it. Its width across is `fits`' to judge.
                let n = (ARCH_STRAIGHT_ALONG_M / 2.5).ceil() as i32;
                let straight = (-n..=n).all(|j| {
                    at((s + j as f32 * 2.5).rem_euclid(lap)).curvature.abs() < 1.0 / ARCH_STRAIGHT_R_M
                });
                // Over a jump as well: a sponsor arch across a face or a landing is where one looks
                // right. Only its legs are kept off the riding surface, by `fits`.
                let crowded = placed.iter().any(|&q| wrap(q - s) < ARCH_APART_M);
                if !straight || crowded {
                    continue;
                }
                let st = at(s);
                let other_leg = coarse
                    .iter()
                    .any(|q| wrap(q.s - s) > reach * 3.0 && (q.x - st.x).hypot(q.z - st.z) < reach + half + 3.0);
                let on_the_start = syn
                    .outside_the_start(st.x, st.z)
                    .is_some_and(|e| e < reach);
                if !other_leg && !on_the_start && inside(prog, st.x, st.z, reach) && fits(s) {
                    return Some(s);
                }
            }
        }
        None
    };
    let mut arches_at: Vec<f32> = Vec::new();

    let pits = Pits::of(prog);
    // The rule lays the edge stakes and posts (`build`); a lifted copy of one stood beside it.
    let edge: Vec<&crate::trackprops::Prop> =
        lib.props.iter().filter(|p| p.id == "edge_stake" || p.id == "edge_post").collect();
    let duplicates_edge = |p: &crate::trackprops::Prop| {
        edge.iter().any(|e| e.sheet == p.sheet && (e.height - p.height).abs() < 0.1 && p.span < 0.25)
    };
    let markers: std::collections::HashSet<usize> = (0..lib.props.len()).filter(|&k| is_marker(lib, &lib.props[k])).collect();
    for inst in &lib.instances {
        let prop = &lib.props[inst.prop];
        if prop.span > LIFT_RUN_SPAN_M && prop.height < LIFT_RUN_HEIGHT_M {
            continue;
        }
        if !whole(prop) || thin_near(prop, inst.offset) {
            continue;
        }
        // Bales are laid by rule (`place_bales`): at the donor's offsets they stood in the trees.
        // Bales, cars and turn markers are laid by rule; at the donor's offsets they stood anywhere.
        if markers.contains(&inst.prop)
            || prop.class == crate::trackobjects::Class::Bale
            || prop.class == crate::trackobjects::Class::Vehicle
            || duplicates_edge(prop)
        {
            continue;
        }
        // An arch or gantry that spanned the donor's track spans ours: across it on a straight,
        // centred, on its lower leg. Pushed out to the shoulder like the rest, it stood in a field.
        if spans_track(prop, inst.offset) {
            // Too tall for an arch: a tower or rig that stood over the donor's track. It fits
            // nowhere on ours, and pushed out it stood in a field.
            if prop.height > ARCH_MAX_H_M {
                continue;
            }
            // Its legs and header only: the lifter clusters an arch with the board and flag runs
            // beside it, and laid across our track those runs lay on the riding surface.
            let arch = tall_parts(&prop.mesh, ARCH_PART_MIN_H_M);
            if arch.vertex_count() == 0 {
                continue;
            }
            // On its own two legs, centred between them, nothing low hung between them.
            let Some((mut mesh, gap, legs_out)) = arch_on_legs(&arch) else {
                continue;
            };
            // Sized by its legs, not its header: both stand ARCH_LEG_CLEAR_M past our edges.
            let k = (2.0 * (half + ARCH_LEG_CLEAR_M) / gap).max(1.0);
            if k > ARCH_MAX_SCALE {
                continue;
            }
            // Its legs' reach, not its header's: the header is overhead, and `fits` has the rest.
            let arch_reach = (legs_out + ARCH_LEG_W_M) * k;
            for v in mesh.positions.iter_mut() {
                *v *= k;
            }
            let place = |s: f32| -> Mesh {
                let st = at(s);
                let (rx, rz) = crate::trackprog::right_vector(st.heading);
                // Centred on the line: the mesh is centred between its legs.
                let off = 0.0;
                // Across the track, whatever the donor's yaw says: that was measured against the
                // piece with its runs, not the arch left once they are gone.
                let across = |deg: f32| {
                    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
                    for v in edfwrite::turned(&mesh, deg).positions.chunks_exact(3) {
                        let d = v[0] * rx + v[2] * rz;
                        lo = lo.min(d);
                        hi = hi.max(d);
                    }
                    hi - lo
                };
                let base = st.heading.to_degrees();
                let deg = if across(base) >= across(base + 90.0) { base } else { base + 90.0 };
                draped(&edfwrite::turned(&mesh, deg), st.x + rx * off, st.z + rz * off, inst.lift * k, syn)
            };
            // Ridden under, so nothing low over a riding surface: the lifter clusters an arch
            // with the board and flag runs round it, and those would lie on the track.
            let fits = |s: f32| {
                let m = place(s);
                !on_riding_surface(syn, half, &m) && !pits.on_lane(lap, syn, &m)
            };
            // The same arch again round the lap, far apart: one on a whole lap went unseen.
            for copy in 0..ARCH_COPIES {
                let s0 = (inst.along * lap + copy as f32 * lap / ARCH_COPIES as f32).rem_euclid(lap);
                if let Some(s) = over_track(s0, arch_reach, &arches_at, &fits) {
                    arches_at.push(s);
                    by_sheet.entry(prop.sheet.clone()).or_default().append(&place(s));
                }
            }
            continue;
        }
        let st = at((inst.along * lap).clamp(0.0, lap));
        let (rx, rz) = crate::trackprog::right_vector(st.heading);

        // Push anything that would land on the riding line out to the shoulder. A donor's
        // corridor is not ours: its 7 m is inside our track where ours is wider.
        //
        // By the prop's own footprint, not by its anchor. A clump anchored exactly on the
        // margin still reaches half its span back over the line, which is how a tree ended up
        // 5.4 m from the centreline of a 6 m corridor.
        let reach = prop.reach;
        let margin = half + 2.0 + reach;
        let want = inst.offset;
        let off = if want.abs() < margin {
            margin * if want == 0.0 { 1.0 } else { want.signum() }
        } else {
            want
        };
        let (x, z) = (st.x + rx * off, st.z + rz * off);

        let clear_of_the_start = syn
            .outside_the_start(x, z)
            .map(|e| e > OFF_THE_START_M + reach)
            .unwrap_or(true);
        if !inside(prog, x, z, 2.0)
            || clearance(&coarse, x, z) < half + 1.5 + reach
            || !clear_of_the_start
        {
            continue;
        }

        // Yaw is relative to the donor's heading, so it adds to ours. Degrees, because
        // `edfwrite::turned` takes degrees and shares this convention — see `principal_axis`.
        let deg = (inst.yaw + st.heading).to_degrees();
        let placed = draped(&edfwrite::turned(&prop.mesh, deg), x, z, inst.lift, syn);
        if on_riding_surface(syn, half, &placed) || pits.on_lane(lap, syn, &placed) {
            continue;
        }
        // The pits park their own, in rows (`place_pits`).
        if pit_parked(prop) && pits.near(lap, syn, x, z) {
            continue;
        }
        by_sheet.entry(prop.sheet.clone()).or_default().append(&placed);
    }

    let bales = place_bales(lib, prog, syn, &mut by_sheet);
    let parked = place_parking(lib, prog, syn, &mut by_sheet);
    let turn_markers = place_markers(lib, prog, syn, &mut by_sheet);
    // Parked in the paddock now (`trackvenue`), not in a row behind the stalls.
    let pit_vehicles = 0;
    let mut out = Vec::new();
    for (sheet, mesh) in by_sheet {
        if mesh.vertex_count() < 8 {
            continue;
        }
        let Some((name, w, h, rgba)) = sheet_rgba.get(sheet.as_str()) else {
            // A prop whose sheet did not inflate would render untextured. Drop it rather
            // than ship a white slab.
            continue;
        };
        let tex = Texture {
            name: name.clone(),
            width: *w,
            height: *h,
            rgba: rgba.clone(),
        };
        // Solid is by class, and by class only: you ride through foliage and into a building.
        let solid = lib
            .props
            .iter()
            .any(|p| p.sheet == sheet && matches!(p.class, crate::trackobjects::Class::Structure | crate::trackobjects::Class::Vehicle | crate::trackobjects::Class::Bale));
        out.push((short_sheet(&sheet), mesh, tex, solid));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    (out, Lifted { arches: arches_at.len(), bales, pit_vehicles, parked, turn_markers })
}

/// The most vertices one model may carry: the game's draw groups index in 16 bits, TerrainEd makes
/// one group of a model, and it moves vertices about as it bakes, so this keeps well clear.
const MODEL_MAX_VERTS: usize = 48_000;

/// A mesh cut into parts of at most `max` vertices, triangle by triangle in order, so what was
/// placed together stays together.
fn split_for_draw(m: &Mesh, max: usize) -> Vec<Mesh> {
    if m.vertex_count() <= max {
        return vec![m.clone()];
    }
    let mut out = vec![Mesh::default()];
    let mut remap: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for t in m.indices.chunks_exact(3) {
        let fresh = t.iter().filter(|i| !remap.contains_key(i)).count();
        if out.last().unwrap().vertex_count() + fresh > max {
            out.push(Mesh::default());
            remap.clear();
        }
        let part = out.last_mut().unwrap();
        for &i in t {
            let j = *remap.entry(i).or_insert_with(|| {
                let k = i as usize;
                part.positions.extend_from_slice(&m.positions[k * 3..k * 3 + 3]);
                part.normals.extend_from_slice(&m.normals[k * 3..k * 3 + 3]);
                part.uvs.extend_from_slice(&m.uvs[k * 2..k * 2 + 2]);
                (part.positions.len() / 3 - 1) as u32
            });
            part.indices.push(j);
        }
    }
    out
}

/// The grid cell a world point falls in.
fn grid_cell(syn: &Synth, x: f32, z: f32) -> usize {
    let gx = (x / syn.mps).round().clamp(0.0, (syn.gw - 1) as f32) as usize;
    let gz = (z / syn.mps).round().clamp(0.0, (syn.gh - 1) as f32) as usize;
    gz * syn.gw + gx
}

/// The pit lane: stalls `PIT_STALL_GAP_M` apart, `PIT_LANE_OUT_M` past the edge, on a strip
/// `PIT_HALF_M` either side of them. As `tracksynth::rdf` writes its `start_stall`s, whose
/// `pit_lane` is private there; `a_stand_at_every_stall_the_rdf_spawns` pins the two together.
const PIT_STALL_GAP_M: f32 = 5.0;
const PIT_LANE_OUT_M: f32 = 6.0;
const PIT_HALF_M: f32 = 4.0;
/// The parking row: how far past the lane's outer edge, the gap between one vehicle and the
/// next, and how far past the first and last stall it runs.
const PIT_ROW_GAP_M: f32 = 2.5;
const PIT_PARK_GAP_M: f32 = 3.0;
const PIT_PARK_PAST_M: f32 = 20.0;
/// How far every vehicle keeps from any leg of the lap, past the half-width.
const PIT_TRACK_CLEAR_M: f32 = 3.0;
/// The row parks only where the lap beside it is at least this straight.
const PIT_ROW_STRAIGHT_R_M: f32 = 80.0;
/// The mat under a spawn spot, the stand beside it, and how far out from the spot it stands.
const PIT_MAT_M: (f32, f32) = (2.2, 1.1);
const PIT_STAND_M: (f32, f32, f32) = (0.45, 0.4, 0.35);
const PIT_STAND_OUT_M: f32 = 1.3;

struct Pits {
    /// Each stall's metres round the lap and signed lateral offset: the `.rdf`'s spawn spots.
    stalls: Vec<(f32, f32)>,
    /// +1 when the pits are on the rider's right.
    side: f32,
    /// Metres from the centreline to the lane's middle.
    lane: f32,
    /// The pits' own stretch of lap, which everything about them is measured against.
    path: Vec<crate::trackprog::Station>,
}

impl Pits {
    fn of(prog: &TrackProgram) -> Pits {
        let side = prog.start_line().map(|l| -l.side).unwrap_or(-1.0);
        let run = prog.opening_straight().max(prog.lap_length() * 0.1);
        let from = 10.0f32.min(run * 0.1);
        let n = (((run - from) / PIT_STALL_GAP_M).floor() as usize).clamp(4, 16);
        let lane = prog.width * 0.5 + PIT_LANE_OUT_M;
        let stalls: Vec<(f32, f32)> = (0..n).map(|i| (from + i as f32 * PIT_STALL_GAP_M, side * lane)).collect();
        let (a, b, lap) = (stalls[0].0, stalls[n - 1].0, prog.lap_length());
        let path = prog
            .stations(1.0)
            .into_iter()
            .filter(|q| {
                let d = if (a..=b).contains(&q.s) { 0.0 } else { (a - q.s).rem_euclid(lap).min((q.s - b).rem_euclid(lap)) };
                d <= PIT_PARK_PAST_M + 30.0
            })
            .collect();
        Pits { stalls, side, lane, path }
    }

    /// Metres along the lap outside the stall range (0 beside it), and metres out toward the pits.
    fn frame(&self, lap: f32, _syn: &Synth, x: f32, z: f32) -> (f32, f32) {
        // Against the pits' own stretch: where another leg runs behind them, the nearest
        // centreline is that leg's, and the pits' frame was lost.
        let st = self.path.iter().min_by(|a, b| (a.x - x).hypot(a.z - z).total_cmp(&(b.x - x).hypot(b.z - z))).unwrap();
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        let out = ((x - st.x) * rx + (z - st.z) * rz) * self.side;
        let (a, b, s) = (self.stalls[0].0, self.stalls[self.stalls.len() - 1].0, st.s);
        let past = if (a..=b).contains(&s) { 0.0 } else { (a - s).rem_euclid(lap).min((s - b).rem_euclid(lap)) };
        (past, out)
    }

    fn lane_at(&self, lap: f32, syn: &Synth, x: f32, z: f32) -> bool {
        let (past, out) = self.frame(lap, syn, x, z);
        past <= PIT_STALL_GAP_M && (out - self.lane).abs() <= PIT_HALF_M
    }

    /// Whether any of a placed mesh stands on the pit lane's strip.
    fn on_lane(&self, lap: f32, syn: &Synth, m: &Mesh) -> bool {
        if m.vertex_count() == 0 {
            return false;
        }
        let (lo, hi) = m.bounds();
        let (cx, cz) = ((lo[0] + hi[0]) * 0.5, (lo[2] + hi[2]) * 0.5);
        let r = (hi[0] - lo[0]).hypot(hi[2] - lo[2]) * 0.5;
        let (past, out) = self.frame(lap, syn, cx, cz);
        if past > PIT_STALL_GAP_M + r || (out - self.lane).abs() > PIT_HALF_M + r {
            return false;
        }
        edge_points(m, 1.0).any(|v| self.lane_at(lap, syn, v[0], v[2]))
    }

    /// The parking and the ground round it, where the replay's own vehicles would double up.
    fn near(&self, lap: f32, syn: &Synth, x: f32, z: f32) -> bool {
        let (past, out) = self.frame(lap, syn, x, z);
        past <= PIT_PARK_PAST_M + 10.0 && out > 0.0 && out < self.lane + PIT_HALF_M + PIT_ROW_GAP_M + 15.0
    }
}

/// A mesh turned so its long axis runs along x, centred on its footprint. A lifted vehicle
/// keeps the angle it was parked at; set square to the lane by its box, it stood askew.
fn aligned(m: &Mesh) -> Mesh {
    // The turn that gives the narrowest footprint: a principal axis leans toward a cab or an
    // awning, where the footprint's own width does not.
    let width = |th: f32| {
        let (sn, cs) = th.sin_cos();
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for v in m.positions.chunks_exact(3) {
            let d = -v[0] * sn + v[2] * cs;
            lo = lo.min(d);
            hi = hi.max(d);
        }
        hi - lo
    };
    let deg = (0..180).map(|d| d as f32).min_by(|a, b| width(a.to_radians()).total_cmp(&width(b.to_radians()))).unwrap_or(0.0);
    // Which way `turned` spins is measured, not assumed: keep the turn that lays it longest in x.
    let x_len = |m: &Mesh| {
        let (lo, hi) = m.bounds();
        hi[0] - lo[0]
    };
    let (a, b) = (edfwrite::turned(m, deg), edfwrite::turned(m, -deg));
    let best = if x_len(&a) >= x_len(&b) { a } else { b };
    let (lo, hi) = best.bounds();
    edfwrite::moved(&best, [-(lo[0] + hi[0]) * 0.5, 0.0, -(lo[2] + hi[2]) * 0.5])
}

/// What the pits park: vehicles, and the tents a paddock puts up.
fn pit_parked(p: &crate::trackprops::Prop) -> bool {
    p.class == crate::trackobjects::Class::Vehicle || ["tent_sides", "big_tent", "easy_ups"].iter().any(|w| p.sheet.starts_with(w))
}

/// Pit parking: whole vehicles and tents in one row behind the stalls, long side along the lane
/// and facing it, evenly spaced, a truck and a tent in turn, draped. Returns how many stood.
fn place_pits(
    lib: &crate::trackprops::PropLibrary,
    prog: &TrackProgram,
    syn: &Synth,
    by_sheet: &mut std::collections::HashMap<String, Mesh>,
) -> usize {
    use crate::trackobjects::Class;
    // Square to the lane by its own long axis, not its box: a donor parks at an angle.
    let size = |p: &crate::trackprops::Prop| {
        let (lo, hi) = aligned(&p.mesh).bounds();
        (hi[0] - lo[0], hi[2] - lo[2])
    };
    // Whole ones only: a lifted trailer can be half a trailer.
    let trucks: Vec<&crate::trackprops::Prop> = lib
        .props
        .iter()
        .filter(|p| {
            let (l, w) = size(p);
            p.class == Class::Vehicle && whole(p) && (1.8..=4.2).contains(&p.height) && (4.5..=14.0).contains(&l) && (1.8..=3.2).contains(&w)
        })
        .collect();
    let tents: Vec<&crate::trackprops::Prop> = lib
        .props
        .iter()
        .filter(|p| {
            let (l, w) = size(p);
            pit_parked(p) && p.class != Class::Vehicle && whole(p) && (2.5..=3.5).contains(&p.height) && (3.0..=5.0).contains(&l) && (3.0..=5.0).contains(&w)
        })
        .collect();
    if trucks.is_empty() && tents.is_empty() {
        return 0;
    }
    let pits = Pits::of(prog);
    let (lap, half, seed) = (prog.lap_length(), prog.width * 0.5, prog.terrain.relief.seed);
    let stations = prog.stations(0.5);
    let at = |s: f32| stations[((s.rem_euclid(lap) / 0.5) as usize).min(stations.len() - 1)];
    let mut s = pits.stalls[0].0 - PIT_PARK_PAST_M;
    let end = pits.stalls[pits.stalls.len() - 1].0 + PIT_PARK_PAST_M;
    let (mut k, mut count) = (0u32, 0usize);
    while s < end {
        let pool = if (k % 2 == 1 && !tents.is_empty()) || trucks.is_empty() { &tents } else { &trucks };
        let p = pool[((rnd(seed ^ 0x9175, k) * pool.len() as f32) as usize).min(pool.len() - 1)];
        k += 1;
        let body = aligned(&p.mesh);
        let (lo, hi) = body.bounds();
        let (len, depth) = (hi[0] - lo[0], hi[2] - lo[2]);
        // Ends inside the row, not merely starts in it.
        if s + len > end {
            break;
        }
        let bend = at(s + len * 0.5).curvature.abs() > 1.0 / PIT_ROW_STRAIGHT_R_M;
        let st = at(s + len * 0.5);
        s += len + PIT_PARK_GAP_M;
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        let out = pits.lane + PIT_HALF_M + PIT_ROW_GAP_M + depth * 0.5;
        let (x, z) = (st.x + rx * out * pits.side, st.z + rz * out * pits.side);
        // A row round a bend is not a row.
        if bend {
            continue;
        }
        // Long side along the lane: long in x, it turns by the heading plus ninety.
        let m = draped(&edfwrite::turned(&body, st.heading.to_degrees() + 90.0), x, z, 0.0, syn);
        if !inside(prog, x, z, 2.0)
            || on_riding_surface(syn, half, &m)
            || pits.on_lane(lap, syn, &m)
            || syn.outside_the_start(x, z).is_some_and(|e| e < OFF_THE_START_M + depth)
            || edge_points(&m, 1.0).any(|v| syn.dist[grid_cell(syn, v[0], v[2])] < half + PIT_TRACK_CLEAR_M)
        {
            continue;
        }
        by_sheet.entry(p.sheet.clone()).or_default().append(&m);
        count += 1;
    }
    count
}

/// A mat under every spawn spot and a stand beside it, out toward the parking. Returns the mesh
/// and how many stalls it marks.
fn pit_stands(pits: &Pits, prog: &TrackProgram, syn: &Synth) -> (Mesh, usize) {
    let (lap, stations) = (prog.lap_length(), prog.stations(0.5));
    let mut m = Mesh::default();
    for &(long, lat) in &pits.stalls {
        let st = stations[((long.rem_euclid(lap) / 0.5) as usize).min(stations.len() - 1)];
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        let (x, z) = (st.x + rx * lat, st.z + rz * lat);
        // Long side along the lap, as the bike stands.
        let deg = st.heading.to_degrees() + 90.0;
        let mat = in_cell(&edfwrite::cuboid(PIT_MAT_M.0, 0.02, PIT_MAT_M.1), (0.0, 1.0), (0.0, 0.5));
        m.append(&draped(&edfwrite::turned(&mat, deg), x, z, 0.0, syn));
        let (sx, sz) = (x + rx * pits.side * PIT_STAND_OUT_M, z + rz * pits.side * PIT_STAND_OUT_M);
        let stand = in_cell(&edfwrite::cuboid(PIT_STAND_M.0, PIT_STAND_M.1, PIT_STAND_M.2), (0.0, 1.0), (0.5, 1.0));
        m.append(&draped(&edfwrite::turned(&stand, deg), sx, sz, 0.0, syn));
    }
    (m, pits.stalls.len())
}

/// The mat's and stand's sheet: a rubber mat framed in the stall's yellow, and a red stand.
fn pit_sheet() -> Texture {
    sheet("pit_stand_c", 128, |u, v| {
        if u < 0.5 {
            let a = u / 0.5;
            if a.min(1.0 - a).min(v).min(1.0 - v) < 0.08 { [232, 196, 24, 255] } else { [44, 44, 46, 255] }
        } else if v < 0.2 {
            [20, 20, 22, 255]
        } else {
            [196, 32, 28, 255]
        }
    })
}

/// Bales: how far past the half-width a row stands, how many are in one, and where rows go —
/// the inside of corners tighter than `BALE_CORNER_R_M`, and beside jump landings — and how
/// many a kilometre, which is what rated tracks carry.
const BALE_OUT_M: (f32, f32) = (1.5, 3.0);
const BALE_ROW: (usize, usize) = (2, 4);
const BALE_CORNER_R_M: f32 = 35.0;
const BALE_APART_M: f32 = 40.0;
const BALE_PER_KM: f32 = 14.0;
/// A single bale, not a stack: at most this tall, and this far across from its middle.
const BALE_MAX_H_M: f32 = 1.4;
const BALE_MAX_REACH_M: f32 = 1.6;

/// Bales where a track lines them: rows just past the edge on the inside of tight corners, where
/// a track marks its turns, and beside jump landings, draped on the ground. Returns how many stood.
/// Spectator parking: the donor's cars and vans in tidy lots off the track, nose in, rather
/// than wherever the donor had them.
/// Whether a piece is one of the donor's yellow standing turn markers (foam blocks at a real
/// track): small, standing, and yellow under its own texture.
fn is_marker(lib: &crate::trackprops::PropLibrary, p: &crate::trackprops::Prop) -> bool {
    if p.class != crate::trackobjects::Class::Structure || !(0.8..=1.6).contains(&p.height) || p.span > 1.0 {
        return false;
    }
    let Some((_, w, h, rgba)) = lib.sheets.iter().find(|s| s.0 == p.sheet) else {
        return false;
    };
    let (mut sum, mut n) = ([0.0f64; 3], 0.0f64);
    for uv in p.mesh.uvs.chunks_exact(2) {
        let x = (uv[0].rem_euclid(1.0) * (*w as f32 - 1.0)) as usize;
        let y = (uv[1].rem_euclid(1.0) * (*h as f32 - 1.0)) as usize;
        let o = (y * *w as usize + x) * 4;
        if o + 2 < rgba.len() {
            for c in 0..3 {
                sum[c] += rgba[o + c] as f64;
            }
            n += 1.0;
        }
    }
    if n == 0.0 {
        return false;
    }
    let (r, g, b) = (sum[0] / n, sum[1] / n, sum[2] / n);
    r > 150.0 && g > 110.0 && b < 110.0 && r > b + 60.0
}

/// The yellow turn markers on the inside of every tight corner, where a rider would cut across,
/// just outside the riding line. They are structures, so hitting one stops you.
fn place_markers(
    lib: &crate::trackprops::PropLibrary,
    prog: &TrackProgram,
    syn: &Synth,
    by_sheet: &mut std::collections::HashMap<String, Mesh>,
) -> usize {
    let blocks: Vec<&crate::trackprops::Prop> = lib.props.iter().filter(|p| is_marker(lib, p)).collect();
    if blocks.is_empty() {
        return 0;
    }
    let (lap, half, seed) = (prog.lap_length(), prog.width * 0.5, prog.terrain.relief.seed);
    let stations = prog.stations(0.5);
    let at = |s: f32| stations[((s.rem_euclid(lap) / 0.5) as usize).min(stations.len() - 1)];
    let coarse = prog.stations(2.0);
    let n = coarse.len();
    let pits = Pits::of(prog);
    let (mut count, mut last) = (0usize, f32::NEG_INFINITY);
    for i in 0..n {
        let c = coarse[i].curvature.abs();
        if c < 1.0 / MARKER_CORNER_R_M
            || c < coarse[(i + n - 1) % n].curvature.abs()
            || c < coarse[(i + 1) % n].curvature.abs()
        {
            continue;
        }
        let s0 = coarse[i].s;
        if s0 - last < MARKER_APART_M {
            continue;
        }
        last = s0;
        // The inside: curvature is positive turning right, and +1 is the rider's right.
        let side = coarse[i].curvature.signum();
        for j in 0..MARKERS_PER_CORNER {
            let st = at(s0 + (j as f32 - (MARKERS_PER_CORNER as f32 - 1.0) * 0.5) * MARKER_PITCH_M);
            let (rx, rz) = crate::trackprog::right_vector(st.heading);
            let out = half + MARKER_OUT_M;
            let (x, z) = (st.x + rx * out * side, st.z + rz * out * side);
            let key = (i * 7 + j) as u32;
            let prop = blocks[((rnd(seed ^ 0x3A7C, key) * blocks.len() as f32) as usize).min(blocks.len() - 1)];
            let m = draped(&edfwrite::turned(&prop.mesh, st.heading.to_degrees()), x, z, 0.0, syn);
            if on_riding_surface(syn, half, &m)
                || pits.on_lane(lap, syn, &m)
                || syn.outside_the_start(x, z).is_some_and(|e| e < 2.0)
            {
                continue;
            }
            by_sheet.entry(prop.sheet.clone()).or_default().append(&m);
            count += 1;
        }
    }
    count
}

/// Turn markers: the corners that get them, how many to one, how far apart, how far past the
/// edge, and the least lap between two corners' sets.
const MARKER_CORNER_R_M: f32 = 35.0;
const MARKERS_PER_CORNER: usize = 4;
const MARKER_PITCH_M: f32 = 3.0;
const MARKER_OUT_M: f32 = 1.3;
const MARKER_APART_M: f32 = 20.0;

fn place_parking(
    lib: &crate::trackprops::PropLibrary,
    prog: &TrackProgram,
    syn: &Synth,
    by_sheet: &mut std::collections::HashMap<String, Mesh>,
) -> usize {
    use crate::trackobjects::Class;
    let cars: Vec<&crate::trackprops::Prop> = lib
        .props
        .iter()
        .filter(|p| p.class == Class::Vehicle && whole(p) && (1.2..=3.2).contains(&p.height) && p.span <= PARK_CAR_MAX_M)
        .collect();
    if cars.is_empty() {
        return 0;
    }
    let (lap, half, seed) = (prog.lap_length(), prog.width * 0.5, prog.terrain.relief.seed);
    let stations = prog.stations(0.5);
    let coarse = prog.stations(2.0);
    let at = |s: f32| stations[((s.rem_euclid(lap) / 0.5) as usize).min(stations.len() - 1)];
    let pits = Pits::of(prog);
    let (depth, len) = (PARK_ROWS as f32 * PARK_ROW_M, PARK_PER_ROW as f32 * PARK_PITCH_M);
    let (mut placed, mut lots, mut s) = (0usize, 0usize, lap * 0.25);
    'lots: while s < lap * 0.95 && lots < PARK_LOTS {
        let st = at(s);
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        let (fx, fz) = crate::trackprog::heading_vector(st.heading);
        for side in [1.0f32, -1.0] {
            let out = half + PARK_OUT_M;
            let (cx, cz) = (st.x + rx * out * side, st.z + rz * out * side);
            let spot = |a: f32, b: f32| (cx + fx * a + rx * b * side, cz + fz * a + rz * b * side);
            let clear = [(-len * 0.5, 0.0), (len * 0.5, 0.0), (-len * 0.5, depth), (len * 0.5, depth)].iter().all(|&(a, b)| {
                let (x, z) = spot(a, b);
                inside(prog, x, z, 4.0)
                    && clearance(&coarse, x, z) >= half + PARK_CLEAR_M
                    && syn.outside_the_start(x, z).map_or(true, |e| e > PARK_CLEAR_M)
                    && !pits.near(lap, syn, x, z)
            });
            if !clear {
                continue;
            }
            let mut k = 0u32;
            for row in 0..PARK_ROWS {
                for j in 0..PARK_PER_ROW {
                    let (x, z) = spot(-len * 0.5 + (j as f32 + 0.5) * PARK_PITCH_M, (row as f32 + 0.5) * PARK_ROW_M);
                    let key = lots as u32 * 97 + k;
                    let prop = cars[((rnd(seed ^ 0xCA25, key) * cars.len() as f32) as usize).min(cars.len() - 1)];
                    let (lo, hi) = prop.mesh.bounds();
                    // Nose in, across the row: a mesh long in x turns by the heading, one long in
                    // z by the heading plus ninety, and the second row faces the first.
                    let base = st.heading.to_degrees() + if hi[0] - lo[0] >= hi[2] - lo[2] { 0.0 } else { 90.0 };
                    let deg = base + if row % 2 == 0 { 0.0 } else { 180.0 };
                    by_sheet
                        .entry(prop.sheet.clone())
                        .or_default()
                        .append(&draped(&edfwrite::turned(&prop.mesh, deg), x, z, 0.0, syn));
                    k += 1;
                }
            }
            placed += k as usize;
            lots += 1;
            s += PARK_LOT_APART_M;
            continue 'lots;
        }
        s += 15.0;
    }
    placed
}

/// Spectator lots: how many, how far out from the edge, rows and cars to a row, the room per car
/// and per row, how clear of the track, how far apart round the lap, and the longest car.
const PARK_LOTS: usize = 2;
const PARK_OUT_M: f32 = 18.0;
const PARK_ROWS: usize = 2;
const PARK_PER_ROW: usize = 8;
const PARK_PITCH_M: f32 = 3.2;
const PARK_ROW_M: f32 = 7.0;
const PARK_CLEAR_M: f32 = 12.0;
const PARK_LOT_APART_M: f32 = 500.0;
const PARK_CAR_MAX_M: f32 = 7.5;

fn place_bales(
    lib: &crate::trackprops::PropLibrary,
    prog: &TrackProgram,
    syn: &Synth,
    by_sheet: &mut std::collections::HashMap<String, Mesh>,
) -> usize {
    use crate::trackprog::Feature;
    let singles: Vec<&crate::trackprops::Prop> = lib
        .props
        .iter()
        .filter(|p| {
            p.class == crate::trackobjects::Class::Bale
                && whole(p)
                && (0.4..=BALE_MAX_H_M).contains(&p.height)
                && p.reach <= BALE_MAX_REACH_M
        })
        .collect();
    if singles.is_empty() {
        return 0;
    }
    let (lap, half, seed) = (prog.lap_length(), prog.width * 0.5, prog.terrain.relief.seed);
    let stations = prog.stations(0.5);
    let at = |s: f32| stations[((s.rem_euclid(lap) / 0.5) as usize).min(stations.len() - 1)];
    let wrap = |d: f32| {
        let d = d.rem_euclid(lap);
        d.min(lap - d)
    };
    // Where: round the lap, and which side, +1 the rider's right.
    let mut spots: Vec<(f32, f32, f32)> = Vec::new();
    for f in &prog.features {
        if matches!(f, Feature::Tabletop { .. } | Feature::Double { .. } | Feature::StepUp { .. }) {
            let s = f.at() + f.length();
            spots.extend([(s, 1.0, 0.0), (s, -1.0, 0.0)]);
        }
    }
    let coarse = prog.stations(2.0);
    let n = coarse.len();
    for i in 0..n {
        let c = coarse[i].curvature.abs();
        if c < 1.0 / BALE_CORNER_R_M
            || c < coarse[(i + n - 1) % n].curvature.abs()
            || c < coarse[(i + 1) % n].curvature.abs()
        {
            continue;
        }
        // The outside, where riders run wide: curvature is positive turning right, and +1 is the
        // rider's right. The inside is the turn markers'.
        spots.push((coarse[i].s, -coarse[i].curvature.signum(), c));
    }
    // The tightest corners first, then landings, until the lap carries a rated track's count.
    spots.sort_by(|a, b| b.2.total_cmp(&a.2));
    let most = (lap / 1000.0 * BALE_PER_KM).round() as usize;
    let pits = Pits::of(prog);
    let mut taken: Vec<(f32, f32)> = Vec::new();
    let mut count = 0;
    for (k, &(s0, side, _)) in spots.iter().enumerate() {
        if count >= most {
            break;
        }
        if taken.iter().any(|&(q, sd)| sd == side && wrap(q - s0) < BALE_APART_M) {
            continue;
        }
        let key = k as u32;
        let prop = singles[((rnd(seed ^ 0xBA1E, key) * singles.len() as f32) as usize).min(singles.len() - 1)];
        let (lo, hi) = prop.mesh.bounds();
        let long_x = hi[0] - lo[0] >= hi[2] - lo[2];
        let pitch = (hi[0] - lo[0]).max(hi[2] - lo[2]) + 0.15;
        let row = (BALE_ROW.0 + (rnd(seed ^ 0xBA1F, key) * (BALE_ROW.1 - BALE_ROW.0 + 1) as f32) as usize).min(BALE_ROW.1);
        let out = half + BALE_OUT_M.0 + rnd(seed ^ 0xBA20, key) * (BALE_OUT_M.1 - BALE_OUT_M.0);
        let mut laid = Mesh::default();
        let mut ok = true;
        for j in 0..row {
            let st = at(s0 + (j as f32 - (row as f32 - 1.0) * 0.5) * pitch);
            let (rx, rz) = crate::trackprog::right_vector(st.heading);
            let (x, z) = (st.x + rx * out * side, st.z + rz * out * side);
            // Along the track: a mesh long in x turns by the heading plus ninety.
            let deg = st.heading.to_degrees() + if long_x { 90.0 } else { 0.0 };
            let bale = draped(&edfwrite::turned(&prop.mesh, deg), x, z, 0.0, syn);
            if !inside(prog, x, z, 2.0)
                || on_riding_surface(syn, half, &bale)
                || pits.on_lane(lap, syn, &bale)
                || syn.outside_the_start(x, z).is_some_and(|e| e < OFF_THE_START_M)
            {
                ok = false;
                break;
            }
            laid.append(&bale);
        }
        if ok {
            taken.push((s0, side));
            by_sheet.entry(prop.sheet.clone()).or_default().append(&laid);
            count += row;
        }
    }
    count
}

/// Tear-off patches per 2 km of lap, and strips in each. Indiana: 1,452 in about ten patches.
const TEAROFF_PATCHES_PER_2KM: f32 = 10.0;
const TEAROFFS_PER_PATCH: u32 = 150;
/// One strip, and how far a patch spreads along the lap and out from the centreline.
const TEAROFF_M: f32 = 0.3;
const TEAROFF_SPREAD_M: f32 = 14.0;
const TEAROFF_OUT_M: f32 = 10.0;

/// Goggle tear-offs: the strips riders peel off and drop where they brake and turn.
///
/// Indiana lays them as flat 0.3 m quads on the dirt, each one of three strips of
/// `tearoffs_c_a` at a random yaw, in patches at the corners, median 4.8 m off the centreline.
/// Double-sided, so the winding can't bury them; the under face is never seen.
fn tearoffs(prog: &TrackProgram, syn: &Synth, seed: u32) -> Mesh {
    let lap = prog.lap_length();
    let stations = prog.stations(2.0);
    let mut m = Mesh::default();
    if stations.len() < 8 {
        return m;
    }
    // The tightest corners, at least 40 m apart.
    let want = ((lap / 2000.0) * TEAROFF_PATCHES_PER_2KM).round().max(1.0) as usize;
    let mut order: Vec<usize> = (0..stations.len()).collect();
    order.sort_by(|&a, &b| stations[b].curvature.abs().total_cmp(&stations[a].curvature.abs()));
    let mut centres: Vec<usize> = Vec::new();
    for i in order {
        if centres.len() >= want || stations[i].curvature.abs() < 1.0 / 60.0 {
            break;
        }
        let gap = |a: usize, b: usize| {
            let d = (a as f32 - b as f32).abs() * 2.0;
            d.min(lap - d)
        };
        if centres.iter().all(|&c| gap(c, i) > 40.0) {
            centres.push(i);
        }
    }
    for (p, &c) in centres.iter().enumerate() {
        for k in 0..TEAROFFS_PER_PATCH {
            let key = p as u32 * 1000 + k;
            let along = (rnd(seed ^ 0x7E01, key) - 0.5) * 2.0 * TEAROFF_SPREAD_M;
            let i = ((c as f32 + along / 2.0).round() as isize).rem_euclid(stations.len() as isize) as usize;
            let st = stations[i];
            // Mostly on the track and its edge: a skew toward the middle of the band.
            let r = rnd(seed ^ 0x7E02, key);
            let out = TEAROFF_OUT_M * r * r.sqrt();
            let side = if rnd(seed ^ 0x7E03, key) < 0.5 { -1.0 } else { 1.0 };
            let (rx, rz) = crate::trackprog::right_vector(st.heading);
            let (x, z) = (st.x + rx * out * side, st.z + rz * out * side);
            if !inside(prog, x, z, 1.0)
                || syn.outside_the_start(x, z).map(|e| e <= OFF_THE_START_M).unwrap_or(false)
            {
                continue;
            }
            let yaw = rnd(seed ^ 0x7E04, key) * std::f32::consts::TAU;
            let strip = ((rnd(seed ^ 0x7E05, key) * 3.0) as u32).min(2) as f32;
            let (c0, s0) = (yaw.cos() * TEAROFF_M * 0.5, yaw.sin() * TEAROFF_M * 0.5);
            let corners = [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
            let base = m.vertex_count() as u32;
            for (cu, cv) in corners {
                let (px, pz) = (x + c0 * cu - s0 * cv, z + s0 * cu + c0 * cv);
                m.positions.extend_from_slice(&[px, ground(syn, px, pz) + 0.015, pz]);
                m.normals.extend_from_slice(&[0.0, 1.0, 0.0]);
                let u = (strip + (cu + 1.0) * 0.5) / 3.0;
                m.uvs.extend_from_slice(&[u, (cv + 1.0) * 0.5]);
            }
            m.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            m.indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
        }
    }
    m
}

/// Where a tree goes, before the library says which one.
#[derive(Clone, Copy)]
struct Plant {
    x: f32,
    z: f32,
    foot: f32,
    /// Degrees.
    yaw: f32,
    key: u32,
    /// On the backdrop, where nobody rides: drawn and never solid.
    far: bool,
}

/// How tall a library tree has to be to stand in for one: under this it is a bush card, over
/// it a whole treeline baked as one object.
const TREE_REAL_MIN_M: f32 = 4.0;
const TREE_REAL_MAX_M: f32 = 40.0;
/// How far a library tree may reach from its own trunk. Some lifted "trees" are whole clumps
/// baked as one object, 40 m across; planted at a point they stood over the track.
const TREE_REAL_REACH_M: f32 = 5.0;

/// How much of a library tree has to land on leaves rather than on the clear part of its sheet.
const TREE_SHOWS_MIN: f32 = 0.25;

/// Whether a library tree shows on its own sheet: enough of its triangles land on texels the
/// cut-out keeps.
///
/// Not everything a donor gives up as a tree is one. Slivers of Indiana's leaf-litter ground
/// sheet split off on hillsides stand 7–18 m tall and a few metres across, so they pass on
/// size — but their UVs span less than a texel of a clear corner, and planted they draw as
/// nothing at all.
fn shows(lib: &crate::trackprops::PropLibrary, p: &crate::trackprops::Prop) -> bool {
    let Some((_, w, h, rgba)) = lib.sheets.iter().find(|s| s.0 == p.sheet) else {
        return false;
    };
    let (w, h, m) = (*w as usize, *h as usize, &p.mesh);
    if w == 0 || h == 0 || m.triangle_count() == 0 || m.uvs.len() < m.vertex_count() * 2 {
        return false;
    }
    let on = m
        .indices
        .chunks_exact(3)
        .filter(|t| {
            let (mut u, mut v) = (0.0f32, 0.0f32);
            for &i in *t {
                u += m.uvs[i as usize * 2] / 3.0;
                v += m.uvs[i as usize * 2 + 1] / 3.0;
            }
            let x = (((u - u.floor()) * w as f32) as usize).min(w - 1);
            let y = (((v - v.floor()) * h as f32) as usize).min(h - 1);
            rgba.get((y * w + x) * 4 + 3).is_some_and(|&a| a >= 128)
        })
        .count();
    on as f32 >= m.triangle_count() as f32 * TREE_SHOWS_MIN
}

/// Every tree the lap and the backdrop asked for, each a real one out of the library — the
/// donor's own and whatever other tracks were baked in beside it. One model a sheet.
fn plant_trees(lib: &crate::trackprops::PropLibrary, plants: &[Plant]) -> Vec<(String, Mesh, Texture, bool)> {
    let trees: Vec<&crate::trackprops::Prop> = lib
        .props
        .iter()
        .filter(|p| {
            p.class == crate::trackobjects::Class::Tree
                && (TREE_REAL_MIN_M..=TREE_REAL_MAX_M).contains(&p.height)
                && !crate::trackprops::is_crowd(&p.sheet)
                && p.reach <= TREE_REAL_REACH_M
                && shows(lib, p)
        })
        .collect();
    if trees.is_empty() {
        return Vec::new();
    }
    let mut by: std::collections::BTreeMap<(String, bool), Mesh> = Default::default();
    for p in plants {
        let pick = ((rnd(0x7A11, p.key) * trees.len() as f32) as usize).min(trees.len() - 1);
        let prop = trees[pick];
        // On its own base: stored above its donor's ground, a small tree whose ground was
        // misread stood that far up in the sky.
        let base = prop.mesh.positions.chunks_exact(3).map(|v| v[1]).fold(f32::INFINITY, f32::min).max(0.0);
        by.entry((prop.sheet.clone(), p.far))
            .or_default()
            .append(&edfwrite::moved(&edfwrite::turned(&prop.mesh, p.yaw), [p.x, p.foot - base, p.z]));
    }
    by.into_iter()
        .filter_map(|((sheet, far), mesh)| {
            let (name, w, h, rgba) = lib.sheets.iter().find(|s| s.0 == sheet)?;
            let tex = Texture { name: name.clone(), width: *w, height: *h, rgba: rgba.clone() };
            let kind = format!("{}_{}", if far { "backdrop_trees" } else { "trees" }, short_sheet(&sheet));
            Some((kind, mesh, tex, !far))
        })
        .collect()
}

/// A sheet name cut down to a model name.
fn short_sheet(sheet: &str) -> String {
    sheet
        .trim_end_matches("_c_a")
        .trim_end_matches("_c")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}
