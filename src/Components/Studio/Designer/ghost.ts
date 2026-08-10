import type { EdfNode } from "../../../types";

/**
 * The reference underlay: what a sheet is being drawn *against*, as opposed to what it holds.
 *
 * A livery is drawn flat and worn curved, and the flat version gives away almost nothing about
 * which rectangle of it ends up on a shroud. There are two ways to answer that, and this holds
 * both: the paint you started from, and the model's own UV islands.
 *
 * None of it is part of the sheet. It is never composited, never staged and never saved —
 * `Sheet` is what the `.pnt` will contain, and mixing a guide into it is how a guide ends up
 * shipped inside somebody's livery. The 2D stage draws these separately, which makes that
 * impossible rather than merely unlikely.
 */
export interface Ghost {
  /**
   * The donor paint's pixels, lifted out of `Sheet.base`.
   *
   * Moving it here rather than copying it is the point: a template that is both the ghost and
   * the base would be traced over *and* saved, which is exactly the thing you were trying not
   * to do when you asked for a tracing guide.
   */
  template: ImageBitmap | null;
  /** The UV islands for this sheet's texture name, rasterised once by {@link uvWireframe}. */
  wire: HTMLCanvasElement | null;
  /**
   * The sheet name `wire` was built for, or null if it has never been built.
   *
   * The name is the whole binding — rename the sheet and the islands it maps to are different
   * ones — so this is what says whether the cached raster still describes the sheet.
   */
  wireFor: string | null;
  showTemplate: boolean;
  showWire: boolean;
  /** 0–1, applied to both. */
  opacity: number;
}

export const EMPTY_GHOST: Ghost = {
  template: null,
  wire: null,
  wireFor: null,
  // On by default so lifting a template into the ghost shows it immediately; the UV map is
  // off because it has to be built, and building one for a sheet nobody asked about is work
  // spent on a guide that was never going to be looked at.
  showTemplate: true,
  showWire: false,
  opacity: 0.35,
};

/** True when there is anything to draw — the stage skips the whole pass otherwise. */
export function ghostShows(ghost: Ghost | null | undefined): boolean {
  if (!ghost || ghost.opacity <= 0) return false;
  return (ghost.showTemplate && !!ghost.template) || (ghost.showWire && !!ghost.wire);
}

/**
 * Biggest edge a wireframe is rasterised at.
 *
 * The wire is a guide blitted under the sheet, not something anyone inspects pixel for pixel,
 * and a 4096² raster per sheet costs 64MB to say the same thing this says in four.
 */
const MAX_WIRE = 1024;

/** A group of triangles that share a name — one piece of bodywork, as the mesh files it. */
interface Island {
  label: string;
  /** uv0 for the whole node, indexed by the entries in `tris`. */
  uvs: number[];
  /** Flat vertex indices, three per triangle. */
  tris: number[];
}

/**
 * The triangles across `nodes` that are textured by `sheetName`, grouped by mesh-group name.
 *
 * Submeshes first and the node's own texture only as a fallback, which is the same order
 * `ModelViewer` binds materials in — the islands drawn here are then the islands the preview
 * beside them actually textures, rather than a second opinion about the same mesh.
 */
function islandsFor(nodes: EdfNode[], sheetName: string): Island[] {
  const want = sheetName.trim().toLowerCase();
  if (!want) return [];
  const islands: Island[] = [];

  for (const node of nodes) {
    if (!node.uvs.length || !node.indices.length) continue;
    const groups = node.submeshes.length
      ? node.submeshes
          .filter((sm) => sm.texture?.toLowerCase() === want)
          .map((sm) => ({ label: sm.name, start: sm.triStart, count: sm.triCount }))
      : node.texture?.toLowerCase() === want
        ? [{ label: node.name, start: 0, count: node.indices.length / 3 }]
        : [];

    for (const { label, start, count } of groups) {
      const tris: number[] = [];
      const end = Math.min(start + count, node.indices.length / 3);
      for (let t = Math.max(0, start); t < end; t += 1) {
        tris.push(node.indices[t * 3], node.indices[t * 3 + 1], node.indices[t * 3 + 2]);
      }
      if (tris.length) islands.push({ label, uvs: node.uvs, tris });
    }
  }
  return islands;
}

