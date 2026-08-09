import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  BikeModels,
  BikeSounds,
  DropCommitItem,
  DropOutcome,
  DropPlan,
  DropPreview,
  LooseSwapBike,
  OrphanedSetup,
  RegisterReport,
  Config,
  DownloadOption,
  FrostmodInstallReport,
  FrostmodReload,
  FrostmodStatus,
  InstalledMod,
  InstallProgress,
  LaunchOutcome,
  LibraryEntry,
  Loadout,
  ModDetail,
  ModEntry,
  ModRating,
  ModsStateOutcome,
  ModSummary,
  StatePlan,
  PkzMeta,
  PaintTexture,
  BikeModel,
  EdfNode,
  RiderModel,
  RiderPart,
  GearPaints,
  Preset,
  PresetApplyOutcome,
  SwapApplyOutcome,
  ReloadOutcome,
  BundlePlan,
  BundleProgress,
  GameId,
  GameInfo,
} from "../types";
import type { TKey } from "../i18n";

/** Results per page (mirrors `PER_PAGE` in the Rust backend). */
export const SEARCH_PAGE_SIZE = 24;

export interface ModCategory {
  id: number;
  label: TKey;
}

/** A top-level kind of mod, with its filter categories and install folder. */
export interface ModType {
  id: string;
  label: TKey;
  /**
   * The same noun as it reads *inside* a sentence ("Search tracks…").
   * A separate key rather than `label.toLowerCase()` — German capitalizes its
   * nouns everywhere, so lowercasing a translated label produces broken German.
   */
  labelInline: TKey;
  /** Parent WordPress category id (also the "All" filter). */
  categoryId: number;
  categories: ModCategory[];
  /** Relative folder under the game root, e.g. `mods/tracks`. */
  installSubpath: string;
}

/**
 * Browse categories per game.
 *
 * mxb-mods.com and gpb-mods.com run the same WordPress build, but their category *ids*
 * are unrelated — each site numbered its own taxonomy. The GP ids below come from
 * `gpb-mods.com/wp-json/wp/v2/categories`.
 */
const MOD_TYPES_BY_GAME: Record<GameId, ModType[]> = {
  mxb: [],
  gpb: [
    {
      id: "tracks",
      label: "modType.tracks",
      labelInline: "modType.tracksInline",
      categoryId: 29,
      installSubpath: "mods/tracks",
      categories: [
        { id: 29, label: "browseCat.all" },
        { id: 68, label: "browseCat.raceTracks" },
        { id: 69, label: "browseCat.kartTracks" },
      ],
    },
    {
      id: "bikes",
      label: "modType.bikes",
      labelInline: "modType.bikesInline",
      categoryId: 18,
      installSubpath: "mods/bikes",
      categories: [
        { id: 18, label: "browseCat.all" },
        { id: 72, label: "browseCat.newBikes" },
        { id: 71, label: "browseCat.liveries" },
        { id: 77, label: "browseCat.sounds" },
        { id: 76, label: "browseCat.others" },
      ],
    },
    {
      id: "rider",
      label: "modType.rider",
      labelInline: "modType.riderInline",
      categoryId: 9,
      installSubpath: "mods/rider",
      categories: [
        { id: 9, label: "browseCat.all" },
        { id: 54, label: "browseCat.riderModels" },
        { id: 55, label: "browseCat.suitPaints" },
        { id: 51, label: "browseCat.helmets" },
        { id: 70, label: "browseCat.helmetModels" },
      ],
    },
    {
      id: "misc",
      label: "modType.misc",
      labelInline: "modType.miscInline",
      categoryId: 30,
      installSubpath: "mods/misc",
      categories: [
        { id: 30, label: "browseCat.all" },
        { id: 31, label: "browseCat.plugins" },
        { id: 32, label: "browseCat.tools" },
        { id: 33, label: "browseCat.menuBackgrounds" },
      ],
    },
  ],
};

/** The MX Bikes catalog — the app's original browse tree. */
export const MOD_TYPES: ModType[] = [
  {
    id: "tracks",
    label: "modType.tracks",
    labelInline: "modType.tracksInline",
    categoryId: 22,
    installSubpath: "mods/tracks",
    categories: [
      { id: 22, label: "browseCat.all" },
      { id: 300, label: "browseCat.beginner" },
      { id: 301, label: "browseCat.intermediate" },
      { id: 302, label: "browseCat.pro" },
      { id: 119, label: "browseCat.assets" },
    ],
  },
  {
    id: "bikes",
    label: "modType.bikes",
    labelInline: "modType.bikesInline",
    categoryId: 29,
    installSubpath: "mods/bikes",
    categories: [
      { id: 29, label: "browseCat.all" },
      { id: 45, label: "browseCat.newBikes" },
      { id: 37, label: "browseCat.liveries" },
      { id: 46, label: "browseCat.sounds" },
    ],
  },
  {
    id: "rider",
    label: "modType.rider",
    labelInline: "modType.riderInline",
    categoryId: 30,
    installSubpath: "mods/rider",
    categories: [
      { id: 30, label: "browseCat.all" },
      { id: 35, label: "browseCat.riderKit" },
      { id: 313, label: "browseCat.helmets" },
      { id: 127, label: "browseCat.helmetPaints" },
      { id: 32, label: "browseCat.gloves" },
      { id: 343, label: "browseCat.boots" },
      { id: 126, label: "browseCat.bootPaints" },
      { id: 36, label: "browseCat.protection" },
      { id: 135, label: "browseCat.protectionPaints" },
    ],
  },
];

MOD_TYPES_BY_GAME.mxb = MOD_TYPES;

/** Browse tree for `game`, falling back to MX Bikes' for an id we don't know. */
export function modTypesFor(game: GameId | undefined): ModType[] {
  return MOD_TYPES_BY_GAME[game ?? "mxb"] ?? MOD_TYPES;
}

export const DEFAULT_MOD_TYPE = MOD_TYPES[0];

