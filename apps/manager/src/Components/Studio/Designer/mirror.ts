import type { EdfNode } from "../../../types";
import { mirrored, sheetRotation, type Layer, type Sheet } from "./layers";
import { partPath, sideAt, type UvPart } from "./uv";

/**
 * Reflecting a place on the sheet through the bike, rather than across the square.
 *
 * The tempting version of "mirror this decal" is to flip it about u = 0.5, and it is wrong
 * often enough to be useless: a bike's two flanks land wherever the unwrapper put them, which
 * is not a reflection of each other about the middle of the texture. The model is the only
 * thing that knows where the far side of a shroud actually is, so the question is asked of it —
 * a point on the sheet becomes a point on a triangle, that point is reflected about the bike's
 * mirror plane, and whatever surface lies under the reflected point says where it lands back
 * on the sheet.
 *
 * *Whatever surface lies under it*, not "the triangle whose corners are the mirrored corners".
 * A bike is only approximately symmetric: a seat, a tank or a fender is modelled once as one
 * piece, so its two halves are near-copies rather than vertex-exact mirrors — 2mm apart on a
 * stock CRF450R's seat — and the corner-matching version of this refused two thirds of that
 * bike's sheet. The far side is a surface, and the nearest point on it is the answer whether
 * or not a vertex happens to sit there.
 *
 * Everything here works in the frame `uvParts` does: assembled, with the mirror plane at x = 0.
 */

/** How far apart two corners can be and still be the same corner, as a fraction of the model. */
const WELD = 1e-5;

/**
 * Cells per axis in the uv lookup.
 *
 * A follower is re-derived on every pointer sample of a drag, and each re-derivation asks
 * where three points land. A linear scan of a bike's triangles per sample is what this is here
 * to avoid.
 */
const GRID = 48;

/**
 * How far off a reflected point a surface may lie and still be its far side, as a fraction of
 * the sheet's own diagonal — about 8mm on a 450. Doubles as the model-space cell size.
 *
 * A panel's far half is a continuous surface, so a reflection that belongs on it lands within
 * a hair of it however roughly the two halves were modelled. The band is here for the other
 * case: a part with no twin at all — a one-off bracket, an exhaust — whose reflection comes
 * out in mid-air.
 *
 * The diagonal rather than the half-width, which is what the weld goes by: a sheet can hold a
 * narrow thing — a pair of wheels is 120mm across and two metres long — and sizing the band
 * off the narrow axis would shrink it to a millimetre and the grid to a quarter-million cells.
 */
const REACH = 0.0035;

/**
 * How closely a candidate has to face the way the reflection does, as a dot product.
 *
 * Distance alone would answer an exhaust's reflection with the frame tube running behind it.
 * A panel's mirror image faces the mirrored way; whatever is stacked behind it does not.
 */
const FACING = 0.25;

/** A triangle reaching more model-space cells than this is checked on every lookup instead. */
const SPRAWL = 256;

export interface MirrorIndex {
  /** Nine numbers per triangle — its three corners in model space. */
  points: Float64Array;
  /** Six numbers per triangle — u0,v0,u1,v1,u2,v2 — in the same corner order as `points`. */
  uvs: Float32Array;
  /** Three numbers per triangle — the unit face normal, for the `FACING` test. */
  normals: Float64Array;
  /** uv cell → the triangles whose bounds reach it. */
  cells: Map<number, number[]>;
  /** Model-space cell → the triangles whose bounds reach it. */
  solid: Map<string, number[]>;
  /** Triangles too spread out to bucket, checked on every reflection instead. */
  sprawl: number[];
  /** `REACH` in the model's own units — the cell size, and the radius a far side is sought in. */
  reach: number;
}

/** What a mirror was asked for and couldn't give. Each reads as its own sentence to the user. */
export type MirrorRefusal = "no-model" | "shared" | "centre" | "asymmetric";

