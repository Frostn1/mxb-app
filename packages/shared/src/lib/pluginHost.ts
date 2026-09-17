import { invoke } from "@tauri-apps/api/core";
import type { ComponentType } from "react";
import * as React from "react";

import { pluginRuntime, type PluginHost, type PluginManifest } from "../api/plugins";
import type { ReplayStatus } from "../api/replay";

/**
 * Loading and mounting a paid plugin's UI.
 *
 * A plugin ships its panels as one ES module. The Rust side has already verified the
 * license and the bundle's signature-named hash before handing us a line of it, so what is
 * left here is the mechanics: turn the source text into a module, hand it a small stable
 * API, and take back the components it registers.
 *
 * **Source text, not a file URL.** The module arrives as a string and is turned into a blob
 * URL right before `import()`. That is deliberate: nothing on disk is ever loaded by path,
 * so a file dropped into the plugins folder by hand is not a way to get code into the app —
 * the only route in goes through the license check that produced this string.
 *
 * The API handed to a plugin is deliberately small. Every method here is one the app is
 * promising to keep working across versions, so the bar for adding one is that a plugin
 * cannot do its job without it.
 */

/** What a plugin registers. One entry per nav row it contributes. */
export interface PluginPanel {
  id: string;
  label: string;
  component: ComponentType;
}

/**
 * The files a plugin may touch: its own, inside the game's user folder.
 *
 * Paths are relative to `Documents\PiBoSo\<game>` and cannot leave it — including through
 * a symlink already on disk, which the backend checks by resolving the path rather than
 * only inspecting it. Every call re-checks the license, so a plugin whose subscription
 * lapses stops being able to write mid-session.
 */
export interface PluginFiles {
  read(path: string): Promise<string>;
  write(path: string, contents: string): Promise<void>;
  list(path: string): Promise<string[]>;
  remove(path: string): Promise<void>;
}

/**
 * The replay recorder, handed to a plugin whose panels run in the Studio.
 *
 * Here rather than left to `invoke` because it is the one thing the Replay Mod's panels
 * cannot do for themselves and the app can: a child encoder against the game's window. A
 * panel that wants a **Record** button of its own uses this; a panel that does nothing gets
 * the recording anyway, because the mod's own take signal starts one without the window
 * being open at all.
 *
 * Only in the Studio. In the mod manager every method rejects — the commands behind them are
 * registered by one binary, and that is the binary the panels are in.
 */
export interface PluginRecorder {
  status(): Promise<ReplayStatus>;
  /** Start recording now. Resolves with the file being written. */
  start(): Promise<string>;
  /** Stop. Resolves with the finished file, or null if nothing was recording. */
  stop(): Promise<string | null>;
  /** Called whenever the recorder's state moves. Returns an unsubscribe. */
  onChange(fn: (status: ReplayStatus) => void): () => void;
}

export interface PluginApi {
  /**
   * Format version of this surface. A plugin should refuse a major it does not know.
   *
   * Still 1 after the move to the Studio, deliberately: everything that arrived with it —
   * `host`, `recorder` — is additive, and a bundle that checks `version === 1` and knows
   * nothing about either still loads and still works.
   */
  readonly version: 1;
  /** Which app this panel is running in. `"studio"` for everything but a plugin that asked
   *  for the manager in its manifest. */
  readonly host: PluginHost;
  /** React itself, so a plugin bundle carries no copy of its own and hooks work. */
  readonly react: typeof React;
  /**
   * Call a backend command the plugin's own payload installed, or one of the app's.
   *
   * Not a general escape hatch by intent, but it is one in effect — a plugin runs with the
   * app's privileges, which is why a bundle only ever arrives over a verified license.
   */
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  /** Read and write inside the game's user folder. */
  readonly files: PluginFiles;
  /**
   * Copy this plugin's `payload/` into the game's plugins folder — how a paid *mod*, as
   * opposed to a paid panel, gets installed. Resolves with the filenames written.
   */
  installPayload(): Promise<string[]>;
  /** Record the game while the mod flies a shot — see [`PluginRecorder`]. */
  readonly recorder: PluginRecorder;
  /** Register the panels this plugin contributes. Called once, from the entry module. */
  registerPanels(panels: PluginPanel[]): void;
}

export interface LoadedPlugin {
  manifest: PluginManifest;
  panels: PluginPanel[];
}

