import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * A track program: the document a track is generated from.
 *
 * Mirrors `src-tauri/src/trackprog.rs` and the Zod schema in
 * `control-plane/src/trackgen.ts`. It is deliberately small — a start pose, a run of
 * straights and arcs, and the jumps laid along them by how far round the lap they are — so
 * that editing a track is editing a list of numbers rather than four million samples.
 */
export interface TrackProgram {
  name: string;
  author: string;
  location: string;
  /** The riding line, metres. Published tracks measure 10–17. */
  width: number;
  terrain: {
    sizeX: number;
    sizeZ: number;
    /** A power of two plus one. */
    samples: number;
    /** The whole height budget, metres — everything is quantised against it. */
    scale: number;
    relief: { amplitude: number; wavelength: number; seed: number; texture: number };
    /**
     * What the ground is made of: how deep it cuts, how wide the shoulder runs. The *ride*.
     */
    surface: "soil" | "sand" | "grass";
    /**
     * And what it looks like, which is a separate question. Left out the look follows
     * `surface`, exactly as it always did.
     */
    texture?: TextureSet;
  };
  /** Degrees: 0 looks down +z, increasing clockwise towards +x. */
  start: { x: number; z: number; angle: number };
  segments: TrackSegment[];
  features: TrackFeature[];
  /**
   * Metres things ease into each other over: where two jumps meet, where a straight becomes
   * a corner, and how long a jump's own ramps are. One control, because those are the same
   * question asked three times.
   */
  blend: number;
  /**
   * Height the track is lifted or dropped by, at points round the lap. Empty means it simply
   * follows the ground it crosses.
   */
  elevation: { at: number; height: number }[];
  /** Which rules the lap is drawn and judged by. Left out for motocross. */
  discipline?: Discipline;
  /** What lines a lane's borders. Only a stadium discipline lays any; left out for soft. */
  border?: LaneBorder;
  /**
   * What this was called before it grew banners and none. A project saved back then still
   * carries it, and Rust still reads it, so the picker falls back to it rather than showing
   * an old track the wrong answer.
   *
   * @deprecated read `border`.
   */
  tuff?: LaneBorder;
  /**
   * Whether a supercross lap stands in a stadium or in the open air. Ignored by the outdoor
   * disciplines; left out for the stadium.
   */
  venue?: VenueKind;
}

/** Motocross, supercross or SuperMotocross. Mirrors `Discipline` in `trackprog.rs`. */
export type Discipline = "mx" | "sx" | "smx";

/**
 * What a track's ground looks like: a look to start from, and the rider's own images over
 * the top of it. Mirrors `TextureSet` in `trackprog.rs`.
 */
export interface TextureSet {
  preset: TexturePreset;
  sheets?: OwnSheet[];
}

/** `ride` follows the surface, which is what the look always did. */
export type TexturePreset = "ride" | "soil" | "sand" | "grass" | "stadium";

/** One of the rider's own images, standing in for a built-in ground sheet. */
export interface OwnSheet {
  slot: SheetSlot;
  /** What the Studio stored it as — see `importTrackTexture`. */
  id: string;
}

/** Which ground an image replaces. */
export type SheetSlot = "ground" | "line" | "rut" | "grass";

/** The four, in the order the picker shows them. */
export const SHEET_SLOTS: SheetSlot[] = ["ground", "line", "rut", "grass"];

/** An image the rider imported, as the picker shows it. Mirrors `OwnTexture`. */
export interface OwnTexture {
  id: string;
  /** The file it came from, so it can be recognised. */
  name: string;
  /** A small PNG as a `data:` URL. */
  thumb: string;
}

/**
 * Take an image off the rider's own disk into the Studio's ground store.
 *
 * The path comes from the file picker. Nothing is ever fetched: the Studio has no list of
 * ground to download and never asks anyone for one.
 */
export function importTrackTexture(path: string): Promise<OwnTexture> {
  return invoke<OwnTexture>("import_track_texture", { path });
}

/** Everything already imported, so nobody has to find the same file twice. */
export function listTrackTextures(): Promise<OwnTexture[]> {
  return invoke<OwnTexture[]>("list_track_textures");
}

