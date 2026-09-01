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

/**
 * Which way a triangle faces — the other half of "where does this land on the bike".
 *
 * A rear fender's outer skin and its underside are one panel with one name, and they routinely
 * unwrap onto one island. Knowing the part is `plastics` doesn't tell you which of the two you
 * are about to paint, and painting the wrong one puts artwork where nobody will ever see it.
 */
export type Facing = "top" | "under" | "edge";

/** A facing a region covers, where `both` means the outer skin and the underside share it. */
export type Face = Facing | "both";

/** Per-triangle flank codes, packed one byte each. */
const CENTRE = 0;
const LEFT = 1;
const RIGHT = 2;

/** Per-triangle facing codes, in their own numbering — see `EDGE`/`TOP`/`UNDER`. */
const EDGE = 0;
const TOP = 1;
const UNDER = 2;

/**
 * How steeply a triangle has to lie before it counts as facing up or down.
 *
 * A normal's y of ±0.3 is about 17° off vertical. Below that the triangle is effectively a
 * wall — the side of a shroud, the flank of a tank — and calling it a top or an underside
 * would put a word on screen that changes as the pointer slides along a curve.
 */
const FACE_TOLERANCE = 0.3;

/** One piece of bodywork: a mesh-group name, and where its triangles land on the sheet. */
export interface UvPart {
  /** Mesh-group name from the `.edf` — `shroud`, `frame.005`, `chain`. */
  label: string;
  /**
   * The mesh node the group sits on — `chassis`, `steer`, `fsusp`, `rsusp` — or null when the
   * group is spread over several.
   *
   * Worth carrying because a group name is whatever the author typed, and authors type
   * anything: the TM pack calls its plastic panels `Metal.NNN` and its metal ones
   * `Plastics.NNN`. The node is the bike's own part, so it still says *where* — a shroud is on
   * the chassis and a rear fender is on the rear suspension, however the group is named.
   */
  owner: string | null;
  /** Triangle corners in uv space, six numbers per triangle: u0,v0,u1,v1,u2,v2. */
  tris: Float32Array;
  /**
   * Where each triangle came from — two numbers each: the node's index in the array the
   * part was read from, and the triangle's index within that node.
   *
   * Carried so a region of the sheet can be pointed at on the *model*. Everything else here
   * describes the flat square, and the one question a flat square keeps failing to answer —
   * "which panel is this, really" — is answered by lighting the panel up on the bike.
   */
  src: Int32Array;
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
  /** Per-triangle facing codes, alongside `flanks` and null on the same terms. */
  faces: Uint8Array | null;
  /** The facings the part covers as a whole. Null when unknown. */
  face: Face | null;
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
 * 4% of the widest of the sheet's own triangles. A bike is a symmetric object with a seam down
 * the middle, and without a dead band that seam's triangles would be sorted left or right by
 * rounding error and report a side each time the pointer crossed it.
 *
 * The widest 99%, and of the sheet rather than of the model. Both halves of that are scars:
 *
 * - The TM MX 530 ships an `HJ` node whose 32768 vertices include six at `f32::MAX`. Taking
 *   the plain maximum made the dead band 1.4e37 wide, so every triangle on the bike read
 *   `centre` and the left/right answer was silently gone for the whole model. Six vertices out
 *   of thirty thousand are not how wide a motorcycle is; a high percentile says so and a
 *   maximum can't.
 * - Measuring the sheet rather than the model keeps a junk node the sheet never binds from
 *   being heard from at all.
 *
 * Still a fraction rather than a length in metres, so a 65 and a 450 are one shape at two sizes.
 */
function lateralTolerance(xs: Iterable<number[]>): number {
  const all: number[] = [];
  for (const run of xs) {
    for (const x of run) {
      // NaN would sort unpredictably and can't be a width; drop it rather than rank it.
      if (Number.isFinite(x)) all.push(Math.abs(x));
    }
  }
  if (!all.length) return 0;
  all.sort((a, b) => a - b);
  return all[Math.floor((all.length - 1) * 0.99)] * 0.04;
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

/** The same, for facing codes. */
function summariseFaces(faces: number[]): Face {
  let top = false;
  let under = false;
  for (const f of faces) {
    if (f === TOP) top = true;
    else if (f === UNDER) under = true;
    if (top && under) return "both";
  }
  return top ? "top" : under ? "under" : "edge";
}

/**
 * The per-triangle codes at a uv point, gathered across **every** part that covers it.
 *
 * Across every part, not the one that happened to win the pick, because a texel is worn by
 * whatever binds it. A bracket and the panel it bolts to regularly overlap on the sheet, and
 * answering from the bracket alone is how the editor came to name the far flank of a shroud:
 * the small part wins the pick by area, and its side is not the side of the panel under the
 * pointer. Painting there paints both, so both have to be in the answer.
 */
function codesAt(parts: UvPart[], u: number, v: number, field: "flanks" | "faces"): number[] {
  const out: number[] = [];
  for (const part of parts) {
    const codes = part[field];
    if (!codes) continue;
    if (u < part.minU || u > part.maxU || v < part.minV || v > part.maxV) continue;
    const { tris } = part;
    for (let i = 0; i < tris.length; i += 6) {
      if (inTriangle(u, v, tris[i], tris[i + 1], tris[i + 2], tris[i + 3], tris[i + 4], tris[i + 5]))
        out.push(codes[i / 6]);
    }
  }
  return out;
}

/**
 * The flank worn at a uv point — `both` when the two sides land on the same spot.
 *
 * That last case is the one worth having: a bike's flanks routinely share one region of the
 * sheet, so a decal placed off-centre there comes out at the mirrored spot on the far side.
 * The editor can't change that, and it can say so.
 */
export function sideAt(parts: UvPart[], u: number, v: number): Side | null {
  const codes = codesAt(parts, u, v, "flanks");
  return codes.length ? summarise(codes) : null;
}

/** Which way the surface at a uv point faces — `both` when a panel's two skins share it. */
export function faceAt(parts: UvPart[], u: number, v: number): Face | null {
  const codes = codesAt(parts, u, v, "faces");
  return codes.length ? summariseFaces(codes) : null;
}

/**
 * Move one triangle into the tile the sheet is actually sampled in, in place.
 *
 * Most bodywork is unwrapped inside the unit square and this does nothing to it. The number
 * plates are not: a side plate runs u -0.44..0.87 and every plate runs v 0.55..1.20, because
 * the sheet is a decal each plate tiles rather than a place on the model. Left raw, those
 * islands are drawn off the edge of the sheet and over the top of each other, so a plate sheet
 * is painted at coordinates the game never reads.
 *
 * The triangle is moved whole. Wrapping each corner on its own would tear any triangle that
 * crosses the seam into a band across the entire sheet — one corner at 0.98 and the next at
 * 0.02 describe a hair's breadth on the model and the full width of the square here. So each
 * corner is first pulled to the tile nearest the triangle's own centre, and only then is the
 * whole thing shifted down to the tile around the origin. A triangle that genuinely straddles
 * the seam still pokes out one side, which is correct — that is what it does on the bike.
 */
function untile(tri: Float32Array): void {
  for (let axis = 0; axis < 2; axis += 1) {
    const mid = (tri[axis] + tri[2 + axis] + tri[4 + axis]) / 3;
    for (let c = 0; c < 3; c += 1) {
      tri[c * 2 + axis] -= Math.round(tri[c * 2 + axis] - mid);
    }
    const shift = Math.floor(mid);
    if (shift) {
      tri[axis] -= shift;
      tri[2 + axis] -= shift;
      tri[4 + axis] -= shift;
    }
  }
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
 * same way as a canvas row and no flip belongs here. The 2D stage does show the sheet flipped,
 * but it flips these islands along with the pixels they describe, so both stay in texture
 * space and the parts named here are the parts the pointer is actually over.
 *
 * `assembled` asks for the left/right and top/underside answers as well. Both read a vertex's
 * position or normal as a statement about where it sits on the bike, which is only true once
 * the `.geom` has placed the parts into one frame about the mirror plane — an unassembled bike
 * is a pile of parts each in its own, and the sign of x there is noise. The backend reports
 * whether that happened; nothing here tries to guess it from the numbers.
 */
export function uvParts(
  nodes: EdfNode[],
  sheetName: string,
  opts?: { assembled?: boolean },
): UvPart[] {
  const want = sheetName.trim().toLowerCase();
  if (!want) return [];

  // A fraction of the model's width, so the dead band around the mirror plane scales with the
  // bike rather than being a number of metres — a 65 and a 450 are one shape at two sizes.
  const sided = !!opts?.assembled;

  const byLabel = new Map<string, number[]>();
  const srcByLabel = new Map<string, number[]>();
  // Triangle centroids on the x axis, held raw until every group has been read: the dead band
  // they are judged against is a fraction of the widest of them, so none can be judged until
  // all of them are in.
  const lateralByLabel = new Map<string, number[]>();
  const facesByLabel = new Map<string, number[]>();
  const nodesByLabel = new Map<string, Set<string>>();
  // One triangle's corners, reused: a bike is a few hundred thousand of them and a fresh
  // array each would be the only allocation in the loop.
  const tri = new Float32Array(6);
  for (let nodeIndex = 0; nodeIndex < nodes.length; nodeIndex += 1) {
    const node = nodes[nodeIndex];
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
      let src = srcByLabel.get(label);
      if (!src) {
        src = [];
        srcByLabel.set(label, src);
      }
      let owners = nodesByLabel.get(label);
      if (!owners) {
        owners = new Set();
        nodesByLabel.set(label, owners);
      }
      owners.add(node.name);
      let sides = lateralByLabel.get(label);
      if (!sides && sided) {
        sides = [];
        lateralByLabel.set(label, sides);
      }
      let faces = facesByLabel.get(label);
      if (!faces && sided) {
        faces = [];
        facesByLabel.set(label, faces);
      }
      // Normals are what say which way a surface looks, and a mesh is free to ship without
      // them. Without them the facing question goes unanswered rather than being answered from
      // the winding order, which flips with the exporter.
      const hasNormals = node.normals.length >= node.positions.length;
      const end = Math.min(start + count, triCount);
      for (let t = Math.max(0, start); t < end; t += 1) {
        src.push(nodeIndex, t);
        let x = 0;
        let ny = 0;
        for (let c = 0; c < 3; c += 1) {
          const v = node.indices[t * 3 + c];
          tri[c * 2] = node.uvs[v * 2];
          tri[c * 2 + 1] = node.uvs[v * 2 + 1];
          if (sides) x += node.positions[v * 3];
          if (faces && hasNormals) ny += node.normals[v * 3 + 1];
        }
        untile(tri);
        flat.push(tri[0], tri[1], tri[2], tri[3], tri[4], tri[5]);
        // The centroid, not every corner: a triangle with one vertex over the line is still on
        // the side the rest of it is, and a panel's inner edge is full of them.
        if (sides) sides.push(x / 3);
        if (faces) {
          const up = hasNormals ? ny / 3 : 0;
          faces.push(up > FACE_TOLERANCE ? TOP : up < -FACE_TOLERANCE ? UNDER : EDGE);
        }
      }
    }
  }

  // Now that the whole sheet has been read, the band is known — and with it, each side.
  //
  // Positive x is the bike's LEFT.
  //
  // This line has now been flipped twice, so the history is worth keeping. It read RIGHT on the
  // grounds that that was where a region of the sheet came out on the model, and the check
  // against the mesh's own group names — `clutch` and `gear` sit at positive x on a 2023
  // YZ450F and both are left-hand controls — was dismissed as the misleading one. Read off the
  // 2D sheet with the stage the right way up, the group names are what agrees: the side a part
  // is labelled is the side it is on. Changed on that reading (2026-08-18).
  //
  // Whichever way it ends up, it is one line, and the hover label and the flank wash both come
  // off it — so they can disagree with the bike, but never with each other.
  const tol = sided ? lateralTolerance(lateralByLabel.values()) : 0;
  const flanksByLabel = new Map<string, number[]>();
  for (const [label, xs] of lateralByLabel) {
    flanksByLabel.set(
      label,
      xs.map((x) => (x > tol ? LEFT : x < -tol ? RIGHT : CENTRE)),
    );
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
    const faces = facesByLabel.get(label);
    const owners = nodesByLabel.get(label);
    parts.push({
      label,
      // Only when it's the one answer. A group spread over three nodes has no single place to
      // point at, and naming the first would be picking one arbitrarily.
      owner: owners?.size === 1 ? [...owners][0] : null,
      tris: new Float32Array(flat),
      src: new Int32Array(srcByLabel.get(label) ?? []),
      flanks: sides ? new Uint8Array(sides) : null,
      side: sides ? summarise(sides) : null,
      faces: faces ? new Uint8Array(faces) : null,
      face: faces ? summariseFaces(faces) : null,
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
 * How much of the sheet a hover lights up on the model, as a fraction of it.
 *
 * Small enough to sit inside a shroud rather than swallow it, big enough to find on the bike
 * without hunting: about 40px of a 2048² sheet.
 */
const SPOT_RADIUS = 0.02;

/**
 * The triangles a hover lands on, as references into the nodes the parts were read from —
 * two numbers each, node index then triangle index.
 *
 * A patch around the point rather than the island it sits in. An island is the tempting
 * answer and the wrong one: uv islands routinely bridge the mirror plane — a seat and a front
 * fender are each one panel with both flanks in them — so lighting the island up would answer
 * "which side does this paint" with most of the bike. The patch is the honest version of the
 * question, and it moves as the pointer moves, which is what makes it readable as *here*.
 */
export function spotAt(parts: UvPart[], u: number, v: number): Int32Array | null {
  const out: number[] = [];
  const r2 = SPOT_RADIUS * SPOT_RADIUS;
  for (const part of parts) {
    if (
      u < part.minU - SPOT_RADIUS ||
      u > part.maxU + SPOT_RADIUS ||
      v < part.minV - SPOT_RADIUS ||
      v > part.maxV + SPOT_RADIUS
    )
      continue;
    const { tris, src } = part;
    for (let i = 0; i < tris.length; i += 6) {
      // Any corner inside the disc, or the point inside the triangle — the second case is what
      // keeps a triangle bigger than the spot from dropping out of its own highlight.
      let hit = inTriangle(u, v, tris[i], tris[i + 1], tris[i + 2], tris[i + 3], tris[i + 4], tris[i + 5]);
      for (let c = 0; !hit && c < 3; c += 1) {
        const du = tris[i + c * 2] - u;
        const dv = tris[i + c * 2 + 1] - v;
        hit = du * du + dv * dv <= r2;
      }
      if (hit) out.push(src[(i / 6) * 2], src[(i / 6) * 2 + 1]);
    }
  }
  return out.length ? new Int32Array(out) : null;
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

/**
 * The single triangle under a uv point, as a part in its own right.
 *
 * What the bucket fills. A mesh group is not one shape on the sheet — `shroud` is both flanks
 * and often several scattered islands — so a fill confined to the group floods panels the
 * press never pointed at. One triangle is the finest the model can describe, and it is the
 * unit the unwrapper actually laid down.
 *
 * Null when the point lands on no triangle of the part: between its islands, or in the slack
 * of its bounding box. The caller falls back to the whole part there, because a bucket that
 * silently does nothing reads as a broken tool.
 */
export function triangleAt(part: UvPart, u: number, v: number): UvPart | null {
  const { tris } = part;
  const count = Math.floor(tris.length / 6);
  for (let t = 0; t < count; t += 1) {
    const i = t * 6;
    if (!inTriangle(u, v, tris[i], tris[i + 1], tris[i + 2], tris[i + 3], tris[i + 4], tris[i + 5])) {
      continue;
    }
    const flanks = part.flanks ? Uint8Array.of(part.flanks[t]) : null;
    const faces = part.faces ? Uint8Array.of(part.faces[t]) : null;
    return {
      ...part,
      tris: tris.slice(i, i + 6),
      src: part.src.slice(t * 2, t * 2 + 2),
      flanks,
      faces,
      // One triangle sits on one side and faces one way, whatever the group as a whole does.
      side: flanks ? summarise([flanks[0]]) : part.side,
      face: faces ? summariseFaces([faces[0]]) : part.face,
      minU: Math.min(tris[i], tris[i + 2], tris[i + 4]),
      minV: Math.min(tris[i + 1], tris[i + 3], tris[i + 5]),
      maxU: Math.max(tris[i], tris[i + 2], tris[i + 4]),
      maxV: Math.max(tris[i + 1], tris[i + 3], tris[i + 5]),
    };
  }
  return null;
}

/**
 * The connected run of triangles containing a uv point, as a part in its own right.
 *
 * What a right-click with the bucket fills. An island is what the eye reads as "this panel":
 * exactly the triangles reachable through shared uv vertices, which is the same cut the
 * unwrapper made — a seam is a duplicated vertex. Filling one triangle at a time is right for
 * a decal edge and hopeless for a whole shroud, so both are offered.
 *
 * Null on the same terms as {@link triangleAt}, and for the same reason.
 */
export function islandAt(part: UvPart, u: number, v: number): UvPart | null {
  const { tris } = part;
  const count = Math.floor(tris.length / 6);
  let seed = -1;
  for (let t = 0; t < count && seed < 0; t += 1) {
    const i = t * 6;
    if (inTriangle(u, v, tris[i], tris[i + 1], tris[i + 2], tris[i + 3], tris[i + 4], tris[i + 5])) {
      seed = t;
    }
  }
  if (seed < 0) return null;

  // Quantised, because two nodes merged under one label hold their own copies of a shared edge
  // and those need not be bit-identical. 1e-5 of uv is a fiftieth of a texel on a 2048 sheet —
  // far below anything an unwrapper would call two places.
  const key = (i: number) => `${Math.round(tris[i] * 1e5)},${Math.round(tris[i + 1] * 1e5)}`;
  const byVertex = new Map<string, number[]>();
  for (let t = 0; t < count; t += 1) {
    for (let c = 0; c < 3; c += 1) {
      const k = key(t * 6 + c * 2);
      const run = byVertex.get(k);
      if (run) run.push(t);
      else byVertex.set(k, [t]);
    }
  }

  const taken = new Uint8Array(count);
  const queue = [seed];
  taken[seed] = 1;
  const picked: number[] = [];
  while (queue.length) {
    const t = queue.pop() as number;
    picked.push(t);
    for (let c = 0; c < 3; c += 1) {
      for (const n of byVertex.get(key(t * 6 + c * 2)) ?? []) {
        if (!taken[n]) {
          taken[n] = 1;
          queue.push(n);
        }
      }
    }
  }
  // The whole part is one island: hand it straight back rather than rebuilding a copy of it.
  if (picked.length === count) return part;
  picked.sort((a, b) => a - b);

  const out = new Float32Array(picked.length * 6);
  const src = new Int32Array(picked.length * 2);
  const flanks = part.flanks ? new Uint8Array(picked.length) : null;
  const faces = part.faces ? new Uint8Array(picked.length) : null;
  let minU = Infinity;
  let minV = Infinity;
  let maxU = -Infinity;
  let maxV = -Infinity;
  for (let n = 0; n < picked.length; n += 1) {
    const t = picked[n];
    for (let j = 0; j < 6; j += 1) {
      const value = tris[t * 6 + j];
      out[n * 6 + j] = value;
      if (j % 2 === 0) {
        if (value < minU) minU = value;
        if (value > maxU) maxU = value;
      } else {
        if (value < minV) minV = value;
        if (value > maxV) maxV = value;
      }
    }
    src[n * 2] = part.src[t * 2];
    src[n * 2 + 1] = part.src[t * 2 + 1];
    if (flanks && part.flanks) flanks[n] = part.flanks[t];
    if (faces && part.faces) faces[n] = part.faces[t];
  }

  return {
    ...part,
    tris: out,
    src,
    flanks,
    faces,
    // Summarised over the island, not inherited: half a shroud is one flank where the group
    // was "both", and what gets said should describe what was filled.
    side: flanks ? summarise(Array.from(flanks)) : part.side,
    face: faces ? summariseFaces(Array.from(faces)) : part.face,
    minU,
    minV,
    maxU,
    maxV,
  };
}

/**
 * A part's bounds in sheet pixels, rounded outwards.
 *
 * Outwards because this is what a redraw is told to catch up with: a box rounded inwards leaves
 * the half-pixel at the edge of a fill on the layer and missing from the composite.
 */
export function partBox(
  part: UvPart,
  width: number,
  height: number,
): { x: number; y: number; w: number; h: number } {
  const x = Math.max(0, Math.floor(part.minU * width));
  const y = Math.max(0, Math.floor(part.minV * height));
  return {
    x,
    y,
    w: Math.min(width, Math.ceil(part.maxU * width)) - x,
    h: Math.min(height, Math.ceil(part.maxV * height)) - y,
  };
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
  // All three zero is not "on every edge at once", it is a triangle with no area — three uv
  // corners collapsed onto a point or a line. Left as inside, such a triangle answers yes to
  // *every* point on the sheet, and meshes are full of them: the 2007 H85R's chassis carries
  // 327, the TM MX 530's `HJ` node 105. One is enough to make its part the answer everywhere —
  // the wrong panel named in the corner, the wrong flank, the wrong place lit up on the bike,
  // and a bucket fill clipped to a panel on the far side. A real triangle can put the point on
  // one of its edges, never on all three, so nothing legitimate is turned away here.
  if (!neg && !pos) return false;
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
  return partsAt(parts, u, v)[0] ?? null;
}

/**
 * Every part covering a uv point, most specific first.
 *
 * Parts genuinely share regions of a sheet — a bracket bolted through a shroud, a badge on a
 * tank — and one of them winning the pick silently is what makes the editor look like it is
 * describing the wrong panel. The list keeps `partAt`'s order, so the smallest is still the
 * one pointed at, while everything else it overlaps stays sayable.
 */
export function partsAt(parts: UvPart[], u: number, v: number): UvPart[] {
  const hits: UvPart[] = [];
  for (const part of parts) {
    if (u < part.minU || u > part.maxU || v < part.minV || v > part.maxV) continue;
    const { tris } = part;
    for (let i = 0; i < tris.length; i += 6) {
      if (inTriangle(u, v, tris[i], tris[i + 1], tris[i + 2], tris[i + 3], tris[i + 4], tris[i + 5])) {
        hits.push(part);
        break;
      }
    }
  }
  return hits.sort(
    (a, b) =>
      (a.maxU - a.minU) * (a.maxV - a.minV) - (b.maxU - b.minU) * (b.maxV - b.minV),
  );
}

/**
 * Biggest edge a wireframe is rasterised at.
 *
 * The wire is a guide blitted under the sheet, not something anyone inspects pixel for pixel,
 * and a 4096² raster per sheet costs 64MB to say the same thing this says in four.
 */
const MAX_WIRE = 1024;

/**
 * Warm for the bike's left, cool for its right — the flank wash the islands are filled with.
 *
 * The one thing the sheet itself will never tell you. A bike's two flanks routinely unwrap as
 * two copies of the same panel, same way up, same graphics, stacked one above the other: the
 * shroud, side panel and airbox of a 2023 YZ450F come out as one island whose top half is the
 * left of the bike and whose bottom half is the right, with nothing in the artwork marking
 * where one becomes the other. Picking the wrong copy is a coin flip, and the way you find out
 * is that the decal you spent an hour on is on the far side. So the answer goes on the sheet,
 * not into a word in the corner that has to be hunted for one texel at a time.
 *
 * Amber and blue rather than a red/green pair, so the two stay apart for the ~8% of riders who
 * can't tell those two colours from each other. Kept faint: this washes over somebody's
 * livery, and a guide that drowns the artwork is one they turn off.
 */
const FLANK_WASH: Record<number, string> = {
  [LEFT]: "hsla(28, 95%, 55%, 0.17)",
  [RIGHT]: "hsla(205, 95%, 60%, 0.17)",
  [CENTRE]: "hsla(0, 0%, 78%, 0.08)",
};

/**
 * The parts drawn as filled, outlined islands.
 *
 * This is the part an image editor cannot tell you: which region of a 2048² square is the
 * airbox and which is the rear fender. The fill says which *side* it is (see `FLANK_WASH`) and
 * the outline says which part, so the two questions are answered by one overlay rather than by
 * a toggle each.
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
    // One path per flank, filled with the nonzero rule: triangles that share an edge stop being
    // separate shapes, so the fill comes out flat instead of banded along every seam the way a
    // per-triangle fill would. Three paths rather than one leaves exactly one seam — the line
    // where the bike's left meets its right, which is the line this is drawn to show.
    if (part.flanks) {
      const paths = new Map<number, Path2D>();
      const { tris, flanks } = part;
      for (let i = 0; i < tris.length; i += 6) {
        const code = flanks[i / 6];
        let path = paths.get(code);
        if (!path) {
          path = new Path2D();
          paths.set(code, path);
        }
        path.moveTo(tris[i] * sx, tris[i + 1] * sy);
        path.lineTo(tris[i + 2] * sx, tris[i + 3] * sy);
        path.lineTo(tris[i + 4] * sx, tris[i + 5] * sy);
        path.closePath();
      }
      for (const [code, path] of paths) {
        ctx.fillStyle = FLANK_WASH[code] ?? FLANK_WASH[CENTRE];
        ctx.fill(path, "nonzero");
      }
    } else {
      // No sides to tell apart — an unassembled bike, or a piece of gear. The part's own hue
      // still separates it from its neighbours, which is what the fill was for before.
      ctx.fillStyle = `hsla(${part.hue}, 70%, 60%, 0.13)`;
      ctx.fill(partPath(part, sx, sy), "nonzero");
    }

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
