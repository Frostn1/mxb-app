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

/** What a part is on the bike, one slot each. Mirrors `bikeparts::Role`. */
export const ROLES = ["chassis", "steer", "fsusp", "rsusp", "wheel_f", "wheel_r", "levers", "pedals"] as const;
export type Role = (typeof ROLES)[number];

/** An attach point a part brings, in Blender's world (Z up). */
export interface PartEmpty {
  name: string;
  parent: string | null;
  location: [number, number, number];
}

/** A part in the library, as `bike_parts_list` shows it. */
export interface LibraryPart {
  id: string;
  name: string;
  source: string;
  role: Role | null;
  /** The role is the library's guess from the names, not the rider's choice. */
  roleGuessed: boolean;
  empties: PartEmpty[];
  objects: number;
  meshes: number;
  tris: number;
  bounds: PartInspection["bounds"];
  hasThumb: boolean;
  hasGlb: boolean;
  added: number;
  /** A `data:` URL of the picture Blender rendered, when it rendered one. */
  thumb: string | null;
  /** The rider's file changed since it was added: add it again to refresh. */
  stale: boolean;
  /** The rider's file is gone. */
  missing: boolean;
}

export type Slots = Partial<Record<Role, string>>;

export interface PartLibrary {
  parts: LibraryPart[];
  slots: Slots;
}

export function listParts(): Promise<PartLibrary> {
  return invoke<PartLibrary>("bike_parts_list");
}

/** Run a part through Blender into the library. Adding one already there refreshes it. */
export function addPart(part: string): Promise<LibraryPart> {
  return invoke<LibraryPart>("bike_part_add", { part });
}

export function setPartRole(id: string, role: Role | null): Promise<LibraryPart> {
  return invoke<LibraryPart>("bike_part_set_role", { id, role });
}

/** Out of the library; the rider's own file stays. */
export function removePart(id: string): Promise<void> {
  return invoke<void>("bike_part_remove", { id });
}

export function setSlot(role: Role, id: string | null): Promise<Slots> {
  return invoke<Slots>("bike_slot_set", { role, id });
}