/** Drop one. A track still naming it paints with the ground it stood in for. */
export function forgetTrackTexture(id: string): Promise<void> {
  return invoke<void>("forget_track_texture", { id });
}

/**
 * What lines a supercross lane: padded blocks a rider rides through, solid ones that stop the
 * bike, a printed banner wall, or nothing. Mirrors `LaneBorder` in `trackprog.rs`.
 */
export type LaneBorder = "soft" | "solid" | "banners" | "none";

/**
 * Whether a supercross lap is laid in a stadium or out in a field. Mirrors `VenueKind` in
 * `trackprog.rs`.
 */
export type VenueKind = "stadium" | "open";

export type TrackSegment =
  /** `rise` is metres climbed over the segment; negative drops, zero follows the ground. */
  | { kind: "straight"; length: number; rise: number }
  /** Signed radius — positive turns right. */
  | { kind: "arc"; radius: number; angle: number; rise: number };

export type TrackFeature =
  /** `finish` names this the finish jump outright — see `setFinish` in TrackStudio. */
  | { kind: "tabletop"; at: number; length: number; height: number; finish?: boolean }
  | { kind: "double"; at: number; height: number; gap: number; lip: number; finish?: boolean }
  | { kind: "roller"; at: number; length: number; height: number }
  | { kind: "whoops"; at: number; count: number; spacing: number; height: number }
  | { kind: "stepUp"; at: number; length: number; height: number }
  | { kind: "berm"; at: number; length: number; height: number }
  | { kind: "rut"; at: number; length: number; depth: number }
  /** A stretch of sand. Supercross lays one; it is ground, not a jump, so it has no height. */
  | { kind: "sand"; at: number; length: number }
  /** A shape drawn point by point. `u` runs 0 at the feature's start to 1 at its end. */
  | { kind: "custom"; at: number; length: number; shape: { u: number; h: number }[] };

export type TrackFeatureKind = TrackFeature["kind"];

/** What a built program measures, next to what it was asked for. */
export interface TrackPreview {
  /** The `.pkz` the viewer opens. Terrain and surfaces only. */
  path: string;
  name: string;
  lapM: number;
  widthM: number;
  features: number;
  /** How far the finish misses the start. */
  closureM: number;
  usedM: number;
  budgetM: number;
  measuredWidthM: number;
  measuredLengthM: number;
  lips: number;
  lipsPerKm: number;
  slopeP99Deg: number;
  reliefP90M: number;
}

/**
 * What a model is asked for. `program` is the whole lap, drawn by the model and measured by
 * the app, and only a strong model manages it. `settings` is the character only, and the
 * app's own walker draws the lap, so any model can do it, a free one included.
 *
 * Not a choice anyone is asked to make: which one works is a fact about the model that is
 * configured, so `generate_track` picks it. Left here because the command still takes it.
 */
export type GenerateMode = "program" | "settings";

/** The character a model picked. Mirrors `TrackSettings` in `src-tauri/src/tracklayout.rs`. */
export interface TrackSettings {
  name: string;
  location: string;
  lapLength: number;
  width: number;
  cornersPerKm: number;
  apexRadius: number;
  sweepShare: number;
  startStraight: number;
  jumpDensity: number;
  bigJumpShare: number;
  jumpScale: number;
  waves: number;
  surface: "soil" | "sand" | "grass";
  wear: number;
  roughness: number;
  hills: number;
  tilt: number;
  landforms: number;
  elevationChanges: number;
  /**
   * Which kind of racing the brief asked for.
   *
   * The model's to pick, unless the switch on screen is set to something other than
   * motocross — an explicit choice is not a brief's to overrule.
   */
  discipline: Discipline;
}

/** A generated track, and the settings it was drawn from when that was the mode. */
export interface Generated {
  program: TrackProgram;
  settings: TrackSettings | null;
}

/**
 * Ask for a track.
 *
 * Slow on purpose — the model lays out a lap that has to close, and the app builds and
 * measures every answer before accepting it, retrying with the measurements when it doesn't
 * land. Minutes, not seconds, unless the model can only be asked for settings, which is.
 *
 * `mode` is left to the app unless something has a reason to force it.
 */