/** The titles this build can drive, with their capabilities. */
export function listGames(): Promise<GameInfo[]> {
  return invoke<GameInfo[]>("list_games");
}

/**
 * Switch which game the app drives. Returns the resulting config — a game being opened
 * for the first time has its folders auto-detected, and comes back with `modsPath` blank
 * when detection found nothing, which is the caller's cue to send the user to setup.
 */
export function setActiveGame(game: GameId): Promise<Config> {
  return invoke<Config>("set_active_game", { game });
}

/** Cosmetic slots this profile actually has, in `profile.ini` order. The two games
 *  don't offer the same ones, so the preset editor asks rather than assuming. */
export function presetsSlots(profile: string): Promise<string[]> {
  return invoke<string[]>("presets_slots", { profile });
}

/** Normalize a mod title or filename into a comparison key. `.pnt` is in the list
 *  because paints and liveries are installed files too — see `lib/installedMatch`. */
export function normalizeModName(s: string): string {
  return s
    .toLowerCase()
    .replace(/\.(pkz|zip|rar|7z|pnt)$/i, "")
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

/** Change only the MX Bikes folder — an empty string re-runs auto-detection. Unlike
 *  `createConfig`, the rest of the settings are preserved. Resolves to the folder actually
 *  adopted: picking the `mods` folder settles on the game folder above it. */
export function setModsPath(path: string): Promise<string> {
  return invoke<string>("set_mods_path", { path });
}

/** Persist that the intro slideshow and/or the guided tour is done. */
export function setIntroSeen(opts: {
  welcome?: boolean;
  tour?: boolean;
}): Promise<void> {
  return invoke<void>("set_intro_seen", {
    welcome: opts.welcome ?? false,
    tour: opts.tour ?? false,
  });
}

/** Persist that this version's release showcase has been seen. */
export function setSeenVersion(version: string): Promise<void> {
  return invoke<void>("set_seen_version", { version });
}

/** Whether this build can decode real bike geometry for the 3D preview. Public
 *  builds without the optional local module return false, so the UI hides it. */
export function bikePreviewAvailable(): Promise<boolean> {
  return invoke<boolean>("bike_preview_available");
}

/** The order a browse listing comes back in. Mirrors `ModSort` on the Rust side. */
export type ModSort =
  | "newest"
  | "oldest"
  | "popularAll"
  | "popularMonth"
  | "popularWeek";

/** The sort choices Browse offers, in menu order. The popular ones are ranked by view
 *  count, which comes from a listing that takes no search term — `noSearch` marks those
 *  so the menu can drop them while the search box has text in it. */
export const MOD_SORTS: { value: ModSort; label: TKey; noSearch?: boolean }[] = [
  { value: "newest", label: "browseSort.newest" },
  { value: "oldest", label: "browseSort.oldest" },
  { value: "popularAll", label: "browseSort.popularAll", noSearch: true },
  { value: "popularMonth", label: "browseSort.popularMonth", noSearch: true },
  { value: "popularWeek", label: "browseSort.popularWeek", noSearch: true },
];

export function searchMods(
  query: string,
  categoryId: number,
  page = 1,
  sort: ModSort = "newest",
): Promise<ModSummary[]> {
  return invoke<ModSummary[]>("search_mods", { query, categoryId, page, sort });
}

export function getModDetail(slug: string): Promise<ModDetail> {
  return invoke<ModDetail>("get_mod_detail", { slug });
}

/** Community scores for the given post ids, keyed by id. Ids the site didn't answer for
 *  are absent — ratings decorate a card, so a miss shows no stars rather than an error. */
export function getModRatings(ids: number[]): Promise<Record<string, ModRating>> {
  return invoke<Record<string, ModRating>>("get_mod_ratings", { ids });
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
  /** Riding-style animations. Both titles read `mods/rider/animations/<name>/`. */
  animations: string[];
  /** What `mods/rider/riders` holds: MX Bikes' rider profiles, GP Bikes' rider models. */
  profiles: string[];
}

export function scanRiderTargets(): Promise<RiderTargets> {
  return invoke<RiderTargets>("scan_rider_targets");
}

/** What a failed scan stands in for: nothing installed, which every picker handles. */
export const EMPTY_RIDER_TARGETS: RiderTargets = {
  helmets: [],
  boots: [],
  protection: [],
  animations: [],
  profiles: [],
};

/**
 * Every bike a paint can be installed for — folders and `.pkz` packages under `mods/bikes`,
 * plus the bike ids read out of the player's profiles. OEM bikes only exist inside the game's
 * locked archive, so the profile is the only place their id can be found until someone
 * installs a paint for one.
 */
export function scanBikeTargets(): Promise<string[]> {
  return invoke<string[]>("scan_bike_targets");
}

export function scanModelSwaps(): Promise<BikeModels[]> {
  return invoke<BikeModels[]>("scan_model_swaps");
}

export function applyModelSwap(bike: string, target: string): Promise<SwapApplyOutcome> {
  return invoke<SwapApplyOutcome>("apply_model_swap", { bike, target });
}

export function scanSoundSwaps(): Promise<BikeSounds[]> {
  return invoke<BikeSounds[]>("scan_sound_swaps");
}

export function applySoundSwap(bike: string, target: string): Promise<SwapApplyOutcome> {
  return invoke<SwapApplyOutcome>("apply_sound_swap", { bike, target });
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

/**
 * Bikes whose setup files were moved into a swap folder by a version that treated the
 * whole bike folder as the model set — such a bike doesn't load in-game at all.
 */
export function detectOrphanedSetup(): Promise<OrphanedSetup[]> {
  return invoke<OrphanedSetup[]>("detect_orphaned_setup");
}

/** Copy a gutted bike's setup files back to its root. Returns how many were restored. */
export function repairOrphanedSetup(bike: string): Promise<number> {
  return invoke<number>("repair_orphaned_setup", { bike });
}

export function getPkzMeta(path: string): Promise<PkzMeta> {
  return invoke<PkzMeta>("get_pkz_meta", { path });
}

/**
 * Metadata for many mods in one call, but only where it's already cached — a `null`
 * marks an entry that still needs {@link getPkzMeta}. Lets a known library paint
 * without opening a single archive.
 */
export function getPkzMetaCached(paths: string[]): Promise<(PkzMeta | null)[]> {
  return invoke<(PkzMeta | null)[]>("get_pkz_meta_cached", { paths });
}

export function getPkzPreview(path: string): Promise<string | null> {
  return invoke<string | null>("get_pkz_preview", { path });
}

export function unpackPaint(path: string): Promise<PaintTexture[]> {
  return invoke<PaintTexture[]>("unpack_paint", { path });
}

/**
 * Raw RGBA for a {@link PaintTexture.token}, straight off the binary IPC channel.
 *
 * The backend answers with `application/octet-stream`, so this resolves to an
 * `ArrayBuffer` rather than JSON — no base64 on the wire and no megabyte strings on the
 * heap. A token whose pixels have been evicted comes back as a single grey pixel, which the
 * caller spots by checking the length against the texture's declared size.
 */
export function textureBytes(token: string): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("texture_bytes", { token });
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
  /** Show the mesh's own texture instead of a `.pnt` — the stock look, per side. */
  stock = false,
  stockGoggles = false,
): Promise<RiderPart> {
  return invoke<RiderPart>("load_gear_model", {
    path,
    part,
    paint,
    goggles,
    stock,
    stockGoggles,
  });
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
/** Scan Steam for a game's install folder. Omit `game` for the active one; setup passes
 *  it explicitly, since on a first run the pick isn't saved yet. */
export function detectGamePath(game?: GameId): Promise<string | null> {
  return invoke<string | null>("detect_game_path", { game });
}

/**
 * Override the PiBoSo `profiles` folder. Pass an empty string to clear it (back to
 * the resolved default — see {@link presetsListProfiles}).
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

/** Stage + classify dropped paths. Reads only — nothing installs until `commitDrop`. */
export function planDrop(paths: string[]): Promise<DropPlan> {
  return invoke<DropPlan>("plan_drop", { paths });
}

/** Re-cost one row after the user picked a different destination. */
export function repreviewDrop(
  planId: string,
  itemId: string,
  subpath: string,
  destFolder: string,
): Promise<DropPreview> {
  return invoke<DropPreview>("repreview_drop", {
    planId,
    itemId,
    subpath,
    destFolder,
  });
}

/** Install the reviewed rows. Rows the user removed are simply not sent. */
export function commitDrop(
  planId: string,
  items: DropCommitItem[],
): Promise<DropOutcome> {
  return invoke<DropOutcome>("commit_drop", { planId, items });
}

/** Discard a plan and delete whatever was staged for it. */
export function cancelDrop(planId: string): Promise<void> {
  return invoke<void>("cancel_drop", { planId });
}

export interface DestOption {
  value: string;
  /** A real folder name — shown verbatim, never translated. */
  label: string;
  /**
   * Set when the label is UI prose rather than a folder name ("Bikes (root)",
   * "{{name}} — paints"). The folder name, when one is involved, rides in
   * `labelVars.name` so the translation can put it where its language wants it.
   */
  labelKey?: TKey;
  labelVars?: Record<string, string>;
}

const stripExt = (s: string) => s.replace(/\.(pkz|zip|rar|7z)$/i, "");
const tokens = (s: string) =>
  new Set(
    s
      .toLowerCase()
      .split(/[^a-z0-9]+/)
      .filter((t) => t.length >= 2),
  );

/**
 * Below this many significant words a category is a grouping, not a machine — "KTM", "OEM",
 * "Beta 18". Matching on those would nominate every bike of the brand.
 */
const MIN_CATEGORY_TOKENS = 3;

/**
 * How much of a category's vocabulary the bike has to account for.
 *
 * The two spellings never line up exactly: the site writes `2023 KTM 450 SX-F OEM`, the game
 * calls the same bike `MX1OEM_2023_KTM_450_SX-F`, and the class prefix swallows the "OEM".
 * Four words of five is the real shape of a hit. Measured against the live catalog, 0.75 is
 * also where the near-misses stop: a 250 scores 0.6 against a 450's category, and noise like
 * "AMA SX 2024" scores 0.67 against any bike of that year.
 */
const MIN_CATEGORY_COVERAGE = 0.75;

/** Compare rank tuples element by element; higher is better. */
function cmpRank(a: number[], b: number[]): number {
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i] - b[i];
  return 0;
}

/**
 * The bikes a post's categories point at, best first.
 *
 * mxb-mods files a livery under one category per bike it fits — "2023 KTM 250 SX-F OEM",
 * "2023 KTM 350 SX-F OEM" — which says outright what the title ("Suzuki made by
 * KindConcepts") only gestures at. Against the live catalog this puts the right bike first
 * for every one of the 107 OEM models.
 */
export function bikesFromCategories(bikes: string[], categories: string[]): string[] {
  const ranked = new Map<string, number[]>();
  for (const cat of categories) {
    const ct = tokens(cat);
    if (ct.size < MIN_CATEGORY_TOKENS) continue;
    // "OEM" is the site's word for stock; the bike spells it into its class prefix instead.
    const core = normalizeModName(cat.replace(/oem/gi, ""));
    for (const bike of bikes) {
      const bt = tokens(bike);
      let shared = 0;
      for (const t of ct) if (bt.has(t)) shared++;
      if (shared / ct.size < MIN_CATEGORY_COVERAGE) continue;

      // Tie-break between siblings that share every word the tokenizer keeps: `250 SX` and
      // `250 SX-F` differ only by an "F" too short to be a token, but the run-together form
      // still tells them apart — `…250sxf` contains `…250sx`, not the other way round.
      const norm = normalizeModName(bike);
      const rank = [shared, norm.includes(core) ? 1 : 0, -norm.length];
      const prev = ranked.get(bike);
      if (!prev || cmpRank(rank, prev) > 0) ranked.set(bike, rank);
    }
  }
  return [...ranked.entries()]
    .sort(([an, a], [bn, b]) => cmpRank(b, a) || an.localeCompare(bn))
    .map(([name]) => name);
}

/**
 * Every bike the picker can offer, merging what the backend found (folders, packages and
 * profile bike ids) with the packages this scan saw. Case-insensitive, so a bike reached both
 * ways is listed once.
 */
function bikeNames(installed: InstalledMod[], targets: string[]): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  const roots = installed.filter((i) => i.folder === "").map((i) => stripExt(i.name));
  for (const name of [...roots, ...targets]) {
    const key = name.toLowerCase();
    if (name && !seen.has(key)) {
      seen.add(key);
      out.push(name);
    }
  }
  return out.sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
}

