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
export const ROLES = [
  "chassis",
  "steer",
  "fsusp",
  "rsusp",
  "wheel_f",
  "wheel_r",
  "levers",
  "pedals",
  "handguards",
  "plate",
] as const;
export type Role = (typeof ROLES)[number];

/** Which role(s) a mount point takes, the frontend's mirror of `role_rule` in
 *  `bikeassemble.rs` — kept in sync by hand since the mount names are the join between
 *  the two sides. Used only to turn a click on an anchor dot into "which role were you
 *  trying to fill", never to compute a placement: the backend's `place()` still owns that.
 *  A mount with more than one role (the steer's `handlebar` takes levers and handguards
 *  both) offers all of them. */
export const MOUNT_ROLES: Record<string, Role[]> = {
  steer_axis: ["steer"],
  swingarm_pivot: ["rsusp"],
  footpegs: ["pedals"],
  fork_clamp: ["fsusp"],
  front_axle: ["wheel_f"],
  rear_axle: ["wheel_r"],
  handlebar: ["levers", "handguards"],
  plate_mount: ["plate"],
};

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
  /** The object names inside hint at three or more different roles — this file is probably
   *  a whole bike (or a big sub-assembly), not the one part `role` says. Studio still picked
   *  its best single guess rather than leave the part unusable; this says to check it. */
  multiPartHint: boolean;
  /** What `multiPartHint` found, broken down: how many objects hinted at each role. Only
   *  meaningful alongside `multiPartHint` — empty otherwise. */
  roleHints: Partial<Record<Role, number>>;
  /** Texture files this part needs that couldn't be found. */
  missingTextures: string[];
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

/** Cut a part with `multiPartHint` into its parts, by name — each object's own name if it
 *  hints at a role, else its nearest named ancestor's. Replaces it in the library with one
 *  new part per group Blender found. */
export function splitPart(id: string): Promise<PartLibrary> {
  return invoke<PartLibrary>("bike_part_split", { id });
}

export function setSlot(role: Role, id: string | null): Promise<Slots> {
  return invoke<Slots>("bike_slot_set", { role, id });
}

// ---------------------------------------------------------------------------
// putting it together (phase C) and the preview (D)

export type V3 = [number, number, number];

export type TemplateSource = { kind: "placeholder" } | { kind: "bike"; path: string };

/** One part on the bike. Mirrors `bikeassemble::Placed`; points are in Blender's frame. */
export interface Placed {
  role: Role;
  partId: string;
  offset: V3;
  nudge: V3;
  mount: string | null;
  at: V3 | null;
  /** How the part's end of the joint was found. */
  by: "empty" | "centre" | "as modelled";
  group: "chassis" | "steer" | "fsusp" | "rsusp" | null;
}

export interface AssemblyView {
  template: { source: TemplateSource; name: string; rideable: boolean; problem: string | null };
  assembly: { placed: Placed[]; anchors: Record<string, V3> };
  name: string;
}

/** Make Studio's placeholder bike and slot all eight parts: the builder, ready to try. */
export function addPlaceholderBike(): Promise<PartLibrary> {
  return invoke<PartLibrary>("bike_placeholder_add");
}

export function getAssembly(): Promise<AssemblyView> {
  return invoke<AssemblyView>("bike_assembly");
}

/** Move a role's part by `delta` metres (Blender's frame); null puts it back where it snapped. */
export function nudge(role: Role, delta: V3 | null): Promise<AssemblyView> {
  return invoke<AssemblyView>("bike_nudge", { role, delta });
}

/** An installed bike's folder or .pkz as the template; null for Studio's placeholder. */
export function setTemplate(path: string | null): Promise<AssemblyView> {
  return invoke<AssemblyView>("bike_template_set", { path });
}

export function setBuildName(name: string): Promise<AssemblyView> {
  return invoke<AssemblyView>("bike_build_name_set", { name });
}

/** A part's preview model, as GLB bytes. */
export function partGlb(id: string): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("bike_part_glb", { id });
}

/** Blender's frame (Z up, facing -Y) → glTF's and three.js' (Y up), as Blender's glTF export maps it. */
export function toThree(p: V3): V3 {
  return [p[0], p[2], -p[1]];
}

// ---------------------------------------------------------------------------
// the build (phase E)

export interface BuildReport {
  folder: string;
  files: string[];
  tris: Record<string, number>;
  shadowTris: Record<string, number>;
  converted: boolean;
  converter: string;
  rideable: boolean;
  notes: string[];
}

export function buildBike(): Promise<BuildReport> {
  return invoke<BuildReport>("bike_build");
}

// ---------------------------------------------------------------------------
// the Part Maker (phase F)

export interface Slider {
  name: string;
  default: number;
  min: number;
  max: number;
}

export interface MakeTemplate {
  role: Role;
  about: string;
  sliders: Slider[];
  choices: Record<string, string[]>;
}

export type Params = Record<string, number | string>;

export interface MakeAnswer {
  template: string | null;
  params: Params;
  code: string | null;
  role: Role | null;
  name: string;
  reply: string;
  /** The model that answered, or "words" when none is set up and the brief was read here. */
  by: string;
}

export interface MadePreview {
  thumb: string | null;
  role: Role;
  tris: number;
  size: V3 | null;
}

export function makeTemplates(): Promise<Record<string, MakeTemplate>> {
  return invoke<Record<string, MakeTemplate>>("bike_make_templates");
}

export function makeAsk(brief: string, current: unknown, images: string[]): Promise<MakeAnswer> {
  return invoke<MakeAnswer>("bike_make_ask", { brief, current, images });
}

export function makePreview(
  req: { template: string; params: Params } | { code: string; role: Role },
): Promise<MadePreview> {
  return "code" in req
    ? invoke<MadePreview>("bike_make_preview", { code: req.code, role: req.role })
    : invoke<MadePreview>("bike_make_preview", { template: req.template, params: req.params });
}

/** Keep the last preview: into the library, and its slot when that's empty. */
export function makeKeep(name: string): Promise<LibraryPart> {
  return invoke<LibraryPart>("bike_make_keep", { name });
}

// ---------------------------------------------------------------------------
// the bike template picker

/** Which of these `.pkz` bikes Studio can actually read a template from — a cheap header
 *  check, nothing opened or decrypted. An entry's `locked` (from `scanLibrary`) only ever
 *  means "an .mxbsecure blob with no key"; the game's own OEM content ships as a plain
 *  `.pkz` in a format only a build with the (optional, not always present) sidecar module
 *  can open, and this is how the picker tells the two apart. */
export function bikeTemplateReadable(paths: string[]): Promise<Record<string, boolean>> {
  return invoke<Record<string, boolean>>("bike_template_readable", { paths });
}
