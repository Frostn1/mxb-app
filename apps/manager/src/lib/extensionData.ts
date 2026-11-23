import { useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";
import {
  cachedMasterServers,
  getInstalledMods,
  isGameRunning,
  modTypesFor,
  launchStudio,
  presetsList,
} from "@frost/shared/api/mods";
import { extCoachSessions } from "@frost/shared/extensions/api";
import {
  provideExtensionData,
  toExtFriend,
  toExtMod,
  toExtServer,
  type ExtMod,
} from "@frost/shared/extensions/data";
import type { GameId } from "@frost/shared/types";
import { friendsList } from "../api/friends";

/**
 * What MXB App hands its extensions. Each source reads through the same API the app's own
 * screens use, then through `data.ts`, which copies out only the fields an extension gets.
 * The permission check is the bridge's job; nothing here is reachable without it.
 */
export function useExtensionData(game: GameId): void {
  useEffect(() => {
    let version = "0.0.0";
    void getVersion().then((v) => {
      version = v;
      provide();
    });
    const provide = () =>
      provideExtensionData({
        app: "manager",
        appVersion: version,
        mods: async () => {
          const out: ExtMod[] = [];
          const seen = new Set<string>();
          for (const type of modTypesFor(game)) {
            if (!type.installSubpath || seen.has(type.installSubpath)) continue;
            seen.add(type.installSubpath);
            const mods = await getInstalledMods(type.installSubpath).catch(() => []);
            out.push(...mods.map((m) => toExtMod(m, type.id)));
          }
          return out;
        },
        servers: async () => {
          const list = await cachedMasterServers();
          return list.servers.filter((s) => !s.hidden).map(toExtServer);
        },
        friends: async () => {
          const state = await friendsList();
          return state.friends.map((f) => toExtFriend(f));
        },
        gameState: async () => ({ game, running: await isGameRunning() }),
        coach: () => extCoachSessions(),
        studio: (screen) => launchStudio(screen),
        presets: async () => (await presetsList()).map((p) => ({ name: String(p.name).slice(0, 80) })),
      });
    provide();
  }, [game]);
}