/** A stable hue per mesh-group name, so a shroud and a fender never read as one shape. */
function hueOf(label: string): number {
  let h = 0;
  for (let i = 0; i < label.length; i += 1) h = (h * 31 + label.charCodeAt(i)) % 360;
  return h;
}

/**
 * The model's UV layout for one sheet, drawn as filled, outlined islands.
 *
 * This is the part an image editor cannot tell you: which region of a 2048² square is the
 * airbox and which is the rear fender. Every triangle that samples `sheetName` is projected
 * into texture space and drawn where it lands.
 *
 * Returns null when nothing on the model binds this name — worth saying out loud, because a
 * sheet named something the mesh never asks for is a paint that loads and shows nothing, and
 * an empty overlay would look identical to a model that simply hadn't finished loading.
 *
 * uv0 is used as-is. The sheet is uploaded with `flipY = false` (see `sheetTexture`), so v runs
 * the same way as a canvas row and no flip belongs here — the same reasoning that keeps the
 * editor from having an opinion about which way up a sheet is. Coordinates outside 0–1 are
 * drawn and clipped rather than wrapped: the tiled case is rare on bodywork, and a guide that
 * silently folded a decal back over itself would be worse than one that stops at the edge.
 */
export function uvWireframe(
  nodes: EdfNode[],
  sheetName: string,
  width: number,
  height: number,
): HTMLCanvasElement | null {
  const islands = islandsFor(nodes, sheetName);
  if (!islands.length) return null;

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

  for (const island of islands) {
    const hue = hueOf(island.label);

    // One path for the whole island, filled with the nonzero rule: triangles that share an
    // edge stop being separate shapes, so the fill comes out flat instead of banded along
    // every seam the way a per-triangle fill would.
    const fill = new Path2D();
    for (let i = 0; i < island.tris.length; i += 3) {
      const [a, b, c] = [island.tris[i], island.tris[i + 1], island.tris[i + 2]];
      fill.moveTo(island.uvs[a * 2] * sx, island.uvs[a * 2 + 1] * sy);
      fill.lineTo(island.uvs[b * 2] * sx, island.uvs[b * 2 + 1] * sy);
      fill.lineTo(island.uvs[c * 2] * sx, island.uvs[c * 2 + 1] * sy);
      fill.closePath();
    }
    ctx.fillStyle = `hsla(${hue}, 70%, 60%, 0.13)`;
    ctx.fill(fill, "nonzero");

    // Edges once each. A closed mesh shares almost every edge between two triangles, and
    // drawing both makes the interior twice as bright as the outline that matters.
    const seen = new Set<number>();
    const edges = new Path2D();
    for (let i = 0; i < island.tris.length; i += 3) {
      const tri = [island.tris[i], island.tris[i + 1], island.tris[i + 2]];
      for (let e = 0; e < 3; e += 1) {
        const p = tri[e];
        const q = tri[(e + 1) % 3];
        // A single number rather than a string: this runs over every edge of every triangle
        // on the bike, and `${min}-${max}` allocates a string for each one. The shift leaves
        // room for four million vertices in a node and still lands well inside an exact
        // integer, so two different edges cannot come out as the same key.
        const key = Math.min(p, q) * 0x400000 + Math.max(p, q);
        if (seen.has(key)) continue;
        seen.add(key);
        edges.moveTo(island.uvs[p * 2] * sx, island.uvs[p * 2 + 1] * sy);
        edges.lineTo(island.uvs[q * 2] * sx, island.uvs[q * 2 + 1] * sy);
      }
    }
    ctx.strokeStyle = `hsla(${hue}, 85%, 72%, 0.85)`;
    ctx.stroke(edges);
  }

  return canvas;
}