export function generateTrack(
  brief: string,
  mode?: GenerateMode,
  discipline: Discipline = "mx",
): Promise<Generated> {
  return invoke<Generated>("generate_track", { brief, mode, discipline });
}

/** Which API shape a model of the user's own speaks. */
export type ModelKind = "openAi" | "anthropic";

/** A model of the user's own, as the app shows it: never the key, only whether there is one. */
export interface TrackModel {
  kind: ModelKind;
  baseUrl: string;
  model: string;
  hasKey: boolean;
}

/** The saved model, or null when tracks go through the MXB account. */
export function getTrackModel(): Promise<TrackModel | null> {
  return invoke<TrackModel | null>("get_track_model");
}

/** Save a model. Leave `key` out to keep the saved one. */
export function setTrackModel(
  kind: ModelKind,
  baseUrl: string,
  model: string,
  key?: string,
): Promise<TrackModel> {
  return invoke<TrackModel>("set_track_model", { kind, baseUrl, model, key });
}

/** Back to the MXB account. */
export function clearTrackModel(): Promise<void> {
  return invoke<void>("clear_track_model");
}

/** One small request to the model on screen, saved or not. Rejects with the reason. */
export function testTrackModel(
  kind: ModelKind,
  baseUrl: string,
  model: string,
  key?: string,
): Promise<void> {
  return invoke<void>("test_track_model", { kind, baseUrl, model, key });
}

/** A lap with nothing on it, to start from scratch. */
export function blankTrackProgram(): Promise<TrackProgram> {
  return invoke<TrackProgram>("blank_track_program");
}

/**
 * A whole track from a number, walked rather than written.
 *
 * The shape of a lap is geometry, and geometry is checkable — so this half needs no model,
 * no key and no round trip. Omit the seed for a different track every time.
 */
export function randomTrackProgram(
  seed?: number,
  scale: TrackScale = "normal",
  discipline: Discipline = "mx",
  density?: number,
): Promise<TrackProgram> {
  return invoke<TrackProgram>("random_track_program", { seed, scale, discipline, density });
}

/**
 * How packed a lap is, as a multiple of what a real round carries. 1 is the measured density.
 *
 * Only the stadium disciplines read it — a national spaces its jumps by a different rule.
 */
export const DENSITY_RANGE = { min: 0.65, max: 1.35, step: 0.05, reference: 1 } as const;

/** Easy: smaller jumps, shallower ruts. Pro: the bigger, rougher raced build. */
export type TrackScale = "easy" | "normal" | "pro";

/**
 * Give the track a height budget that fits it.
 *
 * The budget exists only because samples are quantised against it, so nobody should be asked
 * to guess a number the synthesiser already knows.
 */
export function fitTrackBudget(program: TrackProgram): Promise<TrackProgram> {
  return invoke<TrackProgram>("fit_track_budget", { program });
}

/** Bring an open lap back to its start: a turn, a straight and a turn. */
export function closeTrackLap(program: TrackProgram): Promise<TrackProgram> {
  return invoke<TrackProgram>("close_track_lap", { program });
}

/**
 * What is wrong with a program, in three grades of wrong.
 *
 * `fatal` means there is nothing to build — the lap leaves the terrain, or synthesis failed.
 * `problems` mean it builds and it is wrong, which is a judgement against published tracks
 * and so one a person may overrule. `notes` don't block at all: a blank lap is empty, not
 * broken.
 */
export interface TrackReview {
  fatal: string[];
  problems: string[];
  notes: string[];
}

export function checkTrack(program: TrackProgram): Promise<TrackReview> {
  return invoke<TrackReview>("check_track", { program });
}

/** Build it, and write it where the track viewer can open it. */
export function previewTrack(program: TrackProgram): Promise<TrackPreview> {
  return invoke<TrackPreview>("preview_track", { program });
}

/** Write the folder TerrainEd compiles. Returns the file names written. */
export function exportTrackSource(program: TrackProgram, dir: string): Promise<string[]> {
  return invoke<string[]>("export_track_source", { program, dir });
}

