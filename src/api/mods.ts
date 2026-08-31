import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Attachment,
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
  DownloadRecord,
  LedgerRow,
  NewDownload,
  FrostmodInstallReport,
  FrostmodReload,
  FrostmodStatus,
  RuntimeInstallOutcome,
  RuntimeRepairReport,
  VcRuntime,
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
  StudioImage,
  BuildTexture,
  PaintDest,
  SavedPaint,
  PaintTemplate,
  BikeModel,
  EdfNode,
  RiderModel,
  RiderPart,
  GearPaints,
  Preset,
  PresetApplyOutcome,
  ReshadeApplyOutcome,
  ReshadeStatus,
  SwapApplyOutcome,
  ReloadOutcome,
  BundlePlan,
  BundleProgress,
  SharePlan,
  FileShare,
  GameId,
  GameInfo,
  LockItem,
  LockOutcome,
  LockProgress,
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

/** Routes an install to the ReShade preset folder instead of the mods tree. Must match
 *  `reshade::SUBPATH` in the backend. Declared here rather than beside the mod type that
 *  uses it because that one is a `const` array evaluated at module load. */
export const RESHADE_SUBPATH = "reshade";

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
  {
    id: "reshade",
    label: "modType.reshade",
    labelInline: "modType.reshadeInline",
    // mxb-mods' "ReShade Presets" category. Scoped to it alone rather than to its Misc
    // parent (40): the installer knows how to route a preset, and would have nowhere to
    // put the plugins and tools filed alongside it.
    categoryId: 174,
    // Not a `mods/` folder at all — the game's install dir. `reshade::SUBPATH` is what the
    // backend routes on; see `install::extract_and_place`.
    installSubpath: RESHADE_SUBPATH,
    categories: [{ id: 174, label: "browseCat.all" }],
  },
];

/** Does this mod type install outside the mods tree? Only ReShade presets do. */
export function installsOutsideMods(modType: ModType): boolean {
  return modType.installSubpath === RESHADE_SUBPATH;
}

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

/* ── Content lock ──────────────────────────────────────────────────────────────────── */

/** Whether this build can lock content. Same gate as the bike preview: without the
 *  optional local module the Studio hides the tool rather than offering a dead one. */
export function contentLockAvailable(): Promise<boolean> {
  return invoke<boolean>("content_lock_available");
}

/** What a run would touch — folders walked, files taken as themselves, skips flagged. */
export function contentLockPlan(paths: string[]): Promise<LockItem[]> {
  return invoke<LockItem[]>("content_lock_plan", { paths });
}

/** Write a copy of every file locked to each GUID, under `outDir/<GUID>/`. Reads only:
 *  the creator's originals are never touched. */
export function contentLockRun(
  paths: string[],
  guids: string[],
  outDir: string,
): Promise<LockOutcome> {
  return invoke<LockOutcome>("content_lock_run", { paths, guids, outDir });
}

export function onContentLockProgress(
  cb: (p: LockProgress) => void,
): Promise<UnlistenFn> {
  return listen<LockProgress>("content-lock://progress", (e) => cb(e.payload));
}

/** This player's own MX Bikes GUID, read out of the running game. `null` when the game
 *  isn't running or hasn't signed in to Steam yet — the ordinary case, not an error. */
