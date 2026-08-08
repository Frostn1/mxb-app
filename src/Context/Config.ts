import { createContext, useContext } from "react";
import type { Config, GameCaps, GameId, GameInfo } from "../types";

/** Capabilities assumed before `listGames()` answers: MX Bikes', the title every
 *  existing install is on. Keeps the first paint from flickering features off and back on. */
const MXB_CAPS: GameCaps = {
  frostmod: true,
  instantRefresh: true,
  viewer: true,
  shop: true,
  manage: true,
};

const MXB_FALLBACK: GameInfo = {
  id: "mxb",
  display: "MX Bikes",
  modsDirs: ["bikes", "tracks", "rider", "tyres", "misc"],
  catalogDomain: "mxb-mods.com",
  caps: MXB_CAPS,
};

export interface ConfigContextValue {
  config: Config;
  /** Re-read the saved config (e.g. after the user changes the game folder). */
  reloadConfig: () => Promise<void>;
  /** This build can decode real bike geometry (optional local module compiled in).
   *  False on public builds → hide the bike 3D preview. */
  bikePreview: boolean;
  /** Every title this build can drive, for the switcher. */
  games: GameInfo[];
  /** The active title — its display name, mods folders and capabilities. */
  game: GameInfo;
  /** Switch games: parks the current one's folders, restores the target's (or
   *  auto-detects them on first use) and re-reads the config. */
  switchGame: (id: GameId) => Promise<void>;
}

export const ConfigContext = createContext<ConfigContextValue>({
  config: { modsPath: "" },
  reloadConfig: async () => {},
  bikePreview: false,
  games: [MXB_FALLBACK],
  game: MXB_FALLBACK,
  switchGame: async () => {},
});

export function useConfig() {
  return useContext(ConfigContext);
}

/** The active title's capabilities — the common case, so it gets a shorthand. */
export function useCaps(): GameCaps {
  return useContext(ConfigContext).game.caps;
}

export { MXB_FALLBACK };