/** A saved track project's extension. The file is JSON. */
export const TRACK_PROJECT_EXT = "mxbtrack";

export function saveTrackProject(program: TrackProgram, path: string): Promise<void> {
  return invoke("save_track_project", { program, path });
}

export function openTrackProject(path: string): Promise<TrackProgram> {
  return invoke<TrackProgram>("open_track_project", { path });
}

/**
 * What the ground arrives as when a track is imported.
 *
 * - `keep` — the terrain exactly as its builder left it. Every jump, camber and rut the
 *   source had, and not one the generator invented. A faithful base.
 * - `rut` — that, with the generator's ruts and grooves laid over the riding line. A track
 *   that was compiled clean, ridden in.
 * - `recut` — keep only the landform and let the generator cut its own corridor, jumps and
 *   ruts into it. The layout survives; what was built on it does not.
 */
export type ScanJumps = "keep" | "rut" | "recut";

/** What a compiled track turned out to hold, read before anyone commits to importing it. */
export interface TrackImportPreview {
  name: string;
  sizeX: number;
  sizeZ: number;
  samplesX: number;
  samplesZ: number;
  reliefM: number;
  /** `tracked -merge` writes the centreline into the terrain file, and not every track was
   *  finished with it. Without one there is no lap to rebuild around and no import. */
  hasLap: boolean;
  segments: number;
  surfaces: string[];
}

/** Look inside a compiled track — a `.pkz`, or one track's `prefix` inside a shared one. */
export function inspectTrackImport(
  path: string,
  prefix?: string | null,
): Promise<TrackImportPreview> {
  return invoke<TrackImportPreview>("inspect_track_import", { path, prefix: prefix ?? null });
}

/**
 * Bring a compiled track in as a program to edit and rebuild.
 *
 * Its layout, elevation and footprint come across. Its ground sheets, scenery and props do
 * not — the `.map` is a bake and the source it was baked from isn't in the archive — so the
 * rebuilt track wears the Studio's own. Tell the rider that before calling this.
 */
export function importTrack(
  path: string,
  prefix: string | null,
  jumps: ScanJumps,
): Promise<TrackProgram> {
  return invoke<TrackProgram>("import_track", { path, prefix, jumps });
}

/**
 * The lap as a list you can read in order: a straight, a left turn, a double.
 *
 * The program stores corners and jumps separately — one is the shape of the lap, the other
 * is what is built on it — but nobody describes a track that way. Riding it, they are one
 * sequence, so this is the sequence, and it is what the studio shows and what a brief can be
 * written in.
 */
export type LapStep =
  | { at: number; kind: "straight"; index: number; segment: TrackSegment }
  // Split rather than `kind: "left" | "right"`: a discriminant that is itself a union
  // doesn't narrow, and every reader of this then has to cast.
  | { at: number; kind: "left"; index: number; segment: TrackSegment }
  | { at: number; kind: "right"; index: number; segment: TrackSegment }
  | { at: number; kind: "feature"; index: number; feature: TrackFeature };

export function lapSteps(program: TrackProgram): LapStep[] {
  const steps: LapStep[] = [];
  let at = 0;
  program.segments.forEach((segment, index) => {
    if (segment.kind === "straight") {
      steps.push({ at, kind: "straight", index, segment });
      at += segment.length;
    } else {
      // Signed radius: positive turns right. Which way it goes is the thing a person reads,
      // so it is the thing the row says.
      steps.push({ at, kind: segment.radius >= 0 ? "right" : "left", index, segment });
      at += (Math.abs(segment.radius) * Math.abs(segment.angle) * Math.PI) / 180;
    }
  });
  program.features.forEach((feature, index) =>
    steps.push({ at: feature.at, kind: "feature", index, feature }),
  );
  // Stable by distance round the lap; where a jump starts exactly at a corner, the corner
  // comes first because that is the order you meet them.
  return steps.sort((a, b) => a.at - b.at || (a.kind === "feature" ? 1 : -1));
}

