import { useEffect, useState } from "react";

import { listPlugins } from "@/api/plugins";
import {
  loadedPlugins,
  mountReady,
  onPluginsChanged,
  type LoadedPlugin,
} from "./pluginHost";

/**
 * The plugins running in this session, and the effect that starts them.
 *
 * Mounting happens once, at the point the shell first renders — not on the Plugins page,
 * because a plugin the user is licensed for should be there when they open the app, not
 * when they go looking for it in settings.
 *
 * A plugin that fails to mount is reported through `onError` and then dropped. Everything
 * else carries on: a broken plugin must not be able to take the app down with it, and the
 * app is a mod manager first.
 */
export function usePlugins(onError?: (id: string, message: string) => void): LoadedPlugin[] {
  const [plugins, setPlugins] = useState<LoadedPlugin[]>(() => loadedPlugins());

  useEffect(() => onPluginsChanged(() => setPlugins(loadedPlugins())), []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      let ready: string[] = [];
      try {
        ready = (await listPlugins()).filter((p) => p.ready).map((p) => p.id);
      } catch {
        // No control plane and nothing on disk yet. Not an error worth showing: the person
        // has not bought anything, and the Plugins page is where that conversation happens.
        return;
      }
      if (cancelled || ready.length === 0) return;
      await mountReady(ready, (id, message) => {
        if (!cancelled) onError?.(id, message);
      });
    })();
    return () => {
      cancelled = true;
    };
    // Once per session. Installing from the Plugins page mounts directly.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return plugins;
}

/** `plugin:replaycam/paths` -> `{ plugin: "replaycam", panel: "paths" }`, or null. */
export function parsePluginView(view: string): { plugin: string; panel: string } | null {
  if (!view.startsWith("plugin:")) return null;
  const rest = view.slice("plugin:".length);
  const slash = rest.indexOf("/");
  if (slash < 0) return null;
  return { plugin: rest.slice(0, slash), panel: rest.slice(slash + 1) };
}

export function pluginViewId(plugin: string, panel: string): string {
  return `plugin:${plugin}/${panel}`;
}
