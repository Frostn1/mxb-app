import { invoke } from "@tauri-apps/api/core";

/**
 * Turning a real place into ground a track can be built on.
 *
 * Mirrors `src-tauri/src/trackplace.rs`. Every call here is the direct result of a button
 * being pressed — nothing in this module runs on a timer, on mount, or on a keystroke. The
 * services behind it are public goods paid for by somebody else's taxes, and the least we
 * can do is only ask when a rider actually wants something.
 */

/** Bare earth, or bare earth plus everything standing on it. */
export type Ground = "terrain" | "surface";

/** A place a search turned up. */
export interface PlaceHit {
  label: string;
  lat: number;
  lon: number;
  /** What OpenStreetMap calls it, so a circuit can be told from a street of the same name. */
  kind: string;
}

/** What one elevation source has at one spot. */
export interface SourceCoverage {
  id: string;
  label: string;
  region: string;
  /** What is actually here, which can be much coarser than the source's best. */
  cellM: number;
  ground: Ground;
  dataset: string;
  collected: string;
  licence: string;
  attribution: string;
  termsUrl: string;
  covered: boolean;
  /** Why it can't be used, or what's wrong with it, in a sentence. */
  note: string;
}

/** Everything known about a spot before a byte is downloaded. */
export interface CoverageReport {
  lat: number;
  lon: number;
  epsg: number;
  east: number;
  north: number;
  sources: SourceCoverage[];
  /** The id of the one the tool would pick. Empty where nothing covers it. */
  best: string;
}

/** The elevation a place was fetched with, as it sits on disk. */
export interface DemNote {
  path: string;
  crs: string;
  cellM: number;
  /** The centre of pixel (0, 0). North up, row index increasing southward. */
  originE: number;
  originN: number;
  width: number;
  height: number;
  verticalDatum: string;
  source: string;
  sourceUrl: string;
  collected: string;
  licence: string;
  attribution?: string;
  ground: Ground;
  minZ: number;
  maxZ: number;
}

export interface ImageryNote {
  path: string;
  source: string;
  licence: string;
  attribution?: string;
  captured: string;
}

/** A fetched place: its folder, its ground, its picture, and where they came from. */
export interface Place {
  slug: string;
  name: string;
  lat: number;
  lon: number;
  plotM: number;
  fetched: string;
  dem: DemNote;
  imagery?: ImageryNote | null;
  hillshade: string;
  /** South, west, north, east, in plain degrees. */
  bboxLl: number[];
  hasTrace: boolean;
  bytes?: number | null;
  dir: string;
}

/**
 * The lap a rider drew, exactly as it goes to disk and exactly as the importer reads it.
 *
 * `points` are ordered, in the DEM's own projection, easting first. An entry is
 * `[e, n]` or `[e, n, widthM]`; mixed lengths in one lap are fine, and anything without a
 * width of its own uses `defaultWidthM`. `closed` joins the last point to the first without
 * repeating it.
 */
export interface LapTrace {
  version: number;
  kind: string;
  name: string;
  crs: string;
  units: string;
  closed: boolean;
  defaultWidthM: number;
  startIndex: number;
  points: number[][];
  dem: DemNote;
  imagery?: ImageryNote | null;
}

/** Two pictures and the maths to put a click back on the earth. */
export interface PlaceLayers {
  slug: string;
  name: string;
  /** A data URL, ready for an `<img>`. */
  hillshade: string;
  imagery?: string | null;
  dem: DemNote;
  imageryNote?: ImageryNote | null;
  trace?: LapTrace | null;
  /** Every licence line that has to travel with anything made from this place. */
  attributions: string[];
}

export interface PlacePaths {
  dir: string;
  dem: string;
  trace: string;
  hasTrace: boolean;
}

/** Look a place up by name, or read a pair of coordinates straight off. */
export const findPlace = (query: string) => invoke<PlaceHit[]>("place_find", { query });

/** What each source has here. Ask before spending anything. */
export const placeCoverage = (lat: number, lon: number) =>
  invoke<CoverageReport>("place_coverage", { lat, lon });

/** Fetch the ground and a picture of it. Pass `plotM` 0 for the generator's own plot. */
export const fetchPlace = (
  name: string,
  lat: number,
  lon: number,
  plotM: number,
  sourceId: string,
) => invoke<Place>("place_fetch", { name, lat, lon, plotM, sourceId });

export const listPlaces = () => invoke<Place[]>("place_list");
export const forgetPlace = (slug: string) => invoke<void>("place_forget", { slug });
export const placePaths = (slug: string) => invoke<PlacePaths>("place_paths", { slug });
export const placeLayers = (slug: string) => invoke<PlaceLayers>("place_layers", { slug });

/** Save the lap. Returns the absolute path the importer should be handed. */
export const savePlaceTrace = (
  slug: string,
  points: number[][],
  closed: boolean,
  defaultWidthM: number,
  startIndex: number,
) =>
  invoke<string>("place_save_trace", {
    slug,
    points,
    closed,
    defaultWidthM,
    startIndex,
  });

/** Take a GeoTIFF a rider downloaded themselves. The path for most of the world. */
export const importPlaceDem = (
  name: string,
  path: string,
  licence: string,
  attribution: string,
) => invoke<Place>("place_import_dem", { name, path, licence, attribution });

/**
 * The length of a lap, in metres, closed or not.
 *
 * Here rather than in the component because the tracing canvas shows it live and a rider
 * judges whether they have drawn a real lap almost entirely by this number.
 */
export function traceLength(points: number[][], closed: boolean): number {
  if (points.length < 2) return 0;
  let total = 0;
  const n = points.length;
  const last = closed ? n : n - 1;
  for (let i = 0; i < last; i++) {
    const a = points[i];
    const b = points[(i + 1) % n];
    total += Math.hypot(b[0] - a[0], b[1] - a[1]);
  }
  return total;
}

/** Bytes as something a human reads at a glance. */
export function humanBytes(bytes: number | null | undefined): string {
  if (!bytes || bytes < 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
