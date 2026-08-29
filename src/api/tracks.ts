import { invoke } from "@tauri-apps/api/core";
import type {
  TrackInfo,
  TrackOverview,
  TrackPlacement,
  TrackScenery,
  TrackSceneryGroup,
  TrackSceneryTexture,
  TrackTerrain,
} from "../types";

/**
 * A track's metadata and contents.
 *
 * Cheap enough to call as the view opens: the backend answers from the archive's index and
 * inflates nothing, so this returns in the time it takes to read a few kilobytes even for a
 * track that is hundreds of megabytes on disk.
 */
export function readTrackInfo(path: string): Promise<TrackInfo> {
  return invoke<TrackInfo>("read_track_info", { path });
}

/** Header bytes before the grid. Mirrors `track::BLOB_HEADER`. */
const HEADER = 32;

/** `"FTRN"`, the blob's leading magic. */
const MAGIC = 0x4e525446;

/**
 * A track's terrain grid, at no more than `maxDim` samples on its longest edge.
 *
 * Arrives as raw bytes rather than JSON. A grid is a few hundred thousand floats, and the
 * same numbers as a JSON array would be several times the size on the wire and cost a parse
 * on arrival that is slower than the archive read that produced them. Here the heights are
 * read in place, with no copy and no decode.
 */
export async function loadTrackTerrain(
  path: string,
  maxDim: number,
): Promise<TrackTerrain> {
  const buf = await invoke<ArrayBuffer>("load_track_terrain", { path, maxDim });
  const view = new DataView(buf);
  if (buf.byteLength < HEADER || view.getUint32(0, true) !== MAGIC) {
    throw new Error("terrain blob is not in the expected format");
  }

  const width = view.getUint32(8, true);
  const height = view.getUint32(12, true);
  const expected = width * height * 4;
  if (buf.byteLength - HEADER !== expected) {
    // Reading on regardless would hand three.js a short buffer to walk off the end of.
    throw new Error(
      `terrain grid is ${buf.byteLength - HEADER}B, expected ${expected}B`,
    );
  }

  return {
    width,
    height,
    minHeight: view.getFloat32(16, true),
    maxHeight: view.getFloat32(20, true),
    metresPerSample: view.getFloat32(24, true),
    // Bit 0: the track stated its sample spacing rather than us assuming it.
    // Bit 1: the heights are metres rather than the height file's own raw units.
    scaleKnown: (view.getUint16(6, true) & 1) === 1,
    heightsInMetres: (view.getUint16(6, true) & 2) === 2,
    confidence: view.getFloat32(28, true),
    // The header is a multiple of four bytes precisely so this can be a view rather than
    // a copy — a 512² grid is a megabyte that never needs to be duplicated.
    heights: new Float32Array(buf, HEADER, width * height),
  };
}

/** Header bytes before the pixels. Mirrors `track::TEXTURE_HEADER`. */
const TEXTURE_HEADER = 16;

/** `"FTEX"`, the texture blob's leading magic. */
const TEXTURE_MAGIC = 0x58455446;

/**
 * A picture of a track's surfaces, to lay over its terrain.
 *
 * Built from the coverage masks in the track's own height file — a byte per cell per surface
 * the builder painted — so it describes exactly the ground the grid does. `null` when a track
 * carries no masks, in which case the terrain draws on its relief alone.
 */
export async function loadTrackOverview(
  path: string,
  maxDim: number,
): Promise<TrackOverview | null> {
  const buf = await invoke<ArrayBuffer>("load_track_overview", { path, maxDim });
  // The track simply hasn't got any.
  if (buf.byteLength === 0) return null;

  const view = new DataView(buf);
  if (buf.byteLength < TEXTURE_HEADER || view.getUint32(0, true) !== TEXTURE_MAGIC) {
    throw new Error("track overview is not in the expected format");
  }
  const width = view.getUint32(8, true);
  const height = view.getUint32(12, true);
  const expected = width * height * 4;
  if (buf.byteLength - TEXTURE_HEADER !== expected) {
    throw new Error(
      `track overview is ${buf.byteLength - TEXTURE_HEADER}B, expected ${expected}B`,
    );
  }
  return { width, height, pixels: new Uint8Array(buf, TEXTURE_HEADER, expected) };
}

