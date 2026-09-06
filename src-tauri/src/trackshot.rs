//! The picture the game shows beside a track's name.
//!
//! Not a map. `<track>_map.tga` is the plan view the game draws the route over and it has to
//! stay one; this is the photograph. A flat false-colour relief with the lap tinted over it
//! tells a player nothing they can't read off the map, so this renders the terrain properly:
//! a camera above and off to one side, the grid rasterised through a depth buffer, lit by the
//! same sun the track's own `.amb` declares, and hazed with distance.
//!
//! Nothing here knows what a track is. It takes a heightfield, a sheet of ground colour and a
//! set of points the frame has to hold, which is all a picture of one needs.

/// What to render, and what to light it with.
pub struct Scene<'a> {
    pub gw: usize,
    pub gh: usize,
    /// Metres between samples, both axes — the grid's cells are square.
    pub mps: f32,
    /// Metres, `gw * gh`, row-major, row zero at `z = 0`.
    pub heights: &'a [f32],
    /// The ground's colour, `adim * adim`, sampled by where a point is across the terrain
    /// rather than by grid index — it is a sheet, not a per-vertex attribute, so it can be
    /// finer or coarser than the heightfield without either one caring.
    pub albedo: &'a [[f32; 3]],
    pub adim: usize,
    /// World `(x, z)` the frame must hold. The camera is placed and pulled back to fit these
    /// and nothing else, so a lap on a corner of a big terrain still fills the picture.
    pub focus: &'a [(f32, f32)],
    /// Direction *to* the sun, and the light either side of it. Straight off the `.amb`, so
    /// the picture is lit the way the track is.
    pub sun: [f32; 3],
    pub sun_colour: [f32; 3],
    pub ambient: [f32; 3],
    /// The sky overhead, at the horizon, and the haze the distance goes to.
    pub zenith: [f32; 3],
    pub horizon: [f32; 3],
    pub haze: [f32; 3],
    /// How far above the ground the camera sits, degrees. Low reads as a photograph and hides
    /// the lap behind its own hills; straight down reads as the map again. See [`TILT_DEG`].
    pub tilt_deg: f32,
}

/// Bird's eye, tilted: high enough that the whole lap reads as a shape, shallow enough that
/// the hills under it have a near side and a far side.
pub const TILT_DEG: f32 = 36.0;

/// Fairly long, because a wide lens bends a lap into a bowl and puts the near corner of the
/// terrain in your face.
const FOV_DEG: f32 = 36.0;

/// How much of the frame the focus points fill.
const FIT: f32 = 0.88;

/// Rendered at twice the picture's size and averaged down. A heightfield's silhouette against
/// the sky is one long near-horizontal edge, which is the worst case for aliasing.
const SS: usize = 2;

/// The grid is sampled down to at most this many vertices across before it is drawn. A quad
/// smaller than a pixel costs the same as one that fills the screen and shows less; the relief
/// it carries is kept, because normals are still taken from the full-resolution neighbours.
const MESH_MAX: usize = 400;

/// Nothing closer than this is drawn — it is behind the lens.
const NEAR_M: f32 = 1.0;

/// How many rings of ground are carried on past the terrain's edge before the haze has them.
const APRON: usize = 28;

/// Over how many metres the apron's ground levels off to the country's own height.
const APRON_LEVEL_M: f32 = 90.0;

/// And how quickly it stops being the terrain's colour. Much shorter than the levelling: a
/// smeared colour shows as streaks radiating out of the picture, where a smeared height only
/// shows as a gentle shelf.
const APRON_TINT_M: f32 = 25.0;

