import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Config,
  DownloadOption,
  FrostmodReload,
  FrostmodStatus,
  InstalledMod,
  InstallProgress,
  LibraryEntry,
  ModDetail,
  ModSummary,
  PkzMeta,
  ReloadOutcome,
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
      { id: 36, label: "Protection" },
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

/** Whether we're installing a bike **livery** (→ `<Bike>/paints`) vs a new bike
 * / sound / unknown (→ Bikes root). Drives the destination default so a new bike
 * never inherits a previous livery's `paints` folder. */
export function isLiveryContext(
  modType: ModType,
  categoryId: number | null | undefined,
): boolean {
  return modType.id === "bikes" && categoryId === LIVERY_CATEGORY_ID;
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
 * Read one installed `.pkz`'s structure (name/author/length/preview) for its
 * library card. Parsed lazily per card and cached in the backend. GUID-locked
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
    // Each bike's `paints` folder is always pickable in the dialog…
    for (const b of bikes) add(`${stripExt(b.name)}/paints`, `${stripExt(b.name)} — paints`);

    // …but only *default*/*suggest* a paints folder for actual liveries. A new
    // bike / sound must land in Bikes (root), not some bike's paints.
    if (livery) {
      const tt = tokens(title);
      const scored = bikes
        .map((b) => {
          let score = 0;
          for (const t of tokens(b.name)) if (tt.has(t)) score++;
          return { value: `${stripExt(b.name)}/paints`, score };
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

/**
 * Destinations for **rider** content. Helmet/boot/protection paints drop into
 * their model's `paints` (and helmets also `goggles`); rider kit + gloves live
 * per rider profile under `riders/<profile>/{paints,gloves}`. New models install
 * to their type root. Suggestions rank name-matched model paints first, then the
 * rider profiles (for kit/gloves).
 */
export function buildRiderDestinations(
  targets: RiderTargets,
  title: string,
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

  // Model paints, scored for suggestions.
  const scoredPaints: { value: string; score: number }[] = [];
  for (const h of targets.helmets) {
    add(`helmets/${h}/paints`, `${h} · helmet paints`);
    add(`helmets/${h}/goggles`, `${h} · goggles`);
    scoredPaints.push({ value: `helmets/${h}/paints`, score: score(h) });
  }
  for (const b of targets.boots) {
    add(`boots/${b}/paints`, `${b} · boot paints`);
    scoredPaints.push({ value: `boots/${b}/paints`, score: score(b) });
  }
  for (const p of targets.protection) {
    add(`protection/${p}/paints`, `${p} · protection paints`);
    scoredPaints.push({ value: `protection/${p}/paints`, score: score(p) });
  }

  // Per-profile outfit (rider kit) + gloves.
  for (const prof of targets.profiles) {
    add(`riders/${prof}/paints`, `${prof} · outfit / kit`);
    add(`riders/${prof}/gloves`, `${prof} · gloves`);
  }

  const topPaints = scoredPaints
    .filter((s) => s.score >= 1)
    .sort((a, b) => b.score - a.score);
  const suggestions = [
    ...topPaints.slice(0, 4).map((s) => s.value),
    // Rider kit / gloves can't be name-matched — surface the profiles too.
    ...targets.profiles.map((p) => `riders/${p}/paints`),
  ];
  const guess = topPaints[0] && topPaints[0].score >= 2 ? topPaints[0].value : "";

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
): string {
  const remembered = localStorage.getItem(destStorageKey(modType)) ?? "";
  const rememberedIsPaints = /\/paints$/i.test(remembered);
  if (modType.id === "bikes" && rememberedIsPaints && !livery) return guess;
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

/** Fires after each install with whether FrostMod picked the new mod up live. */
export function onFrostmodReload(
  cb: (payload: FrostmodReload) => void,
): Promise<UnlistenFn> {
  return listen<FrostmodReload>("frostmod-reload", (event) => cb(event.payload));
}
