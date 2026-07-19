import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  BikeModels,
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

/**
 * Mod types on mxb-mods.com. Category ids verified against the live WP API.
 * Install subpaths are the MX Bikes convention and are easy to adjust here.
 */
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
    // Rider content: outfit/kit + gear (helmets, gloves, boots, protection).
    // Helmets/boots/protection are model + paints (a paint needs its model),
    // while rider kit + gloves paints live per rider profile. Category ids
    // verified against the live WP API (parent "Rider" = 30).
    id: "rider",
    label: "Rider",
    categoryId: 30,
    installSubpath: "mods/rider",
    categories: [
      { id: 30, label: "All" },
      { id: 35, label: "Rider Kit" },
      { id: 33, label: "Helmets" },
      { id: 127, label: "Helmet Paints" },
      { id: 32, label: "Gloves" },
      { id: 31, label: "Boots" },
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

/**
 * Rich Library scan: packaged `.pkz`, **extracted** mod folders, and **loose
 * paint files**, each tagged with kind/category/parent so the Library can group
 * and detail them. (Install pickers keep using {@link getInstalledMods}.)
 */
export function scanLibrary(subpath: string): Promise<LibraryEntry[]> {
  return invoke<LibraryEntry[]>("scan_library", { subpath });
}

/** WP category id for bike liveries — the only bike content that routes into a
 * bike's `paints` folder. */
export const LIVERY_CATEGORY_ID = 37;

/** WP category id for bike **sounds** — `engine.scl`/`sfx.cfg` (+ samples) that
 * live at a bike's *root* (next to `paints/`), never inside it. */
export const SOUND_CATEGORY_ID = 46;

/** Whether we're installing a bike **livery** (→ `<Bike>/paints`) vs a new bike
 * / sound / unknown (→ Bikes root). Drives the destination default so a new bike
 * never inherits a previous livery's `paints` folder. */
export function isLiveryContext(
  modType: ModType,
  categoryId: number | null | undefined,
): boolean {
  return modType.id === "bikes" && categoryId === LIVERY_CATEGORY_ID;
}

/** Whether we're installing a bike **sound** (→ `<Bike>` root). A sound targets a
 * bike folder itself, so we default to the matched bike's root — never `paints`
 * — and the mod page usually offers a *different* download link per bike. */
export function isSoundContext(
  modType: ModType,
  categoryId: number | null | undefined,
): boolean {
  return modType.id === "bikes" && categoryId === SOUND_CATEGORY_ID;
}

/** Installed rider models + profiles, for building rider paint destinations. */
export interface RiderTargets {
  helmets: string[];
  boots: string[];
  protection: string[];
  profiles: string[];
}

export function scanRiderTargets(): Promise<RiderTargets> {
  return invoke<RiderTargets>("scan_rider_targets");
}

/**
 * Per-bike model-swap view (the Locker): each extracted bike, its active model,
 * and the variants it can switch between. The app-side twin of FrostMod's in-game
 * swapper — same `FrostMod Models/` + `_active.txt` on-disk scheme.
 */
export function scanModelSwaps(): Promise<BikeModels[]> {
  return invoke<BikeModels[]>("scan_model_swaps");
}

/** Switch a bike to a different model set (backs up the current one). Nudges a
 * running FrostMod to live-reload after. */
export function applyModelSwap(bike: string, target: string): Promise<void> {
  return invoke<void>("apply_model_swap", { bike, target });
}

/**
 * Read one installed `.pkz`'s structure (name/author/length/preview) for its
 * library card. Parsed lazily per card and cached in the backend. non-plain
 * archives come back with `locked: true` and no thumbnail.
 */
export function getPkzMeta(path: string): Promise<PkzMeta> {
  return invoke<PkzMeta>("get_pkz_meta", { path });
}

/** Full-resolution preview image (a `data:` URI) for the library detail
 * lightbox, or `null` if the archive is locked / has no image. Loaded on demand
 * when a detail view opens — not per card. */
export function getPkzPreview(path: string): Promise<string | null> {
  return invoke<string | null>("get_pkz_preview", { path });
}

/** Decode a `.pnt` paint file into its textures (PNG `data:` URIs) for the 3D
 * viewer to map onto a model. Native — no external tools needed. */
export function unpackPaint(path: string): Promise<PaintTexture[]> {
  return invoke<PaintTexture[]>("unpack_paint", { path });
}

/** Extract a `.pkz` to `outDir`, returning the written relative paths — lets the
 * viewer pull a bike's `model.edf` + textures out of a packaged bike. */
export function unpackPkz(path: string, outDir: string): Promise<string[]> {
  return invoke<string[]>("unpack_pkz", { path, outDir });
}

/** Load a bike's real 3D geometry + textures for the viewer. `source` may be the
 * bike's extracted folder, its packaged `.pkz`, or a loose `.edf`. */
export function loadBikeModel(source: string): Promise<BikeModel> {
  return invoke<BikeModel>("load_bike_model", { source });
}

/** Load the rider's real 3D preview for a loadout: the installed gear meshes
 * (helmet/boots/protection) decoded from `.edf`, plus the suit/gloves paints that
 * tint the stand-in body. Unset/missing slots are omitted. */
export function loadRiderModel(loadout: Loadout): Promise<RiderModel> {
  return invoke<RiderModel>("load_rider_model", { loadout });
}

/** Just the rider body mesh nodes for a profile (from the game's `rider.pkz`), for
 * the Library outfit viewer — which supplies its own paint. Empty when the game
 * folder isn't set. */
export function loadRiderBodyModel(profile: string): Promise<EdfNode[]> {
  return invoke<EdfNode[]>("load_rider_body_model", { profile });
}

/** Load an installed gear item (helmet/boots/protection) from its Library path —
 * an extracted folder or a packaged `.pkz` — so it can be previewed on the rider.
 * `part` is the viewer slot to fill. */
export function loadGearModel(
  path: string,
  part: RiderPart["part"],
  paint?: string,
  goggles?: string,
): Promise<RiderPart> {
  return invoke<RiderPart>("load_gear_model", { path, part, paint, goggles });
}

/** The paint sets a gear item ships. Gear installs packaged, so its paints aren't
 * loose files the Library can list — this reads them out of the archive. `goggles`
 * is a helmet's separate lens/strap paints (empty for boots/protection). */
export function listGearPaints(path: string): Promise<GearPaints> {
  return invoke<GearPaints>("list_gear_paints", { path });
}

/** The packed paints/goggles for an installed gear model **named by the loadout**
 * (`part` = helmet/boots/protection, `model` = the folder/`.pkz` name). Gear installs
 * packaged, so its paints live inside the archive — this resolves the model like the
 * renderer does and reads them out, for the Rider studio's paint/goggle pickers.
 * Empty for stock/built-in gear. */
export function listInstalledGearPaints(
  part: RiderPart["part"],
  model: string,
): Promise<GearPaints> {
  return invoke<GearPaints>("list_installed_gear_paints", { part, model });
}

/** Preview a loose gear paint on the game's **stock** model for that slot (boots /
 * helmet / protection) — for paints whose own model isn't installed. `paintPath` is
 * the loose `.pnt`; omit for the stock paint. Mesh comes from the game's `rider.pkz`,
 * so it needs the game path set. */
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

/** Set the MX Bikes **install** directory (holds core `rider.pkz`), so the 3D
 * viewer can load the real rider body model. */
export function setGamePath(path: string): Promise<void> {
  return invoke<void>("set_game_path", { path });
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

/**
 * Build the "install into" destination options for a mod, plus an educated-guess
 * default. Tracks get their existing sub-folders; bikes get each installed
 * bike's `paints` folder, with the guess matched from the mod title.
 */
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
  // Ranked "probable" destinations (best first) — bike liveries matched by name.
  const suggestions: string[] = [];
  if (modType.id === "bikes") {
    const bikes = installed.filter((i) => i.folder === "");
    // A sound targets the bike *folder itself*; a livery targets its `paints`.
    // Offer the bike **root** first (for sounds/new bikes), then its `paints`.
    for (const b of bikes) {
      add(stripExt(b.name), `${stripExt(b.name)} — bike folder`);
      add(`${stripExt(b.name)}/paints`, `${stripExt(b.name)} — paints`);
    }

    // Suggest/guess a specific bike only when we know the intent (livery →
    // paints, sound → bike root). A new bike / unknown stays at Bikes (root).
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

/** Stock MX Bikes rider profiles — always present in-game even when the app sees
 * no `riders/` folder on disk yet. Seeded so kit/gloves always have a valid
 * `riders/<profile>/…` destination (a missing target is what misfiles outfits into
 * `mods/rider` root, where nothing scans them). */
export const STOCK_RIDER_PROFILES = ["default_mx", "default_sm"];

/** WP category ids for rider content that installs per rider **profile** (not per
 * gear model): the outfit / Rider Kit (35 + children Retro 129, Virtual Team 52)
 * and gloves (32). These route to `riders/<profile>/{paints,gloves}`. */
const RIDER_KIT_CATEGORY_IDS = [35, 129, 52];
const RIDER_GLOVES_CATEGORY_ID = 32;

/** The gear kinds whose paints install into a model's `paints` folder. */
export type GearPaintKind = "helmets" | "boots" | "protection";

/** WP category id → gear kind for the model-paint categories. mxb-mods separates
 * each gear type into a model category and a *paints* child (Helmets 33 / Helmet
 * Paints 127, Boots 31 / Boot Paints 126, Protection 36 / Protection Paints 135).
 * Knowing the kind lets an install target the right model's `paints`. */
const RIDER_PAINT_CATEGORY_KIND: Record<number, GearPaintKind> = {
  127: "helmets",
  126: "boots",
  135: "protection",
};

/**
 * For rider content, which gear kind a **paint** belongs to (helmet/boot/
 * protection), derived from its mxb-mods paints category. `null` when the mod
 * isn't a gear paint (a model, kit, gloves, or an unfiltered "All" browse).
 * Lets {@link buildRiderDestinations} bias the target to that kind's installed
 * models instead of name-matching across every gear type.
 */
export function riderPaintKind(
  modType: ModType,
  categoryId: number | null | undefined,
): GearPaintKind | null {
  if (modType.id !== "rider" || categoryId == null) return null;
  return RIDER_PAINT_CATEGORY_KIND[categoryId] ?? null;
}

/**
 * For rider content, whether the mod is per-**profile** content and which
 * sub-folder it targets: `"paints"` for the outfit/Rider Kit, `"gloves"` for
 * gloves. `null` for gear-model paints (helmet/boot/protection) and new models,
 * which target a model folder instead. Mirrors {@link isLiveryContext} /
 * {@link isSoundContext}.
 */
export function riderProfileSub(
  modType: ModType,
  categoryId: number | null | undefined,
): "paints" | "gloves" | null {
  if (modType.id !== "rider") return null;
  if (categoryId === RIDER_GLOVES_CATEGORY_ID) return "gloves";
  if (categoryId != null && RIDER_KIT_CATEGORY_IDS.includes(categoryId)) return "paints";
  return null;
}

/**
 * Destinations for **rider** content. Helmet/boot/protection paints drop into
 * their model's `paints` (and helmets also `goggles`); rider kit + gloves live
 * per rider profile under `riders/<profile>/{paints,gloves}`. New models install
 * to their type root. The stock profiles are always offered so kit/gloves have a
 * valid target even on a fresh install. When `profileSub` is set (kit → `paints`,
 * gloves → `gloves`), the guess defaults to the first stock profile's folder so
 * the content can't fall into `mods/rider` root.
 */
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

  // New-model roots first.
  add("helmets", "Helmets (new model)");
  add("boots", "Boots (new model)");
  add("protection", "Protection (new model)");

  // Model paints, scored for suggestions and tagged with their gear kind so a
  // known paint category can bias the target to just that kind's models.
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

  // Per-profile outfit (rider kit) + gloves — installed profiles plus the stock
  // ones (so a fresh install still has a valid `riders/<profile>/…` target).
  const profiles = [...new Set([...targets.profiles, ...STOCK_RIDER_PROFILES])].sort(
    (a, b) => a.toLowerCase().localeCompare(b.toLowerCase()),
  );
  for (const prof of profiles) {
    add(`riders/${prof}/paints`, `${prof} · outfit / kit`);
    add(`riders/${prof}/gloves`, `${prof} · gloves`);
  }

  // When the paint's gear kind is known (Boot/Helmet/Protection Paints category),
  // only that kind's models are candidates — a boot paint never lands on a helmet.
  const kindPaints = paintKind
    ? scoredPaints.filter((s) => s.kind === paintKind)
    : scoredPaints;
  const topPaints = kindPaints
    .filter((s) => s.score >= 1)
    .sort((a, b) => b.score - a.score);
  const suggestions = [
    ...topPaints.slice(0, 4).map((s) => s.value),
    // Rider kit / gloves can't be name-matched — surface the profiles too.
    ...profiles.map((p) => `riders/${p}/${profileSub ?? "paints"}`),
  ];

  // Per-profile content (kit/gloves) defaults to the first stock profile's folder.
  // Model paints keep the name-matched guess — but with a known gear kind we're
  // confident enough to accept a single name-token match, and to fall back to the
  // sole installed model of that kind (the "just installed a new model" case).
  const guess = profileSub
    ? `riders/${STOCK_RIDER_PROFILES[0]}/${profileSub}`
    : paintKind
      ? topPaints[0]?.value ?? (kindPaints.length === 1 ? kindPaints[0].value : "")
      : topPaints[0] && topPaints[0].score >= 2
        ? topPaints[0].value
        : "";

  return { options, guess, suggestions };
}

/**
 * Hosts that block in-app downloads — the app opens these in the browser and
 * lets the user import the downloaded file instead. Now empty: every supported
 * host installs in-app. MediaFire downloads directly (its CDN no longer blocks
 * the rustls client, so `resolve_mediafire` + the normal download path handle
 * it) and Mega is fetched + decrypted in-app via the `mega` crate
 * (`download_mega_and_place`). The browser-import flow below remains only as a
 * manual escape hatch. Matched against the URL (reliable) as well as the host
 * label (which the site writes inconsistently, e.g. "Media Fire" with a space).
 */
const BLOCKED_HOST_PATTERNS: string[] = [];

export function isBlockedDownload(opt: { url: string; host: string }): boolean {
  const s = `${opt.url} ${opt.host}`.toLowerCase();
  return BLOCKED_HOST_PATTERNS.some((p) => s.includes(p));
}

/**
 * Ordered, de-duped playable mirrors: direct (non-blocked) hosts first, and the
 * author's "Default" first within each group. Server-only builds are dropped
 * unless they're all that's on offer. Shared by the detail view, the install
 * dialog, and quick-install so the "primary" mirror is chosen identically.
 */
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

/**
 * Sound-mod pages list a *different* download per bike ("Just KTM 250SX-F") plus
 * a "Main pack with all bikes" default — these are NOT mirrors of one file. Pick
 * the link whose label / filename best matches the chosen bike; fall back to the
 * author's default (usually the all-bikes pack), else the first mirror.
 */
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

/** localStorage key for the remembered install folder of a mod type. */
export function destStorageKey(modType: ModType): string {
  return `frost-dest-${modType.id}`;
}

/** Remembered folder (if still a valid option) else the educated guess. For
 * bikes, a remembered `…/paints` folder is only reused for a livery — so a new
 * bike/sound install doesn't inherit the last livery's paints destination. */
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
  // A remembered `…/paints` folder is only meaningful for a livery. A sound
  // targets a bike root and a new bike targets Bikes (root) — use the guess.
  if (modType.id === "bikes" && rememberedIsPaints && !livery) return guess;
  // A sound should never inherit some other remembered bike-root either; prefer
  // the name-matched guess when we have one.
  if (modType.id === "bikes" && sound && guess) return guess;
  // Rider gear paints share one remembered key across helmets/boots/protection.
  // When the paint's kind is known, don't inherit a folder from a different kind
  // (a boot paint must not default into the last helmet's `paints`) — use the
  // kind-matched guess instead.
  if (paintKind && remembered && !remembered.startsWith(`${paintKind}/`)) return guess;
  if (destOptions.some((o) => o.value === remembered)) return remembered;
  return guess;
}

/** Fully-resolved input for a one-click install (matches `startInstall`). */
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
 * Resolve everything a silent quick-install needs from a mod slug: fetch the
 * page detail, pick the primary direct mirror, and compute the destination
 * folder the same way the install dialog would. Any host in
 * `BLOCKED_HOST_PATTERNS` (currently none) can't be installed silently and is
 * reported as `blocked` so the caller can route the user to the browser flow.
 */
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

// --- MX Bikes Shop (paid, authenticated) -----------------------------------
// The shop (mxbikes-shop.com) is a WordPress + Easy Digital Downloads store
// behind Cloudflare. The user signs in through a real WebView window; we reuse
// the captured session to list and download the tracks they've purchased.

/** A purchased shop download — a `ModSummary` plus its authenticated file URL. */
export interface ShopItem extends ModSummary {
  downloadUrl: string;
}

/** Open the shop sign-in WebView. Resolves once the window is opened; the
 * `shop-auth` event fires when login completes. */
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

// --- FrostMod live-reload --------------------------------------------------
// FrostMod (github.com/Frostn1/frostmod) live-reloads MX Bikes' content when
// it's running. Installs signal it automatically; these expose a manual trigger
// and a status probe for the UI.

/** Manually ask a running FrostMod to reload the mods folder now. */
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

/** Toggle instant-refresh (re-run the game's profile loader after applying a
 * preset so the look updates live, Windows-only). */
export function setInstantRefresh(enabled: boolean): Promise<void> {
  return invoke<void>("set_instant_refresh", { enabled });
}

/** Fires after each install with whether FrostMod picked the new mod up live. */
export function onFrostmodReload(
  cb: (payload: FrostmodReload) => void,
): Promise<UnlistenFn> {
  return listen<FrostmodReload>("frostmod-reload", (event) => cb(event.payload));
}

// --- Customization presets (per-bike loadouts) -----------------------------
// MX Bikes stores the selected look per-bike in `profile.ini`. A preset is a
// bike-agnostic bundle of every slot value; applying it writes that bike's row.

/** Game/rider profiles that have a `profile.ini` (each keeps its own per-bike look). */
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

/**
 * Apply a loadout to a bike: writes its row across every `profile.ini` slot
 * section and (when `makeActive`) points the game at that bike. Nudges a running
 * FrostMod to reload the mods folder, and — when the `instantRefresh` setting is
 * on — re-runs the game's profile loader in place so the new look shows without a
 * restart or manual reselect. The outcome says exactly how it took effect. A
 * one-shot `profile.ini.bak` is written before the change.
 */
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

// --- Preset full-share bundles (assets uploaded/downloaded) -----------------
// A "full share" packages every asset a preset references into a .zip, uploads
// it to an anonymous host, and embeds the link in the share code. The recipient's
// "Full import" downloads it and installs every file into `mods/`.

/** Preview what a preset's full bundle would carry (assets, what won't travel,
 * total size). Read-only — nothing is uploaded. */
export function presetBundleStats(loadout: Loadout): Promise<BundlePlan> {
  return invoke<BundlePlan>("preset_bundle_stats", { loadout });
}

/** Build + upload a preset's asset bundle; returns the full share code (with the
 * bundle link embedded). Progress arrives via {@link onPresetBundleProgress}. */
export function presetBundleCreate(name: string): Promise<string> {
  return invoke<string>("preset_bundle_create", { name });
}

/** Import a full-share code: download the bundle, install every asset, save the
 * preset. Progress via {@link onPresetBundleProgress} (+ download bytes on
 * {@link onInstallProgress}). */
export function presetBundleImport(text: string): Promise<Preset> {
  return invoke<Preset>("preset_bundle_import", { text });
}

/** Subscribe to bundle create/import phase updates. */
export function onPresetBundleProgress(
  cb: (p: BundleProgress) => void,
): Promise<UnlistenFn> {
  return listen<BundleProgress>("preset-bundle-progress", (event) => cb(event.payload));
}
