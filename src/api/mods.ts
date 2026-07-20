import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  BikeModels,
  BikeSounds,
  LooseSwapBike,
  RegisterReport,
  Config,
  DownloadOption,
  FrostmodReload,
  FrostmodStatus,
  InstalledMod,
  InstallProgress,
  LibraryEntry,
  Loadout,
  ModDetail,
  ModSummary,
  PkzMeta,
  PaintTexture,
  BikeModel,
  EdfNode,
  RiderModel,
  RiderPart,
  GearPaints,
  Preset,
  PresetApplyOutcome,
  ReloadOutcome,
  BundlePlan,
  BundleProgress,
} from "../types";

/** Results per page (mirrors `PER_PAGE` in the Rust backend). */
export const SEARCH_PAGE_SIZE = 24;

export interface ModCategory {
  id: number;
  label: string;
}

/** A top-level kind of mod, with its filter categories and install folder. */
export interface ModType {
  id: string;
  label: string;
  /** Parent WordPress category id (also the "All" filter). */
  categoryId: number;
  categories: ModCategory[];
  /** Relative folder under the MX Bikes root, e.g. `mods/tracks`. */
  installSubpath: string;
}

export const MOD_TYPES: ModType[] = [
  {
    id: "tracks",
    label: "Tracks",
    categoryId: 22,
    installSubpath: "mods/tracks",
    categories: [
      { id: 22, label: "All" },
      { id: 300, label: "Beginner" },
      { id: 301, label: "Intermediate" },
      { id: 302, label: "Pro" },
      { id: 119, label: "Assets" },
    ],
  },
  {
    id: "bikes",
    label: "Bikes",
    categoryId: 29,
    installSubpath: "mods/bikes",
    categories: [
      { id: 29, label: "All" },
      { id: 45, label: "New Bikes" },
      { id: 37, label: "Liveries" },
      { id: 46, label: "Sounds" },
    ],
  },
  {
    id: "rider",
    label: "Rider",
    categoryId: 30,
    installSubpath: "mods/rider",
    categories: [
      { id: 30, label: "All" },
      { id: 35, label: "Rider Kit" },
      { id: 313, label: "Helmets" },
      { id: 127, label: "Helmet Paints" },
      { id: 32, label: "Gloves" },
      { id: 343, label: "Boots" },
      { id: 126, label: "Boot Paints" },
      { id: 36, label: "Protection" },
      { id: 135, label: "Protection Paints" },
    ],
  },
];

export const DEFAULT_MOD_TYPE = MOD_TYPES[0];

/** Normalize a mod title or filename for fuzzy "already installed" matching. */
export function normalizeModName(s: string): string {
  return s
    .toLowerCase()
    .replace(/\.(pkz|zip|rar|7z)$/i, "")
    .replace(/[^a-z0-9]+/g, "");
}

export function isConfigured(): Promise<boolean> {
  return invoke<boolean>("is_configured");
}

export function getConfig(): Promise<Config> {
  return invoke<Config>("get_config");
}

export function createConfig(config: Config): Promise<boolean> {
  return invoke<boolean>("create_config", { config });
}

export function searchMods(
  query: string,
  categoryId: number,
  page = 1,
): Promise<ModSummary[]> {
  return invoke<ModSummary[]>("search_mods", { query, categoryId, page });
}

export function getModDetail(slug: string): Promise<ModDetail> {
  return invoke<ModDetail>("get_mod_detail", { slug });
}

export function getInstalledMods(subpath: string): Promise<InstalledMod[]> {
  return invoke<InstalledMod[]>("get_installed_mods", { subpath });
}

export function scanLibrary(subpath: string): Promise<LibraryEntry[]> {
  return invoke<LibraryEntry[]>("scan_library", { subpath });
}

export const LIVERY_CATEGORY_ID = 37;

export const SOUND_CATEGORY_ID = 46;

export function isLiveryContext(
  modType: ModType,
  categoryId: number | null | undefined,
): boolean {
  return modType.id === "bikes" && categoryId === LIVERY_CATEGORY_ID;
}

export function isSoundContext(
  modType: ModType,
  categoryId: number | null | undefined,
): boolean {
  return modType.id === "bikes" && categoryId === SOUND_CATEGORY_ID;
}

