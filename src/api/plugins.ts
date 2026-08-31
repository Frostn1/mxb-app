import { invoke } from "@tauri-apps/api/core";

/**
 * Why a plugin is or is not runnable.
 *
 * `stale` is the one worth understanding: the license has not expired, but we have not been
 * able to re-check it inside the grace window. It reads to the user as "needs to go online",
 * not as "you have been cut off" — and it is what makes a cancellation take effect on a
 * machine that stops asking.
 */
export type PluginStatus = "live" | "stale" | "expired";

export interface PluginView {
  id: string;
  name: string;
  summary: string | null;
  /** The version on offer, which may be newer than what is installed. */
  version: string | null;
  /** Whether there is a build to install at all. */
  published: boolean;
  status: PluginStatus;
  /** Seconds since epoch. Null if this account has never held a license. */
  expires: number | null;
  installedVersion: string | null;
  /** Licensed, installed, and on the current build — the only state that runs. */
  ready: boolean;
}

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  entry: string;
  minAppVersion?: string | null;
  panels: { id: string; label: string; icon?: string | null }[];
}

export interface PluginRuntime {
  manifest: PluginManifest;
  /** The entry module's source, handed over as text — see `mountPlugin`. */
  source: string;
}

/** The catalogue plus this account's licenses. Works offline, from what is on disk. */
export function listPlugins(): Promise<PluginView[]> {
  return invoke("plugin_list");
}

/** Trade a key for months on a license. Resolves with the plugin's name. */
export function redeemPluginKey(code: string): Promise<string> {
  return invoke("plugin_redeem", { code });
}

/** Download, verify and unpack. Resolves with the plugin's name. */
export function installPlugin(id: string): Promise<string> {
  return invoke("plugin_install", { id });
}

export function removePlugin(id: string): Promise<void> {
  return invoke("plugin_remove", { id });
}

/**
 * Fetch a plugin's manifest and entry source.
 *
 * The license is checked again on this call, at the last point before the code runs — not
 * only on the page that listed it. A plugin whose license lapsed between the list and the
 * mount must not start.
 */
export function pluginRuntime(id: string): Promise<PluginRuntime> {
  return invoke("plugin_runtime", { id });
}