/** Header bytes before the vertex data. Mirrors `map::SCENERY_HEADER`. */
const SCENERY_HEADER = 48;

/** Bytes per entry in the blob's texture table. Mirrors `map::TEXTURE_ENTRY`. */
const TEXTURE_ENTRY = 20;

/** `"FSCN"`, the scenery blob's leading magic. */
const SCENERY_MAGIC = 0x4e435346;

/**
 * A track's scenery — the tents, bales, banners and fences its `.map` bakes, the surfaces
 * that paint them, and the props its `.scr` places.
 *
 * `null` when the track carries none, which is ordinary rather than a failure: an OEM track
 * can declare no materials at all and ship 120 MB of pure texture behind them.
 *
 * Raw bytes for the same reason the terrain is — this is a few hundred thousand triangles
 * and a couple of dozen surfaces, and the arrays are adopted in place rather than parsed.
 */
export async function loadTrackScenery(path: string): Promise<TrackScenery | null> {
  const buf = await invoke<ArrayBuffer>("load_track_scenery", { path });
  // The track simply hasn't got any.
  if (buf.byteLength === 0) return null;

  const view = new DataView(buf);
  if (buf.byteLength < SCENERY_HEADER || view.getUint32(0, true) !== SCENERY_MAGIC) {
    throw new Error("track scenery is not in the expected format");
  }
  const vertexCount = view.getUint32(8, true);
  const indexCount = view.getUint32(12, true);
  const groupCount = view.getUint32(16, true);
  const packed = view.getUint32(20, true);
  const textureCount = packed & 0xffff;
  // Top half: how many separable pieces the scenery comes apart into. Capped at 65535,
  // which only a very dense track reaches and which is plenty to report.
  const pieceCount = (packed >>> 16) & 0xffff;
  if (indexCount === 0) return null;

  const bounds = Array.from({ length: 6 }, (_, i) =>
    view.getFloat32(24 + i * 4, true),
  ) as TrackScenery["bounds"];

  const positionsAt = SCENERY_HEADER;
  const normalsAt = positionsAt + vertexCount * 12;
  const uvsAt = normalsAt + vertexCount * 12;
  const indicesAt = uvsAt + vertexCount * 8;
  const groupsAt = indicesAt + indexCount * 4;
  const piecesAt = groupsAt + groupCount * 12;
  const triangleCount = indexCount / 3;
  // Present only when the backend split the mesh into pieces; older blobs stop at the groups.
  const hasPieces = pieceCount > 0;
  const tableAt = piecesAt + (hasPieces ? triangleCount * 4 : 0);
  const pixelsAt = tableAt + textureCount * TEXTURE_ENTRY;

  const groups: TrackSceneryGroup[] = [];
  for (let i = 0; i < groupCount; i += 1) {
    const o = groupsAt + i * 12;
    groups.push({
      material: view.getUint32(o, true),
      triStart: view.getUint32(o + 4, true),
      triCount: view.getUint32(o + 8, true),
    });
  }

  const textures: TrackSceneryTexture[] = [];
  let at = pixelsAt;
  for (let i = 0; i < textureCount; i += 1) {
    const o = tableAt + i * TEXTURE_ENTRY;
    const width = view.getUint32(o + 4, true);
    const height = view.getUint32(o + 8, true);
    const byteLen = view.getUint32(o + 16, true);
    if (at + byteLen > buf.byteLength) {
      throw new Error("track scenery surfaces run past the end of the blob");
    }
    textures.push({
      material: view.getUint32(o, true),
      width,
      height,
      // Bit 0: the surface is an alpha cut-out.
      alpha: (view.getUint32(o + 12, true) & 1) === 1,
      pixels: new Uint8Array(buf, at, byteLen),
    });
    at += byteLen;
  }
  if (at !== buf.byteLength) {
    // Reading on would hand three.js buffers that don't describe what arrived.
    throw new Error(
      `track scenery is ${buf.byteLength}B, its sections account for ${at}B`,
    );
  }

  return {
    positions: new Float32Array(buf, positionsAt, vertexCount * 3),
    normals: new Float32Array(buf, normalsAt, vertexCount * 3),
    uvs: new Float32Array(buf, uvsAt, vertexCount * 2),
    indices: new Uint32Array(buf, indicesAt, indexCount),
    groups,
    textures,
    pieceCount,
    pieceOfTriangle: hasPieces
      ? new Uint32Array(buf, piecesAt, triangleCount)
      : new Uint32Array(0),
    bounds,
  };
}

