import { invoke } from "@tauri-apps/api/core";

export type GamePluginKind = "thirdParty" | "mxbApp";

export interface GamePluginConfig {
  path: string;
  label: string;
}

export interface GamePluginKey {
  key: string;
  label: string;
}

export interface GamePlugin {
  id: string;
  name: string;
  author: string;
  kind: GamePluginKind;
  enabled: boolean;
  files: string[];
  configs: GamePluginConfig[];
  size: number;
  modified?: number;
  version?: string;
  duplicateDisabledCopy: boolean;
  keys: GamePluginKey[];
  presets: string[];
  note?: string;
  madeFor?: string;
  /** Racing mode turns it off (the player's choice, else the plugin's default). */
  raceOff: boolean;
  /** A crash fix or one of ours: always stays on when racing. */
  raceLocked: boolean;
  /** Another installed plugin already does this one's job (MXBFix under FrostMod). */
  coveredBy?: string;
  /** Browse category: hud, graphics, physics, fixes, timing or other. */
  category: string;
  summary: string;
  /** Racing mode's default for it (catalog, else its manifest). */
  raceDefault: boolean;
  /** The author's page, from the manifest. */
  homepage?: string;
  /** It ships an .mxbplugin.json the app read. */
  hasManifest: boolean;
  /** Where the app can fetch it from: `none` is a plugin it doesn't know. */
  source: "github" | "mxbmods" | "external" | "none";
  /** The mxb-mods upload date recorded when the app installed it. */
  installedUpdated?: string;
}

export interface AvailableGamePlugin {
  id: string;
  name: string;
  author: string;
  source: "github" | "mxbmods" | "external";
  page?: string;
  slug?: string;
  note?: string;
}

/** One card in Browse: every plugin the app knows, installed or not. */
export interface GamePluginCatalogEntry {
  id: string;
  name: string;
  author: string;
  category: string;
  summary: string;
  source: "github" | "mxbmods" | "external";
  page?: string;
  note?: string;
  installed: boolean;
}

export interface GamePluginConflict {
  level: "error" | "warning" | "info";
  ids: string[];
  message: string;
}

export interface GamePluginsView {
  installed: GamePlugin[];
  available: AvailableGamePlugin[];
  catalog: GamePluginCatalogEntry[];
  conflicts: GamePluginConflict[];
  gameBuild?: string;
  gameRunning: boolean;
  raceMode: boolean;
  /** Names racing mode would turn off now. */
  racingPlanned: string[];
  /** Names racing mode has turned off; empty while it's off. */
  racingOff: string[];
  /** Racing mode came on with an app join and goes off when the game closes. */
  racingByJoin: boolean;
  /** Plugins the app just turned off because another installed one covers them. */
  autoTurnedOff: string[];
}
export interface GamePluginLatest {
  id: string;
  version?: string;
  updated?: string;
  page?: string;
  thumbnail?: string;
  rating?: number;
  views?: number;
}

export const gamePluginsList = () => invoke<GamePluginsView>("game_plugins_list");
export const gamePluginsLatest = () => invoke<GamePluginLatest[]>("game_plugins_latest");
export const gamePluginSetEnabled = (id: string, enabled: boolean) =>
  invoke<void>("game_plugin_set_enabled", { id, enabled });
export const gamePluginUninstall = (id: string) =>
  invoke<void>("game_plugin_uninstall", { id });
export const gamePluginsDisableAll = () => invoke<number>("game_plugins_disable_all");
export const gamePluginsRestore = () => invoke<number>("game_plugins_restore");
export const gamePluginOpenConfig = (id: string, path: string) =>
  invoke<void>("game_plugin_open_config", { id, path });
export const gamePluginReveal = (id: string) => invoke<void>("game_plugin_reveal", { id });
export const gamePluginSetRaceOff = (id: string, off: boolean) =>
  invoke<void>("game_plugin_set_race_off", { id, off });
export const gamePluginInstall = (id: string) => invoke<void>("game_plugin_install", { id });

/** `true` when `latest` is newer than what's on disk. GitHub releases compare by version;
 *  mxb-mods posts have no version, so the upload date the app installed is compared with the
 *  post's last edit. A plugin installed by hand has neither, and is never nagged about. */
export function hasUpdate(plugin: GamePlugin, latest?: GamePluginLatest): boolean {
  if (!latest) return false;
  // GitHub releases and manifest plugins (whose author names a GitHub repo) compare versions.
  if (plugin.source === "github" || plugin.source === "external") {
    return Boolean(plugin.version && latest.version && versionOlder(plugin.version, latest.version));
  }
  if (plugin.source === "mxbmods") {
    if (!plugin.installedUpdated || !latest.updated) return false;
    return Date.parse(plugin.installedUpdated) < Date.parse(latest.updated);
  }
  return false;
}

/** Dotted-number compare, `v` prefix ignored: `1.9` is older than `1.10.0`. */
export function versionOlder(installed: string, latest: string): boolean {
  const parts = (value: string) =>
    value.replace(/^v/i, "").split(".").map((part) => Number.parseInt(part, 10) || 0);
  const a = parts(installed);
  const b = parts(latest);
  for (let i = 0; i < Math.max(a.length, b.length); i += 1) {
    if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) < (b[i] ?? 0);
  }
  return false;
}

/** A plugin's page: description and pictures from its mxb-mods post. */
export interface GamePluginDetail {
  id: string;
  descriptionHtml?: string;
  images: string[];
  page?: string;
  rating?: number;
  ratings?: number;
  views?: number;
  updated?: string;
}

export const gamePluginDetail = (id: string) => invoke<GamePluginDetail>("game_plugin_detail", { id });
export const gamePluginReadConfig = (id: string, path: string) =>
  invoke<string>("game_plugin_read_config", { id, path });
export const gamePluginWriteConfig = (id: string, path: string, text: string) =>
  invoke<void>("game_plugin_write_config", { id, path, text });
