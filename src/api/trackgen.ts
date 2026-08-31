import { invoke } from "@tauri-apps/api/core";

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
  };
  /** Degrees: 0 looks down +z, increasing clockwise towards +x. */
  start: { x: number; z: number; angle: number };
  segments: TrackSegment[];
  features: TrackFeature[];
}

export type TrackSegment =
  | { kind: "straight"; length: number }
  /** Signed radius — positive turns right. */
  | { kind: "arc"; radius: number; angle: number };

export type TrackFeature =
  | { kind: "tabletop"; at: number; length: number; height: number }
  | { kind: "double"; at: number; height: number; gap: number; lip: number }
  | { kind: "roller"; at: number; length: number; height: number }
  | { kind: "whoops"; at: number; count: number; spacing: number; height: number }
  | { kind: "stepUp"; at: number; length: number; height: number }
  | { kind: "berm"; at: number; length: number; height: number };

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
 * Ask for a track.
 *
 * Slow on purpose — the model lays out a lap that has to close, and the app builds and
 * measures every answer before accepting it, retrying with the measurements when it doesn't
 * land. Minutes, not seconds.
 */
export function generateTrack(brief: string): Promise<TrackProgram> {
  return invoke<TrackProgram>("generate_track", { brief });
}

/** Everything wrong with a program, held to the same corpus a generated one is. */
export function checkTrack(program: TrackProgram): Promise<string[]> {
  return invoke<string[]>("check_track", { program });
}

/** Build it, and write it where the track viewer can open it. */
export function previewTrack(program: TrackProgram): Promise<TrackPreview> {
  return invoke<TrackPreview>("preview_track", { program });
}

/** Put the preview in the game's tracks folder. Terrain only — it previews, it doesn't play. */
export function installTrackPreview(program: TrackProgram): Promise<string> {
  return invoke<string>("install_track_preview", { program });
}

/** Write the folder TerrainEd compiles. Returns the file names written. */
export function exportTrackSource(program: TrackProgram, dir: string): Promise<string[]> {
  return invoke<string[]>("export_track_source", { program, dir });
}

/** How long the lap is. An arc states its radius and angle, so its length falls out. */
export function lapLength(program: TrackProgram): number {
  return program.segments.reduce(
    (sum, s) =>
      sum + (s.kind === "straight" ? s.length : Math.abs(s.radius) * (Math.abs(s.angle) * Math.PI) / 180),
    0,
  );
}

/** Where a feature sits, and how long it runs — the two numbers every kind has. */
export function featureSpan(f: TrackFeature): { at: number; length: number } {
  switch (f.kind) {
    case "double":
      return { at: f.at, length: (f.lip + Math.min(f.lip, 5)) * 2 + f.gap };
    case "whoops":
      return { at: f.at, length: f.count * f.spacing };
    default:
      return { at: f.at, length: f.length };
  }
}