export function buildDestinations(
  modType: ModType,
  title: string,
  installed: InstalledMod[],
  livery = false,
  sound = false,
  categories: string[] = [],
  bikeTargets: string[] = [],
): { options: DestOption[]; guess: string; suggestions: string[] } {
  const seen = new Set<string>([""]);
  const options: DestOption[] = [
    {
      value: "",
      label: "",
      labelKey: modType.id === "bikes" ? "dest.bikesRoot" : "dest.tracksRoot",
    },
  ];
  const add = (value: string, opt: Omit<DestOption, "value">) => {
    if (!seen.has(value)) {
      options.push({ value, ...opt });
      seen.add(value);
    }
  };

  let guess = "";
  const suggestions: string[] = [];
  if (modType.id === "bikes") {
    const bikes = bikeNames(installed, bikeTargets);
    for (const b of bikes) {
      add(b, { label: "", labelKey: "dest.bikeFolder", labelVars: { name: b } });
      add(`${b}/paints`, { label: "", labelKey: "dest.bikePaints", labelVars: { name: b } });
    }

    if (livery || sound) {
      // A sound replaces the bike itself, so it goes to the bike's root; a paint only ever
      // loads out of `paints/`.
      const dest = (bike: string) => (sound ? bike : `${bike}/paints`);

      // The categories name the bike; the title is only ever a hint, so it ranks below them
      // and stays as the fallback for a post nobody categorised.
      const named = bikesFromCategories(bikes, categories);
      const tt = tokens(title);
      const scored = bikes
        .map((bike) => {
          let score = 0;
          for (const t of tokens(bike)) if (tt.has(t)) score++;
          return { bike, score };
        })
        .filter((s) => s.score >= 1)
        .sort((a, b) => b.score - a.score);

      const ranked = [...named, ...scored.slice(0, 5).map((s) => s.bike)];
      suggestions.push(...new Set(ranked.map(dest)));
      guess = named[0]
        ? dest(named[0])
        : scored[0] && scored[0].score >= 2
          ? dest(scored[0].bike)
          : "";
    }
  }

  for (const f of [...new Set(installed.map((i) => i.folder))].sort((a, b) => a.localeCompare(b))) {
    if (f) add(f, { label: f });
  }

  // Tracks live in folders — a series, a season, a pack — and the root is where they're
  // hardest to find again. Once the library has any folder at all, that's a likelier home
  // for the next track than the root the picker used to fall back to.
  if (modType.id === "tracks" && !guess) {
    guess = options.find((o) => o.value)?.value ?? "";
  }

  return { options, guess, suggestions };
}