export interface RiderTargets {
  helmets: string[];
  boots: string[];
  protection: string[];
  profiles: string[];
}

export function scanRiderTargets(): Promise<RiderTargets> {
  return invoke<RiderTargets>("scan_rider_targets");
}

export function scanModelSwaps(): Promise<BikeModels[]> {
  return invoke<BikeModels[]>("scan_model_swaps");
}

export function applyModelSwap(bike: string, target: string): Promise<void> {
  return invoke<void>("apply_model_swap", { bike, target });
}

export function scanSoundSwaps(): Promise<BikeSounds[]> {
  return invoke<BikeSounds[]>("scan_sound_swaps");
}

export function applySoundSwap(bike: string, target: string): Promise<void> {
  return invoke<void>("apply_sound_swap", { bike, target });
}

/** Tie a sound variant to a model swap so activating that model applies the sound. */
export function bindSound(bike: string, model: string, sound: string): Promise<void> {
  return invoke<void>("bind_sound", { bike, model, sound });
}

export function unbindSound(bike: string, model: string): Promise<void> {
  return invoke<void>("unbind_sound", { bike, model });
}

/** Find model-set folders sitting loose in a bike dir (not yet under `FrostMod Models/`). */
export function detectLooseSwaps(): Promise<LooseSwapBike[]> {
  return invoke<LooseSwapBike[]>("detect_loose_swaps");
}

/**
 * Register the loose swaps found by {@link detectLooseSwaps}. With `move`, each set is
 * moved into its bike's `FrostMod Models/`; without it, only the folder is created.
 */
export function registerLooseSwaps(move: boolean): Promise<RegisterReport> {
  return invoke<RegisterReport>("register_loose_swaps", { moveFiles: move });
}

export function getPkzMeta(path: string): Promise<PkzMeta> {
  return invoke<PkzMeta>("get_pkz_meta", { path });
}

export function getPkzPreview(path: string): Promise<string | null> {
  return invoke<string | null>("get_pkz_preview", { path });
}

export function unpackPaint(path: string): Promise<PaintTexture[]> {
  return invoke<PaintTexture[]>("unpack_paint", { path });
}

export function unpackPkz(path: string, outDir: string): Promise<string[]> {
  return invoke<string[]>("unpack_pkz", { path, outDir });
}

export function loadBikeModel(source: string): Promise<BikeModel> {
  return invoke<BikeModel>("load_bike_model", { source });
}

export function loadRiderModel(loadout: Loadout): Promise<RiderModel> {
  return invoke<RiderModel>("load_rider_model", { loadout });
}

export function loadRiderBodyModel(profile: string): Promise<EdfNode[]> {
  return invoke<EdfNode[]>("load_rider_body_model", { profile });
}

export function loadGearModel(
  path: string,
  part: RiderPart["part"],
  paint?: string,
  goggles?: string,
): Promise<RiderPart> {
  return invoke<RiderPart>("load_gear_model", { path, part, paint, goggles });
}

export function listGearPaints(path: string): Promise<GearPaints> {
  return invoke<GearPaints>("list_gear_paints", { path });
}

export function listInstalledGearPaints(
  part: RiderPart["part"],
  model: string,
): Promise<GearPaints> {
  return invoke<GearPaints>("list_installed_gear_paints", { part, model });
}

export function loadStockGearModel(
  part: RiderPart["part"],
  paintPath?: string,
): Promise<RiderPart> {
  return invoke<RiderPart>("load_stock_gear_model", { part, paintPath });
}

/** Move an installed mod file into a different folder (relative to the type dir). */
export function moveMod(
  fromPath: string,
  toFolder: string,
  subpath: string,
): Promise<void> {
  return invoke<void>("move_mod", { fromPath, toFolder, subpath });
}

/** Move an installed mod file to the OS Recycle Bin / Trash. */
export function uninstallMod(fromPath: string, subpath: string): Promise<void> {
  return invoke<void>("uninstall_mod", { fromPath, subpath });
}

/** Reveal an installed mod file in the OS file manager. */
export function revealInExplorer(path: string): Promise<void> {
  return invoke<void>("reveal_in_explorer", { path });
}