export function localGuid(): Promise<string | null> {
  return invoke<string | null>("local_guid");
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

/** A gear model the game can't reach where it was installed.
 *  `gather` — its files sit loose in the area root instead of in a folder of their own.
 *  `unwrap` — its `.pkz` is buried in a folder, a level below where anything looks. */
export interface GearRepair {
  kind: "gather" | "unwrap";
  /** What `repairGear` is asked for: the area, or `<area>/<folder>` for a buried package. */
  id: string;
  /** The area folder under `mods/rider` — `helmets`, `boots`, … */
  area: string;
  /** The model: the folder loose files gather into, or the name a raised package will show as. */
  model: string;
  dest: string;
  /** What moves, by name. */
  items: string[];
}

export function scanGearRepairs(): Promise<GearRepair[]> {
  return invoke<GearRepair[]>("scan_gear_repairs");
}

/** Carry out one repair by its `id`; resolves with the entries moved. */
export function repairGear(id: string): Promise<number> {
  return invoke<number>("repair_gear", { id });
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

/** The bike folders a model set could be moved to. */
export function bikeFolders(): Promise<string[]> {
  return invoke<string[]>("bike_folders");
}

/** The liveries a model owns outright — what a move offers to take with it. */
export function modelSwapLiveries(bike: string, variant: string): Promise<string[]> {
  return invoke<string[]>("model_swap_liveries", { bike, variant });
}

/**
 * Move a model set to another bike, carrying the named liveries with it.
 *
 * Liveries are opt-in because a `.pnt` is cut for one bike's UV layout. Whatever isn't
 * carried stays on the old bike and simply loses its claim — nothing is deleted.
 */
export function moveModelSwap(
  bike: string,
  variant: string,
  toBike: string,
  carry: string[],
): Promise<void> {
  return invoke<void>("move_model_swap", { bike, variant, toBike, carry });
}

/** Send a model set to the Trash. Its liveries stay on the bike, unclaimed. */
export function deleteModelSwap(bike: string, variant: string): Promise<void> {
  return invoke<void>("delete_model_swap", { bike, variant });
}

export function scanModelSwaps(): Promise<BikeModels[]> {
  return invoke<BikeModels[]>("scan_model_swaps");
}

export function applyModelSwap(bike: string, target: string): Promise<SwapApplyOutcome> {
  return invoke<SwapApplyOutcome>("apply_model_swap", { bike, target });
}

/**
 * The bike as a model swap would leave it, without applying it — the swap's file set is
 * assembled in memory, so nothing moves on disk and the game can stay open. "Stock" shows
 * the packed model the loose files are hiding.
 */
export function previewModelSwap(
  bike: string,
  variant: string,
  tyres?: string,
): Promise<BikeModel> {
  return invoke<BikeModel>("preview_model_swap", {
    bike,
    variant,
    tyres: tyres || null,
  }).then(reviveMesh);
}

/**
 * Every livery the bike has, wherever it currently sits — including the ones shelved
 * because the model that claims them isn't on the bike. The assignment picker needs all of
 * them, or shelving a livery would look like losing it.
 */
export function listBikeLiveries(bike: string): Promise<string[]> {
  return invoke<string[]>("list_bike_liveries", { bike });
}

/**
 * Set which liveries a model swap owns. Liveries belonging to a model that isn't active
 * are moved out of the bike's `paints/` folder, which is what filters MX Bikes' own paint
 * list too — the game reads that one folder and knows nothing about model swaps.
 */
export function setModelPaints(
  bike: string,
  model: string,
  paints: string[],
): Promise<SwapApplyOutcome> {
  return invoke<SwapApplyOutcome>("set_model_paints", { bike, model, paints });
}

export function scanSoundSwaps(): Promise<BikeSounds[]> {
  return invoke<BikeSounds[]>("scan_sound_swaps");
}

/** ReShade's install state and every preset the app can switch to. */
export function reshadeStatus(): Promise<ReshadeStatus> {
  return invoke<ReshadeStatus>("reshade_status");
}

/**
 * Point ReShade support at a folder of the player's choosing. Pass an empty string to clear
 * it, back to the game's install dir.
 *
 * Taken as given, ReShade there or not — {@link reshadeStatus} reports what's actually in the
 * folder on the very next read, which is a better answer than a rejected pick.
 */
export function setReshadePath(path: string): Promise<void> {
  return invoke<void>("set_reshade_path", { path });
}

/** Make `name` the active preset. `RESHADE_OFF` is the no-effects preset. */
export function applyReshadePreset(name: string): Promise<ReshadeApplyOutcome> {
  return invoke<ReshadeApplyOutcome>("apply_reshade_preset", { name });
}

/** Remove a preset the app installed. Presets loose in the game folder aren't ours to
 *  delete — `ReshadePreset.managed` says which is which. */
export function deleteReshadePreset(name: string): Promise<void> {
  return invoke<void>("delete_reshade_preset", { name });
}

/** The no-effects preset — matches `reshade::OFF`, which resolves it to a real file. */
export const RESHADE_OFF = "Off";

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
 * Serialises the calls below, because they overwrite each other.
 *
 * `watch_paint_files` replaces the whole watch set, so two of them racing decides what is
 * watched by whichever lands second — and a switch between paints issues exactly that pair
 * (the old effect's stop, then the new effect's start) with nothing ordering them. A viewer
 * that quietly stopped watching is the one failure this feature can't announce.
 */
let watchTurn: Promise<unknown> = Promise.resolve();

/**
 * Watch these paint files for edits, replacing whatever was being watched. `[]` stops.
 *
 * Changes arrive through {@link onPaintChanged}, carrying back the same path strings passed
 * in here — never the OS's own spelling of them.
 */
export function watchPaintFiles(paths: string[]): Promise<void> {
  // `.catch` first: one refused watch must not wedge every later call behind a rejection.
  watchTurn = watchTurn
    .catch(() => {})
    .then(() => invoke<void>("watch_paint_files", { paths }));
  return watchTurn as Promise<void>;
}

/** Fires when a watched paint is rewritten on disk — a repack, or a save from the studio. */
export function onPaintChanged(cb: (path: string) => void): Promise<UnlistenFn> {
  return listen<{ path: string }>("paint-changed", (e) => cb(e.payload.path));
}

/* ── Paint studio ──────────────────────────────────────────────────────────────────── */

/** Read image files as textures — non-power-of-two sizes come back resized, flagged. */
export function paintStudioLoad(paths: string[]): Promise<StudioImage[]> {
  return invoke<StudioImage[]>("paint_studio_load", { paths });
}

/**
 * One image at its own resolution, for the Designer to composite with.
 *
 * Not {@link paintStudioLoad}: that answers with a thumbnail token, and a sheet composited
 * from thumbnails would be saved as one. Read the pixels with {@link textureBytes}.
 */
export function paintStudioPixels(path: string): Promise<PaintTexture> {
  return invoke<PaintTexture>("paint_studio_pixels", { path });
}

/**
 * Stage a composited sheet as a file, returning the path to pack.
 *
 * The PNG goes up as the raw request body — a 4096² sheet is megabytes, and an argument
 * would be JSON-encoded as a list of numbers. Tauri sends an `ArrayBuffer` passed in the
 * argument position as `InvokeBody::Raw`, so the sheet's name has to travel as a header.
 */
export function paintStudioStage(name: string, png: ArrayBuffer): Promise<string> {
  return invoke<string>("paint_studio_stage", png, {
    headers: { "x-sheet-name": name },
  });
}

/**
 * Write a photo of the 3D preview to a path the user picked in a save dialog.
 *
 * Same shape as {@link paintStudioStage} and for the same reason: a 4K frame is megabytes, so
 * the PNG is the request body and the destination rides in a header — percent-encoded, since
 * a header is ASCII and a pictures folder is usually under somebody's name.
 */
export function photoSave(dest: string, png: ArrayBuffer): Promise<string> {
  return invoke<string>("photo_save", png, {
    headers: { "x-dest": encodeURIComponent(dest) },
  });
}

/**
 * The raw bytes of a `.psd`, for the Designer to take apart itself.
 *
 * The parsing lives in the webview because that is where the pixels have to end up — a PSD
 * layer becomes a canvas — so the backend's whole part is handing the file over. Rejects
 * anything that isn't a `.psd`/`.psb`.
 */
export function psdRead(path: string): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("psd_read", { path });
}

/** Write a sheet's `.psd` to a path the user picked. Body and header, as {@link photoSave}. */
export function psdSave(dest: string, psd: ArrayBuffer): Promise<string> {
  return invoke<string>("psd_save", psd, {
    headers: { "x-dest": encodeURIComponent(dest) },
  });
}

/** The file a save would write, resolved but not written — so we can ask before replacing. */
export function paintStudioTarget(
  fileName: string,
  dest: PaintDest,
): Promise<{ path: string; exists: boolean }> {
  return invoke<{ path: string; exists: boolean }>("paint_studio_target", { fileName, dest });
}

export function paintStudioSave(req: {
  name: string;
  fileName: string;
  textures: BuildTexture[];
  dest: PaintDest;
  overwrite: boolean;
}): Promise<SavedPaint> {
  return invoke<SavedPaint>("paint_studio_save", req);
}

/** Write an existing paint's sheets out as `.tga` files to edit. */
export function paintStudioExtract(path: string, dest?: string): Promise<PaintTemplate> {
  return invoke<PaintTemplate>("paint_studio_extract", { path, dest: dest ?? null });
}

/** The texture names the paints already installed at `rel` supply — what to call yours. */
export function paintStudioHints(rel: string): Promise<string[]> {
  return invoke<string[]>("paint_studio_hints", { rel });
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

/**
 * A bike's geometry, its paints, and the wheels it wears.
 *
 * `tyres` substitutes the pack the bike's own `gfx.cfg` names — a bike names exactly one, so
 * that substitution is the only way to see it on another. Nothing on disk is renamed, and a
 * name that isn't installed falls back to the bike's own rather than losing the wheels.
 * Omit it (or pass `""`) for whatever the bike itself asks for.
 */
/**
 * Turn a mesh's base64 bulk arrays back into typed arrays, in place.
 *
 * The backend sends `positions`/`uvs`/`normals`/`indices` as base64 of their raw bytes
 * rather than as JSON numbers — see `EdfNode` in `edf.rs` for why. Everything downstream
 * reads them by index and by `.length`, which a typed array answers exactly as an array did,
 * so this is the only place that knows the difference.
 *
 * Walks the payload rather than naming a path into it, because five commands return meshes
 * under four different shapes, and a sixth would otherwise arrive silently broken.
 */
function reviveMesh<T>(value: T): T {
  const seen = new Set<object>();
  const walk = (v: unknown): void => {
    if (v === null || typeof v !== "object") return;
    if (seen.has(v as object)) return;
    seen.add(v as object);
    if (Array.isArray(v)) {
      v.forEach(walk);
      return;
    }
    const o = v as Record<string, unknown>;
    if (typeof o.positions === "string") {
      o.positions = f32(o.positions);
      o.uvs = f32(o.uvs as string);
      o.normals = f32(o.normals as string);
      o.indices = u32(o.indices as string);
      // `submeshes` and the rest still need walking — a node can hold further meshes.
    }
    Object.values(o).forEach(walk);
  };
  walk(value);
  return value;
}

/** Base64 → bytes. `atob` is the fastest thing the webview offers for this. */
function bytesOf(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
  return out;
}

/** The buffer from `bytesOf` is freshly allocated, so it starts aligned and a view is free.
 *  A length that isn't a whole number of values means a truncated payload — hand back an
 *  empty array, which every consumer already handles, rather than throwing. */
function f32(b64: string): Float32Array {
  const b = bytesOf(b64);
  if (b.byteLength % 4) return new Float32Array(0);
  return new Float32Array(b.buffer, 0, b.byteLength / 4);
}

function u32(b64: string): Uint32Array {
  const b = bytesOf(b64);
  if (b.byteLength % 4) return new Uint32Array(0);
  return new Uint32Array(b.buffer, 0, b.byteLength / 4);
}

export function loadBikeModel(source: string, tyres?: string): Promise<BikeModel> {
  return invoke<BikeModel>("load_bike_model", { source, tyres: tyres || null }).then(
    reviveMesh,
  );
}

export function loadRiderModel(loadout: Loadout): Promise<RiderModel> {
  return invoke<RiderModel>("load_rider_model", { loadout }).then(reviveMesh);
}

export function loadRiderBodyModel(profile: string): Promise<EdfNode[]> {
  return invoke<EdfNode[]>("load_rider_body_model", { profile }).then(reviveMesh);
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
  }).then(reviveMesh);
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
  return invoke<RiderPart>("load_stock_gear_model", { part, paintPath }).then(reviveMesh);
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

export interface LogFile {
  name: string;
  path: string;
  bytes: number;
  /** Last modified, Unix ms — null when the OS wouldn't say. */
  modified: number | null;
}

/** One place logs come from. `dir` is always set: when nothing was found it's the folder
 *  we looked in, which is still what someone needs to be told. */
export interface LogGroup {
  dir: string;
  /** Whether the folder itself is there — a different problem from it being empty. */
  exists: boolean;
  /** Newest first. */
  files: LogFile[];
}

/** Every log set: MXB App's own, FrostMod's managed folder, and the game's `log.txt`. */
export interface LogsInfo {
  app: LogGroup;
  frostmod: LogGroup;
  game: LogGroup;
}

export type LogsKind = "app" | "frostmod" | "game";

export function logsInfo(): Promise<LogsInfo> {
  return invoke<LogsInfo>("logs_info");
}

/**
 * Write a line into MXB App's own log file from here.
 *
 * The webview's console goes nowhere a player can send us — only what Rust logs reaches
 * the file behind Settings → Logs. Anything the frontend alone can see has to come back
 * through this to survive a bug report.
 */
export function logClient(level: "info" | "warn" | "error", message: string): Promise<void> {
  return invoke<void>("log_client", { level, message });
}

/** Open the folder a log set lives in, newest file selected where the OS can. */
export function openLogsFolder(which: LogsKind): Promise<void> {
  return invoke<void>("open_logs_folder", { which });
}

export interface LogsExport {
  path: string;
  files: number;
  bytes: number;
}

/** Zip every log set to `dest` — a path the user picked in a save dialog. */
export function exportLogs(dest: string): Promise<LogsExport> {
  return invoke<LogsExport>("export_logs", { dest });
}

/** A shared log bundle: what went up, and the link that came back. */
export interface LogsShare {
  /** Direct download link — what gets pasted into a bug report. */
  url: string;
  /** Every slice's link, in order, when the zip was too big to go up in one piece.
   *  Empty for the single-part upload a log bundle almost always is. */
  parts: string[];
  files: number;
  /** Bytes of log collected, before compression. */
  bytes: number;
  /** Bytes uploaded — the zip, a fraction of `bytes` for plain text. */
  size: number;
}

/** Zip every log set, upload it, and hand back the direct link. Same archive as
 *  {@link exportLogs}, and the same upload the Library's file share goes out on. */
export function shareLogs(): Promise<LogsShare> {
  return invoke<LogsShare>("share_logs");
}

/** Subscribe to log-share pack/upload phase updates. Same payload as the file share's,
 *  on its own event so Settings never hears the Library's upload. */
export function onLogsShareProgress(
  cb: (p: BundleProgress) => void,
): Promise<UnlistenFn> {
  return listen<BundleProgress>("logs-share-progress", (event) => cb(event.payload));
}

/** Hide-to-tray + keep-running toggle. */
export function setRunInBackground(enabled: boolean): Promise<void> {
  return invoke<void>("set_run_in_background", { enabled });
}

export function setGamePath(path: string): Promise<void> {
  return invoke<void>("set_game_path", { path });
}

/**
 * macOS only: what the app found to run the game with.
 *
 * MX Bikes is a Windows binary, so on a Mac it runs inside a CrossOver, Whisky or Wine
 * bottle. `runner` is the binary we'd launch through — empty means none was found and
 * Play can't work until one is installed or picked.
 */
export interface WineHostInfo {
  runner: string;
  /** Which wrapper the runner belongs to ("CrossOver", "Whisky", "Wine"). */
  via: string;
  /** Wine prefixes we can see, so a bottle we're missing shows up as missing. */
  bottles: string[];
}

export function wineHostInfo(): Promise<WineHostInfo> {
  return invoke<WineHostInfo>("wine_host_info");
}

/** Pick the Wine binary that starts the game. Pass an empty string to auto-detect again. */
export function setWineRunner(path: string): Promise<WineHostInfo> {
  return invoke<WineHostInfo>("set_wine_runner", { path });
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

/** Where the app actually reads mods from — see {@link getModsRoot}. */
export type ModsRootInfo = {
  path: string;
  exists: boolean;
  /** The folder picked *is* the mods tree, rather than the game folder above it. */
  relocated: boolean;
};

/**
 * The folder content is really read from. `modsPath` can be the game's user folder (the
 * root is its `mods` child) or a relocated tree that is the root itself, so this is the
 * only thing that answers "where is the app looking" — the first question when the
 * library comes up empty.
 */
export function getModsRoot(): Promise<ModsRootInfo> {
  return invoke<ModsRootInfo>("get_mods_root");
}

/** Launch-at-login toggle (also flips the OS autostart entry). */
export function setLaunchAtStartup(enabled: boolean): Promise<void> {
  return invoke<void>("set_launch_at_startup", { enabled });
}

/**
 * Kick off download → extract → place. Progress arrives via `onInstallProgress`.
 *
 * Resolves to `null` when the mod is installed — the ordinary case. A download that turns
 * out to hold *several* mods (the OEM bike pack is 54 bikes and a tyre set in one archive)
 * resolves to a plan instead, with nothing yet written: put it up for review and finish it
 * through `commitDrop`, or free the staged bytes with `cancelDrop`.
 */
export function addToLibrary(
  slug: string,
  url: string,
  host: string,
  subpath: string,
  destFolder: string,
): Promise<DropPlan | null> {
  return invoke<DropPlan | null>("add_to_library", {
    slug,
    url,
    host,
    subpath,
    destFolder,
  });
}

/** Stop the install in flight for `slug`. `false` when nothing is running under it — only the
 *  transfer is interruptible, so this does nothing once extraction has begun. */
export function cancelInstall(slug: string): Promise<boolean> {
  return invoke<boolean>("cancel_install", { slug });
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

/**
 * Every download a mod offers, best first.
 *
 * Dedicated-server builds are sorted last rather than dropped. Hiding them read as "this
 * mod has one file" while the flag was a guess — a mislabelled block took the only download
 * off the page, and a server build the parser missed was silently preselected instead.
 * Ranked below even a browser-only mirror, since that at least installs something playable.
 */
export function sortMirrors(detail: ModDetail): DownloadOption[] {
  return [...(detail.downloads ?? [])].sort((a, b) => {
    if (a.isServer !== b.isServer) return Number(a.isServer) - Number(b.isServer);
    const ab = isBlockedDownload(a) ? 1 : 0;
    const bb = isBlockedDownload(b) ? 1 : 0;
    if (ab !== bb) return ab - bb;
    return Number(b.isDefault) - Number(a.isDefault);
  });
}

/** The downloads that install something playable — everything but the server builds. */
export function playableMirrors(mirrors: DownloadOption[]): DownloadOption[] {
  return mirrors.filter((m) => !m.isServer);
}

/**
 * Which of `mirrors` (in {@link sortMirrors} order) a picker should start on: the best
 * playable file, never a server build while anything else is on offer.
 */
export function defaultMirrorIndex(mirrors: DownloadOption[]): number {
  const i = mirrors.findIndex((m) => !m.isServer);
  return i >= 0 ? i : 0;
}

/** A mod whose every download is a dedicated-server build — nothing here is playable. */
export function isServerOnly(mirrors: DownloadOption[]): boolean {
  return mirrors.length > 0 && mirrors.every((m) => m.isServer);
}

/**
 * The displacement-shaped numbers a piece of text carries — two or three digits, so a year
 * can never pass for a machine ("2026 CLUBMX Redbud" names no bike). Whole runs only: `250`
 * is in `CR250`, and is not in `2023` or `1250`.
 */
const displacements = (s: string): Set<string> =>
  new Set(digitRuns(s).filter((n) => n.length >= 2 && n.length <= 3));

/** `MX1OEM_2023_KTM_250_SX-F/paints` → `MX1OEM_2023_KTM_250_SX-F`. */
export function bikeOfDest(dest: string): string {
  return dest.replace(/[/\\]paints$/i, "");
}

/** The bikes a destination list offers, taken from the `<bike>/paints` entry each one gets. */
export function bikeNamesFromDest(destOptions: DestOption[]): string[] {
  const out = new Set<string>();
  for (const o of destOptions) {
    const m = /^(.+)[/\\]paints$/i.exec(o.value);
    if (m) out.add(m[1]);
  }
  return [...out];
}

export interface BikeVariants {
  /** The bikes each download names, in `mirrors` order. Empty where it names none. */
  bikes: Set<string>[];
  /**
   * Whether these are per-bike *files* rather than mirrors of one — i.e. whether the file
   * to grab depends on which bike it's being installed for.
   */
  perBike: boolean;
}

/**
 * Which bike each download on a page is for, where the page has one file per bike.
 *
 * Authors label those blocks with the displacement and nothing else — `pitfactory 250f pub`
 * beside `pitfactory 125t pub`, or plain `250` beside `450` — while the site flags *both* as
 * the default file. Nothing about that says "different file" to a picker built for mirrors,
 * so the first block wins and the 250's paint lands in the 125's folder.
 *
 * A number in a label only counts when a bike actually installed carries that same run of
 * digits, which is what keeps a rider number ("Gieck 18") or a pack's name ("288essentials")
 * from reading as a machine. The page then only counts as per-bike when *every* playable
 * download names one and at least two of them disagree: pages that mix one paint with a
 * couple of model-swap links, or offer the same paint per rider, are mirrors as far as this
 * is concerned and keep the behaviour they had. Measured over 240 livery posts this picks out
 * the 7 real cases among the 64 with more than one download, and nothing else.
 */
export function bikeVariants(mirrors: DownloadOption[], bikes: string[]): BikeVariants {
  const bikeNums = bikes.map((b) => new Set(digitRuns(b)));
  const sets = mirrors.map((m) => {
    const want = displacements(m.label);
    const out = new Set<string>();
    if (want.size === 0) return out;
    bikes.forEach((b, i) => {
      for (const n of want)
        if (bikeNums[i].has(n)) {
          out.add(b);
          break;
        }
    });
    return out;
  });

  // Server builds are named after the bike too, but nobody rides one — they say nothing
  // about whether the *playable* files differ.
  const idx = mirrors.map((_, i) => i).filter((i) => !mirrors[i].isServer);
  const disjoint = (a: Set<string>, b: Set<string>) => ![...a].some((v) => b.has(v));
  const perBike =
    idx.length >= 2 &&
    idx.every((i) => sets[i].size > 0) &&
    idx.some((i, n) => idx.slice(n + 1).some((j) => disjoint(sets[i], sets[j])));

  return { bikes: sets, perBike };
}

/**
 * The download meant for `bike`, or `null` when no block names it.
 *
 * The narrowest match wins: a file labelled for one bike beats one labelled for several.
 */
export function variantForBike(
  mirrors: DownloadOption[],
  variants: BikeVariants,
  bike: string,
): DownloadOption | null {
  let best: DownloadOption | null = null;
  let bestSize = Infinity;
  for (let i = 0; i < mirrors.length; i++) {
    const set = variants.bikes[i];
    if (mirrors[i].isServer || !set?.has(bike) || set.size >= bestSize) continue;
    best = mirrors[i];
    bestSize = set.size;
  }
  return best;
}

/**
 * The destination a per-bike download is asking for, out of the ones the post itself names,
 * or `null` where that's ambiguous.
 *
 * A label reading `250` fits every 250 in the library, so only the post's own bikes can say
 * which one was meant — and only when exactly one of them is on offer.
 */
export function destForVariant(
  variants: BikeVariants,
  index: number,
  suggestions: string[],
): string | null {
  const set = variants.bikes[index];
  if (!set) return null;
  const hits = suggestions.filter((v) => set.has(bikeOfDest(v)));
  return hits.length === 1 ? hits[0] : null;
}

export function pickDownloadForBike(
  mirrors: DownloadOption[],
  bikeName: string,
): DownloadOption | null {
  if (mirrors.length === 0) return null;
  // A sound pack's server build is named after the same bike as the file to play with, so
  // it matches just as well — pick among the playable ones while there are any.
  const playable = playableMirrors(mirrors);
  const pool = playable.length ? playable : mirrors;
  const fallback = () => pool.find((m) => m.isDefault) ?? pool[0];
  const want = tokens(bikeName);
  if (want.size === 0) return fallback();

  // Deliberately word-matching only, no displacement pass: sound packs are labelled by
  // brand ("Just KTM 250SX-F") and a bare 250 would hand a Honda the KTM's sound. A livery's
  // per-bike files are read by {@link bikeVariants} instead, which has the bikes to check against.
  let best: { m: DownloadOption; score: number } | null = null;
  for (const m of pool) {
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
  | {
      ok: false;
      /** `serverOnly` — every file the page offers is a dedicated-server build. */
      reason: "blocked" | "none" | "serverOnly";
      title: string;
      host?: string;
    };

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
  const primary = mirrors[defaultMirrorIndex(mirrors)];
  if (!primary) return { ok: false, reason: "none", title: detail.title };
  // One click can't ask which build was meant, and a dedicated-server file installed by
  // mistake looks installed while the game shows nothing. Send them to the mod's page,
  // where every download is spelled out.
  if (primary.isServer) return { ok: false, reason: "serverOnly", title: detail.title };
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

  // A page with a file per bike has no single "the download": one click otherwise installs
  // the 250's paint into the folder it just picked for the 125. The file follows the folder.
  const variants =
    modType.id === "bikes"
      ? bikeVariants(mirrors, bikeNames(installed, bikeTargets))
      : { bikes: [], perBike: false };
  const match = variants.perBike
    ? variantForBike(mirrors, variants, bikeOfDest(destFolder))
    : null;
  const file = match ?? primary;
  if (isBlockedDownload(file))
    return { ok: false, reason: "blocked", title: detail.title, host: file.host };

  return {
    ok: true,
    params: {
      slug,
      title: detail.title,
      subpath: modType.installSubpath,
      destFolder,
      url: file.url,
      host: file.host,
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
  /**
   * The product's own name, without the file label. `title` is the display string (and folds
   * the file label in for a multi-file product); this is the string that groups a product's
   * files onto one card and matches it against the public catalog.
   */
  product: string;
  /** Which file of the product this is (`PRO`, `AMS`, …). Empty for a single-file product. */
  fileLabel: string;
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
/**
 * The purchases page is read out of a hidden WebView parked on it — an HTTP client can't get
 * past the store's Cloudflare challenge. That window keeps whatever it last loaded, so
 * `reload` is what makes Refresh actually re-fetch rather than re-read the same DOM.
 */
export function shopMyDownloads(reload = false): Promise<ShopItem[]> {
  return invoke<ShopItem[]>("shop_my_downloads", { reload });
}

/**
 * Download a purchased file and install it where the caller chose.
 *
 * The shop's `addToLibrary`: same destination arguments, same progress events, same placement —
 * only the bytes differ, arriving through a WebView because the store's file URLs sit behind
 * Cloudflare's managed challenge.
 */
export function shopInstall(
  item: ShopItem,
  subpath: string,
  destFolder: string,
): Promise<void> {
  return invoke<void>("shop_install", { item, subpath, destFolder });
}

/**
 * Which purchased products have a recorded install, and the folders they claim.
 *
 * Recorded at install time so the "Installed" badge is a fact. It used to be inferred by
 * comparing the product name to library folder names, which routinely disagree — `2022 ARL MX
 * Round 1` ships as `2022.ARL.MX.RD01.pkz` and lands in a folder named after the file. The
 * fuzzy comparison stays as the fallback for anything installed before this existed.
 */
export function shopInstalledMap(): Promise<Record<string, string[]>> {
  return invoke<Record<string, string[]>>("shop_installed_map");
}

/**
 * The download history — everything the app installed, newest first, failures included.
 *
 * Kept in Rust rather than derived from the library scan because a scan can only see what is
 * on disk: it has no idea when a mod arrived, and a download that failed left nothing to find.
 */
export function downloadHistory(): Promise<DownloadRecord[]> {
  return invoke<DownloadRecord[]>("download_history");
}

/** Note a finished download. Resolves to `null` if there was nothing worth recording. */
export function recordDownload(entry: NewDownload): Promise<DownloadRecord | null> {
  return invoke<DownloadRecord | null>("record_download", { entry });
}

export function forgetDownload(id: string): Promise<void> {
  return invoke<void>("forget_download", { id });
}

export function clearDownloadHistory(): Promise<void> {
  return invoke<void>("clear_download_history");
}

/**
 * Mods under `subpath` the tree no longer holds — deleted, or parked by Manage.
 *
 * Only the missing ones: what is installed came back from {@link scanLibrary} already, and a
 * thumbnail inflated for a mod the Library can see for itself is bytes over IPC for nothing.
 *
 * Distinct from {@link downloadHistory}, which knows only what the app itself fetched. This
 * covers everything that was ever in the folder, however it got there — a track built in the
 * editor and copied in by hand included.
 */
export function libraryLedger(subpath: string): Promise<LedgerRow[]> {
  return invoke<LedgerRow[]>("library_ledger", { subpath });
}

/**
 * Snapshot title, author, location and thumbnail for installed mods whose ledger row hasn't
 * got one yet.
 *
 * Call after {@link primeMetaCache}: most of the work is then already done and reduces to a
 * file read per mod. It is the only chance to capture any of it — once the files are deleted
 * there is no way to learn what they were.
 */
export function ledgerCapture(): Promise<void> {
  return invoke<void>("ledger_capture");
}

/**
 * Put a mod the app deleted back where it came from.
 *
 * Only possible when the row carries a `trashedAt`, or on a platform whose Trash can be
 * asked to undo its own recycle. Refuses rather than overwrites if something is already
 * installed at that path.
 */
export function restoreLedgerEntry(key: string): Promise<void> {
  return invoke<void>("restore_ledger_entry", { key });
}

/** Drop one row, and its thumbnail, from the ledger. */
export function forgetLedgerEntry(key: string): Promise<void> {
  return invoke<void>("forget_ledger_entry", { key });
}

/** Forget everything no longer installed. Rows still on disk stay. */
export function clearLedger(): Promise<void> {
  return invoke<void>("clear_ledger");
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

/** Did FrostMod actually get into the running game, and if not, why not?
 *
 *  Distinct from {@link isFrostmodRunning}, which only reports that the launcher is up. */
export function frostmodAttachment(): Promise<Attachment> {
  return invoke<Attachment>("frostmod_attachment");
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
/** One replay camera editor action and the key it is on, in FrostMod's own text form
 *  (`F9`, `Ctrl+Numpad1`, `none`) — one spelling shared by both programs. */
export type FrostmodKeybind = { id: string; key: string };

/** The bindings FrostMod would read right now: its defaults, plus whatever its config says. */
export function frostmodKeybinds(): Promise<FrostmodKeybind[]> {
  return invoke<FrostmodKeybind[]>("frostmod_keybinds");
}

/** Rebind the editor. FrostMod re-reads its config each time the editor opens, so this
 *  lands without restarting the game. Rejects a key FrostMod could not read back. */
export function setFrostmodKeybinds(binds: FrostmodKeybind[]): Promise<void> {
  return invoke<void>("frostmod_set_keybinds", { binds });
}

/** What FrostMod ships with, for the reset button. */
export function frostmodDefaultKeybinds(): Promise<FrostmodKeybind[]> {
  return invoke<FrostmodKeybind[]>("frostmod_default_keybinds");
}

// ---- replay camera paths ---------------------------------------------------

/** What a key does when the path leaves it. */
export type RcamEase = "smooth" | "hold" | "cut";
export type RcamCurve = "centripetal" | "uniform";
export type RcamRig = "locked" | "handheld" | "drone" | "crane";
/** `clock` = this replay's milliseconds; `track` = the aimed rider's lap fraction. */
export type RcamAnchor = "clock" | "track";

/** A camera key. Times are always on the replay's own 30 ms grid. */
export type RcamKey = {
  t: number;
  x: number;
  y: number;
  z: number;
  yaw: number;
  pitch: number;
  roll: number;
  fov: number;
  ease: RcamEase;
  /** Race number this key aims at, or -1. */
  target: number;
  /** Metres to that rider when the key was set — what auto FOV holds the framing against. */
  aimDist: number;
  /** Lap fraction 0..1 when the key was set; negative means it was never known. */
  tp: number;
};

export type RcamPath = {
  keys: RcamKey[];
  curve: RcamCurve;
  rig: RcamRig;
  rigAmount: number;
  anchor: RcamAnchor;
  autoFov: boolean;
};

/** Enough about a slot to draw the list without opening it again. */
export type RcamSummary = {
  slot: number;
  exists: boolean;
  file: string;
  keys: number;
  shots: number;
  firstMs: number;
  lastMs: number;
  curve: RcamCurve;
  rig: RcamRig;
  rigAmount: number;
  anchor: RcamAnchor;
  autoFov: boolean;
  targets: number[];
  /** The file is there but unreadable — shown rather than passed off as an empty slot. */
  error: string | null;
};

/** Every slot FrostMod saves camera paths to, wherever it wrote them. */
export function rcamPaths(): Promise<RcamSummary[]> {
  return invoke<RcamSummary[]>("replaycam_list");
}

export function rcamRead(slot: number): Promise<RcamPath> {
  return invoke<RcamPath>("replaycam_read", { slot });
}

/** Save a slot back; resolves with the file it landed in. Rejects anything FrostMod
 *  would refuse to load, rather than leaving a slot that silently does nothing. */
export function rcamWrite(slot: number, path: RcamPath): Promise<string> {
  return invoke<string>("replaycam_write", { slot, path });
}

export function rcamDelete(slot: number): Promise<void> {
  return invoke<void>("replaycam_delete", { slot });
}

/** Respace the keys so time between them follows distance — the constant-speed dolly.
 *  Nothing is written: the caller decides whether to keep the result. */
export function rcamRetime(path: RcamPath): Promise<RcamPath> {
  return invoke<RcamPath>("replaycam_retime", { path });
}

export function rcamImport(slot: number, src: string): Promise<RcamPath> {
  return invoke<RcamPath>("replaycam_import", { slot, src });
}

export function rcamExport(slot: number, dst: string): Promise<void> {
  return invoke<void>("replaycam_export", { slot, dst });
}

/** What FrostMod's log says about the replay camera on the build it last attached to. Every
 *  camera global is resolved by signature and MX Bikes moves them between builds, so "the
 *  editor opens and does nothing" is a real state — and the reason for it lived in a log
 *  file next to a DLL. */
export type RcamStatus = {
  /** False when FrostMod has never run, or its log says nothing about the camera. */
  known: boolean;
  ready: boolean;
  /** Why it is unavailable, in FrostMod's own words. */
  reason: string | null;
  /** Whether it has worked out how this build's camera angles count. Aiming needs it. */
  calibrated: boolean;
  log: string;
};

export function rcamStatus(): Promise<RcamStatus> {
  return invoke<RcamStatus>("replaycam_status");
}

export function frostmodStatus(): Promise<FrostmodStatus> {
  return invoke<FrostmodStatus>("frostmod_status");
}

/** Download (or update to) the latest FrostMod release. All-or-nothing: it either
 *  puts both binaries in place or leaves the previous install untouched. */
export function frostmodInstall(): Promise<FrostmodInstallReport> {
  return invoke<FrostmodInstallReport>("frostmod_install");
}

/** Install a Visual C++ runtime `frostmodStatus` reported missing.
 *
 *  Raises a Windows UAC prompt — Microsoft's redistributables need admin rights. A
 *  declined prompt resolves to `"cancelled"` rather than throwing, so the caller can
 *  offer the manual download instead of reporting a failure. */
export function frostmodInstallRuntime(
  runtime: VcRuntime,
): Promise<RuntimeInstallOutcome> {
  return invoke<RuntimeInstallOutcome>("frostmod_install_runtime", { runtime });
}

/** Install everything this PC is short of, and clear a stray `msvcr90.dll` beside the exe.
 *
 *  The repair the warning bar can't reach: it runs whatever detection said, because a
 *  machine can report every runtime present and still fail to start the game. Raises UAC
 *  once per installer, and never rejects — what it couldn't do comes back in
 *  `stillMissing` for the caller to offer links for. */
export function frostmodRepairRuntimes(): Promise<RuntimeRepairReport> {
  return invoke<RuntimeRepairReport>("frostmod_repair_runtimes");
}

/** Move a loose `msvcr90.dll` beside the game exe out of the loader's way.
 *
 *  Renames it to `msvcr90.dll.disabled` rather than deleting: the loader matches the exact
 *  filename, so the rename is what defuses it, and a file somebody else put there survives
 *  the decision. Resolves to where it went; rejects with a message worth showing — a file
 *  the running game holds open is the common one. */
export function frostmodClearStrayMsvcr90(): Promise<string> {
  return invoke<string>("frostmod_clear_stray_msvcr90");
}

/** Where to send someone whose UAC prompt we can't raise (or who declined it).
 *
 *  Microsoft's own direct downloads, the same ones the backend fetches — a download page
 *  would make them pick an architecture, and picking the wrong one fixes nothing. Both
 *  2015–2022 builds are here because that is what Microsoft's "Latest Supported Visual C++
 *  Downloads" page hands out, and a 32-bit plugin or the dedicated-server build needs the
 *  x86 one no matter how many x64 packages are already installed. */
export const RUNTIME_DOWNLOAD_URL: Record<VcRuntime, string> = {
  vc90: "https://download.microsoft.com/download/5/D/8/5D8C65CB-C849-4025-8E95-C3966CAFD8AE/vcredist_x64.exe",
  vc140: "https://aka.ms/vs/17/release/vc_redist.x64.exe",
  vc140_x86: "https://aka.ms/vs/17/release/vc_redist.x86.exe",
};

/** Microsoft's index page for the pair above, for a player who'd rather see the source. */
export const RUNTIME_DOWNLOADS_PAGE =
  "https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist";

/** Translation key naming each runtime the way Microsoft's installer does.
 *
 *  The architecture is part of the name on purpose: someone told to install "Visual C++"
 *  who already has the x64 package needs to see that it's the other one being asked for. */
export const RUNTIME_NAME_KEY: Record<VcRuntime, TKey> = {
  vc90: "runtime.componentVc90",
  vc140: "runtime.componentVc140",
  vc140_x86: "runtime.componentVc140X86",
};

/** Launch the managed FrostMod process if it isn't already running. */
export function frostmodStart(): Promise<boolean> {
  return invoke<boolean>("frostmod_start");
}

/** Stop FrostMod, whoever started it — a hand-launched `frostmod.exe` included.
 *  Resolves to whether it's actually gone; `false` means it outlived the kill. */
export function frostmodStop(): Promise<boolean> {
  return invoke<boolean>("frostmod_stop");
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

export type VoiceDevice = { name: string; isDefault: boolean };
export type VoiceDevices = {
  inputs: VoiceDevice[];
  outputs: VoiceDevice[];
  /** Set when the machine has no audio at all, so the UI can say so instead of
   *  showing two empty dropdowns. */
  error: string | null;
};

/** Every microphone and output the machine currently offers.
 *
 * Call it each time the picker opens rather than caching — the point is to notice the
 * headset that was plugged in after the app launched. */
export function voiceDevices(): Promise<VoiceDevices> {
  return invoke<VoiceDevices>("voice_devices");
}

export function setVoiceEnabled(enabled: boolean): Promise<void> {
  return invoke<void>("set_voice_enabled", { enabled });
}

/** Share this rider's look with the grid, and install the grid's back. */
export function setPaintSyncEnabled(enabled: boolean): Promise<void> {
  return invoke<void>("set_paint_sync_enabled", { enabled });
}

/** Pick the tyre pack the 3D previews fit. `""` means "whatever the bike names". */
export function setPreviewTyres(tyres: string): Promise<void> {
  return invoke<void>("set_preview_tyres", { tyres });
}

/** Pick the microphone. `""` means "follow the system default". */
export function setVoiceInputDevice(device: string): Promise<void> {
  return invoke<void>("set_voice_input_device", { device });
}

/** Pick where other riders come out. `""` means "follow the system default". */
export function setVoiceOutputDevice(device: string): Promise<void> {
  return invoke<void>("set_voice_output_device", { device });
}

/** Rebind push-to-talk. Rejects (leaving the old one live) if the combo is taken. */
export function setVoicePttHotkey(hotkey: string): Promise<void> {
  return invoke<void>("set_voice_ptt_hotkey", { hotkey });
}

/** Switch between push-to-talk (hold) and toggle (latch). Rebinds the mic key. */
export function setVoiceToggleToTalk(toggle: boolean): Promise<void> {
  return invoke<void>("set_voice_toggle_to_talk", { toggle });
}

export function setVoiceLevels(inputGain: number, outputVolume: number): Promise<void> {
  return invoke<void>("set_voice_levels", { inputGain, outputVolume });
}

/** Open the mic and start the level meter. Resolves to a warning when the saved device
 *  is gone and we fell back to the default — the unplugged-headset case. */
export function voiceMeterStart(): Promise<string | null> {
  return invoke<string | null>("voice_meter_start");
}

export function voiceMeterStop(): Promise<void> {
  return invoke<void>("voice_meter_stop");
}

/** Play a short tone on the configured output. Same warning contract as the meter. */
export function voiceTestOutput(): Promise<string | null> {
  return invoke<string | null>("voice_test_output");
}

export type VoiceLevel = { rms: number; peak: number };

/** Live microphone level while the meter is running. */
export function onVoiceInputLevel(cb: (level: VoiceLevel) => void): Promise<UnlistenFn> {
  return listen<VoiceLevel>("voice-input-level", (e) => cb(e.payload));
}

/** Fires on both edges of the push-to-talk key: `true` on press, `false` on release. */
export function onVoicePtt(cb: (down: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>("voice-ptt", (e) => cb(e.payload));
}

/** One other rider in voice on this server. */
export type VoicePeer = {
  peerId: string;
  /** What they call themselves in game. A label — never trusted as an identity. */
  riderName: string;
  /** Their race number, or 0 before they are on the grid. */
  raceNum: number;
  /** The direct connection to them is up. False while it is still being made, and for
   *  anyone whose router we cannot get through to. */
  connected: boolean;
  talking: boolean;
  muted: boolean;
};

export type VoiceStatus = {
  joined: boolean;
  /** The server this room belongs to. */
  server: string;
  peers: VoicePeer[];
  /** Why voice isn't working, when it isn't. */
  error: string | null;
};

/** Who is in voice right now. The panel also gets these pushed; this is the first paint. */
export function voiceStatus(): Promise<VoiceStatus> {
  return invoke<VoiceStatus>("voice_status");
}

/** Silence one rider for the rest of this session. */
export function voiceMute(peerId: string, muted: boolean): Promise<void> {
  return invoke<void>("voice_mute", { peerId, muted });
}

/** Fires whenever the room changes — someone joined, left, started or stopped talking. */
export function onVoiceStatus(cb: (status: VoiceStatus) => void): Promise<UnlistenFn> {
  return listen<VoiceStatus>("voice-status", (e) => cb(e.payload));
}

/** Toggle watching the mods folder to reload the game on external changes. */
export function setWatchModsReload(enabled: boolean): Promise<void> {
  return invoke<void>("set_watch_mods_reload", { enabled });
}

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

/**
 * Drop a bike from a profile and return what's left.
 *
 * The list above is `profile.ini`, not the mods folder, so bikes whose mod is gone linger
 * there with nothing in the Library to uninstall. This is how they go.
 */
export function presetsForgetBike(profile: string, bikeid: string): Promise<string[]> {
  return invoke<string[]>("presets_forget_bike", { profile, bikeid });
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

// ── Sharing installed files (any track, paint or pack — not just presets) ────

/** What sharing these installed paths would carry, and what it would leave out. */
export function fileSharePlan(paths: string[]): Promise<SharePlan> {
  return invoke<SharePlan>("file_share_plan", { paths });
}

/** Pack + upload the picked files, returning a one-line share code (`MXBS1-…`). */
export function fileShareCreate(paths: string[]): Promise<string> {
  return invoke<string>("file_share_create", { paths });
}

/** Read a share code *without* downloading — preview what it carries. */
export function fileSharePreview(text: string): Promise<FileShare> {
  return invoke<FileShare>("file_share_preview", { text });
}

/** Download a share code's files and install them where the sender had them. */
export function fileShareImport(text: string): Promise<FileShare> {
  return invoke<FileShare>("file_share_import", { text });
}

/** Subscribe to file-share create/import phase updates. Same payload as the preset
 *  bundle's, on its own event so the two dialogs never cross. */
export function onFileShareProgress(
  cb: (p: BundleProgress) => void,
): Promise<UnlistenFn> {
  return listen<BundleProgress>("file-share-progress", (event) => cb(event.payload));
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
  /**
   * Whether this install has a control-plane account yet.
   *
   * No longer "typed in an invite code": paint sync claims one on its own the first time it
   * runs, so this is simply whether the control plane knows who this is.
   */
  enrolled: boolean;
  /** Whether paint sync is running at all. Off is a setting, not a missing account. */
  paintSyncEnabled: boolean;
  riderName: string;
  /** What paint sync last achieved — present so a cold start can say so straight away. */
  sync: SyncState;
  /**
   * The MX Bikes profile paint sync speaks for, or `null` when there is none.
   *
   * Null means nothing can ever be published, which is worth saying rather than leaving as
   * a publish that quietly does nothing.
   */
  profile: string | null;
}

export interface PublishOutcome {
  /** Paints sent, across every bike. */
  published: number;
  uploaded: number;
  /** Bikes that carried something worth publishing. */
  bikes: number;
  /** Bikes beyond the cap that were left out. */
  skippedBikes: number;
  /** Paints too large for the control plane to store, so nobody else will see them. */
  oversizedPaints: number;
  digest: string;
  /** The look already matched what was last sent, so nothing went up. */
  unchanged: boolean;
}

export interface PullOutcome {
  riders: number;
  installed: number;
  alreadyHad: number;
  /** Entries refused because their destination wasn't safe to write. */
  rejected: number;
  /** Paints left alone because that file is the player's own, not one the sync installed. */
  keptYours: number;
  /** Destinations two riders disagreed about, where neither was installed. */
  conflicted: number;
}

/** What the last publish and the last pull achieved, as the backend recorded them. */
export interface SyncState {
  publishedDigest: string;
  /** Unix milliseconds; `0` means never. */
  publishedAt: number;
  publishedBikes: number;
  publishedPaints: number;
  pulledAt: number;
  pulledRiders: number;
  keptYours: number;
  conflicted: number;
}

/**
 * A background publish or pull, as it happens.
 *
 * Both run in spawned tasks off actions the player didn't ask for directly — an apply, a
 * launch, a file changing under us. Before this event they were invisible: the only report
 * was a log line, so "is this working?" had no answer anywhere on screen.
 */
export interface SyncEvent {
  phase: "publishing" | "published" | "pulling" | "pulled" | "failed";
  publish?: PublishOutcome;
  pull?: PullOutcome;
  error?: string;
}

export function onSyncEvent(cb: (event: SyncEvent) => void): Promise<UnlistenFn> {
  return listen<SyncEvent>("paint-sync", (e) => cb(e.payload));
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
/**
 * Publish everything this rider is wearing, across every bike.
 *
 * The app does this on its own whenever the look changes, so this is the button for wanting
 * to see it happen. `force` sends an unchanged look anyway — without it, pressing Publish
 * after a successful one is correctly a no-op, which reads as the button being broken.
 */
export function publishPaints(force = false, profile?: string): Promise<PublishOutcome> {
  return invoke<PublishOutcome>("publish_paints", { profile: profile ?? null, force });
}

/**
 * Install every other rider's paints.
 *
 * The app already does this on its own when the game launches — this is the manual retry
 * for a sync that ran before someone had published, or that hit a flaky connection.
 */
export function syncPaints(): Promise<PullOutcome> {
  return invoke<PullOutcome>("sync_paints");
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
  /** Control-plane id once this server is in the public list; empty when it isn't. */
  registryId?: string;
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

/** A server the control plane knows about — the list the join picker offers. */
export interface RegisteredServer {
  id: string;
  name: string;
  region: string;
  /** `host:port`, the form the game's connect flag takes. */
  address: string;
}

/**
 * The joinable servers. Empty when this install hasn't enrolled — the registry needs a
 * token — so callers should fall back to asking for an address rather than showing an error.
 */
export function cpServers(): Promise<RegisteredServer[]> {
  return invoke<RegisteredServer[]>("cp_servers");
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

/** The tracks that host has installed — the only values its `.ini` will accept. */
export function serverTracks(id: string): Promise<string[]> {
  return invoke<string[]>("server_tracks", { id });
}

/**
 * Ask an agent to describe itself before it's saved, so the add form can fill in the name
 * the host already knows — and so a bad address or token fails here rather than as a dead
 * row in the list.
 */
export function serverProbe(url: string, token: string): Promise<ServerStatus> {
  return invoke<ServerStatus>("server_probe", { url, token });
}

/**
 * An `mxb://enroll?code=…` link was opened.
 *
 * Fires with the invite code alone — the backend has already checked the link is the enroll
 * route and that the code is a plain token. It only ever *prefills* the field; enrolling is
 * still a button the player presses, so a link can't spend an invite by itself.
 */
export function onEnrollLink(cb: (code: string) => void): Promise<UnlistenFn> {
  return listen<string>("deep-link-enroll", (event) => cb(event.payload));
}

/** Where a server can be hosted, as the control plane will accept it. */
export const SERVER_REGIONS = [
  "eu-central-1",
  "eu-west-1",
  "us-east-1",
  "us-west-2",
  "ap-southeast-2",
] as const;

/**
 * What the region picker starts on.
 *
 * Where somebody else's hardware physically sits is unknowable from here, so this is a
 * guess either way — but the control plane provisions in `us-west-2` and nothing else, so
 * that is the better guess than whichever region happened to be first in the list.
 */
export const DEFAULT_SERVER_REGION = "us-west-2";

/** An EC2 instance the control plane launched, as AWS reports it. */
export interface FleetInstance {
  instanceId: string;
  state: string;
  publicIp: string | null;
  instanceType: string;
  launchedAt: string | null;
}

export interface FleetState {
  region: string;
  /** Instances running across the whole fleet — what the cap is measured against. */
  running: number;
  cap: number;
  /** Only this account's own; an instance id and IP belong to whoever pays for the box. */
  instances: FleetInstance[];
}

/**
 * A server the control plane runs for this account.
 *
 * Carries the agent token, which the public list never does. That is the point: a
 * provisioned box has no console and prints its pairing code to nobody, so its owner has no
 * other way to get the credential for a server they are paying for.
 */
export interface CloudServer {
  id: string;
  name: string;
  region: string;
  /** `host:port` players connect to. Empty until the box has announced itself. */
  address: string;
  agentUrl: string | null;
  agentToken: string | null;
  instanceId: string | null;
  published: boolean;
  createdAt: number;
  /** When it was last seen empty, or `null` while someone is riding. */
  idleSince: number | null;
  /** Minutes of emptiness before it destroys itself. */
  idleMinutes: number;
  /** `pending` | `running` | `stopping` | `stopped` | `gone` | `self-hosted`. */
  state: string;
  publicIp: string | null;
  /**
   * What the box last reported doing — `downloading the game`, `ready`, `failed`.
   *
   * The only window into a machine with no console and no key pair. A bootstrap that failed
   * and one still downloading are otherwise indistinguishable from here.
   */
  bootstrapStage: string | null;
  bootstrapAt: number | null;
  /** The tail of the transcript, present only when the bootstrap gave up. */
  bootstrapLog: string | null;
}

/** The servers the control plane runs for this account, and what it takes to drive them. */
export function cloudServers(): Promise<CloudServer[]> {
  return invoke<CloudServer[]>("cloud_servers");
}

/** Destroy one, and stop paying for it. */
export function destroyCloudServer(id: string): Promise<void> {
  return invoke<void>("destroy_cloud_server", { id });
}

/**
 * Create a server — the control plane launches a machine for it.
 *
 * The app holds no cloud credentials: a desktop binary can be unpacked, so the key lives in
 * the control plane and this asks it, authenticated as this player.
 */
export function provisionServer(name: string): Promise<{ id: string; instanceId: string }> {
  return invoke<{ id: string; instanceId: string }>("provision_server", { name });
}

/** What's running, read from EC2 — the number that turns into a bill. */
export function fleetState(): Promise<FleetState> {
  return invoke<FleetState>("fleet_state");
}

export interface PublishResult {
  /** The registry id, needed to take it back out of the list. */
  id: string;
  /** False when the control plane couldn't reach the agent — recorded, but not advertised. */
  published: boolean;
}

/**
 * Put a server into the public list.
 *
 * Takes nothing but the region: the address is the agent's host joined to the port it
 * reports, and the name comes from the server's own `.ini`.
 */
export function publishServer(id: string, region: string): Promise<PublishResult> {
  return invoke<PublishResult>("publish_server", { id, region });
}

/** Take it back out. Only the account that registered it may. */
export function unpublishServer(registryId: string): Promise<void> {
  return invoke<void>("unpublish_server", { registryId });
}

/** The address and token packed into the one-line code `mxb-agent` prints at startup. */
export interface Pairing {
  url: string;
  token: string;
}

/**
 * Unpack that code. The agent already knows its own address and token, so pairing is a
 * paste rather than two fields transcribed off a terminal.
 */
export function parsePairing(blob: string): Promise<Pairing> {
  return invoke<Pairing>("parse_pairing", { blob });
}

/** Change server settings. The agent restarts the game, which reads its `.ini` only at startup. */
export function serverSetConfig(
  id: string,
  patch: { track?: string; name?: string; maxClients?: number },
): Promise<unknown> {
  return invoke<unknown>("server_set_config", { id, patch });
}