export const STOCK_RIDER_PROFILES = ["default_mx", "default_sm"];

/**
 * The folder both titles keep their rider models in, under `mods/rider`.
 *
 * MX Bikes calls what lives here a rider *profile* and hangs the kit, gloves and goggles off
 * it. GP Bikes calls the same thing a rider *model*, bakes the boots and gloves into it, and
 * keeps the suits at `riders/<model>/paints` — verified against the loader, which opens
 * `rider\riders\<model>\paints\<paint>.pnt` and nothing else for a suit.
 */
const RIDERS_DIR = "riders";

/** Where a rider category's content belongs, once we know which title's catalog it is. */
export type RiderTarget =
  /** A livery for one model in a gear area — `<area>/<model>/paints`. */
  | { kind: "gearPaint"; area: string }
  /** Something worn by the rider model itself — `riders/<model>/<sub>`. */
  | { kind: "profile"; sub: string }
  /** A model of its own, which by definition isn't installed yet — the area's root. */
  | { kind: "newModel"; folder: string };

const gearPaint = (area: string): RiderTarget => ({ kind: "gearPaint", area });
const profileSub = (sub: string): RiderTarget => ({ kind: "profile", sub });
const newModel = (folder: string): RiderTarget => ({ kind: "newModel", folder });

/**
 * What each catalog's rider categories install, per title.
 *
 * The two sites run the same WordPress build but numbered their taxonomies independently, so
 * an id means nothing without the title it came from — GP's **52** is Helmets › Replicas
 * while MX's **52** is a rider kit. Keying by game is what keeps those apart.
 *
 * GP's Suit Paints tree (55) is deep: every child names the *rider model* the suit fits
 * ("Modern 1", "MGP 21"), and each of those has Replicas and Personal Suits under it. They
 * all install the same way, so the whole subtree maps to the suit slot and the model itself
 * is worked out by `rankRiderProfiles`.
 */
const RIDER_CATEGORIES_BY_GAME: Record<GameId, Record<number, RiderTarget>> = {
  mxb: {
    35: profileSub("paints"), // Rider Kit
    129: profileSub("paints"),
    52: profileSub("paints"),
    32: profileSub("gloves"), // Gloves
    313: newModel("helmets"), // Helmets (models)
    127: gearPaint("helmets"), // Helmet Paints
    343: newModel("boots"),
    126: gearPaint("boots"), // Boot Paints
    36: newModel("protections"),
    135: gearPaint("protection"), // Protection Paints
  },
  gpb: {
    54: newModel(RIDERS_DIR), // Rider Models
    70: newModel("helmets"), // Helmet Models
    51: gearPaint("helmets"), // Helmets, and its Replicas / Personal Paints
    52: gearPaint("helmets"),
    53: gearPaint("helmets"),
    // Suit Paints, and every branch of it.
    ...Object.fromEntries(
      [55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67].map((id) => [
        id,
        profileSub("paints"),
      ]),
    ),
  },
};