/** Hide-to-tray + keep-running toggle. */
export function setRunInBackground(enabled: boolean): Promise<void> {
  return invoke<void>("set_run_in_background", { enabled });
}

export function setGamePath(path: string): Promise<void> {
  return invoke<void>("set_game_path", { path });
}

/** Auto-detect the Steam MX Bikes install (holds `rider.pkz`); null if not found. */
export function detectGamePath(): Promise<string | null> {
  return invoke<string | null>("detect_game_path");
}

/**
 * Override the PiBoSo `profiles` folder for the split-folder edge case. Pass an
 * empty string to clear it (falls back to `<modsPath>/profiles`).
 */
export function setProfilesPath(path: string): Promise<void> {
  return invoke<void>("set_profiles_path", { path });
}

/** Count profiles (dirs with a `profile.ini`) under a folder — validate a pick. */
export function countProfilesIn(path: string): Promise<number> {
  return invoke<number>("count_profiles_in", { path });
}

/** Launch-at-login toggle (also flips the OS autostart entry). */
export function setLaunchAtStartup(enabled: boolean): Promise<void> {
  return invoke<void>("set_launch_at_startup", { enabled });
}

/** Kick off download → extract → place. Progress arrives via `onInstallProgress`. */
export function addToLibrary(
  slug: string,
  url: string,
  host: string,
  subpath: string,
  destFolder: string,
): Promise<void> {
  return invoke<void>("add_to_library", { slug, url, host, subpath, destFolder });
}

/** Import a file the user downloaded manually (extract + place into `subpath`). */
export function importFile(
  path: string,
  subpath: string,
  destFolder: string,
): Promise<void> {
  return invoke<void>("import_file", { path, subpath, destFolder });
}

export interface DestOption {
  value: string;
  label: string;
}

const stripExt = (s: string) => s.replace(/\.(pkz|zip|rar|7z)$/i, "");
const tokens = (s: string) =>
  new Set(
    s
      .toLowerCase()
      .split(/[^a-z0-9]+/)
      .filter((t) => t.length >= 2),
  );

export function buildDestinations(
  modType: ModType,
  title: string,
  installed: InstalledMod[],
  livery = false,
  sound = false,
): { options: DestOption[]; guess: string; suggestions: string[] } {
  const seen = new Set<string>([""]);
  const options: DestOption[] = [
    { value: "", label: modType.id === "bikes" ? "Bikes (root)" : "Tracks (root)" },
  ];
  const add = (value: string, label: string) => {
    if (!seen.has(value)) {
      options.push({ value, label });
      seen.add(value);
    }
  };

  let guess = "";
  const suggestions: string[] = [];
  if (modType.id === "bikes") {
    const bikes = installed.filter((i) => i.folder === "");
    for (const b of bikes) {
      add(stripExt(b.name), `${stripExt(b.name)} — bike folder`);
      add(`${stripExt(b.name)}/paints`, `${stripExt(b.name)} — paints`);
    }

    if (livery || sound) {
      const tt = tokens(title);
      const scored = bikes
        .map((b) => {
          let score = 0;
          for (const t of tokens(b.name)) if (tt.has(t)) score++;
          const value = sound ? stripExt(b.name) : `${stripExt(b.name)}/paints`;
          return { value, score };
        })
        .filter((s) => s.score >= 1)
        .sort((a, b) => b.score - a.score);
      suggestions.push(...scored.slice(0, 5).map((s) => s.value));
      if (scored[0] && scored[0].score >= 2) guess = scored[0].value;
    }
  }

  for (const f of [...new Set(installed.map((i) => i.folder))].sort((a, b) => a.localeCompare(b))) {
    if (f) add(f, f);
  }
  return { options, guess, suggestions };
}

export const STOCK_RIDER_PROFILES = ["default_mx", "default_sm"];

const RIDER_KIT_CATEGORY_IDS = [35, 129, 52];
const RIDER_GLOVES_CATEGORY_ID = 32;

export type GearPaintKind = "helmets" | "boots" | "protection";