/// The picture, `dim * dim` RGB, row zero at the top.
pub fn render(scene: &Scene, dim: usize) -> Vec<[u8; 3]> {
    let sdim = dim * SS;
    let cam = fit(scene);

    // The mesh: the heightfield sampled down, with each vertex already projected and shaded.
    //
    // And an apron of ground carried on past the terrain's own edge. A terrain is a few
    // hundred metres square and the horizon is not, so without one the picture is a slab of
    // land floating in the sky with a cut edge — the apron holds the border height and colour
    // and runs out until the haze has it, which is what standing on a track looks like.
    // What the country past the terrain is made of: the average of the sheet's own border, so
    // the join is invisible whatever the ground is painted with. Guessing it — the field's
    // base colour, say — puts a square of terrain in a plain of a different colour, because
    // the outermost band a track paints is the turf and not the soil under it.
    let country = border(scene);
    let flat = level(scene);
    let stride = scene.gw.max(scene.gh).div_ceil(MESH_MAX).max(1);
    let span_x = (scene.gw - 1) as f32 * scene.mps;
    let span_z = (scene.gh - 1) as f32 * scene.mps;
    let reach = cam.reach * 2.5;
    // Spaced so they bunch up against the terrain, where the join has to be invisible, and
    // stretch out towards the horizon, where nothing is in focus anyway.
    let out = |k: usize| reach * (k as f32 / APRON as f32).powi(2);
    let axis = |span: f32| -> Vec<f32> {
        let mut v: Vec<f32> = (1..=APRON).rev().map(|k| -out(k)).collect();
        let step = stride as f32 * scene.mps;
        let n = (span / step).floor() as usize;
        v.extend((0..=n).map(|i| i as f32 * step));
        // The terrain's own far edge, so the apron starts exactly where the ground stops.
        if v.last().is_some_and(|&x| x < span - 1e-3) {
            v.push(span);
        }
        v.extend((1..=APRON).map(|k| span + out(k)));
        v
    };
    let xs = axis(span_x);
    let zs = axis(span_z);
    let (rw, rh) = (xs.len(), zs.len());
    let mut sx = vec![0.0f32; rw * rh];
    let mut sy = vec![0.0f32; rw * rh];
    let mut depth = vec![0.0f32; rw * rh];
    let mut lit = vec![[0.0f32; 3]; rw * rh];
    for (j, &z) in zs.iter().enumerate() {
        for (i, &x) in xs.iter().enumerate() {
            let y = height(scene, flat, x, z);
            let (ndc, d) = cam.project([x, y, z]);
            let at = j * rw + i;
            sx[at] = (ndc[0] * 0.5 + 0.5) * sdim as f32;
            sy[at] = (0.5 - ndc[1] * 0.5) * sdim as f32;
            depth[at] = d;
            lit[at] = shade(scene, country, flat, x, z);
        }
    }

    // The sky, and the ground drawn over it.
    let mut px = vec![[0.0f32; 3]; sdim * sdim];
    for y in 0..sdim {
        for x in 0..sdim {
            px[y * sdim + x] = cam.sky(scene, x, y, sdim);
        }
    }
    let mut zbuf = vec![f32::INFINITY; sdim * sdim];
    for j in 0..rh.saturating_sub(1) {
        for i in 0..rw.saturating_sub(1) {
            let (a, b, c, d) = (j * rw + i, j * rw + i + 1, (j + 1) * rw + i + 1, (j + 1) * rw + i);
            for tri in [[a, b, c], [a, c, d]] {
                raster(&tri, &sx, &sy, &depth, &lit, scene, &cam, &mut px, &mut zbuf, sdim);
            }
        }
    }

    // Down to the picture's own size.
    let mut out = vec![[0u8; 3]; dim * dim];
    for y in 0..dim {
        for x in 0..dim {
            let mut acc = [0.0f32; 3];
            for sy in 0..SS {
                for sxo in 0..SS {
                    let p = px[(y * SS + sy) * sdim + x * SS + sxo];
                    for k in 0..3 {
                        acc[k] += p[k];
                    }
                }
            }
            let n = (SS * SS) as f32;
            out[y * dim + x] = [
                (acc[0] / n).clamp(0.0, 255.0) as u8,
                (acc[1] / n).clamp(0.0, 255.0) as u8,
                (acc[2] / n).clamp(0.0, 255.0) as u8,
            ];
        }
    }
    out
}