/**
 * The recorder handed to panels, and how the app that has one provides it.
 *
 * Injected rather than imported, because only one binary registers those commands. Importing
 * them here would put `replay_record` in the mod manager's bundle as well, where it is a call
 * that can only ever fail — and the repo's command census would rightly call that a command
 * the app reaches but does not register.
 *
 * The default is what a plugin gets in an app without a recorder: a refusal that says which
 * window has one, rather than a "command not found" from the backend.
 */
const absent = (): Promise<never> =>
  Promise.reject(new Error("The replay recorder only runs in Frost's Studio."));

let recorder: PluginRecorder = {
  status: absent,
  start: absent,
  stop: absent,
  onChange: () => () => {},
};

/** Called once by the app that owns the recorder, before any plugin mounts. */
export function providePluginRecorder(r: PluginRecorder): void {
  recorder = r;
}

/** Plugins mounted this session, by id. A second mount of the same id is a no-op. */
const loaded = new Map<string, LoadedPlugin>();
const listeners = new Set<() => void>();

function announce() {
  for (const l of listeners) l();
}

export function onPluginsChanged(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function loadedPlugins(): LoadedPlugin[] {
  return [...loaded.values()];
}

export function isMounted(id: string): boolean {
  return loaded.has(id);
}

/**
 * Verify, fetch and run a plugin's entry module.
 *
 * Throws with something a person can act on: the license checks in the backend answer in
 * sentences, and a plugin that fails to mount is a thing the user paid for, so "it didn't
 * work" is not an acceptable message.
 */
export async function mountPlugin(id: string): Promise<LoadedPlugin> {
  const already = loaded.get(id);
  if (already) return already;

  const { manifest, source } = await pluginRuntime(id);

  const panels: PluginPanel[] = [];
  const api: PluginApi = {
    version: 1,
    host: manifest.host ?? "studio",
    react: React,
    invoke: <T,>(command: string, args?: Record<string, unknown>) =>
      invoke<T>(command, args),
    files: {
      read: (path) => invoke<string>("plugin_read_file", { id, path }),
      write: (path, contents) => invoke<void>("plugin_write_file", { id, path, contents }),
      list: (path) => invoke<string[]>("plugin_list_dir", { id, path }),
      remove: (path) => invoke<void>("plugin_delete_file", { id, path }),
    },
    installPayload: () => invoke<string[]>("plugin_install_payload", { id }),
    recorder,
    registerPanels: (p) => panels.push(...p),
  };

  // The blob is revoked as soon as the module has been evaluated: the URL is a live handle
  // to executable code, and leaving one lying around for the lifetime of the window is a
  // loose end for no benefit — the module object outlives its URL.
  const url = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
  let mod: { default?: (api: PluginApi) => void | Promise<void> };
  try {
    mod = (await import(/* @vite-ignore */ url)) as typeof mod;
  } catch (e) {
    throw new Error(
      `${manifest.name} failed to load: ${e instanceof Error ? e.message : String(e)}`,
    );
  } finally {
    URL.revokeObjectURL(url);
  }

  if (typeof mod.default !== "function") {
    throw new Error(`${manifest.name}'s entry module has no default export to call.`);
  }
  await mod.default(api);

  if (panels.length === 0) {
    throw new Error(`${manifest.name} loaded but registered nothing to show.`);
  }

  const entry: LoadedPlugin = { manifest, panels };
  loaded.set(id, entry);
  announce();
  return entry;
}

/**
 * Drop a plugin from this session.
 *
 * The module itself cannot be unloaded — nothing in a browser can — so this only stops the
 * app rendering its panels. A license that lapses mid-session therefore takes its UI away
 * without pretending the code has gone; the code that matters is the game-side payload, and
 * that is gated where it is installed.
 */
export function unmountPlugin(id: string): void {
  if (loaded.delete(id)) announce();
}

/** Mount everything this account is licensed for. Failures are reported, never fatal. */
export async function mountReady(
  ids: string[],
  onError?: (id: string, message: string) => void,
): Promise<void> {
  for (const id of ids) {
    if (loaded.has(id)) continue;
    try {
      await mountPlugin(id);
    } catch (e) {
      onError?.(id, e instanceof Error ? e.message : String(e));
    }
  }
}
