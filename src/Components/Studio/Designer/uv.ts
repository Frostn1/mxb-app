import type { EdfNode } from "../../../types";

/**
 * The model's bodywork, expressed in texture space.
 *
 * A sheet is a flat square; the bike is not. Everything here exists to carry the one fact that
 * connects them — which region of the square is the shroud — from the mesh the preview already
 * loaded into the editor, where it can be drawn, pointed at, fitted to and clipped against.
 *
 * Parts are held in uv (0–1), never in sheet pixels. The same list then describes a 512² sheet
 * and a 2048² one, and nothing has to be rebuilt when a sheet is a different size than the one
 * the parts were read for.
 */

/**
 * Which flank of the bike a triangle sits on. `centre` straddles the mirror plane — a front
 * fender, the seat's spine — and belongs to neither.
 */
export type Flank = "left" | "right" | "centre";

/** A flank a region covers, where `both` means the two flanks share it. */
export type Side = Flank | "both";

/** Per-triangle flank codes, packed one byte each. */
const CENTRE = 0;
const LEFT = 1;
const RIGHT = 2;

/** One piece of bodywork: a mesh-group name, and where its triangles land on the sheet. */
export interface UvPart {
  /** Mesh-group name from the `.edf` — `shroud`, `frame.005`, `chain`. */
  label: string;
  /** Triangle corners in uv space, six numbers per triangle: u0,v0,u1,v1,u2,v2. */
  tris: Float32Array;
  /**
   * One flank code per triangle, or null when the mesh's axes can't be trusted.
   *
   * Kept per triangle rather than per part because the question is asked about a *point*: the
   * two flanks of a shroud regularly land on one island, and a part-wide answer would have to
   * say "both" everywhere the precise answer is "here, both".
   */
  flanks: Uint8Array | null;
  /** The flanks the part covers as a whole, for a one-word summary. Null when unknown. */
  side: Side | null;
  /** uv-space bounds, for fitting an image to the part and for cheap hit rejection. */
  minU: number;
  minV: number;
  maxU: number;
  maxV: number;
  /** Stable hue, so a shroud and a fender never read as one shape. */
  hue: number;
}

/**
 * How far from the mirror plane a triangle has to sit before it counts as being on a side.
 *
 * 4% of the model's half-width. A bike is a symmetric object with a seam down the middle, and
 * without a dead band that seam's triangles would be sorted left or right by rounding error and
 * report a side each time the pointer crossed it.
 */
function lateralTolerance(nodes: EdfNode[]): number {
  let maxAbs = 0;
  for (const node of nodes) {
    for (let i = 0; i < node.positions.length; i += 3) {
      const x = Math.abs(node.positions[i]);
      if (x > maxAbs) maxAbs = x;
    }
  }
  return maxAbs * 0.04;
}

/** What a run of flank codes amounts to: one side, both of them, or neither. */
function summarise(sides: number[]): Side {
  let left = false;
  let right = false;
  for (const s of sides) {
    if (s === LEFT) left = true;
    else if (s === RIGHT) right = true;
    if (left && right) return "both";
  }
  return left ? "left" : right ? "right" : "centre";
}

/**
 * The flank of `part` under a uv point — `both` when the two sides land on the same island.
 *
 * That last case is the one worth having: a bike's flanks routinely share one region of the
 * sheet, so a decal placed off-centre there comes out at the mirrored spot on the far side.
 * The editor can't change that, and it can say so.
 */
export function flankAt(part: UvPart, u: number, v: number): Side | null {
  const { tris, flanks } = part;
  if (!flanks) return null;
  let left = false;
  let right = false;
  let centre = false;
  for (let i = 0; i < tris.length; i += 6) {
    if (!inTriangle(u, v, tris[i], tris[i + 1], tris[i + 2], tris[i + 3], tris[i + 4], tris[i + 5]))
      continue;
    const s = flanks[i / 6];
    if (s === LEFT) left = true;
    else if (s === RIGHT) right = true;
    else centre = true;
    if (left && right) return "both";
  }
  if (left) return "left";
  if (right) return "right";
  return centre ? "centre" : null;
}

/** A stable hue per mesh-group name. */
function hueOf(label: string): number {
  let h = 0;
  for (let i = 0; i < label.length; i += 1) h = (h * 31 + label.charCodeAt(i)) % 360;
  return h;
}