const RIDER_PAINT_CATEGORY_KIND: Record<number, GearPaintKind> = {
  127: "helmets",
  126: "boots",
  135: "protection",
};

export function riderPaintKind(
  modType: ModType,
  categoryId: number | null | undefined,
): GearPaintKind | null {
  if (modType.id !== "rider" || categoryId == null) return null;
  return RIDER_PAINT_CATEGORY_KIND[categoryId] ?? null;
}

export function riderProfileSub(
  modType: ModType,
  categoryId: number | null | undefined,
): "paints" | "gloves" | null {
  if (modType.id !== "rider") return null;
  if (categoryId === RIDER_GLOVES_CATEGORY_ID) return "gloves";
  if (categoryId != null && RIDER_KIT_CATEGORY_IDS.includes(categoryId)) return "paints";
  return null;
}

export function buildRiderDestinations(
  targets: RiderTargets,
  title: string,
  profileSub: "paints" | "gloves" | null = null,
  paintKind: GearPaintKind | null = null,
): { options: DestOption[]; guess: string; suggestions: string[] } {
  const seen = new Set<string>();
  const options: DestOption[] = [];
  const add = (value: string, label: string) => {
    if (!seen.has(value)) {
      options.push({ value, label });
      seen.add(value);
    }
  };

  const tt = tokens(title);
  const score = (name: string) => {
    let s = 0;
    for (const t of tokens(name)) if (tt.has(t)) s++;
    return s;
  };

  add("helmets", "Helmets (new model)");
  add("boots", "Boots (new model)");
  add("protection", "Protection (new model)");

  const scoredPaints: { value: string; score: number; kind: GearPaintKind }[] = [];
  for (const h of targets.helmets) {
    add(`helmets/${h}/paints`, `${h} · helmet paints`);
    add(`helmets/${h}/goggles`, `${h} · goggles`);
    scoredPaints.push({ value: `helmets/${h}/paints`, score: score(h), kind: "helmets" });
  }
  for (const b of targets.boots) {
    add(`boots/${b}/paints`, `${b} · boot paints`);
    scoredPaints.push({ value: `boots/${b}/paints`, score: score(b), kind: "boots" });
  }
  for (const p of targets.protection) {
    add(`protection/${p}/paints`, `${p} · protection paints`);
    scoredPaints.push({ value: `protection/${p}/paints`, score: score(p), kind: "protection" });
  }

  const profiles = [...new Set([...targets.profiles, ...STOCK_RIDER_PROFILES])].sort(
    (a, b) => a.toLowerCase().localeCompare(b.toLowerCase()),
  );
  for (const prof of profiles) {
    add(`riders/${prof}/paints`, `${prof} · outfit / kit`);
    add(`riders/${prof}/gloves`, `${prof} · gloves`);
  }

  const kindPaints = paintKind
    ? scoredPaints.filter((s) => s.kind === paintKind)
    : scoredPaints;
  const topPaints = kindPaints
    .filter((s) => s.score >= 1)
    .sort((a, b) => b.score - a.score);
  const suggestions = [
    ...topPaints.slice(0, 4).map((s) => s.value),
    ...profiles.map((p) => `riders/${p}/${profileSub ?? "paints"}`),
  ];

  const guess = profileSub
    ? `riders/${STOCK_RIDER_PROFILES[0]}/${profileSub}`
    : paintKind
      ? topPaints[0]?.value ?? (kindPaints.length === 1 ? kindPaints[0].value : "")
      : topPaints[0] && topPaints[0].score >= 2
        ? topPaints[0].value
        : "";

  return { options, guess, suggestions };
}

const BLOCKED_HOST_PATTERNS: string[] = [];

export function isBlockedDownload(opt: { url: string; host: string }): boolean {
  const s = `${opt.url} ${opt.host}`.toLowerCase();
  return BLOCKED_HOST_PATTERNS.some((p) => s.includes(p));
}

export function sortMirrors(detail: ModDetail): DownloadOption[] {
  const all = detail.downloads ?? [];
  const playable = all.filter((d) => !d.isServer);
  const pool = playable.length ? playable : all;
  return [...pool].sort((a, b) => {
    const ab = isBlockedDownload(a) ? 1 : 0;
    const bb = isBlockedDownload(b) ? 1 : 0;
    if (ab !== bb) return ab - bb;
    return Number(b.isDefault) - Number(a.isDefault);
  });
}

