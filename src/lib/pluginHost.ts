import { invoke } from "@tauri-apps/api/core";
import type { ComponentType } from "react";
import * as React from "react";

import { pluginRuntime, type PluginManifest } from "@/api/plugins";

/**
 * Loading and mounting a paid plugin's UI.
 *
 * A plugin ships its panels as one ES module. The Rust side has already verified the
 * licence and the bundle's signature-named hash before handing us a line of it, so what is
 * left here is the mechanics: turn the source text into a module, hand it a small stable
 * API, and take back the components it registers.
 *
 * **Source text, not a file URL.** The module arrives as a string and is turned into a blob
 * URL right before `import()`. That is deliberate: nothing on disk is ever loaded by path,
 * so a file dropped into the plugins folder by hand is not a way to get code into the app —
 * the only route in goes through the licence check that produced this string.
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
 * only inspecting it. Every call re-checks the licence, so a plugin whose subscription
 * lapses stops being able to write mid-session.
 */
export interface PluginFiles {
  read(path: string): Promise<string>;
  write(path: string, contents: string): Promise<void>;
  list(path: string): Promise<string[]>;
  remove(path: string): Promise<void>;
}

export interface PluginApi {
  /** Format version of this surface. A plugin should refuse a major it does not know. */
  readonly version: 1;
  /** React itself, so a plugin bundle carries no copy of its own and hooks work. */
  readonly react: typeof React;
  /**
   * Call a backend command the plugin's own payload installed, or one of the app's.
   *
   * Not a general escape hatch by intent, but it is one in effect — a plugin runs with the
   * app's privileges, which is why a bundle only ever arrives over a verified licence.
   */
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  /** Read and write inside the game's user folder. */
  readonly files: PluginFiles;
  /**
   * Copy this plugin's `payload/` into the game's plugins folder — how a paid *mod*, as
   * opposed to a paid panel, gets installed. Resolves with the filenames written.
   */
  installPayload(): Promise<string[]>;
  /** Register the panels this plugin contributes. Called once, from the entry module. */
  registerPanels(panels: PluginPanel[]): void;
}

export interface LoadedPlugin {
  manifest: PluginManifest;
  panels: PluginPanel[];
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
 * Throws with something a person can act on: the licence checks in the backend answer in
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
 * app rendering its panels. A licence that lapses mid-session therefore takes its UI away
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