export interface RiderDestinations {
  options: DestOption[];
  /** The preselected folder. */
  guess: string;
  /** The post itself decided the guess (its categories or title named the model), rather
   *  than a default standing in. Only then does the guess outrank a remembered folder. */
  derived: boolean;
  /** Ranked "probably this one" values, shown above the full list. */
  suggestions: string[];
}

/** Where the mod being viewed belongs, or `null` when its category doesn't say. */
export function riderTarget(
  game: GameInfo,
  modType: ModType,
  categoryId: number | null | undefined,
): RiderTarget | null {
  if (modType.id !== "rider" || categoryId == null) return null;
  return RIDER_CATEGORIES_BY_GAME[game.id]?.[categoryId] ?? null;
}

/** `RiderTargets` keys its lists by slot, which is what every other consumer wants; only
 *  the pickers need them addressed by the folder they live in. */
function modelsInArea(targets: RiderTargets, folder: string): string[] {
  switch (folder) {
    case "helmets":
      return targets.helmets;
    case "boots":
      return targets.boots;
    case "protections":
      return targets.protection;
    case "animations":
      return targets.animations ?? [];
    default:
      return [];
  }
}

/** The gear paint slot an area's paints fill — `RiderTarget.area`, which for protection is
 *  the slot's name rather than its folder's. */
function areaSlot(folder: string): string {
  return folder === "protections" ? "protection" : folder;
}

const AREA_LABELS: Record<string, { newModel: TKey; paints?: TKey; goggles?: TKey }> = {
  helmets: {
    newModel: "dest.helmetsNewModel",
    paints: "dest.helmetPaintsFor",
    goggles: "dest.gogglesFor",
  },
  boots: { newModel: "dest.bootsNewModel", paints: "dest.bootPaintsFor" },
  protections: { newModel: "dest.protectionNewModel", paints: "dest.protectionPaintsFor" },
  animations: { newModel: "dest.animationsNewStyle" },
};

/** Digit runs, which the tokenizer drops as too short: `"MGP 21"` → `["21"]`. */
const digitRuns = (s: string): string[] => s.match(/\d+/g) ?? [];

/**
 * Installed rider models ranked against a post, best first.
 *
 * The catalog files a GP suit under the model family it fits, so the post's own categories
 * name the target: "Modern 1" is Manu's `Modern Type 1`. Those families differ from one
 * another *only* by a number, which `tokens` drops as too short — hence the digit pass,
 * which only ever breaks a tie between models that already share a word.
 */
function rankRiderProfiles(
  profiles: string[],
  title: string,
  categories: string[],
): string[] {
  const wanted = tokens([...categories, title].join(" "));
  const wantedDigits = new Set(categories.flatMap(digitRuns));
  return profiles
    .map((name) => {
      let score = 0;
      for (const t of tokens(name)) if (wanted.has(t)) score++;
      if (score > 0 && digitRuns(name).some((d) => wantedDigits.has(d))) score += 2;
      return { name, score };
    })
    .filter((s) => s.score > 0)
    .sort((a, b) => b.score - a.score || a.name.localeCompare(b.name))
    .map((s) => s.name);
}

/**
 * Every folder under `mods/rider` this title installs into, with a default.
 *
 * Driven by the title's own layout (`GameInfo.riderAreas`) rather than a fixed list, because
 * the two games barely overlap here: offering GP Bikes a `boots` folder would file a mod
 * somewhere its loader never looks, which reads as "installed" in the app and as missing in
 * the game.
 */