export function pickDownloadForBike(
  mirrors: DownloadOption[],
  bikeName: string,
): DownloadOption | null {
  if (mirrors.length === 0) return null;
  const fallback = () => mirrors.find((m) => m.isDefault) ?? mirrors[0];
  const want = tokens(bikeName);
  if (want.size === 0) return fallback();

  let best: { m: DownloadOption; score: number } | null = null;
  for (const m of mirrors) {
    const fname = m.url.split(/[/\\]/).pop() ?? "";
    const hay = tokens(`${m.label} ${m.host} ${fname}`);
    let score = 0;
    for (const t of want) if (hay.has(t)) score++;
    if (!best || score > best.score) best = { m, score };
  }
  return best && best.score > 0 ? best.m : fallback();
}

export function destStorageKey(modType: ModType): string {
  return `frost-dest-${modType.id}`;
}

export function resolveInitialFolder(
  modType: ModType,
  destOptions: DestOption[],
  guess: string,
  livery = false,
  sound = false,
  paintKind: GearPaintKind | null = null,
): string {
  const remembered = localStorage.getItem(destStorageKey(modType)) ?? "";
  const rememberedIsPaints = /\/paints$/i.test(remembered);
  if (modType.id === "bikes" && rememberedIsPaints && !livery) return guess;
  if (modType.id === "bikes" && sound && guess) return guess;
  if (paintKind && remembered && !remembered.startsWith(`${paintKind}/`)) return guess;
  if (destOptions.some((o) => o.value === remembered)) return remembered;
  return guess;
}

export interface QuickInstallParams {
  slug: string;
  title: string;
  subpath: string;
  destFolder: string;
  url: string;
  host: string;
}

export type QuickInstallResult =
  | { ok: true; params: QuickInstallParams }
  | { ok: false; reason: "blocked" | "none"; title: string; host?: string };

export async function resolveQuickInstall(
  slug: string,
  modType: ModType,
  livery = false,
): Promise<QuickInstallResult> {
  const detail = await getModDetail(slug);
  const mirrors = sortMirrors(detail);
  const primary = mirrors[0];
  if (!primary) return { ok: false, reason: "none", title: detail.title };
  if (isBlockedDownload(primary))
    return { ok: false, reason: "blocked", title: detail.title, host: primary.host };

  let installed: InstalledMod[] = [];
  try {
    installed = await getInstalledMods(modType.installSubpath);
  } catch {
    installed = [];
  }
  const { options, guess } = buildDestinations(modType, detail.title, installed, livery);
  const destFolder = resolveInitialFolder(modType, options, guess, livery);

  return {
    ok: true,
    params: {
      slug,
      title: detail.title,
      subpath: modType.installSubpath,
      destFolder,
      url: primary.url,
      host: primary.host,
    },
  };
}

export function onInstallProgress(
  cb: (progress: InstallProgress) => void,
): Promise<UnlistenFn> {
  return listen<InstallProgress>("install-progress", (event) =>
    cb(event.payload),
  );
}

export interface ShopItem extends ModSummary {
  downloadUrl: string;
}

export function shopLogin(): Promise<void> {
  return invoke<void>("shop_login");
}

/** Whether a shop session is currently held. */
export function shopStatus(): Promise<boolean> {
  return invoke<boolean>("shop_status");
}

/** Sign out of the shop and forget the stored session. */
export function shopLogout(): Promise<void> {
  return invoke<void>("shop_logout");
}

/** The signed-in user's purchased downloads ("All My Downloads"). */
export function shopMyDownloads(): Promise<ShopItem[]> {
  return invoke<ShopItem[]>("shop_my_downloads");
}

/** Download + install a purchased item. Progress arrives via `onInstallProgress`. */
export function shopInstall(item: ShopItem, destFolder: string): Promise<void> {
  return invoke<void>("shop_install", { item, destFolder });
}