export interface MirrorPlacement {
  x: number;
  y: number;
  rotation: number;
  scale: number;
  flipX: boolean;
  flipY: boolean;
  /**
   * The far side's unwrap is not a clean reflection of this one, so this is the closest rigid
   * answer rather than an exact one. Said out loud rather than quietly applied.
   */
  approximate: boolean;
}

export type MirrorResult = ({ ok: true } & MirrorPlacement) | { ok: false; why: MirrorRefusal };

/** Which uv cell a point is in. Clamped, because uvs outside the square are legal and wrap. */
function cell(u: number, v: number): number {
  const i = Math.min(GRID - 1, Math.max(0, Math.floor(u * GRID)));
  const j = Math.min(GRID - 1, Math.max(0, Math.floor(v * GRID)));
  return j * GRID + i;
}

/**
 * Build the reflection index for one sheet.
 *
 * Over the sheet's own triangles rather than the whole model: the far side of a shroud wears
 * the same texture by definition, so anything the mirror could land on is already in `parts`.
 *
 * Two lookups come out of the same walk — uv → triangle for the question, and model space →
 * triangle for the answer.
 */
export function buildMirror(parts: UvPart[], nodes: EdfNode[]): MirrorIndex | null {
  if (!parts.length || !nodes.length) return null;

  // A tolerance in the model's own units, taken from its size — a 65 and a 450 are one shape
  // at two sizes, and a fixed figure would suit one of them and not the other. Junk vertices
  // are excluded the way `lateralTolerance` excludes them: a motorcycle is not 1e37 wide.
  let span = 0;
  const lo = [Infinity, Infinity, Infinity];
  const hi = [-Infinity, -Infinity, -Infinity];
  for (const part of parts) {
    for (let i = 0; i < part.src.length; i += 2) {
      const node = nodes[part.src[i]];
      if (!node) continue;
      for (let c = 0; c < 3; c += 1) {
        const v = node.indices[part.src[i + 1] * 3 + c];
        for (let k = 0; k < 3; k += 1) {
          const n = node.positions[v * 3 + k];
          if (!Number.isFinite(n) || Math.abs(n) >= 1e3) continue;
          if (n < lo[k]) lo[k] = n;
          if (n > hi[k]) hi[k] = n;
          if (!k && Math.abs(n) > span) span = Math.abs(n);
        }
      }
    }
  }
  const diagonal = Math.hypot(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]);
  if (!span || !diagonal) return null;

  const reach = diagonal * REACH;
  const eps = Math.max(span * WELD, 1e-9);
  const points: number[] = [];
  const uvs: number[] = [];
  const normals: number[] = [];
  const cells = new Map<number, number[]>();
  const solid = new Map<string, number[]>();
  const sprawl: number[] = [];
  // A triangle's three corners, quantised and sorted — the same surface reached through
  // another label answers identically, so the second copy is only work.
  const seen = new Set<string>();

  for (const part of parts) {
    for (let n = 0; n < part.src.length / 2; n += 1) {
      const node = nodes[part.src[n * 2]];
      if (!node) continue;
      const t = part.src[n * 2 + 1];
      const p: number[] = [];
      for (let c = 0; c < 3; c += 1) {
        const v = node.indices[t * 3 + c];
        const x = node.positions[v * 3];
        const y = node.positions[v * 3 + 1];
        const z = node.positions[v * 3 + 2];
        if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(z)) break;
        p.push(x, y, z);
      }
      if (p.length < 9) continue;

      const key = [0, 1, 2]
        .map((c) => `${Math.round(p[c * 3] / eps)},${Math.round(p[c * 3 + 1] / eps)},${Math.round(p[c * 3 + 2] / eps)}`)
        .sort()
        .join("|");
      if (seen.has(key)) continue;
      seen.add(key);

      const ax = p[3] - p[0];
      const ay = p[4] - p[1];
      const az = p[5] - p[2];
      const bx = p[6] - p[0];
      const by = p[7] - p[1];
      const bz = p[8] - p[2];
      let nx = ay * bz - az * by;
      let ny = az * bx - ax * bz;
      let nz = ax * by - ay * bx;
      const len = Math.hypot(nx, ny, nz);
      // A triangle with no area faces nowhere, and nothing can usefully land on it.
      if (!len) continue;
      nx /= len;
      ny /= len;
      nz /= len;

      const index = normals.length / 3;
      points.push(...p);
      normals.push(nx, ny, nz);

      let minU = Infinity;
      let minV = Infinity;
      let maxU = -Infinity;
      let maxV = -Infinity;
      for (let c = 0; c < 3; c += 1) {
        const u = part.tris[n * 6 + c * 2];
        const v = part.tris[n * 6 + c * 2 + 1];
        uvs.push(u, v);
        if (u < minU) minU = u;
        if (u > maxU) maxU = u;
        if (v < minV) minV = v;
        if (v > maxV) maxV = v;
      }
      const i0 = Math.min(GRID - 1, Math.max(0, Math.floor(minU * GRID)));
      const i1 = Math.min(GRID - 1, Math.max(0, Math.floor(maxU * GRID)));
      const j0 = Math.min(GRID - 1, Math.max(0, Math.floor(minV * GRID)));
      const j1 = Math.min(GRID - 1, Math.max(0, Math.floor(maxV * GRID)));
      for (let j = j0; j <= j1; j += 1) {
        for (let i = i0; i <= i1; i += 1) {
          const at = j * GRID + i;
          const run = cells.get(at);
          if (run) run.push(index);
          else cells.set(at, [index]);
        }
      }

      const gi0 = Math.floor(Math.min(p[0], p[3], p[6]) / reach);
      const gi1 = Math.floor(Math.max(p[0], p[3], p[6]) / reach);
      const gj0 = Math.floor(Math.min(p[1], p[4], p[7]) / reach);
      const gj1 = Math.floor(Math.max(p[1], p[4], p[7]) / reach);
      const gk0 = Math.floor(Math.min(p[2], p[5], p[8]) / reach);
      const gk1 = Math.floor(Math.max(p[2], p[5], p[8]) / reach);
      if ((gi1 - gi0 + 1) * (gj1 - gj0 + 1) * (gk1 - gk0 + 1) > SPRAWL) {
        sprawl.push(index);
        continue;
      }
      for (let i = gi0; i <= gi1; i += 1) {
        for (let j = gj0; j <= gj1; j += 1) {
          for (let k = gk0; k <= gk1; k += 1) {
            const at = `${i},${j},${k}`;
            const run = solid.get(at);
            if (run) run.push(index);
            else solid.set(at, [index]);
          }
        }
      }
    }
  }
  if (!normals.length) return null;

  return {
    points: Float64Array.from(points),
    uvs: Float32Array.from(uvs),
    normals: Float64Array.from(normals),
    cells,
    solid,
    sprawl,
    reach,
  };
}

