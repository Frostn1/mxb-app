import { invoke } from "@tauri-apps/api/core";
import { reviveMesh } from "@frost/shared/api/mods";
import type { BikeModel } from "@frost/shared/types";

/** The Gfx tab. Mirrors `src-tauri/src/gfxedit.rs`; the editing is `mxb_core::gfxedit`. */

export interface GfxFile {
  gfxPath: string;
  label: string;
  variant: string | null;
  active: boolean;
  /** Set when FrostMod can be told to re-read the file. */
  bikeId: string | null;
  /** Path (`steer/leftgrip/pos/x`) to value as written. The first-person copy is under `cockpit/`. */
  fields: Record<string, string>;
}

export type ReloadOutcome = "signaled" | "not_running" | "write_failed" | "unsupported" | "withheld";

export interface GfxSaved {
  file: GfxFile;
  reload: ReloadOutcome | null;
}

export const gfxOpen = (path: string) => invoke<GfxFile>("gfx_open", { path });
export const gfxPreview = (path: string) => invoke<BikeModel>("gfx_preview", { path }).then(reviveMesh);
export const gfxSave = (path: string, edits: [string, string][]) => invoke<GfxSaved>("gfx_save", { path, edits });

/** Which part's frame a point is written in. */
export type Frame = "chassis" | "steer";

/** A draggable point: three fields under one block. */
export interface GfxPoint {
  id: string;
  /** Path to the block holding x/y/z. */
  path: string;
  frame: Frame;
}

export const POINTS: GfxPoint[] = [
  { id: "leftgrip", path: "steer/leftgrip/pos", frame: "steer" },
  { id: "rightgrip", path: "steer/rightgrip/pos", frame: "steer" },
  { id: "engine", path: "chassis/chain/engine", frame: "chassis" },
  { id: "exhaust", path: "chassis/exhaust/pos", frame: "chassis" },
];

/** Lever-type blocks: a node turned about `axis` by up to `maxrot` degrees. */
export const LEVERS = [
  { id: "throttlegrip", path: "steer/throttlegrip" },
  { id: "brakelever", path: "steer/brakelever" },
  { id: "clutchlever", path: "steer/clutchlever" },
  { id: "rearbrakepedal", path: "chassis/rearbrakepedal" },
  { id: "shifter", path: "chassis/shifter" },
] as const;

export const AXES = ["x", "x-", "y", "y-", "z", "z-"] as const;

/** Mirror a left grip onto the right, or back: the same point across the centreline. */
export function mirrorGrip(values: Record<string, string>, from: "leftgrip" | "rightgrip"): Record<string, string> {
  const to = from === "leftgrip" ? "rightgrip" : "leftgrip";
  const out = { ...values };
  const x = Number(values[`steer/${from}/pos/x`]);
  if (values[`steer/${from}/pos/x`] !== undefined && Number.isFinite(x)) out[`steer/${to}/pos/x`] = fmt(-x);
  for (const k of ["y", "z"]) {
    const v = values[`steer/${from}/pos/${k}`];
    if (v !== undefined) out[`steer/${to}/pos/${k}`] = v;
  }
  return out;
}

/** Metres to the millimetre, without trailing zeros. */
export function fmt(n: number): string {
  const r = Math.round(n * 1000) / 1000;
  return String(Object.is(r, -0) ? 0 : r);
}

/** The edits a save sends: every main-copy field that changed. */
export function changes(values: Record<string, string>, saved: Record<string, string>): [string, string][] {
  return Object.entries(values).filter(([k, v]) => !k.startsWith("cockpit/") && v.trim() !== "" && v.trim() !== (saved[k] ?? "").trim());
}

/** Main-copy fields whose `cockpit` copy says something else. */
export function outOfStep(fields: Record<string, string>): string[] {
  return Object.keys(fields).filter((k) => {
    const c = fields[`cockpit/${k}`];
    return !k.startsWith("cockpit/") && c !== undefined && c.trim() !== fields[k].trim();
  });
}