/**
 * Where a point on the lap is, in world metres.
 *
 * The same walk the synthesiser does — heading 0 looks down +z and increases clockwise
 * towards +x, and an arc is evaluated from its centre rather than integrated — so a point
 * this returns is the point the terrain was built around. Keep the two in step: if one
 * changes convention the camera flies to the wrong side of the track.
 */
export function positionAt(program: TrackProgram, s: number): { x: number; z: number } {
  let { x, z } = program.start;
  let th = (program.start.angle * Math.PI) / 180;
  let left = s;
  for (const seg of program.segments) {
    const len =
      seg.kind === "straight"
        ? seg.length
        : (Math.abs(seg.radius) * Math.abs(seg.angle) * Math.PI) / 180;
    const part = Math.min(Math.max(left, 0), len);
    if (seg.kind === "straight") {
      x += Math.sin(th) * part;
      z += Math.cos(th) * part;
      if (left <= len) return { x, z };
      th += 0;
    } else {
      const turn = seg.radius >= 0 ? 1 : -1;
      const r = Math.max(Math.abs(seg.radius), 0.01);
      const cx = x + Math.cos(th) * r * turn;
      const cz = z - Math.sin(th) * r * turn;
      const phi = part / r;
      const th2 = th + turn * phi;
      x = cx - Math.cos(th2) * r * turn;
      z = cz + Math.sin(th2) * r * turn;
      if (left <= len) return { x, z };
      th = th2;
    }
    left -= len;
  }
  return { x, z };
}

/**
 * Pull every feature back inside the lap.
 *
 * Shortening or removing a corner leaves the jumps that were past it hanging off the end,
 * and the row editor has no field for where a feature sits — so without this, removing one
 * segment produces an error the person has no way to clear except by deleting their jumps.
 */
/**
 * Where each corner starts and ends, in metres round the lap.
 */
export function corners(program: TrackProgram): { at: number; length: number }[] {
  const out: { at: number; length: number }[] = [];
  let at = 0;
  for (const seg of program.segments) {
    if (seg.kind === "straight") {
      at += seg.length;
    } else {
      const length = (Math.abs(seg.radius) * Math.abs(seg.angle) * Math.PI) / 180;
      out.push({ at, length });
      at += length;
    }
  }
  return out;
}

export function fitFeatures(program: TrackProgram): TrackProgram {
  // A metre short of the line, not exactly on it. The lap is summed here in double precision
  // and in the synthesiser in single, so "exactly on the line" is a different number in each
  // — and clamping to the boundary produced a jump the validator then rejected.
  const lap = lapLength(program) - 1;
  const turns = corners(program);
  return {
    ...program,
    features: program.features.map((f) => {
      // A berm's whole meaning is "on this corner". Move the corners and the berm has to
      // follow, or it lands on a straight where the synthesiser silently drops it.
      if (f.kind === "berm" && turns.length) {
        const on = turns.find((c) => f.at >= c.at && f.at < c.at + c.length);
        if (!on) {
          const near = turns.reduce((best, c) =>
            Math.abs(c.at - f.at) < Math.abs(best.at - f.at) ? c : best,
          );
          return { ...f, at: near.at + 1, length: Math.max(4, near.length - 2) };
        }
      }
      const end = f.at + featureSpan(f).length;
      if (end <= lap && f.at >= 0) return f;
      return { ...f, at: Math.max(0, Math.min(f.at, lap - featureSpan(f).length)) };
    }),
  };
}

/**
 * How high the track sits at a point on the lap, read off the elevation curve.
 *
 * The same easing the synthesiser uses — smoothstep between neighbouring points, wrapping
 * from the last round to the first — so what a row says matches what the ground does.
 */
export function elevationAt(program: TrackProgram, s: number): number {
  const k = [...(program.elevation ?? [])].sort((a, b) => a.at - b.at);
  if (k.length === 0) return 0;
  if (k.length === 1) return k[0].height;
  const lap = lapLength(program);
  let i = k.findIndex((p) => p.at > s);
  i = i <= 0 ? k.length - 1 : i - 1;
  const a = k[i];
  const b = k[(i + 1) % k.length];
  const span = b.at > a.at ? b.at - a.at : lap - a.at + b.at;
  if (span <= 1e-3) return b.height;
  const along = s >= a.at ? s - a.at : lap - a.at + s;
  const u = Math.min(Math.max(along / span, 0), 1);
  return a.height + (b.height - a.height) * (u * u * (3 - 2 * u));
}

