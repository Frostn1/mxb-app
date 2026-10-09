import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * Convert: FBX to EDF on this machine. Mirrors `src-tauri/src/modelconvert.rs`, which runs the
 * converter mxbsecure.com/convert runs in the browser, natively.
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
  textures: { name: string; width: number; height: number; maps: string[] }[];
  warnings: string[];
  bytes: number;
  millis: number;
}

export interface Outcome {
  report: ConvertReport | null;
  error: string | null;
  /** The `.hrc` files written beside a bike's `.edf`. */
  hrc: string[];
  /** The `.shd` files written for its textures: `bike.shd: normal bike_n, reflection bike_r`. */
  shd: string[];
}

export interface Found {
  path: string;
  kind: "model" | "shadow" | "params";
  size: number;
}

export const convertAvailable = () => invoke<boolean>("fbx_convert_available");
export const scanPaths = (paths: string[]) => invoke<Found[]>("fbx_scan", { paths });
export const partNames = (path: string) => invoke<string[]>("fbx_part_names", { path });
export const readText = (path: string) => invoke<string>("fbx_read_text", { path });

export function convertFiles(
  pairs: { input: string; output: string }[],
  options: ConvertOptions,
  hrc: boolean,
  shd: { make: boolean; overwrite: boolean } = { make: false, overwrite: false },
): Promise<Outcome[]> {
  return invoke<Outcome[]>("fbx_convert", { pairs, options, hrc, shd: shd.make, overwriteShd: shd.overwrite });
}

export function onProgress(f: (e: { index: number; done: number; total: number }) => void): Promise<UnlistenFn> {
  return listen<{ index: number; done: number; total: number }>("fbx-convert-progress", (e) => f(e.payload));
}

export function onDone(f: (e: { index: number; outcome: Outcome }) => void): Promise<UnlistenFn> {
  return listen<{ index: number; outcome: Outcome }>("fbx-convert-done", (e) => f(e.payload));
}

const sep = (p: string) => (p.includes("\\") ? "\\" : "/");
export const dirOf = (p: string) => p.slice(0, Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/")));
export const nameOf = (p: string) => p.slice(Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/")) + 1);
const stemOf = (name: string) => name.replace(/\.[^.]*$/, "");

/**
 * Where each model is written: `<name>.edf` beside it, or in `folder` when one is given. A
 * bike's shadow model is always `shadow_model.edf`, the name MX Bikes looks for. Two models
 * landing on one name are kept apart with a number, as the browser converter does.
 */
export function outputPaths(models: { path: string; shadow: boolean }[], folder: string | null): string[] {
  const taken = new Set<string>();
  return models.map((m) => {
    const dir = folder || dirOf(m.path);
    const s = sep(dir || m.path);
    const stem = m.shadow ? "shadow_model" : stemOf(nameOf(m.path));
    let name = `${dir}${s}${stem}.edf`;
    for (let n = 2; taken.has(name.toLowerCase()); n++) name = `${dir}${s}${stem}_${n}.edf`;
    taken.add(name.toLowerCase());
    return name;
  });
}

/** Whether Parts-planned object names look like a bike's, as the browser converter guesses. */
export function looksLikeBike(names: string[]): boolean {
  const has = (n: string) => names.some((x) => x.toLowerCase() === n);
  const others = ["fsusp", "rsusp", "steer"].filter(has).length;
  return others >= 2 || (has("chassis") && others >= 1);
}

const PARTS = ["chassis", "steer", "fsusp", "rsusp"];

/**
 * Check a pasted export script the way the browser converter does: it needs a `[Hierarchy]`
 * section naming at least one bike part. The text itself goes to the converter as is.
 */
export function checkScript(text: string): "ok" | "noSection" | "noParts" {
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