/// One vertex's colour before the distance is put back in: the ground's own sheet, under the
/// track's sun and its sky.
fn shade(scene: &Scene, country: [f32; 3], flat: f32, x: f32, z: f32) -> [f32; 3] {
    let n = normal(scene, flat, x, z);
    let d = (n[0] * scene.sun[0] + n[1] * scene.sun[1] + n[2] * scene.sun[2]).max(0.0);
    // The sky is not a constant: ground facing up gets the whole of it, a slope facing away
    // from it gets less. Without this every flat piece of shadow is the same flat grey.
    let sky = 0.55 + 0.45 * n[1].max(0.0);
    let a = sample(scene, country, x, z);
    let mut c = [0.0f32; 3];
    for k in 0..3 {
        c[k] = a[k] * (scene.ambient[k] * sky + scene.sun_colour[k] * d);
    }
    c
}

/// The height at a place, with the apron past the terrain's edge levelling off to the border's
/// mean.
///
/// Clamping instead — every edge sample keeping its own height all the way out — extrudes the
/// terrain's border into a fan of ridges reaching the horizon, and they shade as bright streaks
/// radiating out of the picture.
fn height(scene: &Scene, flat: f32, x: f32, z: f32) -> f32 {
    let (gx, gy) = cell(scene, x, z);
    let h = scene.heights[gy * scene.gw + gx];
    let past = outside(scene, x, z);
    if past <= 0.0 {
        return h;
    }
    h + (flat - h) * smoothstep(past / APRON_LEVEL_M)
}

/// How far past the terrain's edge a place is, metres, and zero on it.
fn outside(scene: &Scene, x: f32, z: f32) -> f32 {
    let span_x = (scene.gw - 1) as f32 * scene.mps;
    let span_z = (scene.gh - 1) as f32 * scene.mps;
    (-x).max(x - span_x).max((-z).max(z - span_z))
}

/// Which sample a place on the ground belongs to, clamped inside the grid.
fn cell(scene: &Scene, x: f32, z: f32) -> (usize, usize) {
    (
        (x / scene.mps).round().clamp(0.0, (scene.gw - 1) as f32) as usize,
        (z / scene.mps).round().clamp(0.0, (scene.gh - 1) as f32) as usize,
    )
}

/// The heightfield's normal, from the ground either side at full resolution — the relief
/// between two drawn vertices is what tells a rut from a berm, and a mesh sampled down to fit
/// the picture would lose all of it. Taken through [`height`], so the apron flattens out.
fn normal(scene: &Scene, flat: f32, x: f32, z: f32) -> [f32; 3] {
    let e = scene.mps;
    let h = |x: f32, z: f32| height(scene, flat, x, z);
    let n = [
        -(h(x + e, z) - h(x - e, z)) / (2.0 * e),
        1.0,
        -(h(x, z + e) - h(x, z - e)) / (2.0 * e),
    ];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    [n[0] / l, n[1] / l, n[2] / l]
}

/// The height the country settles to: the mean round the terrain's edge.
fn level(scene: &Scene) -> f32 {
    let (w, h) = (scene.gw, scene.gh);
    let mut sum = 0.0f64;
    let mut n = 0.0f64;
    for i in 0..w {
        sum += (scene.heights[i] + scene.heights[(h - 1) * w + i]) as f64;
        n += 2.0;
    }
    for j in 0..h {
        sum += (scene.heights[j * w] + scene.heights[j * w + w - 1]) as f64;
        n += 2.0;
    }
    (sum / n) as f32
}