/**
 * Set the track's height at a point, by putting a curve point there.
 *
 * Within a couple of metres of an existing point it moves that one rather than crowding
 * another in beside it — otherwise nudging a jump's height twice leaves two points fighting
 * over the same stretch of lap.
 */
export function setElevationAt(program: TrackProgram, s: number, height: number): TrackProgram {
  const knots = [...(program.elevation ?? [])];
  const near = knots.findIndex((k) => Math.abs(k.at - s) < 2);
  if (near >= 0) knots[near] = { at: knots[near].at, height };
  else knots.push({ at: s, height });
  return { ...program, elevation: knots };
}

/** The middle of a feature — where its own height point belongs. */
export function featureMiddle(f: TrackFeature): number {
  return f.at + featureSpan(f).length / 2;
}

/**
 * Points along a stretch of the lap, for lighting it up in the preview.
 *
 * A stretch, not a point: a straight is two hundred metres long, and marking only where it
 * starts says almost nothing about which one it is.
 */
export function pathAlong(program: TrackProgram, from: number, length: number): { x: number; z: number }[] {
  const steps = Math.max(2, Math.min(48, Math.ceil(length / 4)));
  return Array.from({ length: steps + 1 }, (_, i) =>
    positionAt(program, from + (length * i) / steps),
  );
}

/** A feature of each kind, with sizes that sit inside what published tracks measure. */
export function newFeature(kind: TrackFeatureKind, at: number): TrackFeature {
  switch (kind) {
    case "tabletop":
      return { kind, at, length: 20, height: 1.6 };
    case "double":
      return { kind, at, height: 1.6, gap: 10, lip: 6 };
    case "roller":
      return { kind, at, length: 14, height: 0.9 };
    case "whoops":
      return { kind, at, count: 5, spacing: 5, height: 0.7 };
    case "stepUp":
      return { kind, at, length: 25, height: 2.5 };
    case "berm":
      return { kind, at, length: 20, height: 1.6 };
    case "rut":
      return { kind, at, length: 20, depth: 0.15 };
    case "sand":
      return { kind, at, length: 40 };
    case "custom":
      return { kind, at, length: 24, shape: [
        { u: 0, h: 0 },
        { u: 0.35, h: 1.6 },
        { u: 1, h: 0 },
      ] };
  }
}

/**
 * The emptiest stretch of the lap, which is where a new feature should go.
 *
 * Dropping one at the finish line means it usually lands on top of something, and the
 * validator then complains about a track the person didn't ask for.
 */
export function roomiestGap(program: TrackProgram, want: number): number {
  const lap = lapLength(program);
  const taken = program.features
    .map((f) => ({ lo: f.at, hi: f.at + featureSpan(f).length }))
    .sort((a, b) => a.lo - b.lo);
  let best = { at: lap / 2, size: -1 };
  let cursor = 0;
  for (const span of [...taken, { lo: lap, hi: lap }]) {
    const size = span.lo - cursor;
    if (size > best.size) best = { at: cursor + (size - want) / 2, size };
    cursor = Math.max(cursor, span.hi);
  }
  return Math.max(0, Math.min(best.at, Math.max(0, lap - want)));
}

/** How long the lap is. An arc states its radius and angle, so its length falls out. */
export function lapLength(program: TrackProgram): number {
  return program.segments.reduce(
    (sum, s) =>
      sum +
      (s.kind === "straight"
        ? s.length
        : (Math.abs(s.radius) * Math.abs(s.angle) * Math.PI) / 180),
    0,
  );
}

/** Whether the app can compile a track here, and what with. */
export interface TrackToolsStatus {
  path: string;
  found: boolean;
  hasTracked: boolean;
}