/** Barycentric coordinates of a point in a uv triangle, or null when it falls outside. */
function barycentric(
  u: number,
  v: number,
  ax: number,
  ay: number,
  bx: number,
  by: number,
  cx: number,
  cy: number,
): [number, number, number] | null {
  const det = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy);
  if (!det) return null;
  const a = ((by - cy) * (u - cx) + (cx - bx) * (v - cy)) / det;
  const b = ((cy - ay) * (u - cx) + (ax - cx) * (v - cy)) / det;
  const c = 1 - a - b;
  // A hair of slack, so a point exactly on a shared edge belongs to one of the two triangles
  // rather than to neither.
  const e = -1e-6;
  return a >= e && b >= e && c >= e ? [a, b, c] : null;
}

/**
 * The closest point on a triangle to `p`, as its barycentric weights and a squared distance.
 *
 * Ericson's routine: the three corners' regions, then the three edges', and the face when none
 * of them claims the point.
 */
function closestOnTri(
  px: number,
  py: number,
  pz: number,
  ax: number,
  ay: number,
  az: number,
  bx: number,
  by: number,
  bz: number,
  cx: number,
  cy: number,
  cz: number,
): [number, number, number, number] {
  const abx = bx - ax;
  const aby = by - ay;
  const abz = bz - az;
  const acx = cx - ax;
  const acy = cy - ay;
  const acz = cz - az;
  const d1 = abx * (px - ax) + aby * (py - ay) + abz * (pz - az);
  const d2 = acx * (px - ax) + acy * (py - ay) + acz * (pz - az);
  let w0 = 1;
  let w1 = 0;
  let w2 = 0;
  if (d1 > 0 || d2 > 0) {
    const d3 = abx * (px - bx) + aby * (py - by) + abz * (pz - bz);
    const d4 = acx * (px - bx) + acy * (py - by) + acz * (pz - bz);
    const d5 = abx * (px - cx) + aby * (py - cy) + abz * (pz - cz);
    const d6 = acx * (px - cx) + acy * (py - cy) + acz * (pz - cz);
    const va = d3 * d6 - d5 * d4;
    const vb = d5 * d2 - d1 * d6;
    const vc = d1 * d4 - d3 * d2;
    if (d3 >= 0 && d4 <= d3) {
      w0 = 0;
      w1 = 1;
    } else if (d6 >= 0 && d5 <= d6) {
      w0 = 0;
      w2 = 1;
    } else if (vc <= 0 && d1 >= 0 && d3 <= 0) {
      const s = d1 / (d1 - d3);
      w0 = 1 - s;
      w1 = s;
    } else if (vb <= 0 && d2 >= 0 && d6 <= 0) {
      const s = d2 / (d2 - d6);
      w0 = 1 - s;
      w2 = s;
    } else if (va <= 0 && d4 - d3 >= 0 && d5 - d6 >= 0) {
      const s = (d4 - d3) / (d4 - d3 + (d5 - d6));
      w0 = 0;
      w1 = 1 - s;
      w2 = s;
    } else {
      const den = 1 / (va + vb + vc);
      w1 = vb * den;
      w2 = vc * den;
      w0 = 1 - w1 - w2;
    }
  }
  const dx = px - (ax * w0 + bx * w1 + cx * w2);
  const dy = py - (ay * w0 + by * w1 + cy * w2);
  const dz = pz - (az * w0 + bz * w1 + cz * w2);
  return [dx * dx + dy * dy + dz * dz, w0, w1, w2];
}