export function buildRiderDestinations(
  game: GameInfo,
  targets: RiderTargets,
  title: string,
  categories: string[] = [],
  target: RiderTarget | null = null,
): RiderDestinations {
  const seen = new Set<string>();
  const options: DestOption[] = [];
  const add = (value: string, labelKey: TKey | null, labelVars?: Record<string, string>) => {
    if (!seen.has(value)) {
      // A folder a future title adds has no phrasing of its own; its name is the honest label.
      options.push(
        labelKey
          ? { value, label: "", labelKey, labelVars }
          : { value, label: labelVars?.name ?? value },
      );
      seen.add(value);
    }
  };

  const tt = tokens(title);
  const score = (name: string) => {
    let s = 0;
    for (const t of tokens(name)) if (tt.has(t)) s++;
    return s;
  };

  // Somewhere to put a brand new model, which by definition isn't installed yet.
  add(RIDERS_DIR, "dest.riderModelsNew");
  for (const area of game.riderAreas) {
    add(area.folder, AREA_LABELS[area.folder]?.newModel ?? null, { name: area.folder });
  }

  const scoredPaints: { value: string; score: number; area: string }[] = [];
  for (const area of game.riderAreas) {
    const labels = AREA_LABELS[area.folder];
    for (const model of modelsInArea(targets, area.folder)) {
      if (area.paints) {
        const value = `${area.folder}/${model}/paints`;
        add(value, labels?.paints ?? null, { name: model });
        scoredPaints.push({ value, score: score(model), area: areaSlot(area.folder) });
      }
      if (area.goggles) {
        add(`${area.folder}/${model}/goggles`, labels?.goggles ?? null, { name: model });
      }
    }
  }

  // The models the game ships exist whether or not anything is installed, so a paint for one
  // always has somewhere to go. GP Bikes ships none that mods target — see `stock_profiles`.
  const profiles = [
    ...new Set([...targets.profiles, ...game.riderStockProfiles]),
  ].sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
  const rankedProfiles = rankRiderProfiles(profiles, title, categories);
  // Both games keep these in the same folder; each has its own word for what's in it. MX
  // Bikes' kit is worn *with* the boots and gloves, GP Bikes' suit has them built in.
  const profilePaints: TKey = game.id === "gpb" ? "dest.suitPaintsFor" : "dest.outfitFor";
  for (const prof of profiles) {
    add(`${RIDERS_DIR}/${prof}/paints`, profilePaints, { name: prof });
    for (const extra of game.riderProfileExtras) {
      add(
        `${RIDERS_DIR}/${prof}/${extra}`,
        extra === "gloves" ? "dest.glovesFor" : "dest.gogglesFor",
        { name: prof },
      );
    }
  }

  const areaPaints =
    target?.kind === "gearPaint"
      ? scoredPaints.filter((s) => s.area === target.area)
      : scoredPaints;
  const topPaints = areaPaints.filter((s) => s.score >= 1).sort((a, b) => b.score - a.score);

  // The rider model a suit belongs on: the one the game ships if it ships one, else the
  // installed model the post points at, else the best guess left — its first model.
  const sub = target?.kind === "profile" ? target.sub : "paints";
  const profileFor = (p: string) => `${RIDERS_DIR}/${p}/${sub}`;
  const bestProfile =
    game.riderStockProfiles[0] ?? rankedProfiles[0] ?? profiles[0] ?? null;

  const suggestions = [
    ...topPaints.slice(0, 4).map((s) => s.value),
    ...[...rankedProfiles, ...profiles].map(profileFor),
  ];

  let guess: string;
  // Whether the post itself settled this, rather than a default standing in. A derived
  // destination outranks the folder used last time — the same rule bike liveries follow,
  // and for the same reason: this content is per model, so last time's folder was another
  // model's.
  let derived = false;
  switch (target?.kind) {
    case "profile":
      // Never the `mods/rider` root: a suit dropped there is invisible to the game.
      guess = bestProfile ? profileFor(bestProfile) : RIDERS_DIR;
      derived = !game.riderStockProfiles.length && !!rankedProfiles[0];
      break;
    case "newModel":
      // Structural, not a preference: a model belongs at its area's root, never inside
      // another model's `paints`.
      guess = target.folder;
      derived = true;
      break;
    case "gearPaint":
      guess = topPaints[0]?.value ?? (areaPaints.length === 1 ? areaPaints[0].value : "");
      break;
    default:
      guess = topPaints[0] && topPaints[0].score >= 2 ? topPaints[0].value : "";
  }

  return { options, guess, derived, suggestions: [...new Set(suggestions)] };
}

/**
 * Hosts we can't download from unattended, so the UI routes them to the
 * download-it-yourself-then-pick-the-file flow instead of failing mid-install.
 *
 * Proton Drive shares are end-to-end encrypted: the decryption key is in the URL
 * fragment, which never reaches the server, and the file arrives as OpenPGP-encrypted
 * blocks. There is no direct link to resolve. Listing it here also sorts it *below* any
 * other mirror on the same mod, so a MediaFire alternative is preferred automatically.
 */
const BLOCKED_HOST_PATTERNS: string[] = ["drive.proton.me", "proton.me"];

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

/**
 * Where the last-used folder for a kind of mod is remembered.
 *
 * Keyed by title as well as kind: the two games' rider folders barely overlap, so an MX
 * Bikes choice preselecting on GP Bikes would put a suit in `boots`.
 */
export function destStorageKey(game: GameInfo, modType: ModType): string {
  return game.id === "mxb"
    ? // The key MX Bikes has always used — renaming it would forget every existing choice.
      `frost-dest-${modType.id}`
    : `frost-dest-${game.id}-${modType.id}`;
}

/** `helmets/AGV/paints` → `helmets/*\/paints`. Two destinations of the same shape are
 *  interchangeable choices; different shapes are different *kinds* of place, and a folder
 *  remembered from one is no answer for the other. */
function destShape(value: string): string {
  const segs = value.split("/").filter(Boolean);
  return segs.length > 1 ? `${segs[0]}/*/${segs[segs.length - 1]}` : (segs[0] ?? "");
}

export function resolveInitialFolder(
  game: GameInfo,
  modType: ModType,
  destOptions: DestOption[],
  guess: string,
  livery = false,
  sound = false,
  rider: { target: RiderTarget | null; derived: boolean } | null = null,
): string {
  const remembered = localStorage.getItem(destStorageKey(game, modType)) ?? "";
  const rememberedIsPaints = /\/paints$/i.test(remembered);
  // An empty remembered value can't be told apart from never having chosen — both read as
  // `""`. For tracks that's the root, the one destination worth talking someone out of, so
  // let the guess (the first existing folder) have it.
  if (modType.id === "tracks" && !remembered && guess) return guess;
  if (modType.id === "bikes" && rememberedIsPaints && !livery) return guess;
  // Liveries and sounds are *per bike*, so the folder used last time is the previous bike's,
  // not this mod's. Whenever we could work out which bike this one is for — its categories
  // name it — that beats the memory. With no match at all, fall through and keep it.
  if (modType.id === "bikes" && (livery || sound) && guess) return guess;
  if (rider?.target) {
    // The post named the model this is for; that outranks the last folder used.
    if (rider.derived) return guess;
    // Otherwise the memory stands, but only where it's the same kind of place: a helmet
    // paint's folder is no home for a helmet model, nor a boot paint for a suit.
    if (destShape(remembered) !== destShape(guess)) return guess;
  }
  if (destOptions.some((o) => o.value === remembered)) return remembered;
  return guess;
}

/**
 * Where a track goes when there's no picker to ask — the MX Bikes Shop installs straight from
 * the purchase list. Same answer the Browse dialog would preselect: the remembered folder, or
 * the first one the library already has.
 */
