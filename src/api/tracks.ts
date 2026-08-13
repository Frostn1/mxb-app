import { invoke } from "@tauri-apps/api/core";
import type { TrackInfo, TrackOverview, TrackTerrain } from "../types";

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
