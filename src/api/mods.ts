import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Config,
  DownloadOption,
  FrostmodReload,
  InstalledMod,
  InstallProgress,
  ModDetail,
  ModSummary,
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
      { id: 95, label: "Wheels" },
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
): { options: DestOption[]; guess: string } {
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
  if (modType.id === "bikes") {
    const bikes = installed.filter((i) => i.folder === "");
    for (const b of bikes) add(`${stripExt(b.name)}/paints`, `${stripExt(b.name)} — paints`);

    const tt = tokens(title);
    let best: InstalledMod | null = null;
    let bestScore = 0;
    for (const b of bikes) {
      let score = 0;
      for (const t of tokens(b.name)) if (tt.has(t)) score++;
      if (score > bestScore) {
        bestScore = score;
        best = b;
      }
    }
    if (best && bestScore >= 2) guess = `${stripExt(best.name)}/paints`;
  }

  for (const f of [...new Set(installed.map((i) => i.folder))].sort((a, b) => a.localeCompare(b))) {
    if (f) add(f, f);
  }
  return { options, guess };
}

/**
 * Hosts that block in-app downloads (TLS fingerprinting) — the app opens these
 * in the browser and lets the user import the downloaded file instead.
 * Matched against the URL (reliable) as well as the host label (which the site
 * writes inconsistently, e.g. "Media Fire" with a space).
 */
const BLOCKED_HOST_PATTERNS = ["mediafire", "media fire", "mega.nz", "mega.co", "mega."];

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

/** Remembered folder (if still a valid option) else the educated guess. */
export function resolveInitialFolder(
  modType: ModType,
  destOptions: DestOption[],
  guess: string,
): string {
  const remembered = localStorage.getItem(destStorageKey(modType)) ?? "";
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
 * folder the same way the install dialog would. Hosts that block in-app
 * downloads (MediaFire/Mega) can't be installed silently — reported as
 * `blocked` so the caller can route the user to the browser flow.
 */
export async function resolveQuickInstall(
  slug: string,
  modType: ModType,
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
  const { options, guess } = buildDestinations(modType, detail.title, installed);
  const destFolder = resolveInitialFolder(modType, options, guess);

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

/** Fires after each install with whether FrostMod picked the new mod up live. */
export function onFrostmodReload(
  cb: (payload: FrostmodReload) => void,
): Promise<UnlistenFn> {
  return listen<FrostmodReload>("frostmod-reload", (event) => cb(event.payload));
}
