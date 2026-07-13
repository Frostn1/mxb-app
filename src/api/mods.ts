import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Config,
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

/** Kick off download → extract → place. Progress arrives via `onInstallProgress`. */
export function addToLibrary(
  slug: string,
  url: string,
  host: string,
  subpath: string,
): Promise<void> {
  return invoke<void>("add_to_library", { slug, url, host, subpath });
}

/** Import a file the user downloaded manually (extract + place into `subpath`). */
export function importFile(path: string, subpath: string): Promise<void> {
  return invoke<void>("import_file", { path, subpath });
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