/**
 * Where the sheet's own surface lies nearest a point in model space, read back as a uv.
 *
 * `nx,ny,nz` is the way the surface being looked for should face — see `FACING`.
 */
function surfaceAt(
  index: MirrorIndex,
  px: number,
  py: number,
  pz: number,
  nx: number,
  ny: number,
  nz: number,
): { u: number; v: number } | null {
  const { points, uvs, normals, reach } = index;
  let best = reach * reach;
  let out: { u: number; v: number } | null = null;

  const consider = (t: number) => {
    if (normals[t * 3] * nx + normals[t * 3 + 1] * ny + normals[t * 3 + 2] * nz < FACING) return;
    const i = t * 9;
    const hit = closestOnTri(
      px, py, pz,
      points[i], points[i + 1], points[i + 2],
      points[i + 3], points[i + 4], points[i + 5],
      points[i + 6], points[i + 7], points[i + 8],
    );
    if (hit[0] >= best) return;
    // On the same side of the bike as the point being asked about. Without this, a decal near
    // the centre line answers with the surface it is already on.
    if ((points[i] * hit[1] + points[i + 3] * hit[2] + points[i + 6] * hit[3]) * px < 0) return;
    best = hit[0];
    const j = t * 6;
    out = {
      u: uvs[j] * hit[1] + uvs[j + 2] * hit[2] + uvs[j + 4] * hit[3],
      v: uvs[j + 1] * hit[1] + uvs[j + 3] * hit[2] + uvs[j + 5] * hit[3],
    };
  };

  const gi = Math.floor(px / reach);
  const gj = Math.floor(py / reach);
  const gk = Math.floor(pz / reach);
  for (let di = -1; di <= 1; di += 1) {
    for (let dj = -1; dj <= 1; dj += 1) {
      for (let dk = -1; dk <= 1; dk += 1) {
        for (const t of index.solid.get(`${gi + di},${gj + dj},${gk + dk}`) ?? []) consider(t);
      }
    }
  }
  for (const t of index.sprawl) consider(t);
  return out;
}

