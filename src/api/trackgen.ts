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
    /** What the ground is, which decides the surfaces either side of the line. */
    surface: "soil" | "sand" | "grass";
  };
  /** Degrees: 0 looks down +z, increasing clockwise towards +x. */
  start: { x: number; z: number; angle: number };
  segments: TrackSegment[];
  features: TrackFeature[];
}

export type TrackSegment =
  /** `rise` is metres climbed over the segment; negative drops, zero follows the ground. */
  | { kind: "straight"; length: number; rise: number }
  /** Signed radius — positive turns right. */
  | { kind: "arc"; radius: number; angle: number; rise: number };

export type TrackFeature =
  | { kind: "tabletop"; at: number; length: number; height: number }
  | { kind: "double"; at: number; height: number; gap: number; lip: number }
  | { kind: "roller"; at: number; length: number; height: number }
  | { kind: "whoops"; at: number; count: number; spacing: number; height: number }
  | { kind: "stepUp"; at: number; length: number; height: number }
  | { kind: "berm"; at: number; length: number; height: number }
  | { kind: "rut"; at: number; length: number; depth: number };

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

/** A track to start from, with no model involved. */
export function baseTrackProgram(): Promise<TrackProgram> {
  return invoke<TrackProgram>("base_track_program");
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
 * Export and compile in one.
 *
 * The compilers are PiBoSo's and Windows-only; on macOS they go through the same Wine prefix
 * the game does. Minutes, not seconds — TerrainEd bakes shadow maps over the whole terrain.
 */
export function buildTrack(program: TrackProgram, dir: string): Promise<BuildStep[]> {
  return invoke<BuildStep[]>("build_track", { program, dir });
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
