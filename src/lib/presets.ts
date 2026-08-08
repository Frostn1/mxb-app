import type { BikeModels, LibraryEntry, Loadout } from "../types";
import {
  scanLibrary,
  scanRiderTargets,
  scanModelSwaps,
  type RiderTargets,
} from "../api/mods";

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

/**
 * Bucket key for paints installed without an owning model folder — a loose `.pnt`
 * dropped straight into `mods/rider/boots` belongs to no model, so it's offered for
 * every model rather than discarded.
 */
export const ANY_MODEL = "*";

export interface Scans {
  bikePaints: Record<string, string[]>; // bikeid → livery names
  modelSwaps: Record<string, string[]>; // bikeid → model-swap variant names
  activeModel: Record<string, string>; // bikeid → the model swap currently on the bike
  /** bikeid → variant name → the liveries that variant claims. See {@link forActiveModel}. */
  modelPaints: Record<string, Record<string, string[]>>;
  helmets: string[];
  helmetPaints: Record<string, string[]>; // helmet model (or ANY_MODEL) → paints
  goggles: Record<string, string[]>; // helmet model / rider profile → goggles
  boots: string[];
  bootPaints: Record<string, string[]>; // boots model (or ANY_MODEL) → paints
  protection: string[];
  protectionPaints: Record<string, string[]>; // protection model (or ANY_MODEL) → paints
  gloves: string[];
  outfits: Record<string, string[]>; // rider profile → kit paints
  riderProfiles: string[];
  tyres: string[];
}

function stripExt(name: string): string {
  return name.replace(/\.(pnt|pkz|zip)$/i, "");
}

function push(map: Record<string, string[]>, key: string, val: string) {
  (map[key] ??= []).push(val);
}

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
    activeModel: {},
    modelPaints: {},
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
  for (const b of swaps) {
    // Only offer sets that can actually be applied — an incomplete one (files but no
    // mesh) is rejected by the backend, so offering it only produces a failed apply.
    const usable = b.variants.filter((v) => v.valid || v.empty).map((v) => v.name);
    if (usable.length) s.modelSwaps[b.bike] = usable;
    s.activeModel[b.bike] = b.active;
    // Every variant's claim, not just the active one's — a livery claimed by *any* model
    // is one the others shouldn't offer.
    s.modelPaints[b.bike] = Object.fromEntries(b.variants.map((v) => [v.name, v.paints]));
  }
  for (const e of rider) {
    const v = stripExt(e.name);
    switch (e.category) {
      case "helmetPaint":
        push(s.helmetPaints, e.parent ?? ANY_MODEL, v);
        break;
      case "goggles":
        push(s.goggles, e.parent ?? ANY_MODEL, v);
        break;
      case "bootPaint":
        push(s.bootPaints, e.parent ?? ANY_MODEL, v);
        break;
      case "protectionPaint":
        push(s.protectionPaints, e.parent ?? ANY_MODEL, v);
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
 * Look a bike up in a scan map. The scans key on the on-disk folder name while the slot
 * asks by the `bikeid` written in `profile.ini`; those agree in case only by convention,
 * and an exact match silently yielded an empty dropdown whenever they didn't.
 */
function byBike<T>(map: Record<string, T>, bikeid: string): T | undefined {
  if (map[bikeid] !== undefined) return map[bikeid];
  const hit = Object.keys(map).find((k) => k.toLowerCase() === bikeid.toLowerCase());
  return hit === undefined ? undefined : map[hit];
}

function forBike(map: Record<string, string[]>, bikeid: string): string[] {
  return byBike(map, bikeid) ?? [];
}

/**
 * Narrow a bike's liveries to the ones that suit the model swap currently on it: the
 * liveries that model claims, plus the ones no model claims at all.
 *
 * A bike wears one model at a time but its liveries all share the single `paints/` folder
 * the game insists on, so a Yami mesh on a KTM otherwise offers every KTM livery too.
 * Unclaimed liveries staying universal is the same rule {@link forModel} applies to gear
 * paints installed without an owning model — and it means a bike nobody has assigned
 * anything on behaves exactly as before.
 */
function forActiveModel(liveries: string[], bikeid: string, scans: Scans): string[] {
  const claims = byBike(scans.modelPaints, bikeid);
  if (!claims) return liveries;
  const active = byBike(scans.activeModel, bikeid) ?? "";
  const keyed = (name: string) =>
    Object.entries(claims).find(([v]) => v.toLowerCase() === name.toLowerCase())?.[1] ?? [];

  const lower = (names: string[]) => new Set(names.map((n) => n.toLowerCase()));
  const mine = lower(keyed(active));
  const spokenFor = lower(Object.values(claims).flat());
  return liveries.filter((l) => mine.has(l.toLowerCase()) || !spokenFor.has(l.toLowerCase()));
}

/** Paints for `model`, plus any installed with no owning model folder. */
function forModel(map: Record<string, string[]>, model: string): string[] {
  return [...(map[model] ?? []), ...(map[ANY_MODEL] ?? [])];
}

/**
 * Per-profile assets for `profile` — or, with no rider profile picked yet, everything
 * installed across profiles, so the slot isn't mysteriously empty on a fresh Rider tab.
 */
function forProfile(map: Record<string, string[]>, profile: string): string[] {
  return profile ? (map[profile] ?? []) : Object.values(map).flat();
}

export function slotOptions(
  slot: SlotDef,
  bikeid: string,
  loadout: Loadout,
  scans: Scans,
): string[] {
  let opts: string[] = [];
  switch (slot.key) {
    case "paint":
      opts = forActiveModel(forBike(scans.bikePaints, bikeid), bikeid, scans);
      break;
    case "modelSwap":
      opts = forBike(scans.modelSwaps, bikeid);
      break;
    case "helmet":
      opts = scans.helmets;
      break;
    case "helmetPaint":
      opts = forModel(scans.helmetPaints, loadout.helmet);
      break;
    case "gogglesPaint":
      opts = [
        ...forModel(scans.goggles, loadout.helmet),
        ...forProfile(scans.goggles, loadout.rider),
      ];
      break;
    case "boots":
      opts = scans.boots;
      break;
    case "bootsPaint":
      opts = forModel(scans.bootPaints, loadout.boots);
      break;
    case "protection":
      opts = scans.protection;
      break;
    case "protectionPaint":
      opts = forModel(scans.protectionPaints, loadout.protection);
      break;
    case "glovesPaint":
      opts = scans.gloves;
      break;
    case "suitPaint":
      opts = forProfile(scans.outfits, loadout.rider);
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

export function missingSlots(bikeid: string, loadout: Loadout, scans: Scans): SlotDef[] {
  return SLOTS.filter((s) => isMissing(s, bikeid, loadout, scans));
}

export function loadoutSummary(loadout: Loadout): string {
  const parts: string[] = [];
  if (loadout.helmet && loadout.helmet !== "default") parts.push(loadout.helmet);
  if (loadout.paint) parts.push(loadout.paint);
  if (loadout.suitPaint) parts.push(loadout.suitPaint);
  return parts.slice(0, 3).join(" · ") || "Stock look";
}