/**
 * The bodywork across `nodes` that is textured by `sheetName`, grouped by mesh-group name.
 *
 * Submeshes first and the node's own texture only as a fallback, which is the same order
 * `ModelViewer` binds materials in — the parts found here are then the parts the preview beside
 * them actually textures, rather than a second opinion about the same mesh.
 *
 * Groups sharing a name are merged across nodes. A bike's bodywork is regularly split into
 * several mesh nodes that all call themselves `shroud`, and to someone picking a part to drop a
 * photo onto, that is one shroud — three entries with the same name and different halves of the
 * answer would be a worse list than no list.
 *
 * uv0 is taken as-is. The sheet uploads with `flipY = false` (see `sheetTexture`), so v runs the
 * same way as a canvas row and no flip belongs here — the same reasoning that keeps the editor
 * from having an opinion about which way up a sheet is.
 *
 * `flanks` asks for the left/right answer as well, and is for bikes only: their positions arrive
 * assembled and centred on the mirror plane, which is what makes the sign of x mean a side.
 */
export function uvParts(
  nodes: EdfNode[],
  sheetName: string,
  opts?: { flanks?: boolean },
): UvPart[] {
  const want = sheetName.trim().toLowerCase();
  if (!want) return [];

  // A fraction of the model's width, so the dead band around the mirror plane scales with the
  // bike rather than being a number of metres — a 65 and a 450 are one shape at two sizes.
  const sided = !!opts?.flanks;
  const tol = sided ? lateralTolerance(nodes) : 0;

  const byLabel = new Map<string, number[]>();
  const flanksByLabel = new Map<string, number[]>();
  for (const node of nodes) {
    if (!node.uvs.length || !node.indices.length) continue;
    const triCount = Math.floor(node.indices.length / 3);
    const groups = node.submeshes.length
      ? node.submeshes
          .filter((sm) => sm.texture?.toLowerCase() === want)
          .map((sm) => ({ label: sm.name, start: sm.triStart, count: sm.triCount }))
      : node.texture?.toLowerCase() === want
        ? [{ label: node.name, start: 0, count: triCount }]
        : [];

    for (const { label, start, count } of groups) {
      let flat = byLabel.get(label);
      if (!flat) {
        flat = [];
        byLabel.set(label, flat);
      }
      let sides = flanksByLabel.get(label);
      if (!sides && sided) {
        sides = [];
        flanksByLabel.set(label, sides);
      }
      const end = Math.min(start + count, triCount);
      for (let t = Math.max(0, start); t < end; t += 1) {
        let x = 0;
        for (let c = 0; c < 3; c += 1) {
          const v = node.indices[t * 3 + c];
          flat.push(node.uvs[v * 2], node.uvs[v * 2 + 1]);
          if (sides) x += node.positions[v * 3];
        }
        // The centroid, not every corner: a triangle with one vertex over the line is still on
        // the side the rest of it is, and a panel's inner edge is full of them.
        if (sides) sides.push(x / 3 > tol ? LEFT : x / 3 < -tol ? RIGHT : CENTRE);
      }
    }
  }

  const parts: UvPart[] = [];
  for (const [label, flat] of byLabel) {
    if (!flat.length) continue;
    let minU = Infinity;
    let minV = Infinity;
    let maxU = -Infinity;
    let maxV = -Infinity;
    for (let i = 0; i < flat.length; i += 2) {
      if (flat[i] < minU) minU = flat[i];
      if (flat[i] > maxU) maxU = flat[i];
      if (flat[i + 1] < minV) minV = flat[i + 1];
      if (flat[i + 1] > maxV) maxV = flat[i + 1];
    }
    const sides = flanksByLabel.get(label);
    parts.push({
      label,
      tris: new Float32Array(flat),
      flanks: sides ? new Uint8Array(sides) : null,
      side: sides ? summarise(sides) : null,
      minU,
      minV,
      maxU,
      maxV,
      hue: hueOf(label),
    });
  }
  // Biggest first: the list is a menu, and the panel someone means when they say "the shroud"
  // is the one that takes up half the sheet, not the bracket bolted behind it.
  parts.sort((a, b) => (b.maxU - b.minU) * (b.maxV - b.minV) - (a.maxU - a.minU) * (a.maxV - a.minV));
  return parts;
}

/**
 * A part's outline as a path in sheet pixels, for clipping a layer to it.
 *
 * Built once and kept on the layer rather than per draw: the composite runs on every pointer
 * sample of a stroke, and rebuilding a few thousand triangles inside it would make painting on
 * a clipped layer slower the more carefully the mesh was modelled.
 */
export function partPath(part: UvPart, width: number, height: number): Path2D {
  const path = new Path2D();
  const { tris } = part;
  for (let i = 0; i < tris.length; i += 6) {
    path.moveTo(tris[i] * width, tris[i + 1] * height);
    path.lineTo(tris[i + 2] * width, tris[i + 3] * height);
    path.lineTo(tris[i + 4] * width, tris[i + 5] * height);
    path.closePath();
  }
  return path;
}

