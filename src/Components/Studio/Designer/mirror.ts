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
 * mirror plane, and whichever triangle wears the reflected point says where it lands back on
 * the sheet.
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

export interface MirrorIndex {
  /** Welded corner positions, three numbers each. */
  points: Float64Array;
  /** Corner id → the corner at its reflection, or -1 where the model isn't symmetric there. */
  mirrorOf: Int32Array;
  /** Three corner ids per triangle. */
  tris: Int32Array;
  /** Six numbers per triangle — u0,v0,u1,v1,u2,v2 — in the same corner order as `tris`. */
  uvs: Float32Array;
  /** A triangle's three corner ids, sorted and joined → its index. */
  byCorners: Map<string, number>;
  /** uv cell → the triangles whose bounds reach it. */
  cells: Map<number, number[]>;
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

/** A hash grid over positions, for welding corners and for finding a reflection's twin. */
class PointGrid {
  private cells = new Map<string, number[]>();
  private xs: number[] = [];

  constructor(private eps: number) {}

  /**
   * The id of a corner within `eps` of this position, or -1.
   *
   * Probes the neighbouring cells as well as the point's own: two positions a hair apart can
   * still land either side of a cell boundary, and a weld that missed those would leave a
   * panel's shared edge as two edges that never find each other.
   */
  find(x: number, y: number, z: number): number {
    const c = this.eps;
    let best = -1;
    let bestDist = this.eps * this.eps;
    const i = Math.floor(x / c);
    const j = Math.floor(y / c);
    const k = Math.floor(z / c);
    for (let di = -1; di <= 1; di += 1) {
      for (let dj = -1; dj <= 1; dj += 1) {
        for (let dk = -1; dk <= 1; dk += 1) {
          for (const id of this.cells.get(`${i + di},${j + dj},${k + dk}`) ?? []) {
            const dx = this.xs[id * 3] - x;
            const dy = this.xs[id * 3 + 1] - y;
            const dz = this.xs[id * 3 + 2] - z;
            const d = dx * dx + dy * dy + dz * dz;
            if (d <= bestDist) {
              bestDist = d;
              best = id;
            }
          }
        }
      }
    }
    return best;
  }

  add(x: number, y: number, z: number): number {
    const id = this.xs.length / 3;
    this.xs.push(x, y, z);
    const key = `${Math.floor(x / this.eps)},${Math.floor(y / this.eps)},${Math.floor(z / this.eps)}`;
    const cell = this.cells.get(key);
    if (cell) cell.push(id);
    else this.cells.set(key, [id]);
    return id;
  }

  positions(): Float64Array {
    return Float64Array.from(this.xs);
  }
}

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
 * the same texture by definition, so anything the mirror could land on is already in `parts`,
 * and a bike's other hundred thousand vertices are not worth welding to find that out.
 *
 * Corners are welded by position rather than trusted as vertex indices, because a panel split
 * across two mesh nodes holds its own copy of the shared edge and the two need not be
 * bit-identical — and the reflected corner is almost always in a different node from the one
 * it came from.
 */
export function buildMirror(parts: UvPart[], nodes: EdfNode[]): MirrorIndex | null {
  if (!parts.length || !nodes.length) return null;

  // A tolerance in the model's own units, taken from its size — a 65 and a 450 are one shape
  // at two sizes, and a fixed figure would weld one of them and not the other. Junk vertices
  // are excluded the way `lateralTolerance` excludes them: a motorcycle is not 1e37 wide.
  let span = 0;
  for (const part of parts) {
    for (let i = 0; i < part.src.length; i += 2) {
      const node = nodes[part.src[i]];
      if (!node) continue;
      for (let c = 0; c < 3; c += 1) {
        const v = node.indices[part.src[i + 1] * 3 + c];
        const x = Math.abs(node.positions[v * 3]);
        if (Number.isFinite(x) && x > span && x < 1e3) span = x;
      }
    }
  }
  if (!span) return null;

  const grid = new PointGrid(Math.max(span * WELD, 1e-9));
  const tris: number[] = [];
  const uvs: number[] = [];
  const byCorners = new Map<string, number>();
  const cells = new Map<number, number[]>();

  for (const part of parts) {
    for (let n = 0; n < part.src.length / 2; n += 1) {
      const node = nodes[part.src[n * 2]];
      if (!node) continue;
      const t = part.src[n * 2 + 1];
      const corners: number[] = [];
      for (let c = 0; c < 3; c += 1) {
        const v = node.indices[t * 3 + c];
        const x = node.positions[v * 3];
        const y = node.positions[v * 3 + 1];
        const z = node.positions[v * 3 + 2];
        if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(z)) break;
        const found = grid.find(x, y, z);
        corners.push(found >= 0 ? found : grid.add(x, y, z));
      }
      if (corners.length < 3) continue;

      const key = [...corners].sort((a, b) => a - b).join(",");
      // Already carrying these three corners: the same surface reached through another label.
      // Keeping the first is enough — a second copy would answer identically.
      if (byCorners.has(key)) continue;

      const index = tris.length / 3;
      byCorners.set(key, index);
      tris.push(corners[0], corners[1], corners[2]);

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
    }
  }
  if (!tris.length) return null;

  const points = grid.positions();
  const mirrorOf = new Int32Array(points.length / 3).fill(-1);
  for (let id = 0; id < mirrorOf.length; id += 1) {
    mirrorOf[id] = grid.find(-points[id * 3], points[id * 3 + 1], points[id * 3 + 2]);
  }

  return { points, mirrorOf, tris: Int32Array.from(tris), uvs: Float32Array.from(uvs), byCorners, cells };
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
 * Where a point on the sheet comes out when reflected through the bike, or null.
 *
 * Null when the point is on no triangle at all, or when the reflected place is on a part with
 * no twin — a one-off bracket, an exhaust that only exists on one side.
 */
export function mirrorPoint(
  index: MirrorIndex,
  u: number,
  v: number,
): { u: number; v: number } | null {
  const { tris, uvs, mirrorOf, byCorners } = index;
  for (const t of index.cells.get(cell(u, v)) ?? []) {
    const i = t * 6;
    const bary = barycentric(u, v, uvs[i], uvs[i + 1], uvs[i + 2], uvs[i + 3], uvs[i + 4], uvs[i + 5]);
    if (!bary) continue;

    const twins = [mirrorOf[tris[t * 3]], mirrorOf[tris[t * 3 + 1]], mirrorOf[tris[t * 3 + 2]]];
    if (twins[0] < 0 || twins[1] < 0 || twins[2] < 0) continue;
    const far = byCorners.get([...twins].sort((a, b) => a - b).join(","));
    if (far === undefined) continue;

    // Matched by corner id rather than by position in the list: the far triangle holds the same
    // three corners in whatever order its own winding gave them, and reading its uvs off in the
    // wrong order is how a decal arrives on the right panel inside out.
    let outU = 0;
    let outV = 0;
    let matched = true;
    for (let c = 0; c < 3; c += 1) {
      const at = [0, 1, 2].find((k) => tris[far * 3 + k] === twins[c]);
      if (at === undefined) {
        matched = false;
        break;
      }
      outU += bary[c] * uvs[far * 6 + at * 2];
      outV += bary[c] * uvs[far * 6 + at * 2 + 1];
    }
    if (matched) return { u: outU, v: outV };
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

    return {
      ok: true,
      x: centre.u * sheet.width,
      y: centre.v * sheet.height,
      ...fieldsFrom(mul(fit, frameOf(layer)), mirrored(layer)),
      approximate: residual > RESIDUAL_LIMIT,
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