/**
 * Where a point on the sheet comes out when reflected through the bike, or null.
 *
 * Null when the point is on no triangle at all, or when the reflected place has no surface
 * under it — a one-off bracket, an exhaust that only exists on one side.
 */
export function mirrorPoint(
  index: MirrorIndex,
  u: number,
  v: number,
): { u: number; v: number } | null {
  const { points, uvs, normals } = index;
  for (const t of index.cells.get(cell(u, v)) ?? []) {
    const j = t * 6;
    const bary = barycentric(u, v, uvs[j], uvs[j + 1], uvs[j + 2], uvs[j + 3], uvs[j + 4], uvs[j + 5]);
    if (!bary) continue;
    const i = t * 9;
    const x = points[i] * bary[0] + points[i + 3] * bary[1] + points[i + 6] * bary[2];
    const y = points[i + 1] * bary[0] + points[i + 4] * bary[1] + points[i + 7] * bary[2];
    const z = points[i + 2] * bary[0] + points[i + 5] * bary[1] + points[i + 8] * bary[2];
    const far = surfaceAt(index, -x, y, z, -normals[t * 3], normals[t * 3 + 1], normals[t * 3 + 2]);
    if (far) return far;
  }
  return null;
}

/** A layer's content-to-sheet 2×2, flattened as [a, b, c, d] for [[a, b], [c, d]]. */
function frameOf(layer: Layer): [number, number, number, number] {
  const p = sheetRotation(layer);
  const sx = layer.scale * (layer.flipX ? -1 : 1);
  const sy = (mirrored(layer) ? -layer.scale : layer.scale) * (layer.flipY ? -1 : 1);
  return [Math.cos(p) * sx, -Math.sin(p) * sy, Math.sin(p) * sx, Math.cos(p) * sy];
}

/** Multiply two 2×2s, both flattened as [a, b, c, d]. */
function mul(
  m: [number, number, number, number],
  n: [number, number, number, number],
): [number, number, number, number] {
  return [
    m[0] * n[0] + m[1] * n[2],
    m[0] * n[1] + m[1] * n[3],
    m[2] * n[0] + m[3] * n[2],
    m[2] * n[1] + m[3] * n[3],
  ];
}

/**
 * The nearest similarity to a 2×2, and how far it had to move to get there.
 *
 * A rotation, a uniform scale and possibly a flip — nothing that could shear a logo. The far
 * flank should be a rigid reflection of this one, so anything the raw map has beyond that is
 * noise in the unwrap rather than something to reproduce. Both handednesses are fitted and
 * the closer one wins, which is also what decides the flip.
 */
function nearestSimilarity(m: [number, number, number, number]): {
  fit: [number, number, number, number];
  residual: number;
} {
  const [a, b, c, d] = m;
  // Same handedness: [[p, -q], [q, p]].
  const keep: [number, number, number, number] = [(a + d) / 2, -(c - b) / 2, (c - b) / 2, (a + d) / 2];
  // Turned over: [[p, q], [q, -p]].
  const flip: [number, number, number, number] = [(a - d) / 2, (c + b) / 2, (c + b) / 2, -(a - d) / 2];
  const err = (f: [number, number, number, number]) =>
    Math.hypot(f[0] - a, f[1] - b, f[2] - c, f[3] - d);
  const ek = err(keep);
  const ef = err(flip);
  return {
    fit: ek <= ef ? keep : flip,
    residual: Math.min(ek, ef) / (Math.hypot(a, b, c, d) || 1),
  };
}

