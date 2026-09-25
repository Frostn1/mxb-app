import { invoke } from "@tauri-apps/api/core";

/** A Blender Studio found, and whether the bike builder can run it. Mirrors `blender.rs`. */
export interface BlenderInfo {
  path: string;
  version: string;
  supported: boolean;
}

export interface BlenderStatus {
  /** The `blender.exe` picked by hand, or "" when Studio finds it itself. */
  saved: string;
  found: BlenderInfo | null;
  minVersion: string;
}

/** One object from an inspected part, as `frost_bike.py` describes it. */
export interface PartObject {
  name: string;
  type: string;
  parent: string | null;
  location: [number, number, number];
  verts?: number;
  tris?: number;
  materials?: string[];
  uv?: boolean;
}

export interface PartInspection {
  objects: PartObject[];
  bounds: { min: [number, number, number]; max: [number, number, number] } | null;
  tris: number;
  fbx?: string;
  glb?: string;
  blender: string;
}

/** The part files the builder reads. */
export const PART_EXTENSIONS = ["blend", "fbx", "obj"];

export function blenderStatus(): Promise<BlenderStatus> {
  return invoke<BlenderStatus>("blender_status");
}

/** Pick `blender.exe` by hand; "" goes back to finding it. */
export function setBlenderPath(path: string): Promise<BlenderStatus> {
  return invoke<BlenderStatus>("set_blender_path", { path });
}

/** Import one part in Blender and say what's in it. Also proves Blender works here. */
export function inspectPart(part: string): Promise<PartInspection> {
  return invoke<PartInspection>("bike_part_inspect", { part });
}

/** Width × height × length in metres, from an inspection's bounds, for a one-line summary. */
export function partSize(b: PartInspection["bounds"]): [number, number, number] | null {
  if (!b) return null;
  return [b.max[0] - b.min[0], b.max[2] - b.min[2], b.max[1] - b.min[1]];
}
