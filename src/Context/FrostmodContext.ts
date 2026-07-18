import { createContext, useContext } from "react";
import type { FrostmodStatus, ReloadOutcome } from "../types";

export interface FrostmodContextValue {
  /** Whether FrostMod is currently running (polled). `null` until first probe. */
  running: boolean | null;
  /** Install/version snapshot (`null` until first fetched). */
  status: FrostmodStatus | null;
  /** True while an install/update download is in flight. */
  installing: boolean;
  /** True while a `refreshStatus` GitHub check is in flight. */
  checking: boolean;
  /** True when the last `refreshStatus` failed (offline / GitHub error). */
  statusError: boolean;
  /** Manually ask FrostMod to live-reload the game now. */
  reload: () => Promise<ReloadOutcome>;
  /** Re-probe running status immediately. */
  refresh: () => void;
  /** Re-fetch the full install/version status (hits GitHub). */
  refreshStatus: () => Promise<void>;
  /** Download the latest FrostMod, then start it. */
  install: () => Promise<void>;
  /** Launch FrostMod now if it isn't already running. */
  start: () => Promise<void>;
}

// Kept in a component-free module (like Config.ts) so the context identity stays
// stable across Vite Fast Refresh. If this lived alongside FrostmodProvider, hot
// updates would re-run createContext() and transiently break useContext, throwing
// "useFrostmod must be used within FrostmodProvider" mid-dev.
export const FrostmodContext = createContext<FrostmodContextValue | null>(null);

export function useFrostmod() {
  const ctx = useContext(FrostmodContext);
  if (!ctx) throw new Error("useFrostmod must be used within FrostmodProvider");
  return ctx;
}