/** Whether (u,v) is inside a triangle, by the sign of the three edge cross-products. */
function inTriangle(
  u: number,
  v: number,
  ax: number,
  ay: number,
  bx: number,
  by: number,
  cx: number,
  cy: number,
): boolean {
  const d1 = (u - bx) * (ay - by) - (ax - bx) * (v - by);
  const d2 = (u - cx) * (by - cy) - (bx - cx) * (v - cy);
  const d3 = (u - ax) * (cy - ay) - (cx - ax) * (v - ay);
  // Mixed signs mean outside. Zeroes are on an edge and count as inside, which is what makes
  // the seam between two triangles of one part hit rather than fall through.
  const neg = d1 < 0 || d2 < 0 || d3 < 0;
  const pos = d1 > 0 || d2 > 0 || d3 > 0;
  return !(neg && pos);
}

/**
 * The part under a uv point, or null.
 *
 * Bounds first, triangles only for the parts that survive it — a point usually lands inside one
 * or two boxes, so the exact test runs over a fraction of the bike. Exact rather than a
 * rasterised lookup table, because a pick map has a resolution and this is asked while the
 * pointer moves across a seam, which is precisely where a low-res answer would be wrong.
 *
 * The *smallest* part that contains the point wins, not the first one found. Panels overlap in
 * texture space — a number board laid inside the shroud's island, a badge on the tank — and the
 * small one is always the one being pointed at: the big one is reachable everywhere else it
 * covers, while the small one would be reachable nowhere. Chosen by area rather than by list
 * order so it cannot be silently undone by sorting the parts differently.
 */
export function partAt(parts: UvPart[], u: number, v: number): UvPart | null {
  let best: UvPart | null = null;
  let bestArea = Infinity;
  for (const part of parts) {
    if (u < part.minU || u > part.maxU || v < part.minV || v > part.maxV) continue;
    const area = (part.maxU - part.minU) * (part.maxV - part.minV);
    if (area >= bestArea) continue;
    const { tris } = part;
    for (let i = 0; i < tris.length; i += 6) {
      if (inTriangle(u, v, tris[i], tris[i + 1], tris[i + 2], tris[i + 3], tris[i + 4], tris[i + 5])) {
        best = part;
        bestArea = area;
        break;
      }
    }
  }
  return best;
}

/**
 * Biggest edge a wireframe is rasterised at.
 *
 * The wire is a guide blitted under the sheet, not something anyone inspects pixel for pixel,
 * and a 4096² raster per sheet costs 64MB to say the same thing this says in four.
 */
const MAX_WIRE = 1024;

/**
 * The parts drawn as filled, outlined islands.
 *
 * This is the part an image editor cannot tell you: which region of a 2048² square is the
 * airbox and which is the rear fender.
 *
 * Coordinates outside 0–1 are drawn and clipped rather than wrapped: the tiled case is rare on
 * bodywork, and a guide that silently folded a decal back over itself would be worse than one
 * that stops at the edge.
 */
export function uvWireframe(
  parts: UvPart[],
  width: number,
  height: number,
): HTMLCanvasElement | null {
  if (!parts.length) return null;

  const k = Math.min(1, MAX_WIRE / Math.max(width, height));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(width * k));
  canvas.height = Math.max(1, Math.round(height * k));
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;

  const sx = canvas.width;
  const sy = canvas.height;
  ctx.lineJoin = "round";
  ctx.lineWidth = Math.max(1, Math.round(Math.max(sx, sy) / 512));

  for (const part of parts) {
    // One path for the whole part, filled with the nonzero rule: triangles that share an edge
    // stop being separate shapes, so the fill comes out flat instead of banded along every seam
    // the way a per-triangle fill would.
    const fill = partPath(part, sx, sy);
    ctx.fillStyle = `hsla(${part.hue}, 70%, 60%, 0.13)`;
    ctx.fill(fill, "nonzero");

    // Edges once each. A closed mesh shares almost every edge between two triangles, and
    // drawing both makes the interior twice as bright as the outline that matters.
    const seen = new Set<string>();
    const edges = new Path2D();
    const { tris } = part;
    for (let i = 0; i < tris.length; i += 6) {
      for (let e = 0; e < 3; e += 1) {
        const p = i + e * 2;
        const q = i + ((e + 1) % 3) * 2;
        // Keyed on the coordinates, since the parts hold corners rather than the vertex
        // indices they were read from. Rounded so two corners that agree to within a
        // ten-thousandth of the sheet count as the shared edge they are.
        const a = `${tris[p].toFixed(4)},${tris[p + 1].toFixed(4)}`;
        const b = `${tris[q].toFixed(4)},${tris[q + 1].toFixed(4)}`;
        const key = a < b ? `${a}|${b}` : `${b}|${a}`;
        if (seen.has(key)) continue;
        seen.add(key);
        edges.moveTo(tris[p] * sx, tris[p + 1] * sy);
        edges.lineTo(tris[q] * sx, tris[q + 1] * sy);
      }
    }
    ctx.strokeStyle = `hsla(${part.hue}, 85%, 72%, 0.85)`;
    ctx.stroke(edges);
  }

  return canvas;
}