/**
 * Read a similarity back out as the fields a layer actually stores — the inverse of `frameOf`.
 *
 * It has to be exactly that inverse. The fields carry two flips that multiply, the stage's and
 * the layer's (see `mirrored`), and anything that reasoned about "the" flip here would put a
 * mirrored logo on the far shroud upside down half the time.
 *
 * A reflection comes out as a turn plus a flip of the y axis. Choosing x instead would be the
 * same transform at a different angle, there is nothing to prefer, and a follower's fields are
 * never edited by hand.
 */
function fieldsFrom(
  m: [number, number, number, number],
  turns: boolean,
): { rotation: number; scale: number; flipX: boolean; flipY: boolean } {
  const [a, b, c, d] = m;
  const det = a * d - b * c;
  const scale = Math.sqrt(Math.abs(det));
  // With sx pinned to +scale, the angle is whatever the first column says and sy is whatever
  // the determinant needs it to be — which is the whole decomposition, in two lines.
  const sy = det < 0 ? -scale : scale;
  const base = turns ? -scale : scale;
  const p = Math.atan2(c, a);
  return { rotation: turns ? -p : p, scale, flipX: false, flipY: sy !== base };
}

/**
 * Offsets to sample the layer's own axes at, as a fraction of the sheet.
 *
 * Tried in turn: a short step still lands on the same panel where a long one has already run
 * off the island and onto something that is not the far side of anything.
 */
const PROBES = [0.03, 0.015, 0.006, 0.002];

/** Beyond this, the two islands are not reflections of each other and the fit is a compromise. */
const RESIDUAL_LIMIT = 0.2;

/**
 * Where the far-flank copy of `layer` goes, or why it can't go anywhere.
 *
 * Three points are mapped rather than one — the centre, and a step along each of the layer's
 * own axes — because a position alone says nothing about which way the far island runs. The two
 * steps are what give the angle, the size and the flip, and fitting a similarity to them is
 * what keeps a logo a logo rather than letting a slightly-off unwrap shear it.
 */