/** Fires after a WebView sign-in completes; payload is whether it succeeded. */
export function onShopAuth(cb: (ok: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>("shop-auth", (event) => cb(event.payload));
}

export function reloadFrostmod(): Promise<ReloadOutcome> {
  return invoke<ReloadOutcome>("frostmod_reload");
}

/** Is FrostMod currently running on this PC? */
export function isFrostmodRunning(): Promise<boolean> {
  return invoke<boolean>("frostmod_running");
}

/** Install/version/running snapshot (hits GitHub for the latest tag). */
export function frostmodStatus(): Promise<FrostmodStatus> {
  return invoke<FrostmodStatus>("frostmod_status");
}

/** Download (or update to) the latest FrostMod release. Returns the version tag. */
export function frostmodInstall(): Promise<string> {
  return invoke<string>("frostmod_install");
}

/** Launch the managed FrostMod process if it isn't already running. */
export function frostmodStart(): Promise<boolean> {
  return invoke<boolean>("frostmod_start");
}

/** Stop the managed FrostMod process. */
export function frostmodStop(): Promise<void> {
  return invoke<void>("frostmod_stop");
}

/** Toggle auto-running FrostMod when the app opens. */
export function setAutoRunFrostmod(enabled: boolean): Promise<void> {
  return invoke<void>("set_auto_run_frostmod", { enabled });
}

export function setInstantRefresh(enabled: boolean): Promise<void> {
  return invoke<void>("set_instant_refresh", { enabled });
}

/** Fires after each install with whether FrostMod picked the new mod up live. */
export function onFrostmodReload(
  cb: (payload: FrostmodReload) => void,
): Promise<UnlistenFn> {
  return listen<FrostmodReload>("frostmod-reload", (event) => cb(event.payload));
}

export function presetsListProfiles(): Promise<string[]> {
  return invoke<string[]>("presets_list_profiles");
}

/** Bike ids present in a profile — the targets a loadout can be applied to. */
export function presetsListBikes(profile: string): Promise<string[]> {
  return invoke<string[]>("presets_list_bikes", { profile });
}

/** Read a bike's current cosmetic column (for "capture current look"). */
export function presetsReadLoadout(
  profile: string,
  bikeid: string,
): Promise<Loadout> {
  return invoke<Loadout>("presets_read_loadout", { profile, bikeid });
}

export function presetsApply(
  profile: string,
  bikeid: string,
  loadout: Loadout,
  makeActive: boolean,
): Promise<PresetApplyOutcome> {
  return invoke<PresetApplyOutcome>("presets_apply", {
    profile,
    bikeid,
    loadout,
    makeActive,
  });
}

/** All saved presets. */
export function presetsList(): Promise<Preset[]> {
  return invoke<Preset[]>("presets_list");
}

/** Save (or overwrite by name) a preset. */
export function presetsSave(preset: Preset): Promise<void> {
  return invoke<void>("presets_save", { preset });
}

/** Delete a preset by name. */
export function presetsDelete(name: string): Promise<void> {
  return invoke<void>("presets_delete", { name });
}

/** Export a saved preset as a portable one-line share code (`MXBP1-…`). */
export function presetsExport(name: string): Promise<string> {
  return invoke<string>("presets_export", { name });
}

/** Decode a share code *without* saving — preview a shared preset + check mods. */
export function presetsDecode(text: string): Promise<Preset> {
  return invoke<Preset>("presets_decode", { text });
}

/** Import a share code: decode + save + return the stored preset. */
export function presetsImport(text: string): Promise<Preset> {
  return invoke<Preset>("presets_import", { text });
}

export function presetBundleStats(loadout: Loadout): Promise<BundlePlan> {
  return invoke<BundlePlan>("preset_bundle_stats", { loadout });
}

export function presetBundleCreate(name: string): Promise<string> {
  return invoke<string>("preset_bundle_create", { name });
}

export function presetBundleImport(text: string): Promise<Preset> {
  return invoke<Preset>("preset_bundle_import", { text });
}

/** Subscribe to bundle create/import phase updates. */
export function onPresetBundleProgress(
  cb: (p: BundleProgress) => void,
): Promise<UnlistenFn> {
  return listen<BundleProgress>("preset-bundle-progress", (event) => cb(event.payload));
}
