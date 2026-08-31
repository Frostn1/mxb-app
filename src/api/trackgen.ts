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

/** A lap with nothing on it, to start from scratch. */
export function blankTrackProgram(): Promise<TrackProgram> {
  return invoke<TrackProgram>("blank_track_program");
}

/** Bring an open lap back to its start: a turn, a straight and a turn. */
export function closeTrackLap(program: TrackProgram): Promise<TrackProgram> {
  return invoke<TrackProgram>("close_track_lap", { program });
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
 * Export and compile in one.
 *
 * The compilers are PiBoSo's and Windows-only; on macOS they go through the same Wine prefix
 * the game does. Minutes, not seconds — TerrainEd bakes shadow maps over the whole terrain.
 */
export function buildTrack(program: TrackProgram, dir: string): Promise<BuildStep[]> {
  return invoke<BuildStep[]>("build_track", { program, dir });
}

/**
 * The colours the preview paints each feature kind with.
 *
 * The same values as `surface_colour` in `src-tauri/src/track.rs`, ids 200–206. They have to
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
};

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