/// The average colour round the edge of the ground sheet.
fn border(scene: &Scene) -> [f32; 3] {
    let d = scene.adim;
    let mut sum = [0.0f64; 3];
    let mut n = 0.0f64;
    for i in 0..d {
        for at in [i, (d - 1) * d + i, i * d, i * d + d - 1] {
            for k in 0..3 {
                sum[k] += scene.albedo[at][k] as f64;
            }
            n += 1.0;
        }
    }
    [(sum[0] / n) as f32, (sum[1] / n) as f32, (sum[2] / n) as f32]
}

/// The ground sheet at a place on the terrain, bilinear so the picture doesn't show the
/// sheet's own pixels where the camera is close.
fn sample(scene: &Scene, country: [f32; 3], x: f32, z: f32) -> [f32; 3] {
    let d = scene.adim as f32;
    let (span_x, span_z) = ((scene.gw - 1) as f32 * scene.mps, (scene.gh - 1) as f32 * scene.mps);
    let u = (x / span_x * d - 0.5).clamp(0.0, d - 1.001);
    let v = (z / span_z * d - 0.5).clamp(0.0, d - 1.001);
    let (x0, y0) = (u as usize, v as usize);
    let (fx, fy) = (u - x0 as f32, v - y0 as f32);
    let at = |x: usize, y: usize| scene.albedo[y.min(scene.adim - 1) * scene.adim + x.min(scene.adim - 1)];
    let (a, b, c, d) = (at(x0, y0), at(x0 + 1, y0), at(x0, y0 + 1), at(x0 + 1, y0 + 1));
    let mut out = [0.0f32; 3];
    for k in 0..3 {
        let top = a[k] + (b[k] - a[k]) * fx;
        let bot = c[k] + (d[k] - c[k]) * fx;
        out[k] = top + (bot - top) * fy;
    }
    // Past the terrain the sheet has nothing left to say, and clamping it paints the border
    // pixel all the way to the horizon — which shows as stripes radiating out of the picture.
    // So the apron settles to open country over the first few tens of metres instead.
    let past = outside(scene, x, z);
    if past > 0.0 {
        let t = smoothstep(past / APRON_TINT_M);
        for k in 0..3 {
            out[k] += (country[k] - out[k]) * t;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The camera
// ---------------------------------------------------------------------------

struct Camera {
    pos: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    fwd: [f32; 3],
    tan_half: f32,
    /// How far the frame's own subject is, which is what the haze is measured against — an
    /// absolute fog density would swallow a big track and leave a small one crisp.
    reach: f32,
}

impl Camera {
    /// Where a world point lands: the frame in `-1..1` both ways, and how far in front of the
    /// lens it is.
    fn project(&self, w: [f32; 3]) -> ([f32; 2], f32) {
        let v = [w[0] - self.pos[0], w[1] - self.pos[1], w[2] - self.pos[2]];
        let dot = |a: [f32; 3]| v[0] * a[0] + v[1] * a[1] + v[2] * a[2];
        let d = dot(self.fwd);
        if d <= NEAR_M {
            return ([0.0, 0.0], d);
        }
        ([dot(self.right) / d / self.tan_half, dot(self.up) / d / self.tan_half], d)
    }

    /// The sky behind everything, as the ray through a pixel sees it.
    fn sky(&self, scene: &Scene, x: usize, y: usize, sdim: usize) -> [f32; 3] {
        let nx = ((x as f32 + 0.5) / sdim as f32 * 2.0 - 1.0) * self.tan_half;
        let ny = (1.0 - (y as f32 + 0.5) / sdim as f32 * 2.0) * self.tan_half;
        let mut d = [0.0f32; 3];
        for k in 0..3 {
            d[k] = self.fwd[k] + self.right[k] * nx + self.up[k] * ny;
        }
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let e = d[1] / l;
        let mut out = [0.0f32; 3];
        for k in 0..3 {
            // Above the horizon the sky climbs to its zenith; below it, where the terrain has
            // run out, the haze the ground fades into — so an edge of terrain against the
            // background has nothing to be an edge against.
            out[k] = if e >= 0.0 {
                let t = smoothstep((e / 0.42).min(1.0));
                scene.horizon[k] + (scene.zenith[k] - scene.horizon[k]) * t
            } else {
                let t = smoothstep((-e / 0.30).min(1.0));
                scene.horizon[k] + (scene.haze[k] - scene.horizon[k]) * t
            };
        }
        out
    }
}

/// Place the camera: above and off to one side, looking along the lap's short axis so its long
/// one runs across the frame, and pulled back until the whole of it fits.
fn fit(scene: &Scene) -> Camera {
    let flat = level(scene);
    let h = |x: f32, z: f32| height(scene, flat, x, z);
    // The subject: where the focus points are, and how far they spread.
    let n = scene.focus.len().max(1) as f32;
    let (mut cx, mut cz, mut cy) = (0.0f32, 0.0f32, 0.0f32);
    for &(x, z) in scene.focus {
        cx += x;
        cz += z;
        cy += h(x, z);
    }
    let (cx, cz, cy) = (cx / n, cz / n, cy / n);
    let (mut sxx, mut sxz, mut szz, mut extent) = (0.0f32, 0.0f32, 0.0f32, 1.0f32);
    for &(x, z) in scene.focus {
        let (dx, dz) = (x - cx, z - cz);
        sxx += dx * dx;
        sxz += dx * dz;
        szz += dz * dz;
        extent = extent.max((dx * dx + dz * dz).sqrt());
    }
    // The long way through the focus points, and the way across it — which is where the camera
    // goes, so the lap is seen along its narrow side and fills the width of the picture.
    let theta = 0.5 * (2.0 * sxz).atan2(sxx - szz);
    let across = (-theta.sin(), theta.cos());
    // Of the two sides, the one the sun is on: a subject lit from behind the camera shows its
    // relief, and one lit from behind itself is a silhouette.
    let sun_h = (scene.sun[0], scene.sun[2]);
    let sign = if across.0 * sun_h.0 + across.1 * sun_h.1 >= 0.0 { 1.0 } else { -1.0 };
    let dir = (across.0 * sign, across.1 * sign);

    let el = scene.tilt_deg.to_radians();
    let tan_half = (FOV_DEG.to_radians() * 0.5).tan();
    let target = [cx, cy, cz];
    let mut dist = extent * 2.6 + 60.0;
    let mut cam = at(target, dir, el, dist, tan_half);
    // Pull back until nothing wanted is outside the frame. The frame's size goes as one over
    // the distance, so scaling by how far over it is converges in a handful of passes.
    for _ in 0..40 {
        let mut m: f32 = 0.0;
        for &(x, z) in scene.focus {
            let (ndc, d) = cam.project([x, h(x, z), z]);
            if d <= NEAR_M {
                m = m.max(4.0);
                continue;
            }
            m = m.max(ndc[0].abs()).max(ndc[1].abs());
        }
        if m <= 1e-4 {
            break;
        }
        let k = m / FIT;
        dist *= k.clamp(0.4, 2.5);
        cam = at(target, dir, el, dist, tan_half);
        if (k - 1.0).abs() < 0.002 {
            break;
        }
    }
    cam
}

/// A camera an azimuth, an elevation and a distance from what it is looking at.
fn at(target: [f32; 3], dir: (f32, f32), el: f32, dist: f32, tan_half: f32) -> Camera {
    let (ce, se) = (el.cos(), el.sin());
    let pos = [
        target[0] + dir.0 * ce * dist,
        target[1] + se * dist,
        target[2] + dir.1 * ce * dist,
    ];
    let mut fwd = [target[0] - pos[0], target[1] - pos[1], target[2] - pos[2]];
    let l = (fwd[0] * fwd[0] + fwd[1] * fwd[1] + fwd[2] * fwd[2]).sqrt().max(1e-6);
    for k in 0..3 {
        fwd[k] /= l;
    }
    // Right is forward crossed with world up, so the horizon in the picture is level.
    let mut right = [fwd[2], 0.0, -fwd[0]];
    let rl = (right[0] * right[0] + right[2] * right[2]).sqrt().max(1e-6);
    right[0] /= rl;
    right[2] /= rl;
    // The world is left-handed — x across, y up, z down the grid — so right is up crossed
    // with forward, and up is forward crossed with right. The other way round turns the
    // picture upside down, which reads as a camera underground.
    let up = [
        fwd[1] * right[2] - fwd[2] * right[1],
        fwd[2] * right[0] - fwd[0] * right[2],
        fwd[0] * right[1] - fwd[1] * right[0],
    ];
    Camera { pos, right, up, fwd, tan_half, reach: dist }
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

/// One triangle into the depth buffer, perspective-correct, with the haze put back in per
/// pixel — across a quad in the foreground it changes fast enough that doing it per vertex
/// bands the ground.
#[allow(clippy::too_many_arguments)]
fn raster(
    tri: &[usize; 3],
    sx: &[f32],
    sy: &[f32],
    depth: &[f32],
    lit: &[[f32; 3]],
    scene: &Scene,
    cam: &Camera,
    px: &mut [[f32; 3]],
    zbuf: &mut [f32],
    sdim: usize,
) {
    let z = [depth[tri[0]], depth[tri[1]], depth[tri[2]]];
    if z.iter().any(|&d| d <= NEAR_M) {
        return;
    }
    let p = [
        [sx[tri[0]], sy[tri[0]]],
        [sx[tri[1]], sy[tri[1]]],
        [sx[tri[2]], sy[tri[2]]],
    ];
    let area = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
    if area.abs() < 1e-9 {
        return;
    }
    let x0 = p.iter().map(|q| q[0]).fold(f32::INFINITY, f32::min).floor().max(0.0) as usize;
    let x1 = (p.iter().map(|q| q[0]).fold(f32::NEG_INFINITY, f32::max).ceil()).clamp(0.0, sdim as f32) as usize;
    let y0 = p.iter().map(|q| q[1]).fold(f32::INFINITY, f32::min).floor().max(0.0) as usize;
    let y1 = (p.iter().map(|q| q[1]).fold(f32::NEG_INFINITY, f32::max).ceil()).clamp(0.0, sdim as f32) as usize;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let c = [lit[tri[0]], lit[tri[1]], lit[tri[2]]];
    let inv = 1.0 / area;
    for y in y0..y1 {
        let py = y as f32 + 0.5;
        for x in x0..x1 {
            let pxc = x as f32 + 0.5;
            let e = |a: [f32; 2], b: [f32; 2]| (b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (pxc - a[0]);
            let w = [e(p[1], p[2]) * inv, e(p[2], p[0]) * inv, e(p[0], p[1]) * inv];
            if w.iter().any(|&v| v < 0.0) {
                continue;
            }
            let iz = w[0] / z[0] + w[1] / z[1] + w[2] / z[2];
            if iz <= 0.0 {
                continue;
            }
            let d = 1.0 / iz;
            let at = y * sdim + x;
            if d >= zbuf[at] {
                continue;
            }
            zbuf[at] = d;
            let mut col = [0.0f32; 3];
            for k in 0..3 {
                col[k] = (w[0] * c[0][k] / z[0] + w[1] * c[1][k] / z[1] + w[2] * c[2][k] / z[2]) * d;
            }
            // Air. Measured against the camera's own reach rather than in absolute metres, so
            // a 400 m track and a 900 m one come out with the same amount of it.
            let t = smoothstep(((d - cam.reach) / (cam.reach * 2.0)).clamp(0.0, 1.0)) * 0.94;
            for k in 0..3 {
                px[at][k] = col[k] + (scene.haze[k] - col[k]) * t;
            }
        }
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square of ground with a jagged border, so the apron has something to smear if it is
    /// going to.
    fn scene(n: usize) -> (Vec<f32>, Vec<[f32; 3]>) {
        let heights = (0..n * n)
            .map(|i| {
                let (x, y) = (i % n, i / n);
                if x == 0 || y == 0 || x == n - 1 || y == n - 1 {
                    // The edge alternates hard, which is what used to be extruded to the
                    // horizon as a fan of ridges.
                    ((x + y) % 2) as f32 * 8.0
                } else {
                    0.0
                }
            })
            .collect();
        let albedo = (0..n * n)
            .map(|i| if i % 3 == 0 { [200.0, 60.0, 60.0] } else { [40.0, 40.0, 40.0] })
            .collect();
        (heights, albedo)
    }

    fn of<'a>(heights: &'a [f32], albedo: &'a [[f32; 3]], n: usize) -> Scene<'a> {
        Scene {
            gw: n,
            gh: n,
            mps: 1.0,
            heights,
            albedo,
            adim: n,
            focus: &[],
            sun: [0.0, 1.0, 0.0],
            sun_colour: [1.0, 1.0, 1.0],
            ambient: [0.0, 0.0, 0.0],
            zenith: [0.0, 0.0, 0.0],
            horizon: [0.0, 0.0, 0.0],
            haze: [0.0, 0.0, 0.0],
            tilt_deg: TILT_DEG,
        }
    }

    /// Up is up.
    ///
    /// It was not: the up vector was the cross product taken the wrong way round for a
    /// left-handed world, and every picture came out upside down — the near ground along the
    /// top and the sky underneath it.
    #[test]
    fn a_point_higher_up_lands_higher_in_the_frame() {
        let cam = at([0.0, 0.0, 0.0], (1.0, 0.0), TILT_DEG.to_radians(), 100.0, 0.3);
        let (low, _) = cam.project([0.0, 0.0, 0.0]);
        let (high, _) = cam.project([0.0, 10.0, 0.0]);
        assert!(high[1] > low[1], "{high:?} should be above {low:?}");
    }

    /// And right is right. The world is left-handed — looking along `+z` with `+y` up puts
    /// `+x` to the right — so a camera facing that way has to put it on the right of the
    /// picture. Cross it the other way and every track ships mirrored.
    #[test]
    fn a_point_to_the_right_lands_on_the_right() {
        let cam = at([0.0, 0.0, 100.0], (0.0, -1.0), 0.0, 100.0, 0.3);
        let (right, _) = cam.project([10.0, 0.0, 100.0]);
        assert!(right[0] > 0.0, "+x should be screen right, not {right:?}");
    }

    /// Past the terrain's edge the ground becomes country: level, and its own colour.
    ///
    /// Carrying the edge samples outwards instead — which is what clamping does — extrudes the
    /// border into ridges reaching the horizon, and they shade as bright streaks radiating out
    /// of the picture.
    #[test]
    fn the_apron_settles_to_open_country() {
        let n = 32;
        let (heights, albedo) = scene(n);
        let s = of(&heights, &albedo, n);
        let flat = level(&s);
        let country = border(&s);
        let span = (n - 1) as f32;
        // Two places well outside, nearest to two edge samples that disagree by the whole of
        // the border's swing. Out there they have to agree with each other.
        for z in [0.0, 1.0] {
            let h = height(&s, flat, span + APRON_LEVEL_M * 3.0, z);
            assert!((h - flat).abs() < 0.01, "apron at z={z} sits at {h}, not {flat}");
            let c = sample(&s, country, span + APRON_TINT_M * 3.0, z);
            assert!(
                (0..3).all(|k| (c[k] - country[k]).abs() < 0.01),
                "apron at z={z} is {c:?}, not {country:?}"
            );
        }
        // And on the terrain itself nothing has been touched.
        assert_eq!(height(&s, flat, 8.0, 8.0), heights[8 * n + 8]);
    }
}