export async function resolveTrackDest(game: GameInfo): Promise<string> {
  const modType = DEFAULT_MOD_TYPE;
  const installed = await getInstalledMods(modType.installSubpath).catch(() => []);
  const { options, guess } = buildDestinations(modType, "", installed);
  return resolveInitialFolder(game, modType, options, guess);
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

/**
 * The folder a one-click install from the Browse grid uses — the same answer the install
 * dialog would have preselected, worked out without opening it.
 *
 * Rider mods route through `buildRiderDestinations` exactly as the dialog does. They used to
 * fall through the generic (track/bike) logic instead, which knows nothing of gear models and
 * so filed every quick-installed helmet, kit and suit in the `mods/rider` root.
 */
export async function resolveQuickInstall(
  slug: string,
  modType: ModType,
  game: GameInfo,
  categoryId: number | null | undefined,
): Promise<QuickInstallResult> {
  const detail = await getModDetail(slug);
  const mirrors = sortMirrors(detail);
  const primary = mirrors[0];
  if (!primary) return { ok: false, reason: "none", title: detail.title };
  if (isBlockedDownload(primary))
    return { ok: false, reason: "blocked", title: detail.title, host: primary.host };

  const livery = isLiveryContext(modType, categoryId);

  if (modType.id === "rider") {
    const target = riderTarget(game, modType, categoryId);
    const targets = await scanRiderTargets().catch(() => EMPTY_RIDER_TARGETS);
    const { options, guess, derived } = buildRiderDestinations(
      game,
      targets,
      detail.title,
      detail.categories,
      target,
    );
    return {
      ok: true,
      params: {
        slug,
        title: detail.title,
        subpath: modType.installSubpath,
        destFolder: resolveInitialFolder(game, modType, options, guess, false, false, {
          target,
          derived,
        }),
        url: primary.url,
        host: primary.host,
      },
    };
  }

  let installed: InstalledMod[] = [];
  let bikeTargets: string[] = [];
  try {
    installed = await getInstalledMods(modType.installSubpath);
  } catch {
    installed = [];
  }
  if (modType.id === "bikes") {
    bikeTargets = await scanBikeTargets().catch(() => []);
  }
  const { options, guess } = buildDestinations(
    modType,
    detail.title,
    installed,
    livery,
    false,
    detail.categories,
    bikeTargets,
  );
  const destFolder = resolveInitialFolder(game, modType, options, guess, livery);

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

/** Start MX Bikes. Resolves to `already_running` when the game is already up. */
export function launchGame(): Promise<LaunchOutcome> {
  return invoke<LaunchOutcome>("launch_game");
}

/**
 * Start MX Bikes connected straight to `address` (`host` or `host:port`; the port
 * defaults to 54210).
 *
 * Resolves to `already_running` when the game is already up — the connect flag is only
 * read at startup, so a running copy can't be steered into a server.
 */
export function joinServer(address: string): Promise<LaunchOutcome> {
  return invoke<LaunchOutcome>("join_server", { address });
}

/** Is MX Bikes currently running? Always false off Windows — the probe is Win32-only. */
export function isGameRunning(): Promise<boolean> {
  return invoke<boolean>("game_running");
}

/** Install/version/running snapshot (hits GitHub for the latest tag). */
export function frostmodStatus(): Promise<FrostmodStatus> {
  return invoke<FrostmodStatus>("frostmod_status");
}

/** Download (or update to) the latest FrostMod release. All-or-nothing: it either
 *  puts both binaries in place or leaves the previous install untouched. */
export function frostmodInstall(): Promise<FrostmodInstallReport> {
  return invoke<FrostmodInstallReport>("frostmod_install");
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

/** The in-game overlay's settings, plus what the game is doing right now. */
export interface OverlayState {
  enabled: boolean;
  /** Tauri accelerator string, e.g. `"CommandOrControl+Shift+X"`. */
  hotkey: string;
  gameRunning: boolean;
  /** A DirectX app owns the screen exclusively — nothing can be drawn over it. */
  fullscreenBlocked: boolean;
  /** Why the shortcut isn't live (usually another app owns the combo), or null. */
  hotkeyError: string | null;
}

export function getOverlayState(): Promise<OverlayState> {
  return invoke<OverlayState>("overlay_state");
}

/** Show or hide the overlay. The global hotkey does the same thing. */
export function overlayToggle(): Promise<void> {
  return invoke<void>("overlay_toggle");
}

/** Dismiss the overlay and hand keyboard focus back to MX Bikes. */
export function overlayHide(): Promise<void> {
  return invoke<void>("overlay_hide");
}

/** Close the overlay and bring the main MXB App window to the front. */
export function overlayOpenMain(): Promise<void> {
  return invoke<void>("overlay_open_main");
}

export function setOverlayEnabled(enabled: boolean): Promise<void> {
  return invoke<void>("set_overlay_enabled", { enabled });
}

/** Rebind the overlay hotkey. Rejects (leaving the old one live) if the combo is taken. */
export function setOverlayHotkey(hotkey: string): Promise<void> {
  return invoke<void>("set_overlay_hotkey", { hotkey });
}

/** Fires when the overlay was summoned while the game held the screen exclusively. */
export function onOverlayFullscreenBlocked(cb: () => void): Promise<UnlistenFn> {
  return listen("overlay-fullscreen-blocked", () => cb());
}

/** Toggle watching the mods folder to reload the game on external changes. */
export function setWatchModsReload(enabled: boolean): Promise<void> {
  return invoke<void>("set_watch_mods_reload", { enabled });
}

/** Sentinel slug the backend tags folder-watch reloads with (vs in-app installs). */
export const MODS_WATCH_SLUG = "__mods_watch__";

/** Fires after each install with whether FrostMod picked the new mod up live. */
export function onFrostmodReload(
  cb: (payload: FrostmodReload) => void,
): Promise<UnlistenFn> {
  return listen<FrostmodReload>("frostmod-reload", (event) => cb(event.payload));
}

/**
 * Fires whenever the mods folder gains content — an in-app install or a file dropped in
 * and caught by the watcher. Screens that scan the folder use this to stay current
 * instead of relying on the user to hit Rescan.
 */
export function onModsChanged(cb: () => void): Promise<UnlistenFn> {
  return onFrostmodReload(() => cb());
}

/** The profiles folder as the backend resolved it, and what it holds. */
export type ProfilesScan = {
  /** Absolute path the profiles were read from. */
  dir: string;
  /** Whether that folder is actually there — an empty list means something different
   *  when it isn't (wrong path) than when it is (game never made a profile). */
  exists: boolean;
  profiles: string[];
};

export function presetsListProfiles(): Promise<ProfilesScan> {
  return invoke<ProfilesScan>("presets_list_profiles");
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

/** Every mod the Manage tab can act on, enabled and disabled alike. */
export function modsStateScan(): Promise<ModEntry[]> {
  return invoke<ModEntry[]>("mods_state_scan");
}

/** What racing this preset would enable and disable — nothing moves. */
export function modsStatePlan(name: string): Promise<StatePlan> {
  return invoke<StatePlan>("mods_state_plan", { name });
}

/** Enable or disable a hand-picked set of mods, by `rel` path. */
export function modsStateSet(
  rels: string[],
  enabled: boolean,
): Promise<ModsStateOutcome> {
  return invoke<ModsStateOutcome>("mods_state_set", { rels, enabled });
}

/** Send mods to the recycle bin, enabled or disabled, by `rel` path. */
export function modsStateDelete(rels: string[]): Promise<ModsStateOutcome> {
  return invoke<ModsStateOutcome>("mods_state_delete", { rels });
}

/**
 * Race mode: apply the preset's look and leave the game holding only the content it
 * needs. Blank `profile`/`bikeid` skips the cosmetics and does the content alone.
 */
export function modsStateApply(
  name: string,
  profile: string,
  bikeid: string,
): Promise<ModsStateOutcome> {
  return invoke<ModsStateOutcome>("mods_state_apply", { name, profile, bikeid });
}

/** Move every disabled mod back to where it came from. */
export function modsStateRestoreAll(): Promise<ModsStateOutcome> {
  return invoke<ModsStateOutcome>("mods_state_restore_all");
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

/** `"windows"` | `"macos"` | `"linux"`. Features gated on the OS ask the backend rather
 *  than guessing from `navigator.userAgent`, which can't tell Windows from Linux. */
export function appPlatform(): Promise<string> {
  return invoke<string>("app_platform");
}

// ── Experimental features, beta builds, paint sync ───────────────────────────

export interface ExperimentalState {
  /** The MX Bikes GUID this account has claimed, if any. */
  guid?: string;
  /** Whether the unfinished multiplayer features should be shown at all. */
  enabled: boolean;
  /** On because `MXB_EXPERIMENTAL=1` was set, so the toggle can explain itself. */
  forcedByEnv: boolean;
  version: string;
  /** A semver pre-release suffix (`0.8.0-beta.1`) — what makes this build a beta. */
  prerelease: boolean;
  /** Whether this install has a control-plane account yet. */
  enrolled: boolean;
  riderName: string;
}

export interface PublishOutcome {
  published: number;
  uploaded: number;
}

export interface PullOutcome {
  riders: number;
  installed: number;
  alreadyHad: number;
  /** Entries refused because their destination wasn't safe to write. */
  rejected: number;
}

export function experimentalState(): Promise<ExperimentalState> {
  return invoke<ExperimentalState>("experimental_state");
}

export function setExperimental(enabled: boolean): Promise<void> {
  return invoke<void>("set_experimental", { enabled });
}

/** Trade an invite code for an account. The token is stored by the backend, never here. */
export function enrollAccount(code: string, riderName: string): Promise<string> {
  return invoke<string>("enroll_account", { code, riderName });
}

/**
 * Claim this player's MX Bikes GUID — the identity that survives a rider-name change and
 * the one the dedicated server logs on every connection. First-come on the server side.
 */
export function setGuid(guid: string): Promise<void> {
  return invoke<void>("set_guid", { guid });
}

/** Publish this rider's paints so everyone else on the server can see them. */
export function publishPaints(profile: string, bike: string): Promise<PublishOutcome> {
  return invoke<PublishOutcome>("publish_paints", { profile, bike });
}

/** Install every other rider's paints. */
export function syncPaints(serverId: string): Promise<PullOutcome> {
  return invoke<PullOutcome>("sync_paints", { serverId });
}

// ── Dedicated servers ────────────────────────────────────────────────────────

/** A dedicated server the player administers, as stored in the app config. */
export interface ServerRef {
  id: string;
  name: string;
  /** Base URL of the `mxb-agent` on the host, e.g. `http://203.0.113.10:8787`. */
  url: string;
  /** Bearer token from that host's `agent.json`. */
  token: string;
}

/** What `mxb-agent` reports about a server. */
export interface ServerStatus {
  game: {
    running: boolean;
    pid: number | null;
    uptime_secs: number;
    /** Times the agent brought the game back after it exited on its own. */
    restarts: number;
  };
  port: number;
  server: {
    name: string | null;
    track: string | null;
    maxClients: string | null;
  };
}

export type ServerAction = "start" | "stop" | "restart";

export function listServers(): Promise<ServerRef[]> {
  return invoke<ServerRef[]>("list_servers");
}

/** Replace the whole saved list — the UI owns add/edit/remove and ordering. */
export function saveServers(servers: ServerRef[]): Promise<void> {
  return invoke<void>("save_servers", { servers });
}

/** Commands take an id, not a token: the frontend never hands back a secret it was given. */
export function serverStatus(id: string): Promise<ServerStatus> {
  return invoke<ServerStatus>("server_status", { id });
}

export function serverAction(id: string, action: ServerAction): Promise<unknown> {
  return invoke<unknown>("server_action", { id, action });
}

/** Change server settings. The agent restarts the game, which reads its `.ini` only at startup. */
export function serverSetConfig(
  id: string,
  patch: { track?: string; name?: string; maxClients?: number },
): Promise<unknown> {
  return invoke<unknown>("server_set_config", { id, patch });
}
