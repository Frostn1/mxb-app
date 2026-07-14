import { createContext, useContext } from "react";
import type { Config } from "../types";

export interface ConfigContextValue {
  config: Config;
  /** Re-read the saved config (e.g. after the user changes the game folder). */
  reloadConfig: () => Promise<void>;
}

export const ConfigContext = createContext<ConfigContextValue>({
  config: { modsPath: "" },
  reloadConfig: async () => {},
});

export function useConfig() {
  return useContext(ConfigContext);
}
