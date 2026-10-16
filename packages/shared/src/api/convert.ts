import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * Convert: FBX to EDF on this machine, in Frost's Studio and Frost Converter alike. Mirrors
 * `crates/fbx2edf/src/commands.rs`, which runs the converter mxbsecure.com/convert runs in the
 * browser, natively.
 */

/** The browser converter's options, read the same way. Anything left out takes its default. */
export interface ConvertOptions {
  /** The text of an `fbx2edf.exe` parameter file or export script. */
  params?: string;
  layout?: "whole" | "parts";
  main?: string;
  scale?: number;
  mergeDistance?: number;
  recalcNormals?: number;
  useFileNormals?: boolean;
  /** Two textures at once rather than one per core. */
  lowMemory?: boolean;
}

export interface ConvertReport {
  objects: { name: string; nodes: number; vertices: number; triangles: number; materials: number }[];
  /** `flags` is the word written: 0x1 colour key, 0x2 alpha blend. */
  textures: { name: string; width: number; height: number; flags: number; maps: string[] }[];
  warnings: string[];
  /** Scene nodes that draw nothing in the file, each with the reason. */
  skipped: { name: string; reason: string }[];
  bytes: number;
  millis: number;
}

export interface Outcome {
  report: ConvertReport | null;
  error: string | null;
  /** The `.hrc` files written beside a bike's `.edf`. */
  hrc: string[];
  /** The `.shd` files made in the model's folder: `bike.shd: normal bike_n, reflection bike_r`. */
  shd: string[];
  /** What was written beside the `.edf`: the `.edf` and its `.hrc` files. */
  files: string[];
  /** The `.shd` files this conversion made in the model's folder, where the converter reads them. */
  shdFiles: string[];
  /** Each material's `.shd` in the model's folder, written now or already there. */
  shaders: ShdFile[];
  /** A map used by name because the material doesn't link it, `.hrc` files kept or replaced. */
  notes: string[];
}

/** What a `.shd` sets besides its maps. Stock is 6, 0, 0.5 and 1. */
export interface ShdValues {
  shininess: number;
  reflectionMin: number;
  reflectionMax: number;
  reflectionExp: number;
}

export const STOCK_SHD: ShdValues = { shininess: 6, reflectionMin: 0, reflectionMax: 0.5, reflectionExp: 1 };

/** The maps a `.shd` names, as it names them. */
export interface ShdMaps {
  normal: string | null;
  reflection: string | null;
  specular: string | null;
}

/** One material's `.shd`, as it is on disk. */
export interface ShdFile {
  /** The material's picture, without extension: the `.shd`'s name. */
  name: string;
  path: string;
  /** Written by this conversion, rather than one that was already there. */
  written: boolean;
  maps: ShdMaps;
  values: ShdValues;
}

/**
 * When a bike's `.hrc` files are written: every time (one already there and different is kept
 * as `.bak`), only into a folder that has none, or never.
 */
export type HrcMode = "always" | "ifNone" | "never";

export interface Found {
  path: string;
  kind: "model" | "shadow" | "params";
  size: number;
}

export const convertAvailable = () => invoke<boolean>("fbx_convert_available");
export const scanPaths = (paths: string[]) => invoke<Found[]>("fbx_scan", { paths });
export const isBike = (path: string) => invoke<boolean>("fbx_is_bike", { path });
/** An export script kept between runs. */
export interface SavedScript {
  name: string;
  text: string;
}
/** The scripts kept in the app's config folder: the last one used, and a saved list. */
export interface KeptScripts {
  last: SavedScript | null;
  saved: SavedScript[];
}
export const keptScripts = () => invoke<KeptScripts>("fbx_scripts");
/** Remember the script just used; empty text forgets it. */
export const rememberScript = (name: string, text: string) => invoke<KeptScripts>("fbx_scripts_last", { name, text });
export const saveScript = (name: string, text: string) => invoke<KeptScripts>("fbx_scripts_save", { name, text });
export const renameScript = (from: string, to: string) => invoke<KeptScripts>("fbx_scripts_rename", { from, to });
export const removeScript = (name: string) => invoke<KeptScripts>("fbx_scripts_remove", { name });
export const readText = (path: string) => invoke<string>("fbx_read_text", { path });