/** One compiler run, and what it said. */
export interface BuildStep {
  name: "map" | "trh" | "centerline";
  ok: boolean;
  code: number | null;
  output: string;
  produced: string | null;
}

export function trackToolsStatus(): Promise<TrackToolsStatus> {
  return invoke<TrackToolsStatus>("track_tools_status");
}

export function setTrackTools(dir: string): Promise<TrackToolsStatus> {
  return invoke<TrackToolsStatus>("set_track_tools", { dir });
}

/**
 * Fetch PiBoSo's track tools, so nobody has to leave the app to find them.
 *
 * They are a public download and not ours to ship. Small — about a megabyte.
 */
export function downloadTrackTools(): Promise<TrackToolsStatus> {
  return invoke<TrackToolsStatus>("download_track_tools");
}

/** Everything a build produced, and where it ended up. */
export interface BuildResult {
  steps: BuildStep[];
  /** The folder the source and the compiled files are in. */
  dir: string;
  /** The archive, once every step has succeeded. */
  pkz: string | null;
  /** Where it was installed, when it was asked for and worked. */
  installed: string | null;
}

/**
 * Export, compile, package and install, in one.
 *
 * `dir` picks where the work happens; leave it null and the app uses a folder of its own,
 * which is what makes building one press. `install` puts the finished `.pkz` in the mods
 * tree, where the game lists it.
 *
 * The compilers are PiBoSo's and Windows-only; off Windows they go through a Wine host the
 * app finds — or fetches — by itself, so there is nothing to install first. Minutes, not
 * seconds — TerrainEd bakes shadow maps over the whole terrain.
 */
export function buildTrack(
  program: TrackProgram,
  dir: string | null,
  install: boolean,
): Promise<BuildResult> {
  return invoke<BuildResult>("build_track", { program, dir, install });
}

/** The phases of a build, in the order they run. */
export type BuildPhase =
  | "preparing"
  | "synthesising"
  | "writing"
  | "map"
  | "trh"
  | "centerline"
  | "packaging"
  | "installing";

/**
 * How far a build has got.
 *
 * Mirrors `BuildProgress` and `trackbuild::Progress` in `src-tauri`. A phase says where it
 * sits on the bar and how long it is expected to run, and the studio eases across that span
 * over that long — the compilers themselves say nothing until they exit, so this is the only
 * sign of life a build has.
 */
export interface BuildProgress {
  /** Which build this belongs to, so a second one can't drive the first one's bar. */
  slug: string;
  phase: BuildPhase;
  /** Where the phase starts and ends on the bar, 0–1. */
  from: number;
  to: number;
  /** Seconds it is expected to run for. */
  expect: number;
}

export function onBuildProgress(
  cb: (p: BuildProgress) => void,
): Promise<UnlistenFn> {
  return listen<BuildProgress>("track-build-progress", (e) => cb(e.payload));
}

/**
 * The colors the preview paints each feature kind with.
 *
 * The same values as `surface_color` in `src-tauri/src/track.rs`, ids 200–206. They have to
 * match: the point of both is that a row in the list and a lump on the ground are obviously
 * the same thing.
 */
export const FEATURE_COLOUR: Record<TrackFeatureKind, string> = {
  tabletop: "rgb(214, 120, 60)",
  double: "rgb(206, 74, 96)",
  roller: "rgb(120, 150, 200)",
  whoops: "rgb(190, 170, 70)",
  stepUp: "rgb(110, 180, 130)",
  berm: "rgb(150, 110, 200)",
  rut: "rgb(90, 90, 110)",
  sand: "rgb(198, 176, 124)",
  custom: "rgb(200, 200, 210)",
};

/** Where a feature sits, and how long it runs — the two numbers every kind has. */
export function featureSpan(f: TrackFeature): { at: number; length: number } {
  switch (f.kind) {
    case "double":
      return { at: f.at, length: (f.lip + Math.min(f.lip, 5)) * 2 + f.gap };
    case "whoops":
      return { at: f.at, length: f.count * f.spacing };
    case "custom":
      return { at: f.at, length: f.length };
    default:
      return { at: f.at, length: f.length };
  }
}