export function mirrorLayer(
  index: MirrorIndex | null,
  parts: UvPart[],
  layer: Layer,
  sheet: Sheet,
): MirrorResult {
  if (!index) return { ok: false, why: "no-model" };

  const cu = layer.x / sheet.width;
  const cv = layer.y / sheet.height;
  // What the sheet says about this spot, before any of the geometry below.
  //
  // `null` is the load-bearing case: the flank codes are exactly the model's statement that
  // its axes can be trusted, so without them there is no left and right to reflect between and
  // a reflection about x = 0 would be a confident invention.
  const side = sideAt(parts, cu, cv);
  if (!side) return { ok: false, why: "no-model" };
  // A region both flanks share already appears on both sides, and a second copy of a decal
  // there is not what anybody means by "mirror this".
  if (side === "both") return { ok: false, why: "shared" };
  if (side === "centre") return { ok: false, why: "centre" };

  const centre = mirrorPoint(index, cu, cv);
  if (!centre) return { ok: false, why: "asymmetric" };

  const rot = sheetRotation(layer);
  const ux = Math.cos(rot);
  const uy = Math.sin(rot);

  // The steadiest reading of the far island, over the steps tried below.
  let best: { fit: [number, number, number, number]; residual: number } | null = null;
  for (const probe of PROBES) {
    const d = Math.min(sheet.width, sheet.height) * probe;
    const along = mirrorPoint(
      index,
      (layer.x + d * ux) / sheet.width,
      (layer.y + d * uy) / sheet.height,
    );
    const across = mirrorPoint(
      index,
      (layer.x - d * uy) / sheet.width,
      (layer.y + d * ux) / sheet.height,
    );
    if (!along || !across) continue;

    // Where the layer's own two axes went, in sheet pixels. Undoing the layer's rotation turns
    // that into the map from sheet to sheet, which is what the far island actually is.
    const k: [number, number, number, number] = [
      ((along.u - centre.u) * sheet.width) / d,
      ((across.u - centre.u) * sheet.width) / d,
      ((along.v - centre.v) * sheet.height) / d,
      ((across.v - centre.v) * sheet.height) / d,
    ];
    const unrotate: [number, number, number, number] = [ux, uy, -uy, ux];
    const { fit, residual } = nearestSimilarity(mul(k, unrotate));
    // A probe that fell off the island gives a map that collapses everything to a point. That
    // is not an answer, it is a shorter step's turn.
    if (!fit.every(Number.isFinite) || fit.every((n) => n === 0)) continue;

    if (!best || residual < best.residual) best = { fit, residual };
    // A long step is the steadiest reading, so the first one that comes back a reflection wins
    // and the rest go untried. A shorter one is only reached for once this one has clearly run
    // off the island — which is what a seat does, its two flanks a strip apiece and neither of
    // them 3% of the sheet tall.
    if (residual <= RESIDUAL_LIMIT) break;
  }

  if (best) {
    return {
      ok: true,
      x: centre.u * sheet.width,
      y: centre.v * sheet.height,
      ...fieldsFrom(mul(best.fit, frameOf(layer)), mirrored(layer)),
      approximate: best.residual > RESIDUAL_LIMIT,
    };
  }

  // The centre mapped and its neighbours didn't: the layer sits on a sliver with no room to
  // read a direction off. Place it where the centre landed, as it is, and say so.
  return {
    ok: true,
    x: centre.u * sheet.width,
    y: centre.v * sheet.height,
    rotation: layer.rotation,
    scale: layer.scale,
    flipX: layer.flipX,
    flipY: layer.flipY,
    approximate: true,
  };
}

/**
 * A follower brought back into step with the layer it reflects.
 *
 * Everything but the placement is copied outright — the artwork, the colours, the blend, the
 * part it is clipped to, whether it is hidden. A follower is not a variant of its source; it is
 * the same artwork seen from the other side of the bike, and every field that could drift is a
 * way for it to stop being that.
 *
 * `placement` is null when the model can no longer say where the far side is. The follower then
 * keeps the position it already had — a guess dressed up as a placement is worse than a stale
 * one, because only one of the two looks wrong.
 *
 * The clip travels by name and is rebuilt here rather than shared: a `Path2D` is built at one
 * sheet's size, and handing over the source's copy would pin the follower to a path built for
 * a sheet it might not be on.
 */
export function derive(
  follower: Layer,
  source: Layer,
  placement: MirrorPlacement | null,
  parts: UvPart[],
  sheet: Sheet,
): Layer {
  const part = source.clip ? parts.find((p) => p.label === source.clip?.label) : null;
  // The part not being found means the model isn't loaded yet, not that the source stopped
  // being clipped — so the follower keeps the path it already had rather than flashing
  // unclipped until the geometry turns up.
  const clip = source.clip ? (part ? { label: part.label, path: partPath(part, sheet.width, sheet.height) } : follower.clip) : null;
  return {
    ...source,
    id: follower.id,
    name: follower.name,
    group: follower.group,
    mirror: follower.mirror,
    clip,
    x: placement ? placement.x : follower.x,
    y: placement ? placement.y : follower.y,
    rotation: placement ? placement.rotation : follower.rotation,
    scale: placement ? placement.scale : follower.scale,
    flipX: placement ? placement.flipX : follower.flipX,
    flipY: placement ? placement.flipY : follower.flipY,
  };
}