export function convertFiles(
  pairs: { input: string; output: string }[],
  options: ConvertOptions,
  hrc: HrcMode,
  makeShd = true,
): Promise<Outcome[]> {
  return invoke<Outcome[]>("fbx_convert", { pairs, options, hrc, writeShd: makeShd });
}

/** Write a material's `.shd` with these values: what the sliders do. */
export const writeShd = (path: string, maps: ShdMaps, values: ShdValues) =>
  invoke<void>("fbx_write_shd", { path, maps, values });

export function onProgress(f: (e: { index: number; done: number; total: number }) => void): Promise<UnlistenFn> {
  return listen<{ index: number; done: number; total: number }>("fbx-convert-progress", (e) => f(e.payload));
}

export function onDone(f: (e: { index: number; outcome: Outcome }) => void): Promise<UnlistenFn> {
  return listen<{ index: number; outcome: Outcome }>("fbx-convert-done", (e) => f(e.payload));
}

export const dirOf = (p: string) => p.slice(0, Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/")));
export const nameOf = (p: string) => p.slice(Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/")) + 1);

/**
 * Where each model is written: beside it, or in `folder` when one is given. A bike's shadow
 * model is always `shadow_model.edf` and, with `bike`, the bike itself is `model.edf`: the
 * names MX Bikes looks for. Otherwise a model keeps its FBX's name. Two models landing on one
 * name are kept apart with a number, as the browser converter does.
 */
export const outputPaths = (models: { path: string; shadow: boolean }[], folder: string | null, bike = false, name: string | null = null) =>
  invoke<string[]>("fbx_output_paths", { models, folder, bike, name });

const PARTS = ["chassis", "steer", "fsusp", "rsusp"];

/**
 * What an export script sets, for the line under its slot: the bike parts its `[Hierarchy]`
 * names (in the game's order), then the `[params]` keys it gives, each once.
 */
export function scriptSettings(text: string): string[] {
  let section = "";
  const parts = new Set<string>();
  const params: string[] = [];
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.split(/[;#]/)[0].trim();
    if (!line) continue;
    const sec = /^\[(.*)\]$/.exec(line);
    if (sec) {
      section = sec[1].trim().toLowerCase();
      continue;
    }
    const kv = /^([a-z_]+)\s*=/i.exec(line);
    if (!kv) continue;
    const key = kv[1].toLowerCase();
    if (section === "hierarchy") {
      const part = key.replace(/_(rot|append)$/, "");
      if (PARTS.includes(part)) parts.add(part);
    } else if (section === "params" && !params.includes(key)) params.push(key);
  }
  return [...PARTS.filter((p) => parts.has(p)), ...params];
}

/**
 * Check a pasted export script the way the browser converter does. Bike parts need a
 * `[Hierarchy]` section naming at least one bike part; a single object (a chain, a prop, a
 * helmet) has no bike part to name, so with `layout` "whole" any script passes. The text
 * itself goes to the converter as is.
 */
export function checkScript(text: string, layout: "whole" | "parts" = "parts"): "ok" | "noSection" | "noParts" {
  if (layout === "whole") return "ok";
  let inside = false;
  let section = false;
  let parts = 0;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.split(/[;#]/)[0].trim();
    if (!line) continue;
    const sec = /^\[(.*)\]$/.exec(line);
    if (sec) {
      inside = sec[1].trim().toLowerCase() === "hierarchy";
      if (inside) section = true;
      continue;
    }
    if (!inside) continue;
    const kv = /^([a-z_]+)\s*=\s*(.*)$/i.exec(line);
    if (!kv) continue;
    const key = kv[1].toLowerCase();
    if (key.endsWith("_rot")) {
      const n = kv[2].trim().split(/\s+/).filter(Boolean).map(Number);
      if (PARTS.includes(key.slice(0, -4)) && n.length === 3 && n.every(Number.isFinite)) parts++;
    } else if (PARTS.includes(key)) parts++;
  }
  return !section ? "noSection" : parts === 0 ? "noParts" : "ok";
}

/** Watch these source files: `onSourceChanged` fires when one is saved. Replaces any earlier watch. */
export const watchSources = (paths: string[]) => invoke<void>("fbx_watch", { paths });
export const unwatchSources = () => invoke<void>("fbx_unwatch");
export const onSourceChanged = (f: (path: string) => void): Promise<UnlistenFn> => listen<{ path: string }>("fbx-source-changed", (e) => f(e.payload.path));