/** Header bytes before the surface table. Mirrors `map::SURFACES_HEADER`. */
const SURFACES_HEADER = 16;

/** `"FSRF"`, the surface blob's leading magic. */
const SURFACES_MAGIC = 0x46525346;

/**
 * A track's surfaces — the second half of the load.
 *
 * Fetched after the mesh is already drawn: a map's sheets are hundreds of megabytes to
 * inflate, and holding the shape of the track back for them is a second of empty canvas.
 * Empty when the track's surfaces can't be bound to its materials.
 */
export async function loadTrackSurfaces(path: string): Promise<TrackSceneryTexture[]> {
  const buf = await invoke<ArrayBuffer>("load_track_surfaces", { path });
  if (buf.byteLength === 0) return [];

  const view = new DataView(buf);
  if (buf.byteLength < SURFACES_HEADER || view.getUint32(0, true) !== SURFACES_MAGIC) {
    throw new Error("track surfaces are not in the expected format");
  }
  const count = view.getUint32(8, true);
  const tableAt = SURFACES_HEADER;
  let at = tableAt + count * TEXTURE_ENTRY;

  const out: TrackSceneryTexture[] = [];
  for (let i = 0; i < count; i += 1) {
    const o = tableAt + i * TEXTURE_ENTRY;
    const byteLen = view.getUint32(o + 16, true);
    if (at + byteLen > buf.byteLength) {
      throw new Error("track surfaces run past the end of the blob");
    }
    out.push({
      material: view.getUint32(o, true),
      width: view.getUint32(o + 4, true),
      height: view.getUint32(o + 8, true),
      alpha: (view.getUint32(o + 12, true) & 1) === 1,
      pixels: new Uint8Array(buf, at, byteLen),
    });
    at += byteLen;
  }
  return out;
}

/**
 * Where a track pins the fixtures it ships no mesh for — marshal posts, TV cameras, crowd
 * sound — plus the props its `.scr` places.
 *
 * Split from the scenery mesh because it costs nothing: these files are kilobytes, so the
 * markers land while the `.map` is still being read out of the archive.
 */
export function readTrackPlacements(path: string): Promise<TrackPlacement[]> {
  return invoke<TrackPlacement[]>("read_track_placements", { path });
}

/**
 * Save a track's props to a `.scr` the game will load.
 *
 * The `.scr` states where a prop goes in plain text and the game reads it at load, so it is
 * where anything placed in the app ends up. Nothing is written inside an archive, and an
 * existing file is left alone unless `overwrite` says otherwise.
 */
export function saveTrackProps(
  target: string,
  props: TrackPlacement[],
  overwrite = false,
): Promise<void> {
  return invoke<void>("save_track_props", { target, props, overwrite });
}

/**
 * A plain-text account of what a track's terrain looks like to the reader: its contents,
 * what its `.ini` claimed, every layout the probe considered, and what it settled on.
 *
 * Only worth fetching when the terrain didn't load — it re-reads the archive rather than
 * using the cached grid, precisely because the interesting case is the one with no grid.
 */
export function diagnoseTrack(path: string): Promise<string> {
  return invoke<string>("diagnose_track", { path });
}
