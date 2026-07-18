import type { BikeModels, LibraryEntry, Loadout } from "../types";
import {
  scanLibrary,
  scanRiderTargets,
  scanModelSwaps,
  type RiderTargets,
} from "../api/mods";

/**
 * Builder metadata + option sourcing for customization presets.
 *
 * MX Bikes' `profile.ini` stores each slot's value as a plain reference to an
 * installed mod/paint folder name (empty = stock). Here we enumerate what's
 * installed so the builder can offer real choices per slot, resolve which options
 * depend on another slot (helmet paints depend on the helmet, etc.), and flag a
 * value whose mod isn't installed (useful when loading a shared preset).
 */

/** Slot fields in the order the builder shows them, grouped for layout. */
export interface SlotDef {
  key: keyof Loadout;
  label: string;
  group: "bike" | "rider" | "head" | "body";
  /** Which other slot this slot's options depend on (for dependent dropdowns). */
  dependsOn?: "bikeid" | "helmet" | "boots" | "protection" | "rider";
  /** Free-text slot (fonts) — no installed source to enumerate. */
  freeText?: boolean;
}

export const SLOTS: SlotDef[] = [
  { key: "paint", label: "Bike livery", group: "bike", dependsOn: "bikeid" },
  { key: "modelSwap", label: "Model swap", group: "bike", dependsOn: "bikeid" },
  { key: "bikeFont", label: "Number font", group: "bike", freeText: true },
  { key: "tyres", label: "Tyres", group: "bike" },
  { key: "rider", label: "Rider profile", group: "rider" },
  { key: "suitPaint", label: "Kit / suit", group: "rider", dependsOn: "rider" },
  { key: "suitFont", label: "Suit font", group: "rider", freeText: true },
  { key: "glovesPaint", label: "Gloves", group: "rider" },
  { key: "ridingStyle", label: "Riding style", group: "rider" },
  { key: "helmet", label: "Helmet", group: "head" },
  { key: "helmetPaint", label: "Helmet paint", group: "head", dependsOn: "helmet" },
  { key: "gogglesPaint", label: "Goggles", group: "head", dependsOn: "helmet" },
  { key: "boots", label: "Boots", group: "body" },
  { key: "bootsPaint", label: "Boot paint", group: "body", dependsOn: "boots" },
  { key: "protection", label: "Protection", group: "body" },
  { key: "protectionPaint", label: "Protection paint", group: "body", dependsOn: "protection" },
];

export const SLOT_GROUPS: { id: SlotDef["group"]; label: string }[] = [
  { id: "bike", label: "Bike" },
  { id: "rider", label: "Rider" },
  { id: "head", label: "Head" },
  { id: "body", label: "Body" },
];

/** A blank loadout — every slot stock/empty. The zero value both the Presets
 * builder and the Rider studio start from. */
export const EMPTY_LOADOUT: Loadout = {
  paint: "",
  bikeFont: "",
  rider: "",
  helmet: "",
  helmetPaint: "",
  gogglesPaint: "",
  suitPaint: "",
  suitFont: "",
  boots: "",
  bootsPaint: "",
  glovesPaint: "",
  protection: "",
  protectionPaint: "",
  ridingStyle: "",
  tyres: "",
  raceNumber: "",
  modelSwap: "",
};

/** Built-in values the game always accepts even with no installed mod. */
const BUILTINS: Partial<Record<keyof Loadout, string[]>> = {
  helmet: ["default"],
  boots: ["default"],
  protection: ["full", "neck"],
  bikeFont: ["default_black", "default_white"],
  suitFont: ["default_white", "default_black"],
  ridingStyle: ["mx", "sm"],
  tyres: ["p_mx"],
};

/** Everything installed that the builder can offer, indexed for quick lookup. */
export interface Scans {
  bikePaints: Record<string, string[]>; // bikeid → livery names
  modelSwaps: Record<string, string[]>; // bikeid → model-swap variant names
  helmets: string[];
  helmetPaints: Record<string, string[]>; // helmet model → paints
  goggles: Record<string, string[]>; // helmet model / rider profile → goggles
  boots: string[];
  bootPaints: Record<string, string[]>; // boots model → paints
  protection: string[];
  protectionPaints: Record<string, string[]>; // protection model → paints
  gloves: string[];
  outfits: Record<string, string[]>; // rider profile → kit paints
  riderProfiles: string[];
  tyres: string[];
}

/** Drop a trailing `.pnt`/`.pkz`/`.zip` from a paint/model file name. */
function stripExt(name: string): string {
  return name.replace(/\.(pnt|pkz|zip)$/i, "");
}

function push(map: Record<string, string[]>, key: string, val: string) {
  (map[key] ??= []).push(val);
}

