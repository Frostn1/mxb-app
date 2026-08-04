import { createContext, useContext } from "react";
import type { Config } from "../types";

export interface ConfigContextValue {
  config: Config;
  /** Re-read the saved config (e.g. after the user changes the game folder). */
  reloadConfig: () => Promise<void>;
  /** This build can decode real bike geometry (optional local module compiled in).
   *  False on public builds → hide the bike 3D preview. */
  bikePreview: boolean;
}

export const ConfigContext = createContext<ConfigContextValue>({
  config: { modsPath: "" },
  reloadConfig: async () => {},
  bikePreview: false,
});

export function useConfig() {
  return useContext(ConfigContext);
}