/** Load + index everything installed that the preset builder needs. */
export async function loadScans(): Promise<Scans> {
  const [bikes, rider, tyres, targets, swaps] = await Promise.all([
    scanLibrary("mods/bikes").catch(() => [] as LibraryEntry[]),
    scanLibrary("mods/rider").catch(() => [] as LibraryEntry[]),
    scanLibrary("mods/tyres").catch(() => [] as LibraryEntry[]),
    scanRiderTargets().catch(
      () => ({ helmets: [], boots: [], protection: [], profiles: [] }) as RiderTargets,
    ),
    scanModelSwaps().catch(() => [] as BikeModels[]),
  ]);

  const s: Scans = {
    bikePaints: {},
    modelSwaps: {},
    helmets: [...targets.helmets],
    helmetPaints: {},
    goggles: {},
    boots: [...targets.boots],
    bootPaints: {},
    protection: [...targets.protection],
    protectionPaints: {},
    gloves: [],
    outfits: {},
    riderProfiles: [...targets.profiles],
    tyres: [],
  };

  for (const e of bikes) {
    if (e.category === "bikePaint" && e.parent) push(s.bikePaints, e.parent, stripExt(e.name));
  }
  // Model-swap variants per bike (only bikes that actually have alternate models).
  for (const b of swaps) {
    if (b.variants.length) s.modelSwaps[b.bike] = b.variants.map((v) => v.name);
  }
  for (const e of rider) {
    const v = stripExt(e.name);
    switch (e.category) {
      case "helmetPaint":
        if (e.parent) push(s.helmetPaints, e.parent, v);
        break;
      case "goggles":
        if (e.parent) push(s.goggles, e.parent, v);
        break;
      case "bootPaint":
        if (e.parent) push(s.bootPaints, e.parent, v);
        break;
      case "protectionPaint":
        if (e.parent) push(s.protectionPaints, e.parent, v);
        break;
      case "gloves":
        s.gloves.push(v);
        break;
      case "outfit":
        if (e.parent) push(s.outfits, e.parent, v);
        break;
    }
  }
  for (const e of tyres) s.tyres.push(stripExt(e.name));

  // De-dupe + sort every list for stable, tidy dropdowns.
  const tidy = (a: string[]) => [...new Set(a)].sort((x, y) => x.localeCompare(y));
  s.helmets = tidy(s.helmets);
  s.boots = tidy(s.boots);
  s.protection = tidy(s.protection);
  s.gloves = tidy(s.gloves);
  s.riderProfiles = tidy(s.riderProfiles);
  s.tyres = tidy(s.tyres);
  for (const m of [s.bikePaints, s.helmetPaints, s.goggles, s.bootPaints, s.protectionPaints, s.outfits])
    for (const k of Object.keys(m)) m[k] = tidy(m[k]);

  return s;
}

/**
 * The installed option values for a slot, given the current bike + loadout (so
 * dependent slots resolve against the selected helmet/boots/etc.). Builtins are
 * appended. Does NOT include the current value or the empty "stock" option — the
 * UI adds those.
 */
export function slotOptions(
  slot: SlotDef,
  bikeid: string,
  loadout: Loadout,
  scans: Scans,
): string[] {
  let opts: string[] = [];
  switch (slot.key) {
    case "paint":
      opts = scans.bikePaints[bikeid] ?? [];
      break;
    case "modelSwap":
      opts = scans.modelSwaps[bikeid] ?? [];
      break;
    case "helmet":
      opts = scans.helmets;
      break;
    case "helmetPaint":
      opts = scans.helmetPaints[loadout.helmet] ?? [];
      break;
    case "gogglesPaint":
      opts = [...(scans.goggles[loadout.helmet] ?? []), ...(scans.goggles[loadout.rider] ?? [])];
      break;
    case "boots":
      opts = scans.boots;
      break;
    case "bootsPaint":
      opts = scans.bootPaints[loadout.boots] ?? [];
      break;
    case "protection":
      opts = scans.protection;
      break;
    case "protectionPaint":
      opts = scans.protectionPaints[loadout.protection] ?? [];
      break;
    case "glovesPaint":
      opts = scans.gloves;
      break;
    case "suitPaint":
      opts = scans.outfits[loadout.rider] ?? [];
      break;
    case "rider":
      opts = scans.riderProfiles;
      break;
    case "tyres":
      opts = scans.tyres;
      break;
    default:
      opts = [];
  }
  const builtins = BUILTINS[slot.key] ?? [];
  return [...new Set([...opts, ...builtins])];
}

/** Whether a slot's chosen value references a mod that isn't installed (empty and
 * built-in/free-text values are never "missing"). Used to warn on shared presets. */
export function isMissing(
  slot: SlotDef,
  bikeid: string,
  loadout: Loadout,
  scans: Scans,
): boolean {
  if (slot.freeText) return false;
  const val = loadout[slot.key];
  if (!val) return false;
  return !slotOptions(slot, bikeid, loadout, scans).includes(val);
}

/** All slots in a loadout whose referenced mod is missing (for a preset preview). */
export function missingSlots(bikeid: string, loadout: Loadout, scans: Scans): SlotDef[] {
  return SLOTS.filter((s) => isMissing(s, bikeid, loadout, scans));
}

/** A short human summary of the non-stock slots in a loadout (for preset cards). */
export function loadoutSummary(loadout: Loadout): string {
  const parts: string[] = [];
  if (loadout.helmet && loadout.helmet !== "default") parts.push(loadout.helmet);
  if (loadout.paint) parts.push(loadout.paint);
  if (loadout.suitPaint) parts.push(loadout.suitPaint);
  return parts.slice(0, 3).join(" · ") || "Stock look";
}
